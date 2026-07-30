// solve() and check() run fully in the browser: a Rust/WASM engine inside a
// Web Worker (src/wasm/) replaces POST /api/solve and /api/check, emitting the
// same REST wire shapes this module always parsed. Endpoints whose engine
// features are not ported yet (REPL, optimize, curve fit, fluids, plots,
// tables, Monte Carlo, control) are stubbed — a neutral resolved value where
// the UI consumes data at boot, otherwise a rejection naming the gap — so
// nothing in this module hits the network anymore. runCompute/pollJob stay
// exported for the day a hybrid remote path returns.
import { wasmCheck, wasmSolve } from './wasm/engineClient'

export interface VariableResult {
  name: string
  value: number
  units: string
  uncertainty?: number | null
}

export interface BlockResult {
  index: number
  equations: string[]
  variables: string[]
}

export interface ResidualResult {
  equation: string
  value: number
}

export interface SolveStats {
  equations: number
  unknowns: number
  blocks: number
  iterations: number
  elapsedMillis: number
  maxResidual: number
}

export interface StopCriteria {
  maxIterations: number
  relativeResiduals: number
  changeInVariables: number
  elapsedTimeSeconds: number
  complexMode?: boolean
}

export const DEFAULT_STOP_CRITERIA: StopCriteria = {
  maxIterations: 250,
  relativeResiduals: 1e-12,
  changeInVariables: 1e-15,
  elapsedTimeSeconds: 3600,
}

export interface TableRowResult {
  success: boolean
  values: Record<string, number>
  error: string | null
}

export type UnitSystem = 'SI' | 'ENG_SI' | 'ENGLISH'

export const UNIT_SYSTEM_OPTIONS: { value: UnitSystem; label: string }[] = [
  { value: 'SI', label: 'SI base (Pa, J, W, K)' },
  { value: 'ENG_SI', label: 'Engineering SI (kPa, kJ, kW, °C)' },
  { value: 'ENGLISH', label: 'US / English (psi, Btu, hp, °F)' },
]

export interface SolutionResult {
  variables: VariableResult[]
  maxResidual: number
}

/** One uncertainty source's signed contribution to a variable (tornado bar). */
export interface UncertaintyContributionResult {
  source: string
  value: number
}

/** Tornado breakdown for one variable: propagated sigma + ranked sources. */
export interface VariableUncertaintyResult {
  variable: string
  total: number
  sources: UncertaintyContributionResult[]
}

export interface SolveResponse {
  success: boolean
  variables: VariableResult[]
  blocks: BlockResult[]
  residuals: ResidualResult[]
  stats: SolveStats | null
  solutions: SolutionResult[]
  unitWarnings: string[]
  error: string | null
  /** 1-based editor line a syntax error points at, or null for whole-system errors. */
  errorLine?: number | null
  cyclePath?: Record<string, number>[]
  /** Function tables parsed from TABLE ... END blocks in the editor text. */
  codeTables?: FunctionTableDto[]
  /** Parametric run-tables parsed from PARAMETRIC ... END blocks. */
  parametricTables?: ParametricTableDto[]
  /** Plots declared in the editor text with PLOT 'name' ... END blocks. */
  definedPlots?: PlotDefDto[]
  /** Fluid state tables declared with STATE TABLE ... END blocks. */
  stateTableDefs?: StateTableDto[]
  /** ODE Tables produced by solved DYNAMIC ... END blocks. */
  odeTables?: OdeTableDto[]
  /** Per-instance component metadata (type + parameter bindings) for the datasheet view. */
  components?: ComponentResult[]
  /** Index of the Tarjan block whose solve gave up (failure diagnostics), or null. */
  failedBlockIndex?: number | null
  /** Tornado breakdown: per uncertain variable, the ranked per-source contributions. */
  uncertaintyBreakdown?: VariableUncertaintyResult[]
}

/** One parameter binding on a component instance (`UA=UA_chl_r`, `SH=5`, `fluid$=R1234yf`). */
export interface ComponentParamResult {
  name: string
  /** Bound expression as written: a variable name, literal, or expression. */
  ref: string
  /** Resolved numeric value when the binding is a variable or number; null for strings/expressions. */
  value?: number | null
  units?: string | null
}

/** A solved component instance: its identity and the parameters it was built with. */
export interface ComponentResult {
  name: string
  type: string
  params: ComponentParamResult[]
}

