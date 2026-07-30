import { describe, it, expect } from 'vitest'
import { parseMapData, buildTableBlock, isValidTableName } from './mapTable'

describe('parseMapData', () => {
  it('detects a 1-D map from two columns', () => {
    const m = parseMapData('0.00 250\n0.05 180\n0.10 60')
    expect(m.kind).toBe('1d')
    expect(m.family).toEqual([])
    expect(m.rows).toEqual([[0, 250], [0.05, 180], [0.1, 60]])
  })

  it('accepts comma-separated values', () => {
    const m = parseMapData('0,250\n0.05,180')
    expect(m.rows).toEqual([[0, 250], [0.05, 180]])
  })

  it('detects a 2-D map with a non-numeric header row supplying family values', () => {
    const m = parseMapData('rpm 0.25 0.5 1.0\n1000 320 300 290\n3000 280 260 250')
    expect(m.kind).toBe('2d')
    expect(m.family).toEqual([0.25, 0.5, 1.0])
    expect(m.rows).toEqual([[1000, 320, 300, 290], [3000, 280, 260, 250]])
  })

  it('defaults 2-D family values to 1..N when no header', () => {
    const m = parseMapData('1000 320 300\n3000 280 260')
    expect(m.kind).toBe('2d')
    expect(m.family).toEqual([1, 2])
  })

  it('throws on ragged rows', () => {
    expect(() => parseMapData('1 2 3\n4 5')).toThrow(/columns/)
  })

  it('throws on empty input', () => {
    expect(() => parseMapData('   \n  ')).toThrow(/No data/)
  })
})

describe('buildTableBlock', () => {
  it('emits a 1-D TABLE with arg and output units', () => {
    const block = buildTableBlock({
      name: 'fanCurve', argName: 'Q', argUnit: 'm^3/s', outUnit: 'Pa',
      rows: [[0, 250], [0.05, 180], [0.1, 60]],
    })
    expect(block).toBe(
      'TABLE fanCurve(Q [m^3/s]) [Pa]\n  0  250\n  0.05  180\n  0.1  60\nEND',
    )
  })

  it('emits a 2-D TABLE with the family axis', () => {
    const block = buildTableBlock({
      name: 'bsfc', argName: 'rpm', outUnit: 'g/kWh',
      famName: 'load', family: [0.25, 0.5, 1.0],
      rows: [[1000, 320, 300, 290], [3000, 280, 260, 250]],
    })
    expect(block).toBe(
      'TABLE bsfc(rpm : load = 0.25, 0.5, 1) [g/kWh]\n  1000  320  300  290\n  3000  280  260  250\nEND',
    )
  })

  it('omits unit brackets when units are absent', () => {
    const block = buildTableBlock({ name: 't', argName: 'x', rows: [[1, 2]] })
    expect(block).toBe('TABLE t(x)\n  1  2\nEND')
  })
})

describe('isValidTableName', () => {
  it('accepts identifiers, rejects bad ones', () => {
    expect(isValidTableName('fanCurve')).toBe(true)
    expect(isValidTableName('1bad')).toBe(false)
    expect(isValidTableName('has space')).toBe(false)
  })
})
