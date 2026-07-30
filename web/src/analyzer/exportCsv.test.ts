// CSV export contract tests (todo.md Phase 2): selected signals × visible
// range on a merged raster with step-hold fill; output re-imports cleanly.

import { describe, expect, it } from 'vitest'
import { buildCsv } from './exportCsv'
import { ingestCsvText } from './csvImport'

const sigA = {
  name: 'a',
  unit: 'm/s',
  t: new Float64Array([0, 1, 2, 3]),
  v: new Float64Array([10, 11, 12, 13]),
}
const sigB = {
  name: 'b',
  t: new Float64Array([0.5, 1.5, 2.5]),
  v: new Float64Array([100, 200, 300]),
}

describe('buildCsv', () => {
  it('merges rasters and step-hold fills the off-raster cells', () => {
    const lines = buildCsv([sigA, sigB], null, null).trim().split('\n')
    expect(lines[0]).toBe('time,a,b')
    expect(lines[1]).toBe('[s],[m/s],')
    // merged raster: 0, 0.5, 1, 1.5, 2, 2.5, 3
    expect(lines.length).toBe(2 + 7)
    expect(lines[2]).toBe('0,10,') // before b's first sample → blank
    expect(lines[3]).toBe('0.5,10,100') // a step-held
    expect(lines[5]).toBe('1.5,11,200')
    expect(lines[8]).toBe('3,13,300') // b step-held past its end
  })

  it('restricts rows to the visible window', () => {
    const lines = buildCsv([sigA, sigB], 1, 2).trim().split('\n')
    // raster within [1, 2]: 1, 1.5, 2
    expect(lines.length).toBe(2 + 3)
    expect(lines[2].startsWith('1,')).toBe(true)
    expect(lines[4].startsWith('2,')).toBe(true)
  })

  it('leaves NaN gaps as empty cells and quotes reserved characters', () => {
    const gap = { name: 'g,1', t: new Float64Array([0, 1]), v: new Float64Array([NaN, 5]) }
    const lines = buildCsv([gap], null, null).trim().split('\n')
    expect(lines[0]).toBe('time,"g,1"')
    expect(lines[2]).toBe('0,')
    expect(lines[3]).toBe('1,5')
  })

  it('round-trips through the analyzer CSV importer', () => {
    const text = buildCsv([sigA, sigB], null, null)
    const outcome = ingestCsvText(text)
    expect(outcome.status).toBe('ok')
    if (outcome.status !== 'ok') return
    expect(outcome.result.rowCount).toBe(7)
    expect(outcome.result.channels.map((c) => c.name)).toEqual(['a', 'b'])
    expect(outcome.result.channels[0].unit).toBe('m/s')
  })
})
