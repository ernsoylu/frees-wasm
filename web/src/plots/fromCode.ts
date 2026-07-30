import { PlotDefDto } from '../api'
import { ChartType, PlotKind, PlotSpec, newPlotSpec } from './types'

/** Maps a backend PlotDefDto (raw attribute map from a PLOT ... END block) onto
 * a fully-formed PlotSpec, starting from the per-kind defaults and overriding
 * only the attributes the user supplied. Code plots get a stable name-derived
 * id so re-solving does not churn React keys, and the fromCode flag so the GUI
 * can badge them and avoid persisting them. */
export function plotDefToSpec(dto: PlotDefDto): PlotSpec {
  const attrs = dto.attributes ?? {}
  const first = (key: string): string | undefined => attrs[key]?.[0]
  const all = (key: string): string[] => attrs[key] ?? []
  const bool = (key: string): boolean | undefined => {
    const v = first(key)
    if (v === undefined) return undefined
    return /^(true|yes|on|1)$/i.test(v)
  }
  const num = (key: string): number | undefined => {
    const v = first(key)
    if (v === undefined) return undefined
    const n = Number(v)
    return Number.isFinite(n) ? n : undefined
  }

  const kind = parseKind(first('kind'))
  const spec = newPlotSpec(kind, dto.name)
  spec.id = `code:${dto.name.toLowerCase()}`
  spec.fromCode = true

  if (kind === 'xy') {
    applyXyAttrs(spec, first, all)
  } else if (kind === 'property') {
    applyPropertyAttrs(spec, first, bool)
  } else if (kind === 'psychro') {
    applyPsychroAttrs(spec, num, bool)
  } else {
    applyControlAttrs(spec, first)
  }
  applyFormatAttrs(spec, first, bool, num)

  return spec
}

type StrGet = (key: string) => string | undefined
type StrAll = (key: string) => string[]
type BoolGet = (key: string) => boolean | undefined
type NumGet = (key: string) => number | undefined

function applyXyAttrs(spec: PlotSpec, first: StrGet, all: StrAll): void {
  const xVar = first('x') ?? first('xvar')
  if (xVar) spec.xy.xVar = xVar
  const yVars = all('y').length ? all('y') : all('yvars')
  if (yVars.length) spec.xy.yVars = yVars
  const y2 = all('y2').length ? all('y2') : all('y2vars')
  if (y2.length) spec.xy.y2Vars = y2
  const type = first('type') ?? first('charttype')
  if (type) spec.xy.chartType = parseChartType(type)
  const z = first('z') ?? first('zvar')
  if (z) spec.xy.zVar = z
  const size = first('size') ?? first('sizevar')
  if (size) spec.xy.sizeVar = size
}

function applyPropertyAttrs(spec: PlotSpec, first: StrGet, bool: BoolGet): void {
  if (first('fluid')) spec.property.fluid = first('fluid')!
  if (first('diagram')) spec.property.diagram = first('diagram')!
  assignBool(bool('quality'), (v) => (spec.property.quality = v))
  assignBool(bool('isolines'), (v) => (spec.property.isolines = v))
  assignBool(bool('overlaystates'), (v) => (spec.property.overlayStates = v))
  assignBool(bool('connectstates'), (v) => (spec.property.connectStates = v))
  assignBool(bool('closecycle'), (v) => (spec.property.closeCycle = v))
}

function applyPsychroAttrs(spec: PlotSpec, num: NumGet, bool: BoolGet): void {
  assignNum(num('pressure') ?? num('pressurekpa'), (v) => (spec.psychro.pressureKPa = v))
  assignNum(num('tmin') ?? num('tminc'), (v) => (spec.psychro.tMinC = v))
  assignNum(num('tmax') ?? num('tmaxc'), (v) => (spec.psychro.tMaxC = v))
  assignBool(bool('wetbulb'), (v) => (spec.psychro.wetBulb = v))
  assignBool(bool('enthalpy'), (v) => (spec.psychro.enthalpy = v))
  assignBool(bool('volume'), (v) => (spec.psychro.volume = v))
  assignBool(bool('overlaystates'), (v) => (spec.psychro.overlayStates = v))
  assignBool(bool('connectstates'), (v) => (spec.psychro.connectStates = v))
}

function applyFormatAttrs(spec: PlotSpec, first: StrGet, bool: BoolGet, num: NumGet): void {
  if (first('title')) spec.format.title = first('title')!
  if (first('xlabel')) spec.format.xLabel = first('xlabel')!
  if (first('ylabel')) spec.format.yLabel = first('ylabel')!
  assignBool(bool('xlog'), (v) => (spec.format.xLog = v))
  assignBool(bool('ylog'), (v) => (spec.format.yLog = v))
  assignBool(bool('grid'), (v) => (spec.format.grid = v))
  assignBool(bool('legend'), (v) => (spec.format.legend = v))
  assignNum(num('fontsize'), (v) => (spec.format.fontSize = v))
  assignNum(num('xmin'), (v) => (spec.format.xMin = v))
  assignNum(num('xmax'), (v) => (spec.format.xMax = v))
  assignNum(num('ymin'), (v) => (spec.format.yMin = v))
  assignNum(num('ymax'), (v) => (spec.format.yMax = v))
}

function assignBool(value: boolean | undefined, set: (v: boolean) => void): void {
  if (value !== undefined) set(value)
}

function assignNum(value: number | undefined, set: (v: number) => void): void {
  if (value !== undefined) set(value)
}

function parseKind(raw: string | undefined): PlotKind {
  const v = (raw ?? '').toLowerCase()
  if (v === 'property') return 'property'
  if (v === 'psychro' || v === 'psychrometric') return 'psychro'
  if (v === 'bode') return 'bode'
  if (v === 'nyquist') return 'nyquist'
  if (v === 'nichols') return 'nichols'
  if (v === 'polezero' || v === 'pzmap') return 'polezero'
  if (v === 'rootlocus' || v === 'rlocus') return 'rootlocus'
  return 'xy'
}

function applyControlAttrs(spec: PlotSpec, first: StrGet): void {
  const omega = first('omega')
  if (omega) spec.control.omega = omega
  const mag = first('mag') ?? first('magnitude')
  if (mag) spec.control.mag = mag
  const phase = first('phase')
  if (phase) spec.control.phase = phase
  const real = first('real')
  if (real) spec.control.real = real
  const imag = first('imag') ?? first('imaginary')
  if (imag) spec.control.imag = imag
  const pr = first('pr') ?? first('pole_real') ?? first('poles_real')
  if (pr) spec.control.pr = pr
  const pi = first('pi') ?? first('pole_imag') ?? first('poles_imag')
  if (pi) spec.control.pi = pi
  const zr = first('zr') ?? first('zero_real') ?? first('zeros_real')
  if (zr) spec.control.zr = zr
  const zi = first('zi') ?? first('zero_imag') ?? first('zeros_imag')
  if (zi) spec.control.zi = zi
}

function parseChartType(raw: string): ChartType {
  const v = raw.toLowerCase()
  const allowed: ChartType[] = ['line', 'bar', 'pie', 'histogram', 'scatter', 'surface3d']
  return (allowed as string[]).includes(v) ? (v as ChartType) : 'line'
}