export interface CheckResponse {
  solvable: boolean
  equations: number
  unknowns: number
  variables: string[]
  unitWarnings: string[]
  inferredUnits: Record<string, string>
  message: string
  /** 1-based editor line a syntax error points at, or null for whole-system errors. */
  errorLine?: number | null
  /** Every syntax error the parse collected (line/column 1-based), so the
   *  editor can mark them all — errorLine keeps pointing at the first. */
  errors?: { line: number; column: number; message: string }[]
  /** Function tables parsed from TABLE ... END blocks in the editor text. */
  codeTables?: FunctionTableDto[]
  /** Parametric run-tables parsed from PARAMETRIC ... END blocks. */
  parametricTables?: ParametricTableDto[]
  /** Plots declared in the editor text with PLOT 'name' ... END blocks. */
  definedPlots?: PlotDefDto[]
  /** Fluid state tables declared with STATE TABLE ... END blocks. */
  stateTableDefs?: StateTableDto[]
  /** Connection topology of the component network (domain + instance.port
   *  endpoints per node) — the rendered schematic's data layer. */
  connections?: ConnectionDto[]
}

/** One connection-topology node of the component network. */
export interface ConnectionDto {
  domain: string
  endpoints: string[]
  /** Fluid connector type (`liquid`, `twophase`, `gas`, `oil`, `moistair`,
   *  `fluid`); null outside the fluid domain. Distinguishes circuits the
   *  bond-graph domain lumps together — a coolant loop and a refrigerant loop
   *  are both `domain: 'fluid'`. */
  connector?: string | null
  /** The CoolProp working fluid this node carries, when the model named one. */
  fluid?: string | null
  /** Per endpoint (aligned by index), the display prefix its member variables
   *  use — `chlr.in` for a connect-wired free port, `s2` for a shared-name
   *  stream. Lets the schematic show an endpoint's solved state. */
  streams?: string[]
}

/** A fluid state table parsed from a STATE TABLE name(...) ... END block: the
 * declared state-point variables and the fluid every state in the block uses
 * (null when no FLUID = ... line was given). */
export interface StateTableDto {
  name: string
  variables: string[]
  fluid: string | null
}

/** A parametric run-table parsed from a PARAMETRIC ... END block: the declared
 * variables and a row-major value grid (null cells where a column is short). */
export interface ParametricTableDto {
  name: string
  vars: string[]
  rows: (number | null)[][]
}

/** A plot parsed from a PLOT 'name' ... END block: the plot name and a raw
 * attribute map (lowercased keys → string values) the frontend maps onto a
 * PlotSpec via plotDefToSpec. */
export interface PlotDefDto {
  name: string
  attributes: Record<string, string[]>
}

/** An ODE Table produced by a solved DYNAMIC ... END block: columns are
 * [time, states…, auxiliaries…] and rows are the sampled trajectory. Shaped
 * like a parametric table so it renders in the Tables window and feeds the
 * Plots window through the same path. */
export interface OdeTableDto {
  name: string
  vars: string[]
  /** Per-column SI unit, aligned to `vars` (the ODE rows are SI). */
  units: string[]
  rows: (number | null)[][]
  events: { name: string; time: number }[]
  method: string
  stopped: boolean
  endTime: number
}

export interface VariableInfo {
  name: string
  guess: number | null
  lower: number | null
  upper: number | null
  units: string | null
  uncertainty: number | null
}

const API_BASE = import.meta.env.VITE_API_BASE || '';

// ── Asynchronous compute client (Epic 15) ────────────────────────────────
// The 202-submit + poll machinery of the server stack. No longer wired to any
// exported function (the browser engine replaced solve/check, and the rest are
// stubbed), but kept — with its tests — for a future hybrid remote path.

/** The message every not-yet-ported endpoint stub reports. */
const NOT_IN_BROWSER_ENGINE = 'not yet available in the browser engine'

interface JobState {
  jobId: string
  status: 'PENDING' | 'COMPLETED' | 'FAILED'
  error: string | null
  result: unknown
}

/** Normalized outcome of an async compute request: either the solver result
 *  DTO (COMPLETED) or an error message (FAILED / rejected / unreachable). */
type ComputeOutcome =
  | { kind: 'completed'; result: any }
  | { kind: 'failed'; error: string }

