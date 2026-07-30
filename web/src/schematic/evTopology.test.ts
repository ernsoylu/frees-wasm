import { describe, expect, it } from 'vitest'
import { EV_TOPOLOGY } from './evTopology.fixture'
import { layoutSchematic, routeEdge, type SchematicNode } from './layout'
import { buildLineStyles, lineId } from './palette'

/**
 * The shipped EV thermal-management example as the acceptance case for a
 * *readable* schematic. Legibility is checked as the properties that make a
 * circuit drawing readable rather than a graph drawing:
 *
 *   - the two fluid loops are told apart and framed apart, even though the
 *     bond-graph domain calls both of them `fluid`;
 *   - each loop reads in flow order, source to sink;
 *   - the cross-loop couplings (radiator bank, chiller wall) stay visible as
 *     couplings;
 *   - boundary conditions are identifiable as boundaries;
 *   - nothing overlaps and nothing is orphaned.
 */
describe('EV thermal-management example', () => {
  const types = new Map<string, { label: string; type?: string }>([
    ['pumpin', { label: 'PUMPIN', type: 'LiquidSource' }],
    ['pump', { label: 'PUMP', type: 'LiquidPump' }],
    ['obat', { label: 'OBAT', type: 'LiquidOrifice' }],
    ['omot', { label: 'OMOT', type: 'LiquidOrifice' }],
    ['bcp', { label: 'BCP', type: 'LiquidWallHX' }],
    ['chlc', { label: 'CHLC', type: 'LiquidWallHX' }],
    ['mcp', { label: 'MCP', type: 'LiquidWallHX' }],
    ['mix', { label: 'MIX', type: 'LiquidMixer' }],
    ['rad1', { label: 'RAD1', type: 'LiquidWallHX' }],
    ['rad2', { label: 'RAD2', type: 'LiquidWallHX' }],
    ['rad3', { label: 'RAD3', type: 'LiquidWallHX' }],
    ['or1', { label: 'OR1', type: 'LiquidOrifice' }],
    ['or2', { label: 'OR2', type: 'LiquidOrifice' }],
    ['or3', { label: 'OR3', type: 'LiquidOrifice' }],
    ['amb', { label: 'AMB', type: 'ThermalSource' }],
    ['pumpout', { label: 'PUMPOUT', type: 'LiquidSink' }],
    ['batt', { label: 'BATT', type: 'MassGen' }],
    ['motor', { label: 'MOTOR', type: 'MassGen' }],
    ['cabin', { label: 'CABIN', type: 'MassGen' }],
    ['feed', { label: 'FEED', type: 'TwoPhasePressureSource' }],
    ['chlr', { label: 'CHLR', type: 'TwoPhaseEvaporatorUA' }],
    ['cabe', { label: 'CABE', type: 'TwoPhaseEvaporatorUA' }],
    ['suc', { label: 'SUC', type: 'TwoPhaseMixer' }],
    ['cmp', { label: 'CMP', type: 'TwoPhaseCompressor' }],
    ['cond', { label: 'COND', type: 'TwoPhaseCondenserFloat' }],
    ['liq', { label: 'LIQ', type: 'TwoPhaseSink' }],
  ])

  const layout = layoutSchematic(EV_TOPOLOGY, types)
  const node = (id: string) => layout.nodes.find((n) => n.id === id) as SchematicNode
  const groupOf = (id: string) => node(id)?.group

  const overlaps = (a: SchematicNode, b: SchematicNode) =>
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h

  it('places every instance in the network', () => {
    const instances = new Set<string>()
    for (const c of EV_TOPOLOGY) {
      for (const e of c.endpoints) {
        instances.add(e.split('.')[0])
      }
    }
    const placed = layout.nodes.filter((n) => n.kind === 'instance').map((n) => n.id)
    expect(new Set(placed)).toEqual(instances)
    expect(instances.size).toBe(26)
  })

  it('draws the coolant loop and the refrigerant loop as separate circuits', () => {
    // Both are `domain: fluid` — this is the whole point.
    const coolant = ['pumpin', 'pump', 'obat', 'bcp', 'chlc', 'mix', 'rad1', 'or3', 'pumpout']
    const refrigerant = ['feed', 'chlr', 'cabe', 'suc', 'cmp', 'cond', 'liq']

    expect(new Set(coolant.map(groupOf)).size).toBe(1)
    expect(new Set(refrigerant.map(groupOf)).size).toBe(1)
    expect(groupOf('pump')).not.toBe(groupOf('cmp'))
  })

  it('gives each working fluid its own line colour', () => {
    const styles = buildLineStyles(layout.lines)
    const eg50 = styles.get(lineId({ domain: 'fluid', connector: 'liquid', fluid: 'eg50' }))
    const r1234yf = styles.get(lineId({ domain: 'fluid', connector: 'twophase', fluid: 'r1234yf' }))
    const heat = styles.get('heat')

    expect(eg50?.color).toBeDefined()
    expect(r1234yf?.color).toBeDefined()
    expect(eg50?.color).not.toEqual(r1234yf?.color)
    expect(heat?.color).not.toEqual(eg50?.color)
  })

  it('frames the circuits as labelled bands that do not overlap', () => {
    expect(layout.groups.length).toBeGreaterThanOrEqual(3) // coolant, refrigerant, thermal
    for (let i = 1; i < layout.groups.length; i++) {
      expect(layout.groups[i].y).toBeGreaterThanOrEqual(
        layout.groups[i - 1].y + layout.groups[i - 1].h,
      )
    }
  })

  it('reads each loop in flow order, boundary to boundary', () => {
    expect(node('pumpin').x).toBeLessThan(node('pump').x)
    expect(node('pump').x).toBeLessThan(node('bcp').x)
    expect(node('bcp').x).toBeLessThan(node('mix').x)
    expect(node('mix').x).toBeLessThan(node('pumpout').x)

    expect(node('feed').x).toBeLessThan(node('chlr').x)
    expect(node('chlr').x).toBeLessThan(node('suc').x)
    expect(node('suc').x).toBeLessThan(node('cmp').x)
    expect(node('cmp').x).toBeLessThan(node('liq').x)
  })

  it('marks the boundary conditions as terminals', () => {
    // A circuit reading needs to see where it starts and ends. Every one of
    // these is a model boundary, not a piece of equipment.
    for (const id of ['pumpin', 'pumpout', 'feed', 'liq', 'amb']) {
      expect(node(id).terminal, `${id} should read as a boundary`).toBe(true)
    }
    for (const id of ['pump', 'cmp', 'bcp', 'batt']) {
      expect(node(id).terminal, `${id} is equipment, not a boundary`).toBe(false)
    }
  })

  it('gives each block the symbol of what it is', () => {
    expect(node('pump').shape).toBe('pump')
    expect(node('cmp').shape).toBe('compressor')
    expect(node('or1').shape).toBe('valve')
    expect(node('bcp').shape).toBe('exchanger')
    expect(node('chlr').shape).toBe('exchanger')
    expect(node('batt').shape).toBe('store')
    expect(node('mix').shape).toBe('junction')
    // A fixed-temperature ambient is a boundary that FEEDS the network, so it
    // takes the source terminal rather than the ground ladder.
    expect(node('amb').shape).toBe('source')
  })

  it('keeps the chiller bridge — the link that makes this one system — visible', () => {
    // `connect(CHLR.wall, CHLC.wall)` ties the refrigerant loop to the coolant
    // loop. It must stay a heat coupling between two DIFFERENT circuits, and
    // attach on the vertical face so it cannot read as more pipework.
    const bridge = layout.edges.find(
      (e) => (e.from === 'chlr' && e.to === 'chlc') || (e.from === 'chlc' && e.to === 'chlr'),
    )
    expect(bridge?.domain).toBe('heat')
    expect(groupOf('chlr')).not.toBe(groupOf('chlc'))

    const side = (id: string) => node(id).ports.find((p) => p.port === 'wall')?.side
    expect(side('chlr')).toBe('top')
    expect(side('chlc')).toBe('top')
  })

  it('renders the three shared nodes as junctions, not cliques', () => {
    const junctions = layout.nodes.filter((n) => n.kind === 'junction')
    expect(junctions).toHaveLength(3)
    // The radiator bank shares one ambient node across three walls: four edges
    // out of one junction, not six pairwise edges.
    const radiator = junctions.find(
      (j) => layout.edges.filter((e) => e.from === j.id || e.to === j.id).length === 4,
    )
    expect(radiator).toBeDefined()
  })

  it('draws without overlapping any two nodes', () => {
    for (let i = 0; i < layout.nodes.length; i++) {
      for (let j = i + 1; j < layout.nodes.length; j++) {
        expect(
          overlaps(layout.nodes[i], layout.nodes[j]),
          `${layout.nodes[i].id} overlaps ${layout.nodes[j].id}`,
        ).toBe(false)
      }
    }
  })

  it('routes every edge with a finite path', () => {
    expect(layout.edges.length).toBeGreaterThan(EV_TOPOLOGY.length)
    for (const e of layout.edges) {
      const a = layout.nodes.find((n) => n.id === e.from) as SchematicNode
      const b = layout.nodes.find((n) => n.id === e.to) as SchematicNode
      const path = routeEdge(a, b, e.fromPort, e.toPort)
      expect(path).toMatch(/^M [\d.-]+ [\d.-]+/)
      expect(path).not.toMatch(/NaN|Infinity/)
    }
  })

  it('carries the variable prefix of every endpoint, so blocks can show results', () => {
    for (const e of layout.edges) {
      if (e.fromPort) {
        expect(e.fromStream, `${e.from}.${e.fromPort} has no variable prefix`).toBeTruthy()
      }
      if (e.toPort) {
        expect(e.toStream, `${e.to}.${e.toPort} has no variable prefix`).toBeTruthy()
      }
    }
  })
})
