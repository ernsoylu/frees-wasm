import { describe, expect, it } from 'vitest'
import { badgeFor, formatCompact, indexVariables, readoutFor } from './readouts'
import { layoutSchematic, type Connection } from './layout'
import type { VariableResult } from '../api'

const NETWORK: Connection[] = [
  {
    domain: 'fluid',
    connector: 'twophase',
    fluid: 'r1234yf',
    endpoints: ['feed.out', 'chlr.in'],
    streams: ['feed.out', 'chlr.in'],
  },
  {
    domain: 'fluid',
    connector: 'twophase',
    fluid: 'r1234yf',
    endpoints: ['chlr.out', 'cmp.in'],
    streams: ['chlr.out', 'cmp.in'],
  },
  { domain: 'heat', endpoints: ['chlr.wall', 'batt.port'], streams: ['chlr.wall', 'batt.port'] },
]

const layout = layoutSchematic(
  NETWORK,
  new Map([
    ['chlr', { label: 'CHLR', type: 'TwoPhaseEvaporatorUA' }],
    ['batt', { label: 'BATT', type: 'MassGen' }],
  ]),
)
const chlr = layout.nodes.find((n) => n.id === 'chlr')!

const v = (name: string, value: number, units = ''): VariableResult => ({ name, value, units })

const VARIABLES: VariableResult[] = [
  v('CHLR.in.P', 350000),
  v('CHLR.in.mdot', 0.047965),
  v('CHLR.out.P', 328300),
  v('CHLR.wall.T', 287.38, 'K'),
  v('CHLR.wall.Qdot', 6444.3, 'W'),
  v('CHLR.Q', 6444.3, 'W'),
  v('CHLR.Tevap', 276.21, 'K'),
  v('CMP.W', 2385.2, 'W'),
  v('UA_chl_r', 576.79, 'W/K'),
]

const COMPONENTS = [
  {
    name: 'CHLR',
    type: 'TwoPhaseEvaporatorUA',
    params: [
      { name: 'fluid$', ref: 'R1234yf', value: null, units: null },
      { name: 'UA', ref: 'UA_chl_r', value: 576.79, units: 'W/K' },
      { name: 'SH', ref: '5', value: 5, units: null },
    ],
  },
]

describe('indexVariables', () => {
  it('indexes case-insensitively', () => {
    const values = indexVariables(VARIABLES)
    expect(values.get('chlr.in.p')?.value).toBe(350000)
  })

  it('falls back to the check units where a solved variable has none', () => {
    // Port members are the solver's own unknowns, so the solve grounds no unit
    // for them; the expander's domain-derived units arrive on the check.
    const values = indexVariables(VARIABLES, { 'chlr.in.p': 'Pa', 'chlr.in.mdot': 'kg/s' })
    expect(values.get('chlr.in.p')?.units).toBe('Pa')
    expect(values.get('chlr.in.mdot')?.units).toBe('kg/s')
  })

  it('never overwrites a unit the solve did supply', () => {
    const values = indexVariables(VARIABLES, { 'chlr.q': 'kJ' })
    expect(values.get('chlr.q')?.units).toBe('W')
  })
})

describe('readoutFor', () => {
  const values = indexVariables(VARIABLES, { 'chlr.in.p': 'Pa', 'chlr.in.mdot': 'kg/s' })
  const readout = readoutFor(chlr, layout.edges, values, COMPONENTS)

  it('reports the state at each wired port, by domain', () => {
    const inlet = readout.ports.find((p) => p.port === 'in')
    expect(inlet?.readings.map((r) => r.label)).toEqual(['P', 'ṁ'])
    expect(inlet?.readings[0]).toMatchObject({ value: 350000, units: 'Pa' })

    // The wall is a HEAT port, so it reports T/Q̇ rather than P/ṁ/h.
    const wall = readout.ports.find((p) => p.port === 'wall')
    expect(wall?.readings.map((r) => r.label)).toEqual(['T', 'Q̇'])
  })

  it('separates the block outputs from its port members', () => {
    // `CHLR.Q` is an output; `CHLR.in.P` is a port member and must not appear
    // as one, or every block would list its ports twice.
    expect(readout.outputs.map((o) => o.label).sort()).toEqual(['Q', 'Tevap'])
  })

  it('never claims another block results', () => {
    expect(readout.outputs.some((o) => o.value === 2385.2)).toBe(false)
  })

  it('shows the parameters the block was built with, and where they came from', () => {
    // The heat-transfer calculation lives outside the component and is injected
    // as UA — without this it is invisible on the drawing.
    const ua = readout.params.find((p) => p.name === 'UA')
    expect(ua?.text).toContain('576.79')
    expect(ua?.text).toContain('W/K')
    expect(ua?.text).toContain('UA_chl_r')
  })

  it('reports a literal parameter without a redundant source', () => {
    expect(readout.params.find((p) => p.name === 'SH')?.text).toBe('5')
  })

  it('keeps a string parameter as written', () => {
    expect(readout.params.find((p) => p.name === 'fluid$')?.text).toBe('R1234yf')
  })

  it('is empty but well-formed for an unsolved network', () => {
    const empty = readoutFor(chlr, layout.edges, new Map(), undefined)
    expect(empty).toEqual({ ports: [], outputs: [], params: [] })
  })
})

describe('badgeFor', () => {
  const values = indexVariables(VARIABLES, {})

  it('puts the duty on a heat exchanger', () => {
    const readout = readoutFor(chlr, layout.edges, values, COMPONENTS)
    expect(badgeFor(chlr, readout)).toMatchObject({ label: 'Q', value: 6444.3 })
  })

  it('falls back to the flow through a block with no named output', () => {
    const feed = layout.nodes.find((n) => n.id === 'feed')!
    const flow = indexVariables([v('FEED.out.mdot', 0.066244, 'kg/s')], {})
    const readout = readoutFor(feed, layout.edges, flow, undefined)
    expect(badgeFor(feed, readout)).toMatchObject({ label: 'ṁ', value: 0.066244 })
  })

  it('returns nothing when there is nothing to say', () => {
    expect(badgeFor(chlr, { ports: [], outputs: [], params: [] })).toBeNull()
  })
})

describe('formatCompact', () => {
  it('keeps a hover card readable', () => {
    expect(formatCompact(350000)).toBe('350000')
    expect(formatCompact(0.047965123)).toBe('0.047965')
    expect(formatCompact(0)).toBe('0')
    expect(formatCompact(1.2e7)).toBe('1.200e+7')
    expect(formatCompact(1.2e-5)).toBe('1.200e-5')
  })
})
