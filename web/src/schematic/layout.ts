// Geometry for the rendered component schematic. Pure functions over the
// backend's connection topology — no DOM, no React, so it is unit-testable.
//
// This lays out a CIRCUIT, not a graph. Three things follow from that:
//
//  1. Circuits are separated. A coolant loop and a refrigerant loop share the
//     bond-graph domain `fluid`, so a domain-only drawing braids them into one
//     ladder. Here the connection's connector type and working fluid partition
//     the network into circuits, and each circuit is laid out and framed on its
//     own band — which is how the same system reads on paper.
//
//  2. Flow sets the direction. The network is acausal, but the PORT NAMES are
//     not: `connect(A.out, B.in)` says A feeds B. Layering is longest-path over
//     that inferred flow direction, so a circuit reads source → … → sink left to
//     right instead of breadth-first out of whichever node happened to have the
//     most edges.
//
//  3. Couplings are visible as couplings. A heat, signal or mechanical node
//     between two fluid circuits is what makes the model one system (the
//     chiller wall tying refrigerant to coolant, say). Those attach to the top
//     and bottom of a block rather than its flow faces, so a cross-circuit
//     coupling never looks like more pipework.
//
// Routing is deliberately NOT baked into the layout: nodes can be dragged, so
// edge paths are recomputed from live positions at render time.

import { lineId, type LineKey } from './palette'
import { symbolOf, type Shape } from './symbols'

/** One connection node as the backend reports it. */
export interface Connection {
  domain: string
  endpoints: string[]
  /** Fluid connector type (`liquid`, `twophase`, …); absent outside fluid. */
  connector?: string | null
  /** Working fluid the node carries, when the model named one. */
  fluid?: string | null
  /** Per endpoint, the display prefix its member variables use — aligned with
   *  `endpoints` by index. Lets the drawing show an endpoint's solved state. */
  streams?: string[]
}

export type PortSide = 'left' | 'right' | 'top' | 'bottom'

export interface PortAnchor {
  port: string
  side: PortSide
  /** Offset from the node origin — absolute position is node.x + dx. */
  dx: number
  dy: number
  /** Line this port carries, when it is wired; drives the stub's color. */
  lineId?: string
}

export interface SchematicNode {
  /** Canonical lowercase instance name (matches endpoint prefixes). */
  id: string
  /** Display label — the instance name as written, when known. */
  label: string
  /** Component type when the document or a solve supplied it. */
  type?: string
  shape: Shape
  /** True for a model boundary (source/sink/ground) — drawn as a terminal. */
  terminal: boolean
  kind: 'instance' | 'junction'
  /** Id of the circuit band this node is drawn in. */
  group: string
  x: number
  y: number
  w: number
  h: number
  ports: PortAnchor[]
}

export interface SchematicEdge {
  id: string
  /** Line identity — what flows here; keys into the style map. */
  lineId: string
  domain: string
  connector?: string | null
  fluid?: string | null
  from: string
  to: string
  fromPort?: string
  toPort?: string
  /** Variable display prefixes at each end (`chlr.in`, `s2`), for readouts. */
  fromStream?: string
  toStream?: string
}

/**
 * Where the user has dragged each block, as an OFFSET from the position the
 * auto-layout gave it, keyed by lowercase instance name.
 *
 * Offsets rather than absolute coordinates, because the drawing is regenerated
 * from the document on every check: a component that gains an upstream stage
 * moves, and an absolute position saved against the old layout would leave the
 * block stranded — possibly on top of another one. An offset means "this far
 * from wherever the layout puts it", which survives the document growing.
 */
export type SchematicOffsets = Record<string, { dx: number; dy: number }>

/** One circuit: a connected run of a single line kind, framed and titled. */
export interface SchematicGroup {
  id: string
  label: string
  lineId: string
  x: number
  y: number
  w: number
  h: number
}

export interface SchematicLayout {
  nodes: SchematicNode[]
  edges: SchematicEdge[]
  groups: SchematicGroup[]
  /** Every distinct line in the drawing, for the legend and the style map. */
  lines: LineKey[]
  width: number
  height: number
}

const NODE_H = 52
const NODE_MIN_W = 104
const CHAR_W = 7.4
const NODE_PAD = 30
const JUNCTION = 13
const LAYER_GAP = 78
const ROW_GAP = 34
const MARGIN = 26
const GROUP_PAD = 22
const GROUP_TITLE = 22
const GROUP_GAP = 26

