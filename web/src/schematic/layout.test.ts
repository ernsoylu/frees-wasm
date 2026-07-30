import { describe, expect, it } from 'vitest'
import { layoutSchematic, routeEdge, type Connection, type SchematicNode } from './layout'

const conn = (domain: string, ...endpoints: string[]): Connection => ({ domain, endpoints })
const fluidConn = (connector: string, fluid: string, ...endpoints: string[]): Connection => ({
  domain: 'fluid',
  connector,
  fluid,
  endpoints,
})

/** True when two placed nodes' boxes intersect. */
function overlaps(a: SchematicNode, b: SchematicNode): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

describe('layoutSchematic', () => {
  it('places both endpoints of a simple pair and connects them', () => {
    const out = layoutSchematic([conn('electrical', 'b.p', 'l.a')])
    expect(out.nodes.map((n) => n.id).sort()).toEqual(['b', 'l'])
    expect(out.edges).toHaveLength(1)
    expect(out.edges[0]).toMatchObject({ from: 'b', to: 'l', fromPort: 'p', toPort: 'a', domain: 'electrical' })
    expect(out.width).toBeGreaterThan(0)
    expect(out.height).toBeGreaterThan(0)
  })

  it('turns a 3-way node into a junction rather than a clique', () => {
    const out = layoutSchematic([conn('fluid', 'a.out', 'b.in', 'c.in')])
    const junctions = out.nodes.filter((n) => n.kind === 'junction')
    expect(junctions).toHaveLength(1)
    // Three edges radiate from the junction — not three pairwise edges.
    expect(out.edges).toHaveLength(3)
    const touching = out.edges.filter((e) => e.from === junctions[0].id || e.to === junctions[0].id)
    expect(touching).toHaveLength(3)
  })

  it('orients an edge from the outlet to the inlet', () => {
    // Port names are the only direction an acausal network carries, and the
    // reading "source on the left" depends on honouring them whichever order
    // the document wrote the endpoints in.
    const forward = layoutSchematic([conn('fluid', 'a.out', 'b.in')]).edges[0]
    const backward = layoutSchematic([conn('fluid', 'b.in', 'a.out')]).edges[0]
    expect(forward).toMatchObject({ from: 'a', to: 'b' })
    expect(backward).toMatchObject({ from: 'a', to: 'b' })
  })

  it('reads a chain left to right in flow order', () => {
    const out = layoutSchematic([
      conn('fluid', 'src.out', 'pmp.in'),
      conn('fluid', 'pmp.out', 'hx.in'),
      conn('fluid', 'hx.out', 'snk.in'),
    ])
    const x = (id: string) => out.nodes.find((n) => n.id === id)?.x ?? 0
    expect(x('src')).toBeLessThan(x('pmp'))
    expect(x('pmp')).toBeLessThan(x('hx'))
    expect(x('hx')).toBeLessThan(x('snk'))
  })

  it('never overlaps two placed nodes', () => {
    const out = layoutSchematic([
      conn('fluid', 'pump.out', 'hx.hot_in'),
      conn('fluid', 'hx.hot_out', 'valve.in'),
      conn('fluid', 'valve.out', 'tank.in'),
      conn('fluid', 'tank.out', 'pump.in'),
      conn('heat', 'hx.cold_out', 'rad.in'),
      conn('heat', 'rad.out', 'amb.p'),
      conn('signal', 'ctl.sig', 'valve.cmd', 'pump.cmd'),
    ])
    for (let i = 0; i < out.nodes.length; i++) {
      for (let j = i + 1; j < out.nodes.length; j++) {
        expect(
          overlaps(out.nodes[i], out.nodes[j]),
          `${out.nodes[i].id} overlaps ${out.nodes[j].id}`,
        ).toBe(false)
      }
    }
  })

  it('is deterministic for the same document', () => {
    const input = [
      conn('fluid', 'a.out', 'b.in'),
      conn('fluid', 'b.out', 'c.in'),
      conn('heat', 'c.h', 'd.h', 'e.h'),
    ]
    expect(JSON.stringify(layoutSchematic(input))).toEqual(JSON.stringify(layoutSchematic(input)))
  })

  it('handles a closed loop without hanging or collapsing', () => {
    // Every node feeds another, so no node has in-degree zero — the layering
    // has to survive a genuine cycle.
    const out = layoutSchematic([
      conn('fluid', 'a.out', 'b.in'),
      conn('fluid', 'b.out', 'c.in'),
      conn('fluid', 'c.out', 'a.in'),
    ])
    expect(out.nodes.filter((n) => n.kind === 'instance')).toHaveLength(3)
    expect(new Set(out.nodes.map((n) => n.x)).size).toBeGreaterThan(1)
  })

  it('takes display labels and types from the solve when available', () => {
    const out = layoutSchematic(
      [conn('fluid', 'pmp.out', 'tnk.in')],
      new Map([
        ['pmp', { label: 'Pmp', type: 'LiquidPump' }],
        ['tnk', { label: 'Tnk', type: 'LiquidTank' }],
      ]),
    )
    const pump = out.nodes.find((n) => n.id === 'pmp')
    expect(pump?.label).toBe('Pmp')
    expect(pump?.type).toBe('LiquidPump')
    expect(pump?.shape).toBe('pump')
    // Without a solve the lowercase wire name is still shown, never blank.
    const bare = layoutSchematic([conn('fluid', 'pmp.out', 'tnk.in')])
    expect(bare.nodes.find((n) => n.id === 'pmp')?.label).toBe('pmp')
    expect(bare.nodes.find((n) => n.id === 'pmp')?.type).toBeUndefined()
  })

  it('drops a self-connection but keeps its instance', () => {
    const out = layoutSchematic([conn('fluid', 'a.out', 'a.in')])
    expect(out.nodes.map((n) => n.id)).toEqual(['a'])
    expect(out.edges).toHaveLength(0)
  })

  it('handles an empty topology', () => {
    const out = layoutSchematic([])
    expect(out.nodes).toEqual([])
    expect(out.edges).toEqual([])
    expect(out.groups).toEqual([])
    expect(out.width).toBeGreaterThan(0)
  })
})

