// Pure strip-list operations for the oscilloscope (oscilloscope-tool parity: signals can be
// dragged between strips, strips reordered by dragging). Kept out of the
// component so the reducer-style spec mutations are unit-testable.

import type { AnalyzerStrip } from './types'

/**
 * Move one signal from strip `fromId` to strip `toId` (append at the end,
 * keeping its color). No-ops when the move is degenerate: same strip, unknown
 * source signal, or the target already shows that exact signal.
 */
export function moveSignal(
  strips: AnalyzerStrip[],
  fromId: string,
  toId: string,
  measurementId: string,
  channel: string,
): AnalyzerStrip[] {
  if (fromId === toId) return strips
  const from = strips.find((s) => s.id === fromId)
  const to = strips.find((s) => s.id === toId)
  const signal = from?.signals.find(
    (sig) => sig.measurementId === measurementId && sig.channel === channel,
  )
  if (!from || !to || !signal) return strips
  if (to.signals.some((sig) => sig.measurementId === measurementId && sig.channel === channel)) {
    return strips
  }
  return strips.map((s) => {
    if (s.id === fromId) {
      return {
        ...s,
        signals: s.signals.filter(
          (sig) => !(sig.measurementId === measurementId && sig.channel === channel),
        ),
      }
    }
    if (s.id === toId) return { ...s, signals: [...s.signals, signal] }
    return s
  })
}

/**
 * Reorder strips by dragging: remove `dragId` and re-insert it at `targetId`'s
 * position (before the target when dragging up, after when dragging down —
 * i.e. the dragged strip takes the target's slot). Unknown ids no-op.
 */
export function reorderStrip(
  strips: AnalyzerStrip[],
  dragId: string,
  targetId: string,
): AnalyzerStrip[] {
  if (dragId === targetId) return strips
  const fromIdx = strips.findIndex((s) => s.id === dragId)
  const toIdx = strips.findIndex((s) => s.id === targetId)
  if (fromIdx < 0 || toIdx < 0) return strips
  const next = [...strips]
  const [dragged] = next.splice(fromIdx, 1)
  next.splice(toIdx, 0, dragged)
  return next
}
