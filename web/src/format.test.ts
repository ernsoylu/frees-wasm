import { describe, it, expect } from 'vitest'
import { formatValue, withStableKeys } from './format'

describe('formatValue', () => {
  it('renders an em dash for missing or non-numeric values', () => {
    expect(formatValue(null)).toBe('—')
    expect(formatValue(undefined)).toBe('—')
    expect(formatValue(Number.NaN)).toBe('—')
  })

  it('passes infinities through as their string form', () => {
    expect(formatValue(Infinity)).toBe('Infinity')
    expect(formatValue(-Infinity)).toBe('-Infinity')
  })

  it('honours an explicit decimal-place override', () => {
    expect(formatValue(3.14159, 2)).toBe('3.14')
    expect(formatValue(3.7, 0)).toBe('4')
    // The override wins even for values that would otherwise use exponential form.
    expect(formatValue(0.0000123, 4)).toBe('0.0000')
  })

  it('returns a bare zero', () => {
    expect(formatValue(0)).toBe('0')
  })

  it('uses exponential notation for very large or very small magnitudes', () => {
    expect(formatValue(1e7)).toBe('1.000000e+7')
    expect(formatValue(1e-5)).toBe('1.000000e-5')
    expect(formatValue(-2.5e9)).toBe('-2.500000e+9')
  })

  it('uses adaptive precision for everyday magnitudes', () => {
    expect(formatValue(3.14159)).toBe('3.14159')
    expect(formatValue(1.23456)).toBe('1.23456')
    expect(formatValue(42)).toBe('42')
    // Trailing precision noise is trimmed via Number(...).toString().
    expect(formatValue(0.1 + 0.2)).toBe('0.3')
  })
})

describe('withStableKeys', () => {
  it('returns one entry per item preserving order and value', () => {
    const result = withStableKeys(['a', 'b', 'c'])
    expect(result.map((r) => r.value)).toEqual(['a', 'b', 'c'])
  })

  it('disambiguates duplicate values with an occurrence suffix', () => {
    const result = withStableKeys(['x', 'y', 'x', 'x'])
    expect(result.map((r) => r.key)).toEqual(['x#1', 'y#1', 'x#2', 'x#3'])
  })

  it('produces unique keys even when every item is identical', () => {
    const keys = withStableKeys(['dup', 'dup', 'dup']).map((r) => r.key)
    expect(new Set(keys).size).toBe(3)
  })

  it('handles an empty list', () => {
    expect(withStableKeys([])).toEqual([])
  })
})
