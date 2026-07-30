import { describe, expect, it } from 'vitest'
import { findPin, pinnableParameters, sliderOverrideEquation, sliderRange, sliderStep, type PinnedSlider } from './sliders'

describe('sliderRange', () => {
  it('uses declared bounds when they bracket the value', () => {
    expect(sliderRange(5, 0, 10)).toEqual({ min: 0, max: 10 })
  })

  it('falls back to +/-50% when bounds are missing or infinite', () => {
    expect(sliderRange(10)).toEqual({ min: 5, max: 15 })
    expect(sliderRange(10, Number.NEGATIVE_INFINITY, Number.POSITIVE_INFINITY)).toEqual({ min: 5, max: 15 })
    expect(sliderRange(10, null, 20)).toEqual({ min: 5, max: 15 })
  })

  it('ignores bounds that do not bracket the value', () => {
    // A stale bound from the Variable Information window would otherwise put
    // the handle outside its own track.
    expect(sliderRange(50, 0, 10)).toEqual({ min: 25, max: 75 })
    expect(sliderRange(5, 10, 0)).toEqual({ min: 2.5, max: 7.5 })
  })

  it('gives a zero value a movable range', () => {
    expect(sliderRange(0)).toEqual({ min: -1, max: 1 })
    expect(sliderRange(Number.NaN)).toEqual({ min: -1, max: 1 })
  })

  it('keeps a negative value ascending', () => {
    const r = sliderRange(-10)
    expect(r.min).toBeLessThan(r.max)
    expect(r).toEqual({ min: -15, max: -5 })
  })
})

describe('sliderStep', () => {
  it('picks a human-sized step across the range', () => {
    expect(sliderStep(0, 10)).toBe(0.05)
    expect(sliderStep(0, 1)).toBe(0.005)
    expect(sliderStep(0, 200000)).toBe(1000)
  })

  it('never returns zero or a non-finite step', () => {
    expect(sliderStep(5, 5)).toBe(1)
    expect(sliderStep(0, Number.POSITIVE_INFINITY)).toBe(1)
  })

  it('divides the range into roughly 200 steps', () => {
    const min = 0
    const max = 37.5
    const steps = (max - min) / sliderStep(min, max)
    expect(steps).toBeGreaterThan(100)
    expect(steps).toBeLessThan(400)
  })
})

describe('sliderOverrideEquation', () => {
  const pin = (over: Partial<PinnedSlider> = {}): PinnedSlider => ({
    name: 'eta', value: 0.75, units: '', min: 0, max: 1, ...over,
  })

  it('writes the REPL override form', () => {
    expect(sliderOverrideEquation(pin())).toBe('eta = 0.75')
  })

  it('carries the unit so the solver reads the value in display units', () => {
    expect(sliderOverrideEquation(pin({ name: 'P', value: 250000, units: 'Pa' })))
      .toBe('P = 250000 [Pa]')
  })

  it('omits the placeholder unit', () => {
    expect(sliderOverrideEquation(pin({ units: '-' }))).toBe('eta = 0.75')
  })
})

describe('findPin', () => {
  const pins: PinnedSlider[] = [
    { name: 'Eta', value: 0.7, units: '', min: 0, max: 1 },
    { name: 'P_in', value: 2e5, units: 'Pa', min: 1e5, max: 3e5 },
  ]

  it('matches case-insensitively, like the solver', () => {
    expect(findPin(pins, 'eta')?.name).toBe('Eta')
    expect(findPin(pins, 'P_IN')?.name).toBe('P_in')
    expect(findPin(pins, 'missing')).toBeUndefined()
  })
})

describe('pinnableParameters', () => {
  it('accepts literal assignments, with or without units', () => {
    const names = pinnableParameters('eta = 0.7\nP = 250 [kPa]\nn = -3\nk = 1.5e-4\n')
    expect([...names].sort()).toEqual(['eta', 'k', 'n', 'p'])
  })

  it('refuses computed variables — a slider must never delete an equation', () => {
    // `c = a*b` defines c. An override on c would REPLACE that line, silently
    // removing the physics, so c is not offered as a slider.
    const names = pinnableParameters('a = 2\nb = 3\nc = a * b\nd = sqrt(a)\n')
    expect(names.has('c')).toBe(false)
    expect(names.has('d')).toBe(false)
    expect(names.has('a')).toBe(true)
  })

  it('ignores comments and inline free text', () => {
    const names = pinnableParameters('// eta = 0.9\nUA = 1200 { the sized value }\n')
    expect(names.has('eta')).toBe(false)
    expect(names.has('ua')).toBe(true)
  })

  it('reads each semicolon-separated segment', () => {
    const names = pinnableParameters('a = 1; b = 2; c = a + b')
    expect([...names].sort()).toEqual(['a', 'b'])
  })

  it('is empty for an empty document', () => {
    expect(pinnableParameters('').size).toBe(0)
  })
})