/** Extracts a human-readable error message from a non-ok response body. Reads the
 *  body exactly once (a Response body is a single-use stream) and prefers a JSON
 *  {@code error}/{@code message} field, falling back to the raw text, then {@code fallback}. */
async function extractErrorMessage(response: Response, fallback: string): Promise<string> {
  let body: string
  try {
    body = await response.text()
  } catch {
    return fallback
  }
  if (!body) return fallback
  try {
    const data = JSON.parse(body)
    if (data && typeof data === 'object') {
      const msg = (data as Record<string, unknown>).error ?? (data as Record<string, unknown>).message
      if (typeof msg === 'string' && msg) return msg
    }
  } catch {
    // Body is not JSON — fall through to the raw text.
  }
  return body
}

/** Polls GET /api/jobs/{jobId} until the job reaches a terminal state. */
async function pollJob(jobId: string, timeoutMs = 120_000): Promise<JobState> {
  const deadline = Date.now() + timeoutMs
  const url = `${API_BASE}/api/jobs/${encodeURIComponent(jobId)}`

  if (typeof EventSource !== 'undefined') {
    try {
      return await new Promise<JobState>((resolve, reject) => {
        const sse = new EventSource(`${url}/stream`)
        const timeout = setTimeout(() => {
          sse.close()
          reject(new Error('Job timed out waiting for completion via SSE'))
        }, timeoutMs)

        sse.onmessage = (event) => {
          try {
            const state = JSON.parse(event.data) as JobState
            if (state.status === 'COMPLETED' || state.status === 'FAILED') {
              clearTimeout(timeout)
              sse.close()
              resolve(state)
            }
          } catch (e) {
            // ignore malformed
          }
        }

        sse.onerror = () => {
          clearTimeout(timeout)
          sse.close()
          reject(new Error('SSE connection failed'))
        }
      })
    } catch (e) {
      // Fall through to polling
    }
  }

  // Fallback to polling
  while (Date.now() < deadline) {
    let response: Response
    try {
      response = await fetch(url)
    } catch (e) {
      throw new Error(`Could not reach the solver backend: ${String(e)}`, { cause: e })
    }
    if (response.status === 404) {
      throw new Error('Job not found')
    }
    if (!response.ok) {
      throw new Error(`Job poll failed (${response.status})`)
    }
    const state = await response.json() as JobState
    if (state.status === 'COMPLETED' || state.status === 'FAILED') {
      return state
    }
    // Only reached when SSE is unavailable/failed; 250 ms keeps latency low
    // without hammering the API node (the SSE push is the primary path).
    await new Promise(resolve => setTimeout(resolve, 250))
  }
  throw new Error('Job timed out waiting for completion')
}

/** Submits a compute request and, in async mode, polls for its result.
 *  Returns the terminal outcome (completed result DTO or failure message).
 *  @param endpoint the POST URL (e.g. "/api/solve")
 *  @param init the fetch init (method/body/headers) for the submit POST
 */
export async function runCompute(endpoint: string, init: RequestInit,
                                 timeoutMs = 120_000): Promise<ComputeOutcome> {
  let response: Response
  try {
    response = await fetch(`${API_BASE}${endpoint}`, init)
  } catch (e) {
    return { kind: 'failed', error: `Could not reach the solver backend: ${String(e)}` }
  }

  // Synchronous validation rejection (4xx): the body carries the error.
  if (!response.ok && response.status !== 202) {
    return { kind: 'failed', error: await extractErrorMessage(response, `Server error (${response.status})`) }
  }

  // Asynchronous path: 202 + jobId, then poll.
  if (response.status === 202) {
    let ticket: { jobId?: string }
    try {
      ticket = await response.json()
    } catch {
      return { kind: 'failed', error: 'Malformed job submission response' }
    }
    const jobId = ticket?.jobId
    if (!jobId) {
      return { kind: 'failed', error: 'Job submission did not return a jobId' }
    }
    try {
      const state = await pollJob(jobId, timeoutMs)
      if (state.status === 'COMPLETED') {
        return { kind: 'completed', result: state.result }
      }
      return { kind: 'failed', error: state.error ?? 'Job failed' }
    } catch (e) {
      return { kind: 'failed', error: e instanceof Error ? e.message : String(e) }
    }
  }

  // Defensive: a 200 in async mode (e.g. a backend not yet switched over) —
  // treat the body as the completed result DTO.
  const data = await response.json().catch(() => null)
  return { kind: 'completed', result: data }
}