describe('circuit grouping', () => {
  it('separates two fluid circuits that share a bond-graph domain', () => {
    // The exact case a domain-coloured drawing cannot express: coolant and
    // refrigerant are both `domain: fluid`.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pumpin.out', 'pump.in'),
      fluidConn('liquid', 'eg50', 'pump.out', 'hx.in'),
      fluidConn('twophase', 'r1234yf', 'feed.out', 'evap.in'),
      fluidConn('twophase', 'r1234yf', 'evap.out', 'cmp.in'),
    ])
    const groupOf = (id: string) => out.nodes.find((n) => n.id === id)?.group
    expect(groupOf('pump')).toBe(groupOf('pumpin'))
    expect(groupOf('evap')).toBe(groupOf('cmp'))
    expect(groupOf('pump')).not.toBe(groupOf('evap'))
    expect(out.groups).toHaveLength(2)
  })

  it('bands the circuits apart vertically', () => {
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'a.out', 'b.in'),
      fluidConn('twophase', 'r1234yf', 'x.out', 'y.in'),
    ])
    const [first, second] = out.groups
    expect(second.y).toBeGreaterThanOrEqual(first.y + first.h)
  })

  it('keeps a heat-exchanger wall in its fluid circuit, not the thermal one', () => {
    // A wall HX is a piece of the coolant line that also touches heat. Drawing
    // it in the thermal band would tear the coolant line in half.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pump.out', 'hx.in'),
      fluidConn('liquid', 'eg50', 'hx.out', 'snk.in'),
      conn('heat', 'hx.wall', 'mass.port'),
    ])
    const groupOf = (id: string) => out.nodes.find((n) => n.id === id)?.group
    expect(groupOf('hx')).toBe(groupOf('pump'))
    expect(groupOf('mass')).not.toBe(groupOf('hx'))
  })

  it('names each circuit by what flows in it', () => {
    const out = layoutSchematic(
      [fluidConn('liquid', 'eg50', 'a.out', 'b.in'), conn('heat', 'b.wall', 'm.port')],
      new Map(),
      [],
      new Map(),
      (key) => (key.fluid ? `${key.fluid}/${key.connector}` : key.domain),
    )
    expect(out.groups.map((g) => g.label).sort()).toEqual(['eg50/liquid', 'heat'])
  })

  it('keeps a coupling domain in ONE band instead of one band per pendant', () => {
    // Every thermal mass hangs off exactly one wall, and the wall belongs to
    // its fluid loop — so splitting heat by connectivity would make a separate
    // full-width band for each mass.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pump.out', 'hx1.in'),
      fluidConn('liquid', 'eg50', 'hx1.out', 'hx2.in'),
      fluidConn('liquid', 'eg50', 'hx2.out', 'snk.in'),
      conn('heat', 'hx1.wall', 'm1.port'),
      conn('heat', 'hx2.wall', 'm2.port'),
    ])
    expect(out.groups.filter((g) => g.lineId === 'heat')).toHaveLength(1)
    expect(out.nodes.find((n) => n.id === 'm1')?.group).toBe(out.nodes.find((n) => n.id === 'm2')?.group)
  })

  it('aligns each pendant coupling under the block it couples to', () => {
    // The whole value of a coupling band: a short vertical drop from the cold
    // plate to its battery, not a wire across the drawing.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pump.out', 'hx1.in'),
      fluidConn('liquid', 'eg50', 'hx1.out', 'hx2.in'),
      fluidConn('liquid', 'eg50', 'hx2.out', 'snk.in'),
      conn('heat', 'hx1.wall', 'm1.port'),
      conn('heat', 'hx2.wall', 'm2.port'),
    ])
    const centre = (id: string) => {
      const n = out.nodes.find((x) => x.id === id)
      return n ? n.x + n.w / 2 : NaN
    }
    // Each mass tracks its own exchanger, and they keep the same left-to-right
    // order — m1 under hx1, m2 under hx2.
    expect(centre('m1')).toBeLessThan(centre('m2'))
    expect(Math.abs(centre('m1') - centre('hx1'))).toBeLessThan(60)
    expect(Math.abs(centre('m2') - centre('hx2'))).toBeLessThan(60)
    // A row, not a column.
    expect(out.nodes.find((n) => n.id === 'm1')?.y).toBe(out.nodes.find((n) => n.id === 'm2')?.y)
  })

  it('still layers a coupling network that has real internal structure', () => {
    // A wall stack is not a set of pendants; its structure is the point.
    const out = layoutSchematic([
      conn('heat', 'src.port', 'cond.in'),
      conn('heat', 'cond.out', 'mass.a'),
      conn('heat', 'mass.b', 'conv.in'),
      conn('heat', 'conv.out', 'amb.port'),
    ])
    const ys = new Set(out.nodes.map((n) => n.y))
    const xs = new Set(out.nodes.map((n) => n.x))
    expect(xs.size).toBeGreaterThan(1)
    expect(ys.size).toBeGreaterThanOrEqual(1)
  })

  it('slots a coupling band next to the circuit it couples to most', () => {
    // Two loops bridged by heat: the thermal band belongs BETWEEN them, or its
    // couplings have to cross a whole unrelated circuit.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pump.out', 'hx1.in'),
      fluidConn('liquid', 'eg50', 'hx1.out', 'hx2.in'),
      fluidConn('twophase', 'r1234yf', 'feed.out', 'evap.in'),
      conn('heat', 'hx1.wall', 'm1.port'),
      conn('heat', 'hx2.wall', 'm2.port'),
      conn('heat', 'evap.wall', 'm3.port'),
    ])
    const order = out.groups.map((g) => g.lineId)
    expect(order[0]).toContain('liquid')
    expect(order[1]).toBe('heat')
    expect(order[2]).toContain('twophase')
  })

  it('aligns a pendant to a circuit stacked BELOW its own band', () => {
    // The thermal band sits between the two loops, so a cabin mass coupled to
    // the refrigerant evaporator is positioned against a band drawn after it.
    // Horizontal placement therefore cannot follow the stacking order.
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'pump.out', 'hx1.in'),
      fluidConn('liquid', 'eg50', 'hx1.out', 'hx2.in'),
      fluidConn('liquid', 'eg50', 'hx2.out', 'snk.in'),
      fluidConn('twophase', 'r1234yf', 'feed.out', 'xv.in'),
      fluidConn('twophase', 'r1234yf', 'xv.out', 'evap.in'),
      fluidConn('twophase', 'r1234yf', 'evap.out', 'cmp.in'),
      fluidConn('twophase', 'r1234yf', 'cmp.out', 'liq.in'),
      conn('heat', 'hx1.wall', 'm1.port'),
      conn('heat', 'evap.wall', 'cab.port'),
    ])
    const centre = (id: string) => {
      const n = out.nodes.find((x) => x.id === id)
      return n ? n.x + n.w / 2 : NaN
    }
    // `cab` couples to `evap`, which is in the band drawn after the heat band.
    expect(out.groups.findIndex((g) => g.lineId === 'heat')).toBeLessThan(
      out.groups.findIndex((g) => g.lineId.includes('twophase')),
    )
    expect(Math.abs(centre('cab') - centre('evap'))).toBeLessThan(60)
  })

  it('reports every distinct line for the legend', () => {
    const out = layoutSchematic([
      fluidConn('liquid', 'eg50', 'a.out', 'b.in'),
      fluidConn('twophase', 'r1234yf', 'x.out', 'y.in'),
      conn('heat', 'b.wall', 'm.port'),
    ])
    expect(out.lines).toHaveLength(3)
    expect(out.lines.filter((l) => l.domain === 'fluid').map((l) => l.fluid).sort()).toEqual([
      'eg50',
      'r1234yf',
    ])
  })
})

