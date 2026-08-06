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
//    release() from onDeleteAnalyzer, so a
//    closed-but-not-deleted analyzer keeps its data and reopen is instant;
//  - warn at ~50M cells; past the ceiling, LRU-evict measurements not
//    referenced by any *open* analyzer window. Evicted entries keep their
//    metadata (flagged) so the UI degrades to a "re-import file" placeholder,
//    never a silent OOM crash.

import { booleanEnvelope, lowerBound, minMaxEnvelope } from './decimate'
import type { ImportedMeasurement } from './csvImport'
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

interface Entry {
  meta: MeasurementMeta
  time: Float64Array
  channels: Map<string, StoredChannel>
  /** Analyzer ids referencing this measurement (refcount, §2.5a). */
  refs: Set<string>
  lastAccess: number
  cells: number
  evicted: boolean
}

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
   * Sample at/before time x (the conventional measurement-suite exact cursor
   * pattern), answered exactly from the resident columns.
   */
  exactValueAt(ref: SignalRef, x: number): { t: number; v: number } | null {
    const entry = this.entries.get(ref.measurementId)
    if (!entry || entry.evicted) return null
    const channel = entry.channels.get(ref.channel)
    if (!channel || channel.values === null) return null
    const idx = Math.max(0, Math.min(entry.time.length - 1, lowerBound(entry.time, x)))
    const i = idx > 0 && entry.time[idx] > x ? idx - 1 : idx
    return { t: entry.time[i], v: channel.values[i] }
  }

  /** Time of the sample NEAREST to x (for the sample-snap cursor mode). */
  nearestTime(ref: SignalRef, x: number): number | null {
    const entry = this.entries.get(ref.measurementId)
    if (!entry || entry.evicted) return null
    const t = entry.time
    if (t.length === 0) return null
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
