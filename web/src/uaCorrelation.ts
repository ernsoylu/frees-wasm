// Builds the correlation equations that compute a heat-exchanger UA [W/K] from
// each side's film coefficient and area, for the wizard's "Compute UA…" flow.
// Reuses the already-wired frees correlation toolkit (props/HxCorrelations.java):
//   htc_1phase / htc_evap / htc_cond / htc_extair  → film coefficient h [W/m^2-K]
//   ua_hx(h1, A1, h2, A2, Rwall)                    → UA [W/K]
// The emitted lines mirror the canonical worked example (examples.ts:44-130):
//   hA_<inst> = htc_evap('R1234yf', 350000, 0.5, 0.05, 0.006, 8e-5)
//   AA_<inst> = 3 * hx_aconv(8e-5, 0.30, 0.006)
//   ...
//   UA_<inst> = ua_hx(hA_<inst>, AA_<inst>, hB_<inst>, AB_<inst>, 1e-4)

export type Phase = 'single' | 'boiling' | 'condensing' | 'extair'

export interface UaSide {
  phase: Phase
  fluid: string   // CoolProp/fluid name, e.g. R1234yf, EG50, Air
  P: string       // pressure [Pa]
  state: string   // temperature [K] for single/extair, quality x [-] for two-phase
  mdot: string    // mass flow [kg/s]
  Dh: string      // hydraulic (or tube) diameter [m]
  Aflow: string   // flow cross-section [m^2]
  // Convective area: either entered directly (area), or built as count*hx_aconv.
  area: string    // either a number/expr [m^2], or a passage count when L given
  L?: string      // passage length [m]; when set, area is treated as a count
}

export const PHASE_LABELS: Record<Phase, string> = {
  single: 'Single-phase',
  boiling: 'Boiling (two-phase)',
  condensing: 'Condensing (two-phase)',
  extair: 'External air',
}

// Phase → film-coefficient function. Two-phase sides take a quality `x`,
// single-phase/external-air take a temperature `T` (the signature discriminator).
const FILM_FN: Record<Phase, string> = {
  single: 'htc_1phase',
  boiling: 'htc_evap',
  condensing: 'htc_cond',
  extair: 'htc_extair',
}

function filmLine(hVar: string, side: UaSide): string {
  const fn = FILM_FN[side.phase]
  // htc_extair uses tube diameter D in the same slot; all four share the arg order
  // (fluid$, P, state, mdot, Dh|D, Aflow).
  return `${hVar} = ${fn}('${side.fluid}', ${side.P}, ${side.state}, ${side.mdot}, ${side.Dh}, ${side.Aflow})`
}

function areaLine(aVar: string, side: UaSide): string {
  if (side.L && side.L.trim() !== '') {
    // area is a passage count → count * hx_aconv(Aflow, L, Dh)
    return `${aVar} = ${side.area} * hx_aconv(${side.Aflow}, ${side.L}, ${side.Dh})`
  }
  return `${aVar} = ${side.area}`
}

export interface UaResult {
  uaVar: string
  lines: string[]
}

/** Emit the correlation preamble + the UA variable name to wire into `UA=`. */
export function buildUaCorrelation(
  instanceName: string,
  sideA: UaSide,
  sideB: UaSide,
  Rwall: string,
): UaResult {
  const inst = (instanceName || 'X').replace(/[^A-Za-z0-9_]/g, '')
  const hA = `hA_${inst}`
  const aA = `AA_${inst}`
  const hB = `hB_${inst}`
  const aB = `AB_${inst}`
  const uaVar = `UA_${inst}`
  const rwall = Rwall.trim() === '' ? '0' : Rwall
  const lines = [
    filmLine(hA, sideA),
    areaLine(aA, sideA),
    filmLine(hB, sideB),
    areaLine(aB, sideB),
    `${uaVar} = ua_hx(${hA}, ${aA}, ${hB}, ${aB}, ${rwall})`,
  ]
  return { uaVar, lines }
}

/** Whether a numeric param looks like a heat-exchanger conductance the UA builder
 *  can populate (UA, UA_cool, …, in W/K). */
export function isUaParam(name: string, unit: string): boolean {
  return /^UA(_|$)/i.test(name) || unit === 'W/K'
}
