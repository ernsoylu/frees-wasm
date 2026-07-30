// Module-level columnar sample store for the Data Analyzer (contract §2.5a).
//
// Deliberately the first non-React store in the frontend (ADR note, todo.md
// §2): it holds BULK DATA ONLY — Float64Array columns keyed by measurementId —
// never UI state, so 50M-cell imports cause no re-render storms and never
// bloat the autosaved project. React components subscribe for invalidation
// (a version counter) and read windows imperatively.
//
// Lifecycle (§2.5a):
//  - cache key = measurementId (uuid minted at import time); the AnalyzerSpec
//    stores the id + file signature, never the data;
//  - entries are refcounted by the AnalyzerSpecs that reference them (a Set of
//    analyzer ids — two windows on the same file share one entry);
//  - release binds to analyzer *deletion*, not window close: App calls
//    release() from onDeleteAnalyzer, mirroring onDeleteWhiteboard, so a
//    closed-but-not-deleted analyzer keeps its data and reopen is instant;
//  - warn at ~50M cells; past the ceiling, LRU-evict measurements not
//    referenced by any *open* analyzer window. Evicted entries keep their
//    metadata (flagged) so the UI degrades to a "re-import file" placeholder,
//    never a silent OOM crash.

import { booleanEnvelope, lowerBound, minMaxEnvelope } from './decimate'
import type { ImportedMeasurement } from './csvImport'
import {
  deleteMeasurement,
  fetchChannelWindow,
  MeasurementApiError,
  type RemoteMeasurement,
} from './measurementApi'
import type {
  ChannelKind,
  ChannelWindow,
  FileSignature,
  MeasurementMeta,
  SignalRef,
} from './types'

export interface StoredChannel {
  name: string
  unit?: string
  kind: ChannelKind
  values: Float64Array | null
  min: number
  max: number
}

/** Server-side measurement (RemoteSource): windows fetched + cached on demand. */
interface RemoteState {
  serverId: string
  /** Display channel name → server (group, channel) address. */
  channels: Map<string, { group: number; name: string }>
  windows: Map<string, ChannelWindow>
  pending: Set<string>
  /**
   * Window keys whose fetch FAILED. Load-bearing: getWindow is called on
   * every render, so without failure memoization an error would loop
   * notify → re-render → refetch forever (and trip the API rate limiter).
   * A different zoom produces a different key and gets one fresh attempt.
   */
  failed: Set<string>
  /** Last window-fetch failure (typed server message), surfaced in the UI. */
  lastError?: string
}

interface Entry {
  meta: MeasurementMeta
  time: Float64Array
  channels: Map<string, StoredChannel>
  /** Analyzer ids referencing this measurement (refcount, §2.5a). */
  refs: Set<string>
  lastAccess: number
  cells: number
  evicted: boolean
  remote?: RemoteState
}

/** Flattened remote channel list (display names deduped across groups). */
export function flattenRemoteChannels(
  remote: RemoteMeasurement,
): { display: string; group: number; name: string; unit?: string; kind: ChannelKind }[] {
  const seen = new Set<string>()
  const out: { display: string; group: number; name: string; unit?: string; kind: ChannelKind }[] = []
  for (const group of remote.metadata.groups) {
    for (const ch of group.channels) {
      if (ch.timeMaster) continue
      const display = seen.has(ch.name) ? `${ch.name}#g${group.index}` : ch.name
      seen.add(ch.name)
      out.push({
        display,
        group: group.index,
        name: ch.name,
        unit: ch.unit ?? undefined,
        kind: ch.kind,
      })
    }
  }
  return out
}

/** Cap on cached remote windows per measurement (FIFO past this). */
const REMOTE_WINDOW_CACHE = 48

/** ~50M cells ≈ 400 MB of Float64Arrays — the §2.5a warning threshold. */
export const WARN_CELLS = 50_000_000
/** Deliberate headroom past the warning before LRU eviction kicks in. */
export const CEILING_CELLS = 62_500_000

export type StoreEventType = 'change' | 'warn'