interface Endpoint {
  instance: string
  port?: string
  stream?: string
}

function splitEndpoint(raw: string, stream?: string): Endpoint {
  const dot = raw.indexOf('.')
  if (dot <= 0) {
    return { instance: raw.toLowerCase(), stream }
  }
  return { instance: raw.slice(0, dot).toLowerCase(), port: raw.slice(dot + 1), stream }
}

function nodeWidth(label: string, type?: string): number {
  const longest = Math.max(label.length, (type ?? '').length)
  return Math.max(NODE_MIN_W, Math.round(longest * CHAR_W) + NODE_PAD)
}

/**
 * An `out`-ish port feeds; anything else is fed. Port names are the only
 * direction information an acausal network carries, and the standard library
 * is consistent about them (`out`, `hot_out`, `ref_out`, `discharge`).
 *
 * `b` counts only as a WHOLE port name — the two-terminal `a`/`b` convention.
 * As a prefix it does not: `b_in` is an inlet, and treating it as an outlet
 * put it on the wrong face of the block.
 */
function isOutlet(port?: string): boolean {
  if (port === undefined) {
    return false
  }
  return /^(out|outlet|b|discharge|supply)$/i.test(port)
    || /(^|_)(out|outlet|discharge|supply)\d*(_|$)/i.test(port)
}

/** Which face of a block a port attaches to. The DOMAIN decides first: heat
 *  goes up, everything non-fluid goes down, so cross-domain couplings never sit
 *  on the flow axis and can't be mistaken for pipework. */
function portSide(domain: string, port?: string): PortSide {
  switch (domain?.toLowerCase()) {
    case 'heat':
      return 'top'
    case 'fluid':
      return isOutlet(port) ? 'right' : 'left'
    default:
      return 'bottom'
  }
}

interface BuiltGraph {
  nodes: Map<string, SchematicNode>
  edges: SchematicEdge[]
  /** instance → ports it uses, with the side and line each carries. */
  portUse: Map<string, Map<string, { side: PortSide; lineId: string }>>
  /** Undirected adjacency restricted to one line kind, for circuit finding. */
  byLine: Map<string, Map<string, Set<string>>>
  /** Directed flow adjacency (upstream → downstream) per line kind. */
  flow: Map<string, Map<string, Set<string>>>
  lines: Map<string, LineKey>
}

function emptyGraph(): BuiltGraph {
  return {
    nodes: new Map(),
    edges: [],
    portUse: new Map(),
    byLine: new Map(),
    flow: new Map(),
    lines: new Map(),
  }
}

function addAdjacency(map: Map<string, Map<string, Set<string>>>, key: string, a: string, b: string) {
  let g = map.get(key)
  if (!g) {
    g = new Map()
    map.set(key, g)
  }
  if (!g.has(a)) {
    g.set(a, new Set())
  }
  if (!g.has(b)) {
    g.set(b, new Set())
  }
  g.get(a)?.add(b)
}

