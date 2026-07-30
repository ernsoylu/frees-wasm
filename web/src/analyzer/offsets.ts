// Offset-aware ChannelStore accessors (todo.md Phase 5a): a per-file time
// offset shifts a measurement along the shared time axis (displayed time =
// recorded time + offset) so multi-file recordings can be synchronized.
// Offsets live on the AnalyzerSpec (per window), so they are applied here —
// outside the shared store — by shifting the query into recorded time and
// the result back into display time.

import { channelStore } from './channelStore'
import type { AnalyzerSpec, ChannelWindow, SignalRef } from './types'

/** measurementId → offset seconds for one analyzer. */
export function offsetsOf(spec: AnalyzerSpec): Map<string, number> {
  const map = new Map<string, number>()
  for (const f of spec.files) {
    if (f.offset !== undefined && f.offset !== 0) map.set(f.measurementId, f.offset)
  }
  return map
}

function shifted(t: Float64Array, offset: number): Float64Array {
  if (offset === 0) return t
  const out = new Float64Array(t.length)
  for (let i = 0; i < t.length; i++) out[i] = t[i] + offset
  return out
}

/** getWindow in display time. */
export function offsetWindow(
  ref: SignalRef,
  offset: number,
  from: number | null,
  to: number | null,
  maxPoints: number,
): ChannelWindow | null {
  const win = channelStore.getWindow(
    ref,
    from === null ? null : from - offset,
    to === null ? null : to - offset,
    maxPoints,
  )
  if (!win || offset === 0) return win
  return { ...win, t: shifted(win.t, offset) }
}

/** getRawRange in display time (t is a shifted copy when offset ≠ 0). */
export function offsetRawRange(
  ref: SignalRef,
  offset: number,
  from: number | null,
  to: number | null,
): { t: Float64Array; v: Float64Array; unit?: string } | null {
  const raw = channelStore.getRawRange(
    ref,
    from === null ? null : from - offset,
    to === null ? null : to - offset,
  )
  if (!raw || offset === 0) return raw
  return { ...raw, t: shifted(raw.t, offset) }
}

/** Exact sample at/before display time x (`approx` = remote envelope estimate). */
export function offsetExactValueAt(
  ref: SignalRef,
  offset: number,
  x: number,
): { t: number; v: number; approx?: boolean } | null {
  const hit = channelStore.exactValueAt(ref, x - offset)
  return hit ? { t: hit.t + offset, v: hit.v, approx: hit.approx } : null
}

/** Nearest sample time in display time (cursor snap). */
export function offsetNearestTime(ref: SignalRef, offset: number, x: number): number | null {
  const t = channelStore.nearestTime(ref, x - offset)
  return t === null ? null : t + offset
}
