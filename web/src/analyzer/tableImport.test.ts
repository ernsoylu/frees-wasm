import { describe, expect, it } from 'vitest'
import { importableTables, tableToMeasurement } from './tableImport'
import type { ParamTableSpec, TableSpec } from '../tables'

function paramTable(over: Partial<ParamTableSpec>): ParamTableSpec {
  return {
    id: 't1',
    kind: 'parametric',
    name: 'Table 1',
    vars: [],
    rows: [],
    results: [],
    stats: null,
    checkResult: null,
    checkMessage: '',
    ...over,
  }
}

const row = (values: Record<string, string>) => ({ id: crypto.randomUUID(), values })

describe('tableToMeasurement — ODE origin', () => {
  const ode = paramTable({
    name: 'ode1',
    origin: 'ode',
    vars: ['Time', 'ch.ev$hg', 'note$'],
    columnUnits: { Time: 's', 'ch.ev$hg': 'K' },
    rows: [
      row({ Time: '0', 'ch.ev$hg': '300', note$: 'a' }),
      row({ Time: '2', 'ch.ev$hg': '340', note$: 'b' }), // out of order on purpose
      row({ Time: '1', 'ch.ev$hg': '320', note$: 'c' }),
      row({ Time: '', 'ch.ev$hg': '999', note$: 'd' }), // no time → dropped
    ],
  })

  it('uses the first column as the time base, sorted ascending', () => {
    const m = tableToMeasurement(ode)!
    expect(Array.from(m.time)).toEqual([0, 1, 2])
    expect(m.rowCount).toBe(3)
  })

  it('imports value columns with units, demangles $, and drops all-NaN columns', () => {
    const m = tableToMeasurement(ode)!
    // 'ch.ev$hg' → dotted display form; the all-string 'note$' column is dropped.
    expect(m.channels.map((c) => c.name)).toEqual(['ch.ev.hg'])
    expect(m.channels[0].unit).toBe('K')
    expect(Array.from(m.channels[0].values!)).toEqual([300, 320, 340])
    expect(m.channels[0].min).toBe(300)
    expect(m.channels[0].max).toBe(340)
    expect(m.signatureName).toBe('⌗ ode1')
  })
})

describe('tableToMeasurement — parametric', () => {
  it('uses run numbers 1..N as the time base and prefers solved results', () => {
    const t = paramTable({
      vars: ['x', 'y'],
      rows: [row({ x: '1', y: '' }), row({ x: '2', y: '' })],
      results: [
        { success: true, values: { y: 10.123456789 }, error: null },
        { success: true, values: { y: 20 }, error: null },
      ],
    })
    const m = tableToMeasurement(t)!
    expect(Array.from(m.time)).toEqual([1, 2])
    expect(Array.from(m.channels.find((c) => c.name === 'y')!.values!)).toEqual([
      10.123456789, 20,
    ])
    expect(Array.from(m.channels.find((c) => c.name === 'x')!.values!)).toEqual([1, 2])
  })

  it('returns null when nothing is plottable', () => {
    expect(tableToMeasurement(paramTable({ vars: ['a'], rows: [row({})] }))).toBeNull()
  })
})

describe('importableTables', () => {
  it('keeps only parametric-kind tables with rows', () => {
    const tables = [
      paramTable({ id: 'p', vars: ['x'], rows: [row({ x: '1' })] }),
      paramTable({ id: 'empty', vars: ['x'], rows: [] }),
      { kind: 'function' } as unknown as TableSpec,
    ]
    expect(importableTables(tables).map((t) => t.id)).toEqual(['p'])
  })
})