/** Nodes, edges, per-line adjacency and inferred flow direction. */
function buildGraph(
  connections: readonly Connection[],
  labels: ReadonlyMap<string, { label: string; type?: string }>,
  instances: readonly string[],
): BuiltGraph {
  const g = emptyGraph()

  const touch = (id: string, kind: 'instance' | 'junction') => {
    if (g.nodes.has(id)) {
      return
    }
    const known = labels.get(id)
    const label = known?.label ?? id
    const sym = kind === 'junction' ? { shape: 'junction' as Shape, terminal: false } : symbolOf(known?.type)
    g.nodes.set(id, {
      id,
      label,
      type: known?.type,
      shape: sym.shape,
      terminal: sym.terminal,
      kind,
      group: '',
      x: 0,
      y: 0,
      w: kind === 'junction' ? JUNCTION : nodeWidth(label, known?.type),
      h: kind === 'junction' ? JUNCTION : NODE_H,
      ports: [],
    })
  }

  const notePort = (e: Endpoint, domain: string, line: string) => {
    if (!e.port) {
      return
    }
    let ports = g.portUse.get(e.instance)
    if (!ports) {
      ports = new Map()
      g.portUse.set(e.instance, ports)
    }
    ports.set(e.port, { side: portSide(domain, e.port), lineId: line })
  }

  connections.forEach((conn, index) => {
    const ends = conn.endpoints.map((raw, i) => splitEndpoint(raw, conn.streams?.[i]))
    const key: LineKey = { domain: conn.domain, connector: conn.connector, fluid: conn.fluid }
    const line = lineId(key)
    g.lines.set(line, key)
    ends.forEach((e) => {
      touch(e.instance, 'instance')
      notePort(e, conn.domain, line)
    })

    if (ends.length === 2) {
      addPairEdge(ends[0], ends[1], conn, line, `c${index}`, g)
      return
    }
    // A junction (3+ endpoints, or a degenerate 1-endpoint node) becomes its
    // own small node so the star reads as one shared node rather than a clique
    // of edges that would imply pairwise connections.
    const junctionId = `$node${index}`
    touch(junctionId, 'junction')
    ends.forEach((e, k) => {
      // Direction still comes from the port: feeders point into the junction.
      const [from, to] = isOutlet(e.port) ? [e, { instance: junctionId }] : [{ instance: junctionId }, e]
      g.edges.push({
        id: `c${index}_${k}`,
        lineId: line,
        domain: conn.domain,
        connector: conn.connector,
        fluid: conn.fluid,
        from: from.instance,
        to: to.instance,
        fromPort: 'port' in from ? from.port : undefined,
        toPort: 'port' in to ? to.port : undefined,
        fromStream: 'stream' in from ? from.stream : undefined,
        toStream: 'stream' in to ? to.stream : undefined,
      })
      addAdjacency(g.byLine, line, junctionId, e.instance)
      addAdjacency(g.byLine, line, e.instance, junctionId)
      addAdjacency(g.flow, line, from.instance, to.instance)
    })
  })

  // Declared but unwired instances still get a node, so the canvas shows the
  // whole network — including the parts the user is about to connect.
  for (const instance of instances) {
    touch(instance.toLowerCase(), 'instance')
  }
  return g
}

/** A two-endpoint connection, oriented by port name. A component wired to
 *  itself carries no layout information, so it contributes no edge. */
function addPairEdge(a: Endpoint, b: Endpoint, conn: Connection, line: string, id: string, g: BuiltGraph): void {
  if (a.instance === b.instance) {
    return
  }
  // The outlet feeds the inlet; when neither or both say so, keep written order.
  const [from, to] = isOutlet(b.port) && !isOutlet(a.port) ? [b, a] : [a, b]
  g.edges.push({
    id,
    lineId: line,
    domain: conn.domain,
    connector: conn.connector,
    fluid: conn.fluid,
    from: from.instance,
    to: to.instance,
    fromPort: from.port,
    toPort: to.port,
    fromStream: from.stream,
    toStream: to.stream,
  })
  addAdjacency(g.byLine, line, a.instance, b.instance)
  addAdjacency(g.byLine, line, b.instance, a.instance)
  addAdjacency(g.flow, line, from.instance, to.instance)
}

/**
 * Assigns every node to a circuit. A circuit is a connected run of ONE line
 * kind — so the coolant loop, the refrigerant loop and the thermal network are
 * three circuits even though the chiller belongs to two of them.
 *
 * A node on more than one circuit is drawn in the one it has the most links
 * on, with fluid winning ties: a wall heat exchanger is a piece of the coolant
 * line that also touches heat, not a piece of the thermal network.
 */
function assignGroups(g: BuiltGraph): Map<string, string> {
  const group = new Map<string, string>()
  const score = new Map<string, number>()

  // Fluid lines are claimed first, so a node they contain keeps its fluid band
  // when a coupling line later reaches for it.
  const lineOrder = [...g.byLine.keys()].sort(
    (a, b) => rankBand(a) - rankBand(b) || a.localeCompare(b),
  )

  for (const line of lineOrder) {
    const adjacency = g.byLine.get(line)
    if (!adjacency) {
      continue
    }
    // Only a FLUID line is split into separate circuits. A coupling domain is
    // point-to-point by nature — every thermal mass hangs off exactly one heat
    // exchanger wall, and the wall belongs to its fluid loop — so splitting
    // heat by connectivity yields one band per mass. All of a coupling line's
    // nodes therefore share one band.
    const split = line.startsWith('fluid:')
    connectedComponents(adjacency).forEach((members, index) => {
      const groupId = `${line}#${split ? index : 0}`
      for (const id of members) {
        // Links within this circuit; fluid outranks a coupling domain.
        const links = (adjacency.get(id)?.size ?? 0) + (split ? 100 : 0)
        if (links > (score.get(id) ?? -1)) {
          score.set(id, links)
          group.set(id, groupId)
        }
      }
    })
  }

  // Anything the topology never touched (declared but unwired) gets its own band.
  for (const id of g.nodes.keys()) {
    if (!group.has(id)) {
      group.set(id, 'unwired#0')
    }
  }
  return group
}

