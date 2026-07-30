import { describe, expect, it } from 'vitest'
import { moveSignal, reorderStrip } from './stripOps'
import type { AnalyzerStrip } from './types'

const sig = (measurementId: string, channel: string, color = '#4dabf7') => ({
  measurementId,
  channel,
  color,
})

const strips = (): AnalyzerStrip[] => [
  { id: 's1', signals: [sig('m1', 'rpm'), sig('m1', 'speed')] },
  { id: 's2', signals: [sig('m2', 'temp')] },
  { id: 's3', signals: [] },
]

describe('moveSignal', () => {
  it('moves a signal to the target strip, keeping its color', () => {
    const next = moveSignal(strips(), 's1', 's3', 'm1', 'rpm')
    expect(next[0].signals.map((s) => s.channel)).toEqual(['speed'])
    expect(next[2].signals).toEqual([sig('m1', 'rpm')])
  })

  it('no-ops for same strip, unknown signal, or duplicate at target', () => {
    const base = strips()
    expect(moveSignal(base, 's1', 's1', 'm1', 'rpm')).toBe(base)
    expect(moveSignal(base, 's1', 's2', 'm9', 'nope')).toBe(base)
    const dup: AnalyzerStrip[] = [
      { id: 'a', signals: [sig('m1', 'rpm')] },
      { id: 'b', signals: [sig('m1', 'rpm')] },
    ]
    expect(moveSignal(dup, 'a', 'b', 'm1', 'rpm')).toBe(dup)
  })
})

describe('reorderStrip', () => {
  it('drags a strip onto a later target (takes its slot)', () => {
    expect(reorderStrip(strips(), 's1', 's3').map((s) => s.id)).toEqual(['s2', 's3', 's1'])
  })

  it('drags a strip onto an earlier target', () => {
    expect(reorderStrip(strips(), 's3', 's1').map((s) => s.id)).toEqual(['s3', 's1', 's2'])
  })

  it('no-ops for same or unknown ids', () => {
    const base = strips()
    expect(reorderStrip(base, 's1', 's1')).toBe(base)
    expect(reorderStrip(base, 'sX', 's1')).toBe(base)
    expect(reorderStrip(base, 's1', 'sX')).toBe(base)
  })
})