describe('port anchors', () => {
  const ports = new Map([['p', ['in', 'out', 'wall']]])

  it('puts flow inlets left, outlets right and heat couplings on top', () => {
    // A cross-domain coupling on the flow axis reads as more pipework; on the
    // top face it reads as what it is.
    const out = layoutSchematic(
      [conn('fluid', 'p.out', 'q.in'), conn('fluid', 'z.out', 'p.in'), conn('heat', 'p.wall', 'm.port')],
      new Map(),
      [],
      ports,
    )
    const node = out.nodes.find((n) => n.id === 'p')
    const side = (port: string) => node?.ports.find((a) => a.port === port)?.side
    expect(side('in')).toBe('left')
    expect(side('out')).toBe('right')
    expect(side('wall')).toBe('top')
  })

  it('places declared ports that are not wired yet', () => {
    const out = layoutSchematic([], new Map(), ['p'], ports)
    const node = out.nodes.find((n) => n.id === 'p')
    expect(node?.ports.map((a) => a.port).sort()).toEqual(['in', 'out', 'wall'])
  })

  it('spaces several ports along a face without collisions', () => {
    const out = layoutSchematic([], new Map(), ['p'], new Map([['p', ['a_in', 'b_in', 'c_in']]]))
    const left = out.nodes.find((n) => n.id === 'p')?.ports.filter((a) => a.side === 'left') ?? []
    expect(left).toHaveLength(3)
    expect(new Set(left.map((a) => a.dy)).size).toBe(3)
  })

  it('gives junctions no ports', () => {
    const out = layoutSchematic([conn('fluid', 'a.out', 'b.in', 'c.in')])
    expect(out.nodes.find((n) => n.kind === 'junction')?.ports).toEqual([])
  })
})