/** Connected components of one line's adjacency, each a list of node ids.
 *  Roots are visited in name order so the same document always yields the same
 *  components in the same order. */
function connectedComponents(adjacency: ReadonlyMap<string, Set<string>>): string[][] {
  const seen = new Set<string>()
  const out: string[][] = []
  for (const start of [...adjacency.keys()].sort((a, b) => a.localeCompare(b))) {
    if (seen.has(start)) {
      continue
    }
    const members: string[] = []
    const stack = [start]
    seen.add(start)
    while (stack.length > 0) {
      const id = stack.pop() as string
      members.push(id)
      for (const n of adjacency.get(id) ?? []) {
        if (!seen.has(n)) {
          seen.add(n)
          stack.push(n)
        }
      }
    }
    out.push(members)
  }
  return out
}

/**
 * Longest-path layering over the inferred flow direction, restricted to the
 * nodes of one circuit. Sources land in layer 0 and each node sits one layer
 * past its furthest upstream neighbour, which is what makes a circuit read left
 * to right. A cycle (a genuinely closed loop) cannot be layered by depth alone,
 * so the walk stops at nodes it has already placed — the loop then reads as a
 * chain with its closing edge drawn back, exactly as it is drawn by hand.
 */
function layerCircuit(members: readonly string[], flow: ReadonlyMap<string, Set<string>>): string[][] {
  const inSet = new Set(members)
  const depth = longestPathDepths(members, inSet, flow)
  placeCycleRemnants(members, depth, flow)
  return bucketByDepth(members, depth)
}

/** Kahn's algorithm over the flow edges inside one circuit, recording each
 *  node's longest distance from a source. Nodes left in a cycle get no depth
 *  and are handled separately. */
function longestPathDepths(
  members: readonly string[],
  inSet: ReadonlySet<string>,
  flow: ReadonlyMap<string, Set<string>>,
): Map<string, number> {
  const depth = new Map<string, number>()
  const indegree = new Map<string, number>(members.map((id) => [id, 0]))
  for (const id of members) {
    for (const next of flow.get(id) ?? []) {
      if (inSet.has(next)) {
        indegree.set(next, (indegree.get(next) ?? 0) + 1)
      }
    }
  }
  const queue = members.filter((id) => indegree.get(id) === 0).sort((a, b) => a.localeCompare(b))
  for (const id of queue) {
    depth.set(id, 0)
  }
  // The queue grows as nodes are freed; iterating it picks those up in turn.
  for (const id of queue) {
    const downstream = [...(flow.get(id) ?? [])].filter((n) => inSet.has(n)).sort((a, b) => a.localeCompare(b))
    for (const next of downstream) {
      depth.set(next, Math.max(depth.get(next) ?? 0, (depth.get(id) ?? 0) + 1))
      indegree.set(next, (indegree.get(next) ?? 0) - 1)
      if (indegree.get(next) === 0) {
        queue.push(next)
      }
    }
  }
  return depth
}

/** A genuinely closed loop has no source to start from, so the topological
 *  walk never reaches it. Each remaining node is placed one layer past its
 *  deepest feeder that WAS placed — the loop then reads as a chain with its
 *  closing edge drawn back, exactly as it is drawn by hand. */
function placeCycleRemnants(
  members: readonly string[],
  depth: Map<string, number>,
  flow: ReadonlyMap<string, Set<string>>,
): void {
  for (const id of members) {
    if (depth.has(id)) {
      continue
    }
    let d = 0
    for (const [from, tos] of flow) {
      if (tos.has(id) && depth.has(from)) {
        d = Math.max(d, (depth.get(from) ?? 0) + 1)
      }
    }
    depth.set(id, d)
  }
}

/** Depth map → dense layer lists, one column per depth. */
function bucketByDepth(members: readonly string[], depth: ReadonlyMap<string, number>): string[][] {
  const layers: string[][] = []
  for (const id of members) {
    const d = depth.get(id) ?? 0
    while (layers.length <= d) {
      layers.push([])
    }
    layers[d].push(id)
  }
  return layers.filter((l) => l.length > 0)
}

/** Barycenter sweeps: each node drifts toward the mean position of its
 *  neighbours in the previous layer, the cheap standard way to cut crossings. */
