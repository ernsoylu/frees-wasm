import { describe, expect, it } from 'vitest'
import { buildLineStyles, lineId, lineLabel, type LineKey } from './palette'

const fluid = (connector: string, f: string): LineKey => ({ domain: 'fluid', connector, fluid: f })

describe('lineId', () => {
  it('separates fluid lines by connector and working fluid', () => {
    // The identity a domain-only payload cannot express.
    expect(lineId(fluid('liquid', 'eg50'))).not.toEqual(lineId(fluid('twophase', 'r1234yf')))
    expect(lineId(fluid('liquid', 'eg50'))).not.toEqual(lineId(fluid('liquid', 'water')))
  })

  it('collapses a non-fluid domain to the domain itself', () => {
    expect(lineId({ domain: 'heat', connector: 'liquid', fluid: 'eg50' })).toBe('heat')
  })

  it('is case-insensitive, like the language', () => {
    expect(lineId(fluid('LIQUID', 'EG50'))).toBe(lineId(fluid('liquid', 'eg50')))
  })
})

describe('buildLineStyles', () => {
  it('gives every fluid family its own colour', () => {
    // These are the connector types the expander can report; each has to be
    // visually distinct or two circuits read as one.
    const keys = [
      fluid('liquid', 'eg50'),
      fluid('twophase', 'r1234yf'),
      fluid('gas', 'air'),
      fluid('oil', 'iso vg46'),
      fluid('moistair', 'airh2o'),
      fluid('fluid', 'water'),
      { domain: 'heat' },
      { domain: 'electrical' },
      { domain: 'mechanical' },
      { domain: 'signal' },
    ]
    const styles = buildLineStyles(keys)
    const colours = keys.map((k) => styles.get(lineId(k))?.color)
    expect(colours.every(Boolean)).toBe(true)
    expect(new Set(colours).size).toBe(colours.length)
  })

  it('keeps rotational and translational mechanics in one family', () => {
    // They are the same physics in two coordinate systems; a reader should not
    // have to learn two colours for it.
    const styles = buildLineStyles([{ domain: 'mechanical' }, { domain: 'translational' }])
    expect(styles.get('mechanical')?.color).toEqual(styles.get('translational')?.color)
  })

  it('separates two fluids that share one connector', () => {
    const styles = buildLineStyles([fluid('liquid', 'eg50'), fluid('liquid', 'water')])
    expect(styles.get(lineId(fluid('liquid', 'eg50')))?.color).not.toEqual(
      styles.get(lineId(fluid('liquid', 'water')))?.color,
    )
  })

  it('assigns colours by sorted fluid name, not discovery order', () => {
    // Otherwise the same document recolours itself when an edge is re-ordered.
    const a = buildLineStyles([fluid('liquid', 'eg50'), fluid('liquid', 'water')])
    const b = buildLineStyles([fluid('liquid', 'water'), fluid('liquid', 'eg50')])
    expect(a.get(lineId(fluid('liquid', 'eg50')))).toEqual(b.get(lineId(fluid('liquid', 'eg50'))))
  })

  it('draws a causal signal thin and dashed — it is not a physical flow', () => {
    const style = buildLineStyles([{ domain: 'signal' }]).get('signal')
    expect(style?.dash).toBeTruthy()
    expect(style?.width).toBeLessThan(2)
    expect(buildLineStyles([{ domain: 'heat' }]).get('heat')?.dash).toBeUndefined()
  })
})

describe('lineLabel', () => {
  it('leads with the working fluid, qualified by the connector', () => {
    expect(lineLabel(fluid('twophase', 'r1234yf'))).toBe('r1234yf · two-phase')
    expect(lineLabel(fluid('gas', 'air'))).toBe('air · pneumatic')
    expect(lineLabel(fluid('oil', 'oil'))).toBe('oil · hydraulic')
    expect(lineLabel(fluid('moistair', 'airh2o'))).toBe('airh2o · humid air')
  })

  it('uses the spelling the document wrote', () => {
    const spelling = new Map([['r1234yf', 'R1234yf']])
    expect(lineLabel(fluid('twophase', 'r1234yf'), spelling)).toBe('R1234yf · two-phase')
  })

  it('names a fluid-less line by its connector, and a coupling by its domain', () => {
    expect(lineLabel({ domain: 'fluid', connector: 'moistair' })).toBe('humid air')
    expect(lineLabel({ domain: 'heat' })).toBe('heat')
  })
})
