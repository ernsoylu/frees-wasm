import { StateTableDto, VariableResult } from '../api'

/**
 * Thermodynamic state detection: solved variables named like h1, s[2] or
 * T_3 are grouped into numbered states (rows) by property (columns), the
 * the usual convention for cycle analyses. All values are SI.
 */

/** Canonical property columns, in display order. */
const STATE_PROPERTIES = ['T', 'P', 'v', 'u', 'h', 's', 'x', 'rho', 'w', 'Twb', 'Tdp', 'rh']

const PROPERTY_ALIASES: Record<string, string> = {
  t: 'T',
  drybulb: 'T',
  tdrybulb: 'T',
  p: 'P',
  pressure: 'P',
  v: 'v',
  volume: 'v',
  u: 'u',
  internalenergy: 'u',
  h: 'h',
  enthalpy: 'h',
  s: 's',
  entropy: 's',
  x: 'x',
  quality: 'x',
  rho: 'rho',
  density: 'rho',
  w: 'w',
  humrat: 'w',
  omega: 'w',
  humidityratio: 'w',
  twb: 'Twb',
  twetbulb: 'Twb',
  wetbulb: 'Twb',
  tdp: 'Tdp',
  tdew: 'Tdp',
  tdewpoint: 'Tdp',
  dewpoint: 'Tdp',
  rh: 'rh',
  relhum: 'rh',
  phi: 'rh',
  relativehumidity: 'rh',
}

export interface StateTable {
  /** State indices in ascending numeric order. */
  indices: number[]
  /** Properties (subset of STATE_PROPERTIES) that occur in any state. */
  columns: string[]
  /** values[index][property] = SI value. */
  values: Record<number, Record<string, number>>
}

function matchStateVariable(name: string): { property: string; index: number } | null {
  const m = /^([a-z]+(?:_[a-z]+)*?)_?(\d+)$|^([a-z]+)\[(\d+)\]$/i.exec(name)
  if (!m) return null
  const base = (m[1] ?? m[3]).replaceAll('_', '').toLowerCase()
  const property = PROPERTY_ALIASES[base]
  if (!property) return null
  return { property, index: Number(m[2] ?? m[4]) }
}

/** Property symbols, longest first, for leading-prefix matching of declared
 * state-table variables (so `rho…` beats `r…`). Mirrors the backend. */
const PROPERTY_PREFIXES = Object.keys(PROPERTY_ALIASES).sort(
  (a, b) => b.length - a.length,
)

/**
 * Match a STATE TABLE block variable as `<prop><tag><index>`: the longest
 * leading property symbol is the property, any middle characters are the
 * circuit tag (the `w` in `Pw_1`), and the trailing digits are the state
 * index. Unlike {@link matchStateVariable} this tolerates the circuit tag, so
 * `Pw_1` / `Pref_1` from different circuits are recognised.
 */
function matchBlockStateVariable(
  name: string,
): { property: string; tag: string; index: number } | null {
  const lower = name.toLowerCase()
  let prefix: string
  let index: number
  const br = /^([a-z][a-z_]*)\[(\d+)\]$/.exec(lower)
  if (br) {
    prefix = br[1]
    index = Number(br[2])
  } else {
    const pl = /^([a-z][a-z_]*?)_?(\d+)$/.exec(lower)
    if (!pl) return null
    prefix = pl[1]
    index = Number(pl[2])
  }
  for (const sym of PROPERTY_PREFIXES) {
    if (prefix.startsWith(sym)) {
      // Whatever follows the property symbol is the circuit tag (the `w` in
      // `Pw_1`), used to keep colliding state indices in separate circuits.
      return { property: PROPERTY_ALIASES[sym], tag: prefix.slice(sym.length).replaceAll('_', ''), index }
    }
  }
  return null
}

/** A state table tied to one declared STATE TABLE block: its name and fluid. */
interface NamedStateTable extends StateTable {
  name: string
  fluid: string | null
}

/**
 * Build one fluid-aware state table per declared STATE TABLE block. Only the
 * variables listed in each block are grouped (so circuits with colliding state
 * indices stay separate), using the block's fluid. Returns an empty list when
 * no blocks are declared — callers then fall back to {@link detectStates}.
 */
export function detectStateTables(
  variables: VariableResult[],
  defs: StateTableDto[] | undefined,
): NamedStateTable[] {
  if (!defs || defs.length === 0) return []
  // Pre-match every solved variable once into (property, tag, index).
  const matched: { property: string; tag: string; index: number; value: number }[] = []
  for (const v of variables) {
    if (!Number.isFinite(v.value)) continue
    const m = matchBlockStateVariable(v.name)
    if (m) matched.push({ ...m, value: v.value })
  }
  return defs.map((def) => {
    // The declared members define the table's state points as (tag, index)
    // signatures. Any solved variable sharing a signature — including
    // properties computed by Fill Missing Values (hw_1, sw_1, …) that were not
    // listed in the declaration — is pulled in, so computed values populate the
    // table instead of only appearing in the Solution window.
    const signatures = new Set<string>()
    for (const varName of def.variables) {
      const m = matchBlockStateVariable(varName)
      if (m) signatures.add(`${m.tag}|${m.index}`)
    }
    const values: Record<number, Record<string, number>> = {}
    const columnSet = new Set<string>()
    for (const m of matched) {
      if (!signatures.has(`${m.tag}|${m.index}`)) continue
      values[m.index] = values[m.index] ?? {}
      values[m.index][m.property] = m.value
      columnSet.add(m.property)
    }
    const indices = Object.keys(values)
      .map(Number)
      .sort((a, b) => a - b)
    const columns = STATE_PROPERTIES.filter((p) => columnSet.has(p))
    return { name: def.name, fluid: def.fluid ?? null, indices, columns, values }
  })
}

export function detectStates(variables: VariableResult[]): StateTable {
  const values: Record<number, Record<string, number>> = {}
  const columnSet = new Set<string>()
  for (const variable of variables) {
    const match = matchStateVariable(variable.name)
    if (!match || !Number.isFinite(variable.value)) continue
    values[match.index] = values[match.index] ?? {}
    values[match.index][match.property] = variable.value
    columnSet.add(match.property)
  }
  const indices = Object.keys(values)
    .map(Number)
    .sort((a, b) => a - b)
  const columns = STATE_PROPERTIES.filter((p) => columnSet.has(p))
  return { indices, columns, values }
}

/**
 * States that have both axis properties needed by a diagram, in index
 * order — these can be overlaid as points on the diagram.
 */
export function statesForAxes(
  table: StateTable,
  xProperty: string,
  yProperty: string,
): { index: number; x: number; y: number }[] {
  const points: { index: number; x: number; y: number }[] = []
  for (const index of table.indices) {
    const x = table.values[index][xProperty]
    const y = table.values[index][yProperty]
    if (x !== undefined && y !== undefined) {
      points.push({ index, x, y })
    }
  }
  return points
}