function orderLayers(
  layers: string[][],
  adjacency: ReadonlyMap<string, Set<string>>,
  order: ReadonlyMap<string, number>,
): void {
  for (let sweep = 0; sweep < 3; sweep++) {
    for (let i = 1; i < layers.length; i++) {
      const previous = new Map(layers[i - 1].map((id, k) => [id, k]))
      layers[i] = [...layers[i]].sort((a, b) => {
        const ba = barycenter(a, previous, adjacency)
        const bb = barycenter(b, previous, adjacency)
        return ba - bb || (order.get(a) ?? 0) - (order.get(b) ?? 0)
      })
    }
  }
}

function barycenter(
  id: string,
  previous: ReadonlyMap<string, number>,
  adjacency: ReadonlyMap<string, Set<string>>,
): number {
  let sum = 0
  let count = 0
  for (const neighbour of adjacency.get(id) ?? []) {
    const at = previous.get(neighbour)
    if (at !== undefined) {
      sum += at
      count++
    }
  }
  return count === 0 ? Number.MAX_SAFE_INTEGER : sum / count
}

/** Fluid circuits are the spine (0), coupling bands hang off them (1), and
 *  unwired components trail at the end (2). */
function rankBand(id: string): number {
  if (id.startsWith('fluid:')) {
    return 0
  }
  return id.startsWith('unwired') ? 2 : 1
}

/**
 * Band order. Fluid circuits are the spine and keep their declaration order;
 * a coupling band (heat, signal, mechanical) is slotted immediately AFTER the
 * fluid circuit it has the most links to, so its couplings are short and stay
 * out of the other circuits. In the common shape — two loops bridged by a heat
 * network — that puts the thermal band between them instead of below both,
 * which is where the coupling lines would otherwise have to cross a whole
 * unrelated circuit to reach it.
 */
function orderBands(
  ids: readonly string[],
  members: ReadonlyMap<string, string[]>,
  g: BuiltGraph,
): string[] {
  const fluid = ids.filter((id) => id.startsWith('fluid:')).sort((a, b) => a.localeCompare(b))
  const unwired = ids.filter((id) => id.startsWith('unwired'))
  const coupling = ids
    .filter((id) => !id.startsWith('fluid:') && !id.startsWith('unwired'))
    .sort((a, b) => a.localeCompare(b))

  const bandOf = new Map<string, string>()
  for (const [band, nodes] of members) {
    for (const id of nodes) {
      bandOf.set(id, band)
    }
  }

  const out = [...fluid]
  for (const band of coupling) {
    const best = mostLinkedFluidBand(band, bandOf, g)
    const at = best === undefined ? out.length : out.indexOf(best) + 1
    out.splice(at, 0, band)
  }
  return [...out, ...unwired]
}

/** The fluid band a coupling band has the most edges into, if any. */
function mostLinkedFluidBand(
  band: string,
  bandOf: ReadonlyMap<string, string>,
  g: BuiltGraph,
): string | undefined {
  const links = new Map<string, number>()
  const note = (inside: string, outside: string) => {
    if (bandOf.get(inside) !== band) {
      return
    }
    const other = bandOf.get(outside)
    if (other?.startsWith('fluid:')) {
      links.set(other, (links.get(other) ?? 0) + 1)
    }
  }
  for (const e of g.edges) {
    // Either end may be the one inside this band.
    note(e.from, e.to)
    note(e.to, e.from)
  }
  let best: string | undefined
  for (const [target, count] of links) {
    if (best === undefined || count > (links.get(best) ?? 0)) {
      best = target
    }
  }
  return best
}

/**
 * Places a band of PENDANT couplings — thermal masses, ambient nodes, probes —
 * in a ROW at the horizontal position of the blocks they couple to. This is
 * what turns a coupling into something a reader can follow: the battery sits
 * under its cold plate and the line between them is a short vertical drop,
 * instead of both being stacked in a column and joined by a wire that crosses
 * the whole drawing.
 *
 * Applies only when the band is genuinely a set of pendants: no node has more
 * than one neighbour INSIDE the band. A thermal network with real internal
 * structure (a wall stack conduction → mass → convection) has a node with two
 * internal neighbours and falls through to the normal layered placement, where
 * its structure is what matters. Returns null in that case.
 */
/** Each node's neighbours WITHIN the band, or null if any node has more than
 *  one — the signal that this is a real network rather than a set of pendants. */