/** A Function Table in solver wire format (Epic 8): the table name is the
 * function name callable from equations; argNames lists the column names
 * (lookup argument first, then the family parameter, if any). */
export interface FunctionTableDto {
  name: string
  argNames: string[]
  xLog: boolean
  yLog: boolean
  curves: { param: number | null; points: number[][] }[]
}

export async function check(
  text: string,
  variableInfo: VariableInfo[],
  complexMode: boolean,
  functionTables: FunctionTableDto[] = [],
  overrides: string[] = [],
): Promise<CheckResponse> {
  try {
    // The wasm boundary consumes the same request body POST /api/check did
    // (unknown fields are ignored until their machinery ports) and returns a
    // full CheckResponse for every document problem — syntax errors arrive as
    // data with errorLine/errors[], never as a thrown exception.
    const data = await wasmCheck(
      text,
      JSON.stringify({ variableInfo, stopCriteria: { complexMode }, functionTables, overrides }),
    )
    return {
      solvable: data.solvable ?? false,
      equations: data.equations ?? 0,
      unknowns: data.unknowns ?? 0,
      variables: data.variables ?? [],
      unitWarnings: data.unitWarnings ?? [],
      inferredUnits: data.inferredUnits ?? {},
      message: data.message ?? '',
      errorLine: data.errorLine ?? null,
      errors: data.errors ?? [],
      // Not produced by the wasm boundary yet (Phases 5+); the App.tsx call
      // sites treat empty collections as "the document declares none".
      codeTables: [],
      parametricTables: [],
      definedPlots: [],
      stateTableDefs: [],
      connections: [],
    }
  } catch (e) {
    // Only infrastructure can land here (worker died, wasm failed to load) —
    // document problems are data above.
    return {
      solvable: false,
      equations: 0,
      unknowns: 0,
      variables: [],
      unitWarnings: [],
      inferredUnits: {},
      message: `Browser engine error: ${e instanceof Error ? e.message : String(e)}`,
    }
  }
}

/** The empty solve response returned on any failure (network, server, FAILED job). */
const SOLVE_FAILURE: Omit<SolveResponse, 'error'> = {
  success: false,
  variables: [],
  blocks: [],
  residuals: [],
  stats: null,
  solutions: [],
  unitWarnings: [],
}

/** Maps a solve result DTO (from a sync 200 body or an async COMPLETED `result`)
 *  to the typed SolveResponse. */
function mapSolveData(data: any): SolveResponse {
  return {
    success: data.success ?? false,
    variables: data.variables ?? [],
    blocks: data.blocks ?? [],
    residuals: data.residuals ?? [],
    stats: data.stats ?? null,
    solutions: data.solutions ?? [],
    unitWarnings: data.unitWarnings ?? [],
    error: data.error ?? null,
    cyclePath: data.cyclePath ?? [],
    codeTables: data.codeTables ?? [],
    parametricTables: data.parametricTables ?? [],
    definedPlots: data.definedPlots ?? [],
    stateTableDefs: data.stateTableDefs ?? [],
    odeTables: data.odeTables ?? [],
    components: data.components ?? [],
    errorLine: data.errorLine ?? null,
    failedBlockIndex: data.failedBlockIndex ?? null,
    uncertaintyBreakdown: data.uncertaintyBreakdown ?? [],
  }
}

export async function solve(
  text: string,
  stopCriteria: StopCriteria,
  variableInfo: VariableInfo[],
  findAllSolutions: boolean,
  displayUnitSystem: UnitSystem,
  fillMissing: boolean,
  functionTables: FunctionTableDto[] = [],
  // The session id tagged server-side result caching for the REPL; the
  // browser engine has no result cache yet, so it is accepted and unused.
  _sessionId?: string,
  overrides: string[] = [],
): Promise<SolveResponse> {
  // The full former POST body (minus `text`, which travels as the source
  // argument). The wasm boundary honours variableInfo + stopCriteria today
  // and ignores the rest until those features port, so the signature and the
  // request stay stable as the engine grows.
  const request = JSON.stringify({
    stopCriteria,
    variableInfo,
    findAllSolutions,
    displayUnitSystem,
    fillMissing,
    functionTables,
    // REPL overrides ("eta = 0.75") take priority over the editor's value.
    overrides,
  })

  try {
    // Success, syntax errors (errorLine) and solver failures (error +
    // failedBlockIndex) all arrive as one SolveResponse envelope — the wasm
    // boundary folds the Java 200/400/422 split into data.
    return mapSolveData(await wasmSolve(text, request))
  } catch (e) {
    // Only infrastructure can land here (worker died, wasm failed to load).
    return {
      ...SOLVE_FAILURE,
      error: `Browser engine error: ${e instanceof Error ? e.message : String(e)}`,
    }
  }
}

