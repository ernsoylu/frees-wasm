// Turning a solve into what a schematic should say when you point at a block.
//
// A drawn network with no numbers on it is a picture of the topology only: the
// reader has to carry an instance name over to the Solution window and rejoin
// it there by hand. Everything needed to avoid that is already on the wire —
// the connection payload names each endpoint's variable prefix, and the solve
// returns every variable — so this module joins them.
//
// Three things are worth showing, in this order:
//   1. the STATE at each port (P, T, ṁ, …) — what the block does to the stream;
//   2. the block's own OUTPUTS (Q, W, …) — what it computes;
//   3. the PARAMETERS it was built with (UA, η, CdA) — including the heat
//      transfer coefficients a document computes outside the component and
//      injects, which are otherwise invisible on the drawing.

import type { ComponentParamResult, ComponentResult, VariableResult } from '../api'
import type { SchematicEdge, SchematicNode } from './layout'

export interface Reading {
  label: string
  value: number
  units: string
}

export interface PortReading {
  port: string
  /** The line this port carries, for coloring the readout in the card. */
  lineId?: string
  readings: Reading[]
}

export interface NodeReadout {
  ports: PortReading[]
  outputs: Reading[]
  params: { name: string; text: string }[]
}

/**
 * Stream members worth reporting, per domain, in reading order. Only members
 * the solve actually produced are shown — a derived property (a fluid stream's
 * T) exists as a variable only when the document referenced it, and inventing
 * a blank row for every member it did not would bury the ones it did.
 */
const MEMBERS: Record<string, [string, string][]> = {
  fluid: [
    ['p', 'P'],
    ['t', 'T'],
    ['mdot', 'ṁ'],
    ['h', 'h'],
    ['x', 'x'],
    ['w', 'W'],
    ['y', 'y'],
    ['z', 'z'],
    ['oc', 'oil frac'],
  ],
  heat: [
    ['t', 'T'],
    ['qdot', 'Q̇'],
  ],
  electrical: [
    ['v', 'V'],
    ['i', 'I'],
  ],
  mechanical: [
    ['w', 'ω'],
    ['tau', 'τ'],
  ],
  translational: [
    ['vel', 'v'],
    ['f', 'F'],
  ],
  signal: [['sig', 'value']],
}

/**
 * Case-insensitive variable index — solver names are canonical lowercase.
 *
 * `inferred` is the check's unit map, used where a solved variable carries no
 * unit of its own. Port members are exactly that case: a stream's P/ṁ/h are
 * the solver's own unknowns, so nothing in the solve grounds them, and the
 * expander's domain-derived units reach the frontend on the CHECK payload
 * instead. Without this fallback a hover card reports pressures as bare
 * numbers, which on a schematic in SI is worse than useless.
 */
export function indexVariables(
  variables: readonly VariableResult[] | undefined,
  inferred?: Readonly<Record<string, string>>,
): Map<string, VariableResult> {
  const out = new Map<string, VariableResult>()
  for (const v of variables ?? []) {
    const key = v.name.toLowerCase()
    out.set(key, v.units ? v : { ...v, units: inferred?.[key] ?? '' })
  }
  return out
}

/**
 * What to show for one block. `edges` supplies the variable prefix of every
 * wired port (`chlr.in` for a connect-wired free port, `s2` for a shared-name
 * stream), which is the only reliable way to reach a port's state — the two
 * wiring styles name their variables differently.
 */
export function readoutFor(
  node: SchematicNode,
  edges: readonly SchematicEdge[],
  values: ReadonlyMap<string, VariableResult>,
  components: readonly ComponentResult[] | undefined,
): NodeReadout {
  const ports = portReadings(node, edges, values)
  const portNames = new Set(node.ports.map((p) => p.port.toLowerCase()))
  return {
    ports,
    outputs: outputReadings(node, values, portNames),
    params: paramReadings(node, components),
  }
}

/** State at each of a block's wired ports, in the order the ports are drawn. */
function portReadings(
  node: SchematicNode,
  edges: readonly SchematicEdge[],
  values: ReadonlyMap<string, VariableResult>,
): PortReading[] {
  const prefixes = streamPrefixes(node, edges)
  const out: PortReading[] = []
  for (const anchor of node.ports) {
    const hit = prefixes.get(anchor.port)
    if (!hit) {
      continue
    }
    const readings = membersOf(hit.stream, hit.domain, values)
    if (readings.length > 0) {
      out.push({ port: anchor.port, lineId: hit.lineId, readings })
    }
  }
  return out
}

/** Each of a block's wired ports → the stream naming its variables, with the
 *  domain that decides which members to report. A port can appear on several
 *  edges (at a junction); they all name the same stream, so the first wins. */