function internalNeighbours(
  ids: readonly string[],
  inside: ReadonlySet<string>,
  adjacency: ReadonlyMap<string, Set<string>> | undefined,
): Map<string, string[]> | null {
  const out = new Map<string, string[]>()
  for (const id of ids) {
    const kin = [...(adjacency?.get(id) ?? [])].filter((n) => inside.has(n))
    if (kin.length > 1) {
      return null
    }
    out.set(id, kin)
  }
  return out
}

/** For each node, the mean centre of its partners OUTSIDE the band — i.e. the
 *  x it wants to sit at. Nodes with no outside partner are simply absent. */
function partnerPositions(
  ids: readonly string[],
  inside: ReadonlySet<string>,
  g: BuiltGraph,
): Map<string, number> {
  const target = new Map<string, number>()
  for (const id of ids) {
    let sum = 0
    let count = 0
    for (const e of g.edges) {
      const other = edgePartner(e, id)
      if (other === null || inside.has(other)) {
        continue
      }
      const n = g.nodes.get(other)
      if (n) {
        sum += n.x + n.w / 2
        count++
      }
    }
    if (count > 0) {
      target.set(id, sum / count)
    }
  }
  return target
}

/** The other end of an edge from `id`'s point of view, or null if `id` is not
 *  on it. */
function edgePartner(e: SchematicEdge, id: string): string | null {
  if (e.from === id) {
    return e.to
  }
  return e.to === id ? e.from : null
}

function placeCouplingRow(
  ids: readonly string[],
  g: BuiltGraph,
  line: string,
  originX: number,
  originY: number,
): { w: number; h: number } | null {
  if (ids.length < 2) {
    return null
  }
  const inside = new Set(ids)
  const internal = internalNeighbours(ids, inside, g.byLine.get(line))
  if (internal === null) {
    return null // real internal structure — layer it instead
  }

  const target = partnerPositions(ids, inside, g)
  // A node whose only link is inside the band (an ambient source feeding a
  // junction) follows that neighbour rather than being exiled to the end.
  for (const id of ids) {
    if (target.has(id)) {
      continue
    }
    const kin = internal.get(id)?.find((n) => target.has(n))
    target.set(id, kin !== undefined ? (target.get(kin) as number) : Number.MAX_SAFE_INTEGER)
  }

  const ordered = [...ids].sort((a, b) => (target.get(a) ?? 0) - (target.get(b) ?? 0))
  let cursor = originX
  let height = 0
  for (const id of ordered) {
    const n = g.nodes.get(id)
    if (!n) {
      continue
    }
    const wanted = target.get(id) as number
    // Honour the partner's position where there is room; otherwise pack left
    // to right so two nodes never land on top of each other.
    const centred = wanted === Number.MAX_SAFE_INTEGER ? cursor : wanted - n.w / 2
    n.x = Math.max(cursor, centred)
    n.y = originY
    cursor = n.x + n.w + ROW_GAP
    height = Math.max(height, n.h)
  }
  return { w: Math.max(0, cursor - ROW_GAP - originX), h: height }
}

/** Places one circuit's layers in columns; returns its bounding size. */
function placeCircuit(
  layers: string[][],
  nodes: Map<string, SchematicNode>,
  originX: number,
  originY: number,
): { w: number; h: number } {
  const columnHeights = layers.map((layer) =>
    layer.reduce((h, id) => h + (nodes.get(id)?.h ?? NODE_H) + ROW_GAP, -ROW_GAP),
  )
  const tallest = Math.max(0, ...columnHeights)
  let x = originX
  layers.forEach((layer, li) => {
    const widest = Math.max(...layer.map((id) => nodes.get(id)?.w ?? NODE_MIN_W))
    let y = originY + (tallest - columnHeights[li]) / 2
    for (const id of layer) {
      const n = nodes.get(id)
      if (!n) {
        continue
      }
      n.x = x + (widest - n.w) / 2
      n.y = y
      y += n.h + ROW_GAP
    }
    x += widest + LAYER_GAP
  })
  return { w: Math.max(0, x - LAYER_GAP - originX), h: tallest }
}

/** Port anchors for a node: wired ports take the side their line implies,
 *  unwired catalog ports fall back to the name. Ports are spread along the
 *  face in a stable order so the drawing does not jitter between checks. */