/** Result of evaluating one REPL line against the cached solved workspace. */
export interface ReplResponse {
  success: boolean
  /** Numeric result (SI for compound expressions, display value for a bare variable). */
  value: number | null
  /** Print-ready rendering, e.g. "600" or "300 ± 0.5 [K]". */
  text: string | null
  units: string | null
  uncertainty: number | null
  error: string | null
  /** Set (display spelling) when the line defined a variable, so the UI can reflect it. */
  name: string | null
  assignedVariables?: VariableResult[]
}

/** Evaluates a single REPL expression against the session's last solve.
 *  Not ported yet — resolves to a failed ReplResponse (the terminal prints
 *  `error`; its call site has no catch, so this must never reject). */
export async function replEvaluate(
  _sessionId: string,
  _expression: string,
  _unitSystem?: UnitSystem,
): Promise<ReplResponse> {
  return {
    success: false,
    value: null,
    text: null,
    units: null,
    uncertainty: null,
    error: NOT_IN_BROWSER_ENGINE,
    name: null,
  }
}

/** Clears all (or a specific) REPL-defined/overridden variables for the session.
 *  There is no REPL state in the browser engine yet — a best-effort no-op,
 *  same as the old fire-and-forget POST (App.tsx calls it with `void`). */
export async function replClear(_sessionId: string, _variableName?: string): Promise<void> {
  /* nothing to clear yet */
}

export type OptimizeMethod = 'brent' | 'nelder-mead' | 'bobyqa'

export interface OptimizeParams {
  objective: string
  decisions: string[]
  lowers: number[]
  uppers: number[]
  method: OptimizeMethod
  maximize: boolean
  constraints?: string[]
}

export interface OptimizeResponse {
  success: boolean
  error: string | null
  warning: string | null
  objective: VariableResult | null
  decision: VariableResult | null
  decisions: VariableResult[]
  evaluations: number
  variables: VariableResult[]
}

/** The empty optimize response returned on any failure. */
const OPTIMIZE_FAILURE: Omit<OptimizeResponse, 'error'> = {
  success: false,
  warning: null,
  objective: null,
  decision: null,
  decisions: [],
  evaluations: 0,
  variables: [],
}

/** Single-objective optimization is not ported yet — resolves to a failed
 *  OptimizeResponse whose `error` the Min/Max modal displays inline. */
export async function optimize(
  _text: string,
  _stopCriteria: StopCriteria,
  _variableInfo: VariableInfo[],
  _displayUnitSystem: UnitSystem,
  _params: OptimizeParams,
): Promise<OptimizeResponse> {
  return { ...OPTIMIZE_FAILURE, error: NOT_IN_BROWSER_ENGINE }
}

export interface MultiObjectiveParams {
  objectives: string[]
  maximize: boolean[]
  decisions: string[]
  lowers: number[]
  uppers: number[]
  populationSize?: number
  generations?: number
  constraints?: string[]
}

export interface ParetoPoint {
  decisions: number[]
  objectives: number[]
}

export interface ParetoResponse {
  success: boolean
  error: string | null
  decisionNames: string[]
  objectiveNames: string[]
  front: ParetoPoint[]
  evaluations: number
}

/** The empty Pareto response returned on any failure. */
const PARETO_FAILURE: Omit<ParetoResponse, 'error'> = {
  success: false,
  decisionNames: [],
  objectiveNames: [],
  front: [],
  evaluations: 0,
}

/** Multi-objective (Pareto) optimization is not ported yet — resolves to a
 *  failed ParetoResponse whose `error` the Min/Max modal displays inline. */
export async function optimizeMulti(
  _text: string,
  _stopCriteria: StopCriteria,
  _variableInfo: VariableInfo[],
  _params: MultiObjectiveParams,
): Promise<ParetoResponse> {
  return { ...PARETO_FAILURE, error: NOT_IN_BROWSER_ENGINE }
}

