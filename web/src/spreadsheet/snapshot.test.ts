import { describe, expect, it } from 'vitest'
import { buildSnapshotSheet, SNAPSHOT_MAX_ROWS, SNAPSHOT_WARN_ROWS } from './snapshot'

const input = (rows: (string | number | null)[][]) => ({
  title: 'ODE run',
  columns: [{ name: 'time', unit: 's' }, { name: 'T', unit: 'K' }],
  rows,
})

describe('buildSnapshotSheet (contract e)', () => {
  it('builds a timestamped sheet with bold unit headers and numeric cells', () => {
    const out = buildSnapshotSheet(input([[0, 300], [1, 310.5]]))
    if (!out.ok) throw new Error('expected ok')
    expect(out.name).toMatch(/^ODE run \(\d{4}-\d{2}-\d{2} \d{2}:\d{2}\)$/)
    const at = (r: number, c: number) => out.sheet.celldata.find((cd) => cd.r === r && cd.c === c)?.v
    expect(at(0, 0)?.v).toBe('time [s]')
    expect(out.sheet.styles?.A1).toContain('bold')
    expect(at(1, 1)?.v).toBe(300)
    expect(at(2, 1)?.v).toBe(310.5)
  })

  it('skips blank cells and keeps non-numeric strings as strings', () => {
    const out = buildSnapshotSheet(input([['1 ✗', null]]))
    if (!out.ok) throw new Error('expected ok')
    expect(out.sheet.celldata.filter((cd) => cd.r === 1)).toHaveLength(1)
    expect(out.sheet.celldata.find((cd) => cd.r === 1 && cd.c === 0)?.v.v).toBe('1 ✗')
  })

  it('asks for confirmation above the warn threshold and honours cancel', () => {
    const rows = Array.from({ length: SNAPSHOT_WARN_ROWS + 1 }, (_, i) => [i, i])
    let asked = false
    const cancelled = buildSnapshotSheet(input(rows), () => {
      asked = true
      return false
    })
    expect(asked).toBe(true)
    expect(cancelled.ok).toBe(false)
    if (!cancelled.ok) expect(cancelled.reason).toBe('cancelled')
    const accepted = buildSnapshotSheet(input(rows), () => true)
    expect(accepted.ok).toBe(true)
  })

  it('hard-caps and routes to CSV above the limit', () => {
    const rows = Array.from({ length: SNAPSHOT_MAX_ROWS + 1 }, (_, i) => [i, i])
    const out = buildSnapshotSheet(input(rows), () => true)
    expect(out.ok).toBe(false)
    if (!out.ok) {
      expect(out.reason).toBe('too-big')
      expect(out.message).toContain('CSV')
    }
  })
})