function anchorPorts(node: SchematicNode, declared: readonly string[], used: ReadonlyMap<string, { side: PortSide; lineId: string }>): PortAnchor[] {
  if (node.kind !== 'instance') {
    return []
  }
  const sides: Record<PortSide, { port: string; lineId?: string }[]> = { left: [], right: [], top: [], bottom: [] }
  const names = declared.length > 0 ? declared : [...used.keys()]
  for (const port of names) {
    const hit = used.get(port)
    const unwiredSide: PortSide = isOutlet(port) ? 'right' : 'left'
    sides[hit?.side ?? unwiredSide].push({ port, lineId: hit?.lineId })
  }
  const out: PortAnchor[] = []
  for (const side of ['left', 'right', 'top', 'bottom'] as PortSide[]) {
    const list = sides[side]
    list.forEach((p, i) => {
      // Evenly spaced along the face, inset from the corners.
      const t = (i + 1) / (list.length + 1)
      out.push({ port: p.port, side, lineId: p.lineId, ...faceOffset(side, node, t) })
    })
  }
  return out
}

/** Where along a node's face a port sits: pinned to the edge on the axis the
 *  face fixes, spread by `t` along the other. */
function faceOffset(side: PortSide, node: SchematicNode, t: number): { dx: number; dy: number } {
  switch (side) {
    case 'left':
      return { dx: 0, dy: node.h * t }
    case 'right':
      return { dx: node.w, dy: node.h * t }
    case 'top':
      return { dx: node.w * t, dy: 0 }
    default:
      return { dx: node.w * t, dy: node.h }
  }
}

/**
 * Builds the placed drawing. `labels` maps a lowercase instance name to its
 * written spelling and component type; `ports` gives each instance's declared
 * port list (from the component catalog) so unwired ports still show.
 */
export function layoutSchematic(
  connections: readonly Connection[],
  labels: ReadonlyMap<string, { label: string; type?: string }> = new Map(),
  instances: readonly string[] = [],
  ports: ReadonlyMap<string, readonly string[]> = new Map(),
  groupLabel: (line: LineKey, index: number) => string = (line) => line.domain,
): SchematicLayout {
  const g = buildGraph(connections, labels, instances)
  const groupOf = assignGroups(g)

  // Declaration order = first appearance; breaks every tie so a given document
  // always lays out the same way.
  const order = new Map<string, number>()
  ;[...g.nodes.keys()].forEach((id, i) => order.set(id, i))

  const members = new Map<string, string[]>()
  for (const [id, group] of groupOf) {
    if (!members.has(group)) {
      members.set(group, [])
    }
    members.get(group)?.push(id)
  }

  const groupIds = orderBands([...members.keys()], members, g)
  const sizes = placeBandsHorizontally(groupIds, members, order, g)
  const { groups, height, width } = stackBands(groupIds, members, sizes, g, groupLabel)

  for (const node of g.nodes.values()) {
    node.ports = anchorPorts(node, ports.get(node.id) ?? [], g.portUse.get(node.id) ?? new Map())
  }

  return {
    nodes: [...g.nodes.values()],
    edges: g.edges,
    groups,
    lines: [...g.lines.values()],
    width,
    height,
  }
}

/**
 * Lays out each band's contents left to right, FLUID BANDS FIRST whatever
 * order the bands are stacked in: a coupling band positions each pendant under
 * the block it couples to, so every fluid band must already have an x by then.
 * Vertical placement is a separate pass. Returns each band's size.
 */
function placeBandsHorizontally(
  groupIds: readonly string[],
  members: ReadonlyMap<string, string[]>,
  order: ReadonlyMap<string, number>,
  g: BuiltGraph,
): Map<string, { w: number; h: number }> {
  const sizes = new Map<string, { w: number; h: number }>()
  const originX = MARGIN + GROUP_PAD
  for (const groupId of [...groupIds].sort((a, b) => rankBand(a) - rankBand(b))) {
    const ids = (members.get(groupId) ?? []).sort((a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0))
    const line = lineOfBand(groupId)
    // A band of pendant couplings is aligned to its partners; anything with
    // real structure of its own is layered.
    let size = placeCouplingRow(ids, g, line, originX, 0)
    if (!size) {
      const layers = layerCircuit(ids, g.flow.get(line) ?? new Map())
      orderLayers(layers, g.byLine.get(line) ?? new Map(), order)
      size = placeCircuit(layers, g.nodes, originX, 0)
    }
    sizes.set(groupId, size)
    for (const id of ids) {
      const n = g.nodes.get(id)
      if (n) {
        n.group = groupId
      }
    }
  }
  return sizes
}