export interface CurveFitParams {
  model: string
  yVariable: string
  xVariable: string
  parameters: string[]
  xData: number[]
  yData: number[]
  initialGuess?: number[]
}

export interface CurveFitResponse {
  success: boolean
  error: string | null
  fittedParameters: number[]
  parameterNames: string[]
  rSquared: number
  rmse: number
  iterations: number
  residuals: number[]
  fittedValues: number[]
}

const CURVE_FIT_FAILURE: Omit<CurveFitResponse, 'error'> = {
  success: false,
  fittedParameters: [],
  parameterNames: [],
  rSquared: 0,
  rmse: 0,
  iterations: 0,
  residuals: [],
  fittedValues: [],
}

/** Curve fitting (Levenberg–Marquardt) is not ported yet — resolves to a
 *  failed CurveFitResponse whose `error` the Curve Fit modal displays inline. */
export async function curveFit(_params: CurveFitParams): Promise<CurveFitResponse> {
  return { ...CURVE_FIT_FAILURE, error: NOT_IN_BROWSER_ENGINE }
}

// ---------------------------------------------------------------------------
// Parameter estimation — POST /api/measurements/parameter-fit
// ---------------------------------------------------------------------------

export interface ParameterFitParams {
  text: string
  stopCriteria: StopCriteria
  variableInfo: VariableInfo[]
  functionTables: FunctionTableDto[]
  parameters: string[]
  initial: number[]
  lower: number[]
  upper: number[]
  odeBlock: string
  column: string
  measuredT: number[]
  measuredV: number[]
  maxEvaluations?: number
}

export interface ParameterFitResult {
  success: boolean
  error: string | null
  parameterNames: string[]
  fittedValues: number[]
  rmse: number
  initialRmse: number
  evaluations: number
  truncated: boolean
  fittedT: number[]
  fittedV: number[]
}

const PARAMETER_FIT_FAILURE: ParameterFitResult = {
  success: false,
  error: null,
  parameterNames: [],
  fittedValues: [],
  rmse: 0,
  initialRmse: 0,
  evaluations: 0,
  truncated: false,
  fittedT: [],
  fittedV: [],
}

/** Parameter estimation needs DYNAMIC (ODE) solving, not ported yet — resolves
 *  to a failed ParameterFitResult; the modal shows `error` via setError. */
export async function parameterFit(_params: ParameterFitParams): Promise<ParameterFitResult> {
  return { ...PARAMETER_FIT_FAILURE, error: NOT_IN_BROWSER_ENGINE }
}

export interface DiagramCurve {
  family: string
  label: string
  x: (number | null)[]
  y: (number | null)[]
}

export interface DiagramMarker {
  label: string
  x: number
  y: number
}

export interface DiagramResponse {
  fluid: string
  kind: string
  xProperty: string
  yProperty: string
  xLog: boolean
  yLog: boolean
  dome: DiagramCurve[]
  isolines: DiagramCurve[]
  markers: DiagramMarker[]
}

export interface PsychartResponse {
  pressure: number
  tMin: number
  tMax: number
  curves: DiagramCurve[]
}

/** Fluid properties (CoolProp) are not in the browser engine yet. Consumed at
 *  boot (App.tsx, PlotTab, HelpPage) — the empty list is exactly what the old
 *  path returned when the backend had no CoolProp, and the UI already renders
 *  that state ("no fluids available"). */
export async function getFluids(): Promise<string[]> {
  return []
}

export interface UnitInfo {
  symbol: string
  dimension: string
  siFactor: number
}

export interface ConstantInfo {
  name: string
  value: number
  unit: string
  description: string
}

export interface LanguageReference {
  units: UnitInfo[]
  constants: ConstantInfo[]
}

/** Supported units and built-in constants. The wasm boundary does not export
 *  the registry tables yet; the empty reference is the old path's unreachable-
 *  backend fallback, which the Help page renders as empty tables without
 *  crashing. */
export async function getReference(): Promise<LanguageReference> {
  return { units: [], constants: [] }
}

/** Property diagrams need CoolProp, not ported yet — rejects; the PlotCard
 *  catch shows the message in the card's error state. */
export async function getPropertyDiagram(
  _fluid: string,
  _type: string,
): Promise<DiagramResponse> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