describe('routeEdge', () => {
  const out = layoutSchematic(
    [conn('fluid', 'a.out', 'b.in'), conn('heat', 'a.wall', 'm.port')],
    new Map(),
    [],
    new Map([['a', ['in', 'out', 'wall']]]),
  )
  const node = (id: string) => out.nodes.find((n) => n.id === id) as SchematicNode

  it('produces a finite orthogonal path between two placed nodes', () => {
    const path = routeEdge(node('a'), node('b'), 'out', 'in')
    expect(path).toMatch(/^M [\d.-]+ [\d.-]+/)
    expect(path).not.toMatch(/NaN|Infinity/)
  })

  it('leaves a node through the face its port sits on', () => {
    // The heat coupling exits upward, so its first move is vertical.
    const heat = routeEdge(node('a'), node('m'), 'wall', 'port')
    const [, , y0, , , y1] = heat.split(/[ ,]+/)
    expect(Number(y1)).toBeLessThan(Number(y0))
  })

  it('survives a dragged node (recomputes from live positions)', () => {
    const moved = { ...node('b'), x: node('b').x - 400, y: node('b').y + 260 }
    const path = routeEdge(node('a'), moved, 'out', 'in')
    expect(path).not.toMatch(/NaN/)
    expect(path).not.toEqual(routeEdge(node('a'), node('b'), 'out', 'in'))
  })
})

describe('unwired instances', () => {
  it('places a declared component that has no connections yet', () => {
    const out = layoutSchematic([conn('fluid', 'src.out', 'pmp.in')], new Map(), ['src', 'pmp', 'snk'])
    expect(out.nodes.map((n) => n.id).sort()).toEqual(['pmp', 'snk', 'src'])
    const snk = out.nodes.find((n) => n.id === 'snk')
    expect(snk).toBeDefined()
    expect(out.edges.some((e) => e.from === 'snk' || e.to === 'snk')).toBe(false)
  })

  it('does not duplicate an instance that is already wired', () => {
    const out = layoutSchematic([conn('fluid', 'a.out', 'b.in')], new Map(), ['a', 'b'])
    expect(out.nodes.filter((n) => n.kind === 'instance')).toHaveLength(2)
  })
})