/** Stacks the bands in display order, shifting each band's nodes down onto it,
 *  and builds the frames. Every frame spans the drawing so the bands read as
 *  bands rather than as boxes of differing width. */
function stackBands(
  groupIds: readonly string[],
  members: ReadonlyMap<string, string[]>,
  sizes: ReadonlyMap<string, { w: number; h: number }>,
  g: BuiltGraph,
  groupLabel: (line: LineKey, index: number) => string,
): { groups: SchematicGroup[]; width: number; height: number } {
  const groups: SchematicGroup[] = []
  let cursorY = MARGIN
  let maxX = 0

  for (const groupId of groupIds) {
    const size = sizes.get(groupId) ?? { w: 0, h: 0 }
    const line = lineOfBand(groupId)
    const originY = cursorY + GROUP_TITLE + GROUP_PAD / 2
    for (const id of members.get(groupId) ?? []) {
      const n = g.nodes.get(id)
      if (n) {
        n.y += originY
      }
    }
    const key = g.lines.get(line)
    groups.push({
      id: groupId,
      lineId: line,
      label: key ? groupLabel(key, indexOfBand(groupId)) : 'unwired',
      x: MARGIN,
      y: cursorY,
      w: size.w + GROUP_PAD * 2,
      h: size.h + GROUP_TITLE + GROUP_PAD * 1.5,
    })
    maxX = Math.max(maxX, MARGIN + size.w + GROUP_PAD * 2)
    cursorY += size.h + GROUP_TITLE + GROUP_PAD * 1.5 + GROUP_GAP
  }

  for (const group of groups) {
    group.w = maxX - MARGIN
  }
  return {
    groups,
    width: Math.max(maxX + MARGIN, MARGIN * 2),
    height: Math.max(cursorY - GROUP_GAP + MARGIN, MARGIN * 2),
  }
}

/** A band id is `<lineId>#<index>`; these split it back apart. */
function lineOfBand(groupId: string): string {
  return groupId.slice(0, groupId.lastIndexOf('#'))
}

function indexOfBand(groupId: string): number {
  return Number(groupId.slice(groupId.lastIndexOf('#') + 1))
}

/** Absolute position of a port anchor, given the node's live position. */
export function anchorAt(node: SchematicNode, port: string | undefined): { x: number; y: number; side: PortSide } {
  const hit = node.ports.find((p) => p.port === port)
  if (!hit) {
    return { x: node.x + node.w / 2, y: node.y + node.h / 2, side: 'right' }
  }
  return { x: node.x + hit.dx, y: node.y + hit.dy, side: hit.side }
}

/**
 * Orthogonal route between two ports, leaving each node along the face its
 * port sits on. Recomputed from live positions, so it stays correct while a
 * node is being dragged.
 */
export function routeEdge(
  a: SchematicNode,
  b: SchematicNode,
  fromPort?: string,
  toPort?: string,
): string {
  const p = anchorAt(a, fromPort)
  const q = anchorAt(b, toPort)
  const STUB = 14
  const p1 = stubOut(p, STUB)
  const q1 = stubOut(q, STUB)

  // Vertical couplings (heat up, signal/mechanical down) route through a shared
  // horizontal lane; flow routes through a shared vertical one.
  const vertical = p.side === 'top' || p.side === 'bottom' || q.side === 'top' || q.side === 'bottom'
  if (vertical) {
    const midY = round((p1.y + q1.y) / 2)
    return `M ${round(p.x)} ${round(p.y)} L ${round(p1.x)} ${round(p1.y)} V ${midY} H ${round(q1.x)} V ${round(q1.y)} L ${round(q.x)} ${round(q.y)}`
  }
  const midX = round((p1.x + q1.x) / 2)
  return `M ${round(p.x)} ${round(p.y)} L ${round(p1.x)} ${round(p1.y)} H ${midX} V ${round(q1.y)} H ${round(q1.x)} L ${round(q.x)} ${round(q.y)}`
}

function stubOut(p: { x: number; y: number; side: PortSide }, d: number): { x: number; y: number } {
  switch (p.side) {
    case 'left':
      return { x: p.x - d, y: p.y }
    case 'right':
      return { x: p.x + d, y: p.y }
    case 'top':
      return { x: p.x, y: p.y - d }
    default:
      return { x: p.x, y: p.y + d }
  }
}

function round(v: number): number {
  return Math.round(v * 10) / 10
}