/** Psychrometric charts need CoolProp's humid-air model, not ported yet —
 *  rejects; the PlotCard catch shows the message in the card's error state. */
export async function getPsychrometricChart(
  _pressure: number,
  _tMin: number,
  _tMax: number,
): Promise<PsychartResponse> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

/** Vector export used the backend's FOP transcoder — rejects; the PlotCard
 *  export catch shows the message (SVG/PNG export stays fully client-side). */
export async function exportVector(
  _svg: string,
  _format: 'pdf' | 'eps',
): Promise<Blob> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

export interface TableStats {
  runs: number
  solved: number
  failed: number
  equations: number
  unknowns: number
  iterations: number
  elapsedMillis: number
  maxResidual: number
}

export interface SolveTableResponse {
  results: TableRowResult[]
  stats: TableStats | null
  variables: VariableResult[]
}

/** Parametric table solving is not ported yet — rejects; the App.tsx call site
 *  catches and writes the message into every row of the table. */
export async function solveTable(
  _text: string,
  _stopCriteria: StopCriteria,
  _variableInfo: VariableInfo[],
  _displayUnitSystem: UnitSystem,
  _variables: string[],
  _rows: Record<string, number>[],
  _functionTables: FunctionTableDto[] = [],
): Promise<SolveTableResponse> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

// ---------------------------------------------------------------------------
// Monte Carlo uncertainty — POST /api/solve/montecarlo
// ---------------------------------------------------------------------------

/** Aggregate statistics for one variable across the Monte Carlo samples. */
export interface McVariableStat {
  variable: string
  mean: number
  sigma: number
  p5: number
  p50: number
  p95: number
  firstOrderSigma: number
}

export interface MonteCarloResult {
  stats: McVariableStat[]
  samples: { success: boolean; values: Record<string, number>; error: string | null }[]
  sources: string[]
  requestedSamples: number
  failedSamples: number
  truncated: boolean
}

/** Monte Carlo uncertainty propagation is not ported yet — rejects; the
 *  MonteCarloModal catch shows the message via setError. */
export async function runMonteCarlo(
  _text: string,
  _stopCriteria: StopCriteria,
  _variableInfo: VariableInfo[],
  _displayUnitSystem: UnitSystem,
  _functionTables: FunctionTableDto[],
  _samples: number,
  _seed: number,
): Promise<MonteCarloResult> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

// ---------------------------------------------------------------------------
// PID Tuner (control design) — POST /api/control/pidtune
// ---------------------------------------------------------------------------

export interface PidTuneRequest {
  /** Plant transfer function, descending powers. */
  num: number[]
  den: number[]
  type: 'p' | 'pi' | 'pid'
  /** Target open-loop gain crossover (rad/s) — the "response time" knob. */
  wc?: number
  /** Target phase margin (deg) — the "transient behaviour"/robustness knob. */
  pm?: number
  horizon?: number
  points?: number
}

export interface PidTuneResponse {
  kp: number
  ki: number
  kd: number
  wc: number
  pm: number
  t: number[]
  y: number[]
  riseTime: number
  peakTime: number
  settlingTime: number
  overshoot: number
  gainMargin: number
  phaseMargin: number
}

/** PID tuning (loop shaping + step metrics) is not ported yet — rejects; both
 *  PidTunerModal call sites catch and surface the message via setError. */
export async function pidTune(_req: PidTuneRequest): Promise<PidTuneResponse> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}

// ---------------------------------------------------------------------------
// Plant extraction for the SigPID "Tune…" path — POST /api/control/plant
// ---------------------------------------------------------------------------

export interface PlantRequest {
  text: string
  dynamic: string
  /** Reference SigConstant instance whose constant is perturbed (e.g. "SP"). */
  reference: string
  /** Measured plant-output variable (e.g. "bp.t"). */
  output: string
  /** True when the reference drives the PID's sp input (the common wiring). */
  referenceOnSp: boolean
  type: 'p' | 'pi' | 'pid'
  kp: number
  ki: number
  kd: number
}

/** Plant linearization needs DYNAMIC solving + the CAS, not ported yet —
 *  rejects; the App.tsx catch degrades to "enter the plant manually". */
export async function extractPlant(_req: PlantRequest): Promise<{ num: number[]; den: number[] }> {
  throw new Error(NOT_IN_BROWSER_ENGINE)
}