function streamPrefixes(
  node: SchematicNode,
  edges: readonly SchematicEdge[],
): Map<string, { stream: string; domain: string; lineId: string }> {
  const out = new Map<string, { stream: string; domain: string; lineId: string }>()
  const note = (port: string | undefined, stream: string | undefined, e: SchematicEdge) => {
    if (port && stream && !out.has(port)) {
      out.set(port, { stream, domain: e.domain, lineId: e.lineId })
    }
  }
  for (const e of edges) {
    if (e.from === node.id) {
      note(e.fromPort, e.fromStream, e)
    }
    if (e.to === node.id) {
      note(e.toPort, e.toStream, e)
    }
  }
  return out
}

/** The members of one stream that the solve actually produced, in reading
 *  order for its domain. */
function membersOf(
  stream: string,
  domain: string,
  values: ReadonlyMap<string, VariableResult>,
): Reading[] {
  const readings: Reading[] = []
  for (const [member, label] of MEMBERS[domain.toLowerCase()] ?? MEMBERS.fluid) {
    const v = values.get(`${stream}.${member}`.toLowerCase())
    if (v && Number.isFinite(v.value)) {
      readings.push({ label, value: v.value, units: v.units })
    }
  }
  return readings
}

/**
 * A block's named outputs — `CHLR.Q`, `CMP.W`. These are `instance.name`
 * variables whose name is not one of the block's ports, which is exactly how
 * the expander distinguishes an output from a port member.
 */
function outputReadings(
  node: SchematicNode,
  values: ReadonlyMap<string, VariableResult>,
  portNames: ReadonlySet<string>,
): Reading[] {
  const prefix = `${node.id.toLowerCase()}.`
  const out: Reading[] = []
  for (const [name, v] of values) {
    if (!name.startsWith(prefix)) {
      continue
    }
    const rest = name.slice(prefix.length)
    // `chlr.in.p` is a port member, not an output; `chlr.q` is an output.
    if (rest.includes('.') || portNames.has(rest)) {
      continue
    }
    if (Number.isFinite(v.value)) {
      out.push({ label: v.name.slice(prefix.length), value: v.value, units: v.units })
    }
  }
  return out.sort((a, b) => a.label.localeCompare(b.label))
}

/**
 * The parameters a block was built with, as written and as resolved. This is
 * where a heat exchanger's UA shows up — a document that sizes its UA from
 * correlations and geometry injects the result as a parameter, so without this
 * the entire heat-transfer calculation is invisible on the drawing.
 */
function paramReadings(
  node: SchematicNode,
  components: readonly ComponentResult[] | undefined,
): { name: string; text: string }[] {
  const hit = components?.find((c) => c.name.toLowerCase() === node.id.toLowerCase())
  if (!hit) {
    return []
  }
  return hit.params.map((p) => ({ name: p.name, text: parameterText(p) }))
}

/** One parameter as "<value> <unit>  (<source variable>)" — the source shown
 *  only when the binding was a named variable, so `UA=UA_chl_r` still points
 *  back at the correlation that produced it. */
function parameterText(p: ComponentParamResult): string {
  const resolved = describeParamValue(p)
  const named = /^[a-z_]\w*$/i.test(p.ref) && resolved !== p.ref
  return named ? `${resolved}  (${p.ref})` : resolved
}

function describeParamValue(p: ComponentParamResult): string {
  if (p.value === null || p.value === undefined || !Number.isFinite(p.value)) {
    return p.ref
  }
  const unit = p.units ? ` ${p.units}` : ''
  return formatCompact(p.value) + unit
}

/** Compact fixed/exponential rendering — a hover card has no room for 8 digits. */
export function formatCompact(value: number): string {
  if (!Number.isFinite(value)) {
    return String(value)
  }
  if (value === 0) {
    return '0'
  }
  const abs = Math.abs(value)
  if (abs >= 1e6 || abs < 1e-3) {
    return value.toExponential(3)
  }
  return Number(value.toPrecision(5)).toString()
}

/**
 * The one number worth printing ON a block, without hovering. A heat exchanger
 * is defined by its duty, a machine by its power, a pump by its flow — so the
 * label is chosen per block from what it actually computed.
 */
export function badgeFor(node: SchematicNode, readout: NodeReadout): Reading | null {
  const byName = (re: RegExp) => readout.outputs.find((o) => re.test(o.label))
  // An exchanger is defined by its duty, a machine by its power.
  let preferred: Reading | undefined
  if (node.shape === 'exchanger') {
    preferred = byName(/^q/i)
  } else if (node.shape === 'compressor' || node.shape === 'pump' || node.shape === 'machine') {
    preferred = byName(/^[wp]$/i) ?? byName(/^(power|w)/i)
  }
  if (preferred) {
    return preferred
  }
  if (readout.outputs.length > 0) {
    return readout.outputs[0]
  }
  // No named output: fall back to the flow through the block, the single most
  // useful number on a fluid schematic.
  for (const p of readout.ports) {
    const flow = p.readings.find((r) => r.label === 'ṁ' || r.label === 'Q̇' || r.label === 'I')
    if (flow) {
      return flow
    }
  }
  return null
}
