// Line colors for the rendered schematic. Deliberately a separate palette from
// the plots' series colors (categorical, assigned by slot) and their property
// colors (isobars/isotherms): here the color carries MEANING — it
// names WHAT FLOWS in a line, so the same hue must always mean the same thing.
// Hues are Mantine-scale *-3/*-5 values, tuned to stay legible on the dark
// theme, and are literal hex rather than CSS variables because a serialized SVG
// has to keep its colors outside the app.
//
// The bond-graph DOMAIN is too coarse to color by on its own: a coolant loop
// and a refrigerant loop are both `domain=fluid`, so a domain-colored drawing
// paints two independent circuits the same blue and they read as one tangle.
// Fluid lines are therefore colored by their CONNECTOR TYPE (liquid, two-phase,
// pneumatic, hydraulic, humid air), and a second fluid sharing a connector
// takes the next shade in that connector's ramp.

/** Non-fluid bond-graph domains keep one fixed, semantic color each. */
const DOMAIN_COLORS: Record<string, string> = {
  heat: '#ff6b6b', // red — heat flow
  electrical: '#ffd43b', // yellow — current
  mechanical: '#a9e34b', // lime — torque
  translational: '#a9e34b', // same family as rotational mechanics
  signal: '#3bc9db', // cyan — causal control values, drawn dashed
}

/**
 * Fluid connector ramps. The first entry is the connector's canonical color;
 * later entries only come into play when one model runs two different working
 * fluids over the same connector type (two coolant loops, say), so they stay
 * within the same hue family and the circuits still read as "both coolant".
 */
const CONNECTOR_RAMPS: Record<string, string[]> = {
  liquid: ['#4dabf7', '#1c7ed6', '#74c0fc'], // blue — coolant / water-glycol
  twophase: ['#e599f7', '#cc5de8', '#f3d9fa'], // violet — refrigerant
  fluid: ['#38d9a9', '#0ca678', '#96f2d7'], // teal — generic thermofluid / steam
  gas: ['#ffa94d', '#f76707', '#ffd8a8'], // orange — pneumatic
  oil: ['#f08c00', '#e67700', '#ffec99'], // amber — hydraulic
  moistair: ['#99e9f2', '#22b8cf', '#c5f6fa'], // pale cyan — humid air
}

const UNKNOWN = '#adb5bd'

/** How a line is drawn, beyond its color. A causal SIGNAL is not a physical
 *  flow — block-diagram convention draws it thin and dashed, which also keeps
 *  it from competing with the pipework it supervises. */
export interface LineStyle {
  color: string
  width: number
  dash?: string
}

/** Identity of one line in the drawing: what physics it obeys and, for a fluid
 *  line, which circuit it belongs to. */
export interface LineKey {
  domain: string
  /** Fluid connector type (`liquid`, `twophase`, …); absent outside fluid. */
  connector?: string | null
  /** Working fluid (`eg50`, `r1234yf`); absent when the model never named one. */
  fluid?: string | null
}

/** Stable key for "same kind of line" — the unit the legend lists and the
 *  circuit grouping partitions on. */
export function lineId(key: LineKey): string {
  const domain = key.domain?.toLowerCase() ?? ''
  if (domain !== 'fluid') {
    return domain
  }
  return `fluid:${key.connector?.toLowerCase() ?? 'fluid'}:${key.fluid?.toLowerCase() ?? ''}`
}

/**
 * Resolves every line in a drawing at once, so a fluid's color depends on the
 * whole set (a second fluid on one connector has to differ from the first) and
 * stays stable for a given document: assignment walks the fluids of each
 * connector in sorted order, never in discovery order.
 */
export function buildLineStyles(keys: readonly LineKey[]): Map<string, LineStyle> {
  const out = new Map<string, LineStyle>()
  const fluidsByConnector = new Map<string, Set<string>>()

  for (const key of keys) {
    const id = lineId(key)
    if ((key.domain?.toLowerCase() ?? '') !== 'fluid') {
      const domain = key.domain?.toLowerCase() ?? ''
      out.set(id, {
        color: DOMAIN_COLORS[domain] ?? UNKNOWN,
        width: domain === 'signal' ? 1.4 : 2,
        dash: domain === 'signal' ? '5 3' : undefined,
      })
      continue
    }
    const connector = key.connector?.toLowerCase() ?? 'fluid'
    const fluid = key.fluid?.toLowerCase() ?? ''
    let set = fluidsByConnector.get(connector)
    if (!set) {
      set = new Set()
      fluidsByConnector.set(connector, set)
    }
    set.add(fluid)
  }

  for (const [connector, fluids] of fluidsByConnector) {
    const ramp = CONNECTOR_RAMPS[connector] ?? [UNKNOWN]
    ;[...fluids].sort((a, b) => a.localeCompare(b)).forEach((fluid, i) => {
      out.set(`fluid:${connector}:${fluid}`, { color: ramp[i % ramp.length], width: 2.4 })
    })
  }
  return out
}

/** Human-readable connector names for the legend — `twophase` is not a word. */
const CONNECTOR_LABELS: Record<string, string> = {
  liquid: 'liquid',
  twophase: 'two-phase',
  fluid: 'fluid',
  gas: 'pneumatic',
  oil: 'hydraulic',
  moistair: 'humid air',
}

/**
 * The legend entry for a line: the working fluid leads, because that is what
 * the reader is tracing ("where does the R1234yf go?"), with the connector type
 * as the qualifier. Non-fluid lines are named by their domain.
 */
export function lineLabel(key: LineKey, fluidSpelling?: ReadonlyMap<string, string>): string {
  const domain = key.domain?.toLowerCase() ?? ''
  if (domain !== 'fluid') {
    return domain || 'unknown'
  }
  const connector = CONNECTOR_LABELS[key.connector?.toLowerCase() ?? 'fluid'] ?? key.connector ?? 'fluid'
  const raw = key.fluid?.toLowerCase()
  if (!raw) {
    return connector
  }
  return `${fluidSpelling?.get(raw) ?? key.fluid} · ${connector}`
}
