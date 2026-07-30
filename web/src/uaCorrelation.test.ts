import { describe, it, expect } from 'vitest'
import { buildUaCorrelation, isUaParam, UaSide } from './uaCorrelation'

const boilingRef: UaSide = {
  phase: 'boiling', fluid: 'R1234yf', P: '350000', state: '0.5',
  mdot: '0.05', Dh: '0.006', Aflow: '8e-5', area: '3', L: '0.30',
}
const singleCool: UaSide = {
  phase: 'single', fluid: 'EG50', P: '200000', state: '290',
  mdot: '0.23', Dh: '0.008', Aflow: '1.5e-4', area: '12', L: '0.40',
}

describe('buildUaCorrelation', () => {
  const { uaVar, lines } = buildUaCorrelation('CHLR', boilingRef, singleCool, '1e-4')

  it('names the UA variable after the instance', () => {
    expect(uaVar).toBe('UA_CHLR')
  })

  it('emits the boiling-side film via htc_evap (quality arg)', () => {
    expect(lines[0]).toBe("hA_CHLR = htc_evap('R1234yf', 350000, 0.5, 0.05, 0.006, 8e-5)")
  })

  it('emits the single-side film via htc_1phase (temperature arg)', () => {
    expect(lines[2]).toBe("hB_CHLR = htc_1phase('EG50', 200000, 290, 0.23, 0.008, 1.5e-4)")
  })

  it('builds area as count * hx_aconv when L is given', () => {
    expect(lines[1]).toBe('AA_CHLR = 3 * hx_aconv(8e-5, 0.30, 0.006)')
  })

  it('combines both sides with ua_hx and the wall resistance', () => {
    expect(lines[4]).toBe('UA_CHLR = ua_hx(hA_CHLR, AA_CHLR, hB_CHLR, AB_CHLR, 1e-4)')
  })
})

describe('area without L', () => {
  it('treats area as a direct value', () => {
    const { lines } = buildUaCorrelation('X', { ...boilingRef, area: '0.25', L: '' }, singleCool, '0')
    expect(lines[1]).toBe('AA_X = 0.25')
  })
  it('defaults an empty Rwall to 0', () => {
    const { lines } = buildUaCorrelation('X', boilingRef, singleCool, '')
    expect(lines[4]).toContain(', 0)')
  })
})

describe('isUaParam', () => {
  it('matches UA-named or W/K params', () => {
    expect(isUaParam('UA', '')).toBe(true)
    expect(isUaParam('UA_cool', 'W/K')).toBe(true)
    expect(isUaParam('Q', 'W')).toBe(false)
    expect(isUaParam('eta', '')).toBe(false)
  })
})