class ChannelStore {
  private entries = new Map<string, Entry>()
  private listeners = new Set<(ev: StoreEventType) => void>()
  private openAnalyzers = new Set<string>()
  private versionCounter = 0
  private warned = false

  subscribe = (listener: (ev: StoreEventType) => void): (() => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Monotonic counter bumped on every mutation (for useSyncExternalStore). */
  version = (): number => this.versionCounter

  private notify(ev: StoreEventType) {
    this.versionCounter++
    for (const l of this.listeners) l(ev)
  }

  /**
   * Register a freshly imported measurement, referenced by one analyzer.
   * Passing `reuseMeasurementId` re-binds an existing id to new data — the
   * template-mode re-pick (§2.5b): strips keep their SignalRefs and simply
   * resolve again. Existing analyzer refs on that id are preserved.
   */
  register(
    measurement: ImportedMeasurement,
    analyzerId: string,
    reuseMeasurementId?: string,
  ): MeasurementMeta {
    const previous = reuseMeasurementId ? this.entries.get(reuseMeasurementId) : undefined
    const measurementId = reuseMeasurementId ?? crypto.randomUUID()
    const signature: FileSignature = {
      name: measurement.signatureName,
      size: measurement.size,
      headerHash: measurement.headerHash,
    }
    const channels = new Map<string, StoredChannel>()
    let cells = measurement.time.length
    for (const ch of measurement.channels) {
      channels.set(ch.name, {
        name: ch.name,
        unit: ch.unit,
        kind: ch.kind,
        values: ch.values,
        min: ch.min,
        max: ch.max,
      })
      cells += ch.values?.length ?? 0
    }
    const meta: MeasurementMeta = {
      measurementId,
      signature,
      channels: measurement.channels.map((ch) => ({
        name: ch.name,
        unit: ch.unit,
        kind: ch.kind,
        min: ch.min,
        max: ch.max,
      })),
      totalSamples: measurement.rowCount,
    }
    const refs = new Set(previous?.refs ?? [])
    refs.add(analyzerId)
    this.entries.set(measurementId, {
      meta,
      time: measurement.time,
      channels,
      refs,
      lastAccess: Date.now(),
      cells,
      evicted: false,
    })
    this.enforceCeiling()
    this.notify('change')
    return meta
  }

  /**
   * Register a server-side .mf4 measurement (RemoteSource, Phase 3): metadata
   * only — windows are fetched lazily as decimated envelopes and cached. The
   * same reuse-id path as register() serves the template-mode re-pick.
   */
  registerRemote(
    remote: RemoteMeasurement,
    analyzerId: string,
    reuseMeasurementId?: string,
  ): MeasurementMeta {
    const previous = reuseMeasurementId ? this.entries.get(reuseMeasurementId) : undefined
    const measurementId = reuseMeasurementId ?? crypto.randomUUID()
    const flattened = flattenRemoteChannels(remote)
    const meta: MeasurementMeta = {
      measurementId,
      // headerHash is a constant marker for remote files: the server id
      // changes per upload, so hashing it would make every re-pick advisory.
      signature: { name: remote.name, size: remote.size, headerHash: 'mf4' },
      channels: flattened.map((ch) => ({
        name: ch.display,
        unit: ch.unit,
        kind: ch.kind,
        min: Number.NaN,
        max: Number.NaN,
      })),
      totalSamples: remote.metadata.groups.reduce((acc, g) => Math.max(acc, g.records), 0),
    }
    const refs = new Set(previous?.refs ?? [])
    refs.add(analyzerId)
    this.entries.set(measurementId, {
      meta,
      time: new Float64Array(0),
      channels: new Map(),
      refs,
      lastAccess: Date.now(),
      cells: 0,
      evicted: false,
      remote: {
        serverId: remote.measurementId,
        channels: new Map(flattened.map((ch) => [ch.display, { group: ch.group, name: ch.name }])),
        windows: new Map(),
        pending: new Set(),
        failed: new Set(),
      },
    })
    this.notify('change')
    return meta
  }

  /** Add an analyzer reference (e.g. on project load in Phase 2). */
  retain(measurementId: string, analyzerId: string) {
    this.entries.get(measurementId)?.refs.add(analyzerId)
  }

  /**
   * Drop one analyzer's reference; the entry is freed when the last
   * referencing analyzer releases it. Called on analyzer deletion (§2.5a) and
   * on detaching a file from an analyzer.
   */
  release(measurementId: string, analyzerId: string) {
    const entry = this.entries.get(measurementId)
    if (!entry) return
    entry.refs.delete(analyzerId)
    if (entry.refs.size === 0) {
      if (entry.remote) deleteMeasurement(entry.remote.serverId)
      this.entries.delete(measurementId)
      this.notify('change')
    }
  }

  /** Drop everything (New Project / example load). */
  clear() {
    if (this.entries.size === 0) return
    this.entries.clear()
    this.warned = false
    this.notify('change')
  }

  /** Analyzer ids with an open dock window — protected from LRU eviction. */
  setOpenAnalyzers(ids: Iterable<string>) {
    this.openAnalyzers = new Set(ids)
  }

  getMeta(measurementId: string): MeasurementMeta | null {
    return this.entries.get(measurementId)?.meta ?? null
  }

  /** True when samples are resident (false: unknown id or evicted → placeholder). */
  isLoaded(measurementId: string): boolean {
    const entry = this.entries.get(measurementId)
    return entry !== undefined && !entry.evicted
  }

  /** Last remote window-fetch failure for a measurement, if any. */
  remoteError(measurementId: string): string | null {
    return this.entries.get(measurementId)?.remote?.lastError ?? null
  }

  /** Server address of a remote channel (for the calc endpoint); null = local. */
  remoteAddress(ref: SignalRef): { serverId: string; group: number; name: string } | null {
    const remote = this.entries.get(ref.measurementId)?.remote
    const target = remote?.channels.get(ref.channel)
    if (!remote || !target) return null
    return { serverId: remote.serverId, group: target.group, name: target.name }
  }

  totalCells(): number {
    let sum = 0
    for (const e of this.entries.values()) sum += e.cells
    return sum
  }

  /**
   * The shared window DTO (todo.md §2): raw samples when the range fits
   * `maxPoints`, a type-aware min/max envelope otherwise. `from`/`to` null →
   * the full recording. Returns null for unknown/evicted measurements and
   * string channels (unplottable, §2.5d).
   */
  getWindow(
    ref: SignalRef,
    from: number | null,
    to: number | null,
    maxPoints: number,
  ): ChannelWindow | null {
    const entry = this.entries.get(ref.measurementId)
    if (!entry || entry.evicted) return null
    if (entry.remote) return this.remoteWindow(entry, entry.remote, ref, from, to, maxPoints)
    const channel = entry.channels.get(ref.channel)
    if (!channel || channel.values === null) return null
    entry.lastAccess = Date.now()

    const t = entry.time
    const v = channel.values
    const n = t.length
    if (n === 0) return null
    // Include one sample beyond each edge so lines span the view boundary.
    const i0 = Math.max(0, (from === null ? 0 : lowerBound(t, from)) - 1)
    const i1 = Math.min(n - 1, to === null ? n - 1 : lowerBound(t, to))
    const count = i1 - i0 + 1

    if (count <= maxPoints) {
      return {
        t: t.slice(i0, i1 + 1),
        v: v.slice(i0, i1 + 1),
        decimated: false,
        totalSamples: n,
        unit: channel.unit,
        kind: channel.kind,
      }
    }
    const buckets = Math.max(1, Math.floor(maxPoints / 2))
    const envelope =
      channel.kind === 'boolean'
        ? booleanEnvelope(t, v, i0, i1, buckets)
        : minMaxEnvelope(t, v, i0, i1, buckets)
    return {
      t: envelope.t,
      min: envelope.min,
      max: envelope.max,
      decimated: true,
      totalSamples: n,
      unit: channel.unit,
      kind: channel.kind,
    }
  }

  /**
   * RemoteSource window: exact-key cache with deduped in-flight fetches.
   * Returns null while loading — the store notifies on arrival and the strip
   * re-renders. A 404 means the ephemeral server store lost the file (API
   * node restart): degrade to the evicted/"Locate file…" banner, the same
   * path as project load (§2.5b mid-session 404).
   */
  private remoteWindow(
    entry: Entry,
    remote: RemoteState,
    ref: SignalRef,
    from: number | null,
    to: number | null,
    maxPoints: number,
  ): ChannelWindow | null {
    const target = remote.channels.get(ref.channel)
    if (!target) return null
    entry.lastAccess = Date.now()
    const key = `${ref.channel}|${from}|${to}|${maxPoints}`
    const hit = remote.windows.get(key)
    if (hit) return hit
    if (remote.failed.has(key)) return null
    if (!remote.pending.has(key)) {
      remote.pending.add(key)
      fetchChannelWindow(remote.serverId, target.group, target.name, from, to, maxPoints)
        .then((w) => {
          remote.pending.delete(key)
          const win: ChannelWindow = {
            t: Float64Array.from(w.t),
            v: w.v ? Float64Array.from(w.v) : undefined,
            min: w.min ? Float64Array.from(w.min) : undefined,
            max: w.max ? Float64Array.from(w.max) : undefined,
            decimated: w.decimated,
            totalSamples: w.totalSamples,
            unit: w.unit ?? undefined,
            kind: w.kind === 'boolean' || w.kind === 'string' ? w.kind : 'analog',
          }
          remote.windows.set(key, win)
          remote.lastError = undefined
          while (remote.windows.size > REMOTE_WINDOW_CACHE) {
            const oldest = remote.windows.keys().next().value
            if (oldest === undefined) break
            remote.windows.delete(oldest)
          }
          this.notify('change')
        })
        .catch((err) => {
          remote.pending.delete(key)
          if (remote.failed.size > 64) remote.failed.clear()
          remote.failed.add(key)
          if (err instanceof MeasurementApiError && err.status === 404) {
            entry.evicted = true
          } else {
            // Typed server message (e.g. unsupported DZ compression) — shown
            // in the signal browser next to the file.
            remote.lastError = err instanceof Error ? err.message : String(err)
          }
          this.notify('change')
        })
    }
    return null
  }

  /**
   * Sample at/before time x (the conventional measurement-suite lazy `~` →
   * exact cursor pattern). Local
   * measurements answer exactly from the resident columns. Remote (.mf4)
   * measurements answer from the cached windows: a raw window containing x is
   * exact; if only a decimated envelope covers x, the bucket midpoint value is
   * returned with `approx: true` (the `~` approximate readout) AND a small raw window
   * around x is fetched in the background — the store notifies when it lands
   * and the next read is exact.
   */
  exactValueAt(ref: SignalRef, x: number): { t: number; v: number; approx?: boolean } | null {
    const entry = this.entries.get(ref.measurementId)
    if (!entry || entry.evicted) return null
    if (entry.remote) return this.remoteValueAt(entry, entry.remote, ref, x)
    const channel = entry.channels.get(ref.channel)
    if (!channel || channel.values === null) return null
    const idx = Math.max(0, Math.min(entry.time.length - 1, lowerBound(entry.time, x)))
    const i = idx > 0 && entry.time[idx] > x ? idx - 1 : idx
    return { t: entry.time[i], v: channel.values[i] }
  }

  /** Best cached window of a remote channel that covers x (prefer raw). */
  private static bestCachedWindow(
    remote: RemoteState,
    channel: string,
    x: number,
  ): ChannelWindow | null {
    let fallback: ChannelWindow | null = null
    for (const [key, win] of remote.windows) {
      if (!key.startsWith(`${channel}|`) || win.t.length === 0) continue
      // Envelope bucket midpoints are inset from the window edges — allow one
      // bucket spacing of slop so edge positions still resolve.
      const last = win.t.length - 1
      const slop = last > 0 ? (win.t[last] - win.t[0]) / last : 0
      if (x < win.t[0] - slop || x > win.t[last] + slop) continue
      if (!win.decimated && win.v) return win
      if (fallback === null || win.t[last] - win.t[0] < fallback.t[fallback.t.length - 1] - fallback.t[0]) {
        fallback = win
      }
    }
    return fallback
  }

  private remoteValueAt(
    entry: Entry,
    remote: RemoteState,
    ref: SignalRef,
    x: number,
  ): { t: number; v: number; approx?: boolean } | null {
    const win = ChannelStore.bestCachedWindow(remote, ref.channel, x)
    if (win === null) return null
    const t = win.t
    const idx = Math.max(0, Math.min(t.length - 1, lowerBound(t, x)))
    const i = idx > 0 && t[idx] > x ? idx - 1 : idx
    if (!win.decimated && win.v) return { t: t[i], v: win.v[i] }
    // Approximate from the envelope bucket, and refine: fetch a raw window one
    // bucket wide around x. remoteWindow dedupes/memoizes and notifies.
    const last = t.length - 1
    const spacing = last > 0 ? (t[last] - t[0]) / last : 1
    this.remoteWindow(entry, remote, ref, x - spacing, x + spacing, 2048)
    const lo = win.min?.[i]
    const hi = win.max?.[i]
    if (lo === undefined || hi === undefined) return null
    return { t: t[i], v: (lo + hi) / 2, approx: true }
  }

  /** Time of the sample NEAREST to x (for the sample-snap cursor mode). */
  nearestTime(ref: SignalRef, x: number): number | null {
    const entry = this.entries.get(ref.measurementId)
    if (!entry || entry.evicted) return null
    let t: Float64Array | undefined
    if (entry.remote) {
      // Remote: snap against the best cached window (raw = exact samples,
      // decimated = bucket midpoints — refined by the exactValueAt fetch).
      t = ChannelStore.bestCachedWindow(entry.remote, ref.channel, x)?.t
    } else {
      t = entry.time
    }
    if (t === undefined || t.length === 0) return null
    const lb = Math.min(t.length - 1, lowerBound(t, x))
    const before = Math.max(0, lb - (t[lb] > x ? 1 : 0))
    const after = Math.min(t.length - 1, before + 1)
    return Math.abs(t[after] - x) < Math.abs(t[before] - x) ? t[after] : t[before]
  }

  /**
   * Full-resolution slice over [from, to] (null = open end) as SUBARRAY VIEWS
   * onto the stored columns — zero-copy, read-only by convention. Feeds the
   * Statistics/Table instruments and the CSV exporter.
   */
  getRawRange(
    ref: SignalRef,
    from: number | null,
    to: number | null,
  ): { t: Float64Array; v: Float64Array; unit?: string } | null {
    const entry = this.entries.get(ref.measurementId)
    const channel = entry?.channels.get(ref.channel)
    if (!entry || entry.evicted || !channel || channel.values === null) return null
    const t = entry.time
    const i0 = from === null ? 0 : lowerBound(t, from)
    let i1 = to === null ? t.length : lowerBound(t, to)
    if (i1 < t.length && t[i1] <= (to ?? Infinity)) i1++
    entry.lastAccess = Date.now()
    return { t: t.subarray(i0, i1), v: channel.values.subarray(i0, i1), unit: channel.unit }
  }

  private enforceCeiling() {
    let total = this.totalCells()
    if (total > WARN_CELLS && !this.warned) {
      this.warned = true
      this.notify('warn')
    }
    if (total <= CEILING_CELLS) return
    // LRU-evict measurements not referenced by any OPEN analyzer window.
    const evictable = [...this.entries.values()]
      .filter((e) => !e.evicted && ![...e.refs].some((id) => this.openAnalyzers.has(id)))
      .sort((a, b) => a.lastAccess - b.lastAccess)
    for (const entry of evictable) {
      if (total <= CEILING_CELLS) break
      total -= entry.cells
      entry.cells = 0
      entry.time = new Float64Array(0)
      entry.channels.clear()
      entry.evicted = true
    }
  }
}

/** The app-wide singleton (bulk data only — deliberately outside React). */
export const channelStore = new ChannelStore()
