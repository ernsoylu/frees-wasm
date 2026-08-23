import {
  ActionIcon,
  Alert,
  Badge,
  Button,
  Checkbox,
  ColorSwatch,
  FileButton,
  Group,
  NumberInput,
  Paper,
  ScrollArea,
  SegmentedControl,
  Slider,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
  Tooltip,
} from '@mantine/core'
import {
  IconColorPicker,
  IconCrosshair,
  IconDownload,
  IconEraser,
  IconMathFunction,
  IconPhotoUp,
  IconPlus,
  IconTable,
  IconTrash,
  IconWand,
  IconZoomIn,
  IconZoomOut,
} from '@tabler/icons-react'
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { FunctionTableSpec, TableSpec } from './tables'

// Digitizer → Fit → Function (Wave H): loaded on demand so the fit dialog
// (and the shared curve-fit pieces) stay out of the digitizer chunk.
const DigitizerFitModal = lazy(() => import('./DigitizerFitModal'))

// ---------------------------------------------------------------------------
// Graph Digitizer (Epic 8, Stories 8.1-8.5): turn a chart image into numeric
// curves. Calibrate the axes with two points each (linear or log), then click
// points manually or auto-extract them by color; every curve is a named
// dataset with an optional parameter value (e.g. T = 100). "Send to Curve
// Table" turns the datasets into a tabulated function callable in equations.
// ---------------------------------------------------------------------------

/** Calibrated curves handed to the Tables window. */
export interface DigitizedExport {
  xName: string
  yName: string
  xLog: boolean
  yLog: boolean
  curves: { param: string; points: { x: number; y: number }[] }[]
}

interface CalPoint {
  px: number
  py: number
  raw: string // value as typed (math expressions allowed)
}

interface Calibration {
  x1?: CalPoint
  x2?: CalPoint
  y1?: CalPoint
  y2?: CalPoint
  xLog: boolean
  yLog: boolean
  xName: string
  yName: string
}

interface DigPoint {
  x: number // image pixels (float)
  y: number
}

interface Dataset {
  id: number
  name: string
  /** The curve's color sampled from the image with the picker; null until
   * picked. Drives both the swatch and auto-extraction for this dataset. */
  color: string | null
  param: string // optional curve parameter value, e.g. "100" for T = 100
  points: DigPoint[]
}

type Mode = 'x1' | 'x2' | 'y1' | 'y2' | 'add' | 'select' | 'pick' | 'mask'

interface MaskRect {
  x0: number
  y0: number
  x1: number
  y1: number
}

const DATASET_COLORS = [
  '#fa5252',
  '#4dabf7',
  '#40c057',
  '#fab005',
  '#be4bdb',
  '#fd7e14',
  '#15aabf',
  '#e64980',
]

const STORAGE_KEY = 'frees-digitizer'

// --- tiny math-expression evaluator (PlotDigitizer-style inputs: 2*10^4, pi)

/** Returns a copy of {@code items} with the matching id removed. */
function removeById<I, T extends { id: I }>(items: T[], id: I): T[] {
  return items.filter((x) => x.id !== id)
}

/** Returns a copy of {@code arr} with the element at {@code index} removed. */
function removeAt<T>(arr: T[], index: number): T[] {
  return arr.filter((_, i) => i !== index)
}

function evalMathExpr(input: string): number {
  const tokens = input.toLowerCase().match(/\d+\.?\d*(?:e[+-]?\d+)?|pi|e|[+\-*/^()]/g)
  if (!tokens || tokens.join('') !== input.toLowerCase().replace(/\s+/g, '')) {
    return Number.NaN
  }
  let pos = 0
  const peek = () => tokens[pos]
  const next = () => tokens[pos++]
  function parsePrimary(): number {
    const t = next()
    if (t === '(') {
      const v = parseAddSub()
      if (next() !== ')') return Number.NaN
      return v
    }
    if (t === '-') return -parsePrimary()
    if (t === '+') return parsePrimary()
    if (t === 'pi') return Math.PI
    if (t === 'e') return Math.E
    const n = Number(t)
    return Number.isFinite(n) ? n : Number.NaN
  }
  function parsePower(): number {
    const base = parsePrimary()
    if (peek() === '^') {
      next()
      return Math.pow(base, parsePower())
    }
    return base
  }
  function parseMulDiv(): number {
    let v = parsePower()
    while (peek() === '*' || peek() === '/') {
      v = next() === '*' ? v * parsePower() : v / parsePower()
    }
    return v
  }
  function parseAddSub(): number {
    let v = parseMulDiv()
    while (peek() === '+' || peek() === '-') {
      v = next() === '+' ? v + parseMulDiv() : v - parseMulDiv()
    }
    return v
  }
  const result = parseAddSub()
  return pos === tokens.length ? result : Number.NaN
}

// --- pixel <-> value mapping (general affine form supports tilted scans)

function ratioToValue(r: number, a: number, b: number, log: boolean): number {
  return log
    ? Math.pow(10, r * (Math.log10(b) - Math.log10(a)) + Math.log10(a))
    : r * (b - a) + a
}

interface ResolvedCal {
  xa: number
  ya: number
  xb: number
  yb: number
  a: number
  b: number
  xc: number
  yc: number
  xd: number
  yd: number
  c: number
  d: number
  xLog: boolean
  yLog: boolean
}

function resolveCalibration(cal: Calibration): ResolvedCal | null {
  const { x1, x2, y1, y2 } = cal
  if (!x1 || !x2 || !y1 || !y2) return null
  const [a, b, c, d] = [x1.raw, x2.raw, y1.raw, y2.raw].map(evalMathExpr)
  if ([a, b, c, d].some((v) => !Number.isFinite(v)) || a === b || c === d) return null
  if (cal.xLog && (a <= 0 || b <= 0)) return null
  if (cal.yLog && (c <= 0 || d <= 0)) return null
  return {
    xa: x1.px, ya: x1.py, xb: x2.px, yb: x2.py, a, b,
    xc: y1.px, yc: y1.py, xd: y2.px, yd: y2.py, c, d,
    xLog: cal.xLog, yLog: cal.yLog,
  }
}

/** Image pixel -> data value; the r/s solve handles tilted axes too. */
function pxToValue(rc: ResolvedCal, xt: number, yt: number): { x: number; y: number } | null {
  const xab = rc.xb - rc.xa
  const yab = rc.yb - rc.ya
  const xcd = rc.xd - rc.xc
  const ycd = rc.yd - rc.yc
  const det = yab * xcd - xab * ycd
  if (Math.abs(det) < 1e-9) return null
  const r = ((yt - rc.ya) * xcd - (xt - rc.xa) * ycd) / det
  const s = ((yt - rc.yc) * xab - (xt - rc.xc) * yab) / -det
  return {
    x: ratioToValue(r, rc.a, rc.b, rc.xLog),
    y: ratioToValue(s, rc.c, rc.d, rc.yLog),
  }
}

// --- color-based auto extraction (ports of the starry-digitizer strategies)

/**
 * Perceptual "redmean" color distance: plain RGB distance treats a red
 * curve, a magenta curve, and red scatter markers as near-identical;
 * redmean weights the channels by human sensitivity and separates chart
 * colors much more reliably (the classic low-cost CIELAB approximation).
 */
function colorMatches(
  data: Uint8ClampedArray,
  idx: number,
  target: [number, number, number],
  threshold: number,
): boolean {
  if (data[idx + 3] <= 64) return false
  const rMean = (data[idx] + target[0]) / 2
  const dr = data[idx] - target[0]
  const dg = data[idx + 1] - target[1]
  const db = data[idx + 2] - target[2]
  const dist = Math.sqrt(
    (2 + rMean / 256) * dr * dr + 4 * dg * dg + (2 + (255 - rMean) / 256) * db * db,
  )
  return dist <= threshold * 2
}

/**
 * Rejects extraction outliers in line mode: a curve is a continuous path,
 * so cluster centroids far from the local moving median of y (legend
 * swatches, stray markers of a similar color) are dropped.
 */
function filterByContinuity(points: DigPoint[]): DigPoint[] {
  if (points.length < 8) return points
  const sorted = [...points].sort((p, q) => p.x - q.x)
  const half = 4
  const kept: DigPoint[] = []
  for (let i = 0; i < sorted.length; i++) {
    const lo = Math.max(0, i - half)
    const hi = Math.min(sorted.length, i + half + 1)
    const window = sorted.slice(lo, hi).map((p) => p.y).sort((a, b) => a - b)
    const median = window[Math.floor(window.length / 2)]
    const deviations = window.map((y) => Math.abs(y - median)).sort((a, b) => a - b)
    const mad = deviations[Math.floor(deviations.length / 2)]
    if (Math.abs(sorted[i].y - median) <= Math.max(6, 4 * mad)) {
      kept.push(sorted[i])
    }
  }
  return kept
}

interface ExtractOptions {
  kind: 'line' | 'symbol'
  target: [number, number, number]
  threshold: number
  minDiameterPx: number
  maxDiameterPx: number
  mask: MaskRect | null
}

/**
 * Flood-fill clustering over color-matched pixels. Line mode scans columns
 * left-to-right and emits each cluster's centroid (one pass per curve
 * segment); symbol mode emits centroids of marker-sized blobs only.
 */
function extractByColor(image: ImageData, opts: ExtractOptions): DigPoint[] {
  const { width, height, data } = image
  const visited = new Uint8Array(width * height)
  if (opts.mask) {
    applyMask(visited, width, height, opts.mask)
  }
  const points: DigPoint[] = []
  for (let w = 0; w < width; w++) {
    for (let h = 0; h < height; h++) {
      const point = clusterPointAt(data, visited, width, height, w, h, opts)
      if (point) points.push(point)
    }
  }
  return points
}

/** Marks every pixel outside the mask rectangle as visited so it is skipped. */
function applyMask(visited: Uint8Array, width: number, height: number,
                   mask: { x0: number; y0: number; x1: number; y1: number }): void {
  const { x0, y0, x1, y1 } = mask
  for (let h = 0; h < height; h++) {
    for (let w = 0; w < width; w++) {
      if (w < x0 || w > x1 || h < y0 || h > y1) visited[h * width + w] = 1
    }
  }
}

/** Visits the matching cluster seeded at (w, h) and returns its centroid, or null
 *  when the seed is already visited / doesn't match / fails the symbol-size gate. */
function clusterPointAt(data: Uint8ClampedArray, visited: Uint8Array, width: number, height: number,
                        w: number, h: number, opts: ExtractOptions): DigPoint | null {
  const i = h * width + w
  if (visited[i]) return null
  visited[i] = 1
  if (!colorMatches(data, i * 4, opts.target, opts.threshold)) return null
  const { sumX, sumY, count } = growCluster(data, visited, width, height, w, h, opts)
  if (opts.kind === 'symbol') {
    const diameter = Math.sqrt(count / Math.PI) * 2
    if (diameter < opts.minDiameterPx || diameter > opts.maxDiameterPx) return null
  }
  return { x: sumX / count + 0.5, y: sumY / count + 0.5 }
}

/** BFS over the connected matching cluster seeded at (seedW, seedH); returns the
 *  pixel-coordinate sums and count used to compute the centroid. */
function growCluster(data: Uint8ClampedArray, visited: Uint8Array, width: number, height: number,
                     seedW: number, seedH: number, opts: ExtractOptions): { sumX: number; sumY: number; count: number } {
  const queue: number[] = [seedH * width + seedW]
  let sumX = 0
  let sumY = 0
  let count = 0
  while (queue.length > 0) {
    const ci = queue.pop() as number
    const cw = ci % width
    const ch = (ci - cw) / width
    sumX += cw
    sumY += ch
    count++
    enqueueMatchingNeighbors(data, visited, width, height, cw, ch, seedW, seedH, opts, queue)
  }
  return { sumX, sumY, count }
}

/** Enqueues the unvisited, matching 8-connected neighbours of (cw, ch). */
function enqueueMatchingNeighbors(data: Uint8ClampedArray, visited: Uint8Array, width: number, height: number,
                                  cw: number, ch: number, seedW: number, seedH: number,
                                  opts: ExtractOptions, queue: number[]): void {
  for (let nh = ch - 1; nh <= ch + 1; nh++) {
    for (let nw = cw - 1; nw <= cw + 1; nw++) {
      tryEnqueueNeighbor(data, visited, width, height, nw, nh, seedW, seedH, opts, queue)
    }
  }
}

function tryEnqueueNeighbor(data: Uint8ClampedArray, visited: Uint8Array, width: number, height: number,
                            nw: number, nh: number, seedW: number, seedH: number,
                            opts: ExtractOptions, queue: number[]): void {
  if (nh < 0 || nw < 0 || nh >= height || nw >= width) return
  // line mode: keep clusters local so one long curve becomes many column-wise
  // centroids instead of a single blob
  if (opts.kind === 'line' && (Math.abs(nw - seedW) > 10 || Math.abs(nh - seedH) > 10)) return
  const ni = nh * width + nw
  if (visited[ni]) return
  visited[ni] = 1
  if (colorMatches(data, ni * 4, opts.target, opts.threshold)) {
    queue.push(ni)
  }
}

/** Evenly thin a curve to ~target points by bucket-averaging along X. */
function resampleByX(points: DigPoint[], target: number): DigPoint[] {
  if (target <= 0 || points.length <= target) return points
  const sorted = [...points].sort((p, q) => p.x - q.x)
  const minX = sorted[0].x
  const maxX = sorted.at(-1)!.x
  const span = maxX - minX || 1
  const buckets: DigPoint[][] = Array.from({ length: target }, () => [])
  for (const p of sorted) {
    const bi = Math.min(target - 1, Math.floor(((p.x - minX) / span) * target))
    buckets[bi].push(p)
  }
  return buckets
    .filter((b) => b.length > 0)
    .map((b) => ({
      x: b.reduce((s, p) => s + p.x, 0) / b.length,
      y: b.reduce((s, p) => s + p.y, 0) / b.length,
    }))
}

// --- persistence

interface SavedState {
  calibration: Calibration
  datasets: Dataset[]
  imageDataUrl: string | null
}

function loadSaved(): SavedState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? (JSON.parse(raw) as SavedState) : null
  } catch {
    return null
  }
}

// --- formatting

function fmt(v: number): string {
  if (!Number.isFinite(v)) return 'NaN'
  const abs = Math.abs(v)
  if (abs !== 0 && (abs >= 1e6 || abs < 1e-4)) return v.toExponential(5)
  return Number.parseFloat(v.toPrecision(6)).toString()
}

const CAL_LABELS: Record<string, string> = {
  x1: 'X1', x2: 'X2', y1: 'Y1', y2: 'Y2',
}

const MAG_SIZE = 170
const MAG_ZOOM = 5

let nextDatasetId = 1

function newDataset(existing: Dataset[]): Dataset {
  const id = nextDatasetId++
  return {
    id,
    name: `Curve ${existing.length + 1}`,
    color: null,
    param: '',
    points: [],
  }
}

/** Points are rendered in the picked color; unpicked datasets fall back to
 * a palette color so several curves stay distinguishable on the canvas. */
function renderColor(ds: Dataset, index: number): string {
  return ds.color ?? DATASET_COLORS[index % DATASET_COLORS.length]
}

function rgbToHex(rgb: [number, number, number]): string {
  return `#${rgb.map((c) => c.toString(16).padStart(2, '0')).join('')}`
}

function hexToRgb(hex: string): [number, number, number] {
  return [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
  ]
}

/** Draws the four calibration crosshairs (X red, Y green) at their picked pixels. */
function drawCalibrationMarks(ctx: CanvasRenderingContext2D, calibration: Calibration, scale: number): void {
  const cross = (px: number, py: number, color: string, label: string) => {
    const cx = px * scale
    const cy = py * scale
    ctx.strokeStyle = color
    ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(cx - 8, cy)
    ctx.lineTo(cx + 8, cy)
    ctx.moveTo(cx, cy - 8)
    ctx.lineTo(cx, cy + 8)
    ctx.stroke()
    ctx.fillStyle = color
    ctx.font = 'bold 11px sans-serif'
    ctx.fillText(label, cx + 5, cy - 5)
  }
  for (const key of ['x1', 'x2', 'y1', 'y2'] as const) {
    const cp = calibration[key]
    if (cp) cross(cp.px, cp.py, key.startsWith('x') ? '#ff6b6b' : '#51cf66', CAL_LABELS[key])
  }
}

/** Draws each in-view dataset point magnified into the loupe canvas. */
function drawMagnifiedPoints(ctx: CanvasRenderingContext2D, datasets: Dataset[],
                            toMag: (p: DigPoint) => { x: number; y: number },
                            inView: (m: { x: number; y: number }) => boolean): void {
  for (let di = 0; di < datasets.length; di++) {
    const ds = datasets[di]
    ctx.fillStyle = renderColor(ds, di)
    for (const p of ds.points) {
      const m = toMag(p)
      if (!inView(m)) continue
      ctx.beginPath()
      ctx.arc(m.x, m.y, 4, 0, Math.PI * 2)
      ctx.fill()
    }
  }
}

/** Draws the in-view calibration crosshairs magnified into the loupe canvas. */
function drawMagnifiedCalibration(ctx: CanvasRenderingContext2D, calibration: Calibration,
                                  toMag: (p: DigPoint) => { x: number; y: number },
                                  inView: (m: { x: number; y: number }) => boolean): void {
  for (const key of ['x1', 'x2', 'y1', 'y2'] as const) {
    const cp = calibration[key]
    if (!cp) continue
    const m = toMag({ x: cp.px, y: cp.py })
    if (!inView(m)) continue
    ctx.strokeStyle = key.startsWith('x') ? '#ff6b6b' : '#51cf66'
    ctx.lineWidth = 1.5
    ctx.beginPath()
    ctx.moveTo(m.x - 7, m.y)
    ctx.lineTo(m.x + 7, m.y)
    ctx.moveTo(m.x, m.y - 7)
    ctx.lineTo(m.x, m.y + 7)
    ctx.stroke()
    ctx.fillStyle = ctx.strokeStyle
    ctx.font = 'bold 10px sans-serif'
    ctx.fillText(CAL_LABELS[key], m.x + 4, m.y - 4)
  }
}

/** Draws every dataset's digitized points, highlighting the selected one. */
function drawDatasetPoints(ctx: CanvasRenderingContext2D, datasets: Dataset[], scale: number,
                           selected: { datasetId: number; index: number } | null, activeDatasetId: number | null): void {
  for (let di = 0; di < datasets.length; di++) {
    const ds = datasets[di]
    for (let i = 0; i < ds.points.length; i++) {
      const p = ds.points[i]
      const isSel = selected?.datasetId === ds.id && selected.index === i
      ctx.beginPath()
      ctx.arc(p.x * scale, p.y * scale, isSel ? 6 : 4, 0, Math.PI * 2)
      ctx.fillStyle = renderColor(ds, di)
      ctx.globalAlpha = ds.id === activeDatasetId ? 0.95 : 0.55
      ctx.fill()
      ctx.globalAlpha = 1
      if (isSel) {
        ctx.strokeStyle = '#ffffff'
        ctx.lineWidth = 1.5
        ctx.stroke()
      }
    }
  }
}

/** Draws the dashed mask rectangle, if one is set or being dragged. */
function drawMaskRect(ctx: CanvasRenderingContext2D, rect: MaskRect | null, scale: number): void {
  if (!rect) return
  ctx.strokeStyle = '#fcc419'
  ctx.setLineDash([6, 4])
  ctx.lineWidth = 1.5
  ctx.strokeRect(
    rect.x0 * scale,
    rect.y0 * scale,
    (rect.x1 - rect.x0) * scale,
    (rect.y1 - rect.y0) * scale,
  )
  ctx.setLineDash([])
}

export function DigitizerTab({
  onSendToFunctionTable,
  tables,
  onInsertEquation,
  onCreateFunctionTable,
}: Readonly<{
  onSendToFunctionTable?: (data: DigitizedExport) => void
  /** For function-name collision checks in the fit dialog (Wave H). */
  tables?: TableSpec[]
  /** Insert the fitted analytic form into the editor (Wave H). */
  onInsertEquation?: (eq: string) => void
  /** Add a sampled fitted curve as a Function Table (Wave H). */
  onCreateFunctionTable?: (spec: FunctionTableSpec) => void
}>) {
  const saved = useMemo(() => loadSaved(), [])
  const [image, setImage] = useState<HTMLImageElement | null>(null)
  const [imageDataUrl, setImageDataUrl] = useState<string | null>(saved?.imageDataUrl ?? null)
  const [scale, setScale] = useState(1)
  const [mode, setMode] = useState<Mode>('x1')
  const [calibration, setCalibration] = useState<Calibration>(
    saved?.calibration ?? { xLog: false, yLog: false, xName: 'X', yName: 'Y' },
  )
  const [datasets, setDatasets] = useState<Dataset[]>(saved?.datasets ?? [])
  const [activeDatasetId, setActiveDatasetId] = useState<number | null>(
    saved?.datasets?.[0]?.id ?? null,
  )
  const [selected, setSelected] = useState<{ datasetId: number; index: number } | null>(null)
  const [cursor, setCursor] = useState<DigPoint | null>(null)
  const [maskRect, setMaskRect] = useState<MaskRect | null>(null)
  const [maskDraft, setMaskDraft] = useState<MaskRect | null>(null)
  const [threshold, setThreshold] = useState(60)
  const [algorithm, setAlgorithm] = useState<'line' | 'symbol'>('line')
  const [minDia, setMinDia] = useState(4)
  const [maxDia, setMaxDia] = useState(60)
  const [resampleCount, setResampleCount] = useState(40)
  const [showPreview, setShowPreview] = useState(true)
  const [extracting, setExtracting] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  // Digitizer → Fit → Function (Wave H): the fit dialog's frozen input —
  // the active dataset's calibrated values at the moment Fit curve… opened.
  const [fitTarget, setFitTarget] = useState<{
    datasetName: string
    points: { x: number; y: number }[]
  } | null>(null)

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const magRef = useRef<HTMLCanvasElement>(null)
  const dragRef = useRef<{ datasetId: number; index: number } | null>(null)

  useEffect(() => {
    if (datasets.length > 0) {
      nextDatasetId = Math.max(nextDatasetId, ...datasets.map((d) => d.id + 1))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Restore the saved image once on mount.
  useEffect(() => {
    if (imageDataUrl && !image) {
      const img = new Image()
      img.onload = () => setImage(img)
      img.src = imageDataUrl
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [imageDataUrl])

  // Persist (image data URLs from book scans fit comfortably in localStorage;
  // on quota overflow the state is kept without the image).
  useEffect(() => {
    const handle = setTimeout(() => {
      const state: SavedState = { calibration, datasets, imageDataUrl }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
      } catch {
        try {
          localStorage.setItem(
            STORAGE_KEY,
            JSON.stringify({ ...state, imageDataUrl: null }),
          )
        } catch {
          // storage unavailable: digitizer still works, just not persisted
        }
      }
    }, 500)
    return () => clearTimeout(handle)
  }, [calibration, datasets, imageDataUrl])

  const resolved = useMemo(() => resolveCalibration(calibration), [calibration])
  const activeDataset = datasets.find((d) => d.id === activeDatasetId) ?? null
  // Auto-extraction targets the active dataset's picked color.
  const targetColor = activeDataset?.color ? hexToRgb(activeDataset.color) : null

  // Live overlay of the pixels the current color + threshold would match,
  // so the user can tune the picker before extracting.
  const targetColorKey = activeDataset?.color ?? null
  const previewCanvas = useMemo(() => {
    if (!image || !showPreview || !targetColorKey) return null
    const target = hexToRgb(targetColorKey)
    const off = document.createElement('canvas')
    off.width = image.width
    off.height = image.height
    const ctx = off.getContext('2d')
    if (!ctx) return null
    ctx.drawImage(image, 0, 0)
    const src = ctx.getImageData(0, 0, image.width, image.height)
    const overlay = ctx.createImageData(image.width, image.height)
    for (let i = 0; i < src.data.length; i += 4) {
      if (colorMatches(src.data, i, target, threshold)) {
        overlay.data[i] = 0
        overlay.data[i + 1] = 255
        overlay.data[i + 2] = 255
        overlay.data[i + 3] = 170
      }
    }
    ctx.clearRect(0, 0, off.width, off.height)
    ctx.putImageData(overlay, 0, 0)
    return off
  }, [image, showPreview, targetColorKey, threshold])

  const loadImageFile = useCallback((file: File | null) => {
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      const url = reader.result as string
      const img = new Image()
      img.onload = () => {
        setImage(img)
        setImageDataUrl(url)
        setScale(1)
        setNotice(null)
      }
      img.src = url
    }
    reader.readAsDataURL(file)
  }, [])

  // Clipboard paste: book scans usually arrive as screenshots.
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const item = Array.from(e.clipboardData?.items ?? []).find((i) =>
        i.type.startsWith('image/'),
      )
      if (item) loadImageFile(item.getAsFile())
    }
    window.addEventListener('paste', onPaste)
    return () => window.removeEventListener('paste', onPaste)
  }, [loadImageFile])

  // --- canvas drawing

  const draw = useCallback(() => {
    const canvas = canvasRef.current
    if (!canvas || !image) return
    canvas.width = image.width * scale
    canvas.height = image.height * scale
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.imageSmoothingEnabled = scale < 2
    ctx.drawImage(image, 0, 0, canvas.width, canvas.height)
    if (previewCanvas) {
      ctx.drawImage(previewCanvas, 0, 0, canvas.width, canvas.height)
    }

    drawCalibrationMarks(ctx, calibration, scale)
    drawDatasetPoints(ctx, datasets, scale, selected, activeDatasetId)
    drawMaskRect(ctx, maskDraft ?? maskRect, scale)
  }, [image, scale, calibration, datasets, selected, activeDatasetId, maskRect, maskDraft, previewCanvas])

  useEffect(draw, [draw])

  const drawMagnifier = useCallback(
    (pt: DigPoint | null) => {
      const mag = magRef.current
      if (!mag) return
      const ctx = mag.getContext('2d')
      if (!ctx) return
      ctx.fillStyle = '#1a1b1e'
      ctx.fillRect(0, 0, MAG_SIZE, MAG_SIZE)
      if (!image || !pt) return
      const region = MAG_SIZE / MAG_ZOOM
      const originX = pt.x - region / 2
      const originY = pt.y - region / 2
      ctx.imageSmoothingEnabled = false
      ctx.drawImage(image, originX, originY, region, region, 0, 0, MAG_SIZE, MAG_SIZE)

      // Placed markers are part of the precision loop: show them magnified.
      const toMag = (p: DigPoint) => ({
        x: (p.x - originX) * MAG_ZOOM,
        y: (p.y - originY) * MAG_ZOOM,
      })
      const inView = (m: { x: number; y: number }) =>
        m.x >= -10 && m.x <= MAG_SIZE + 10 && m.y >= -10 && m.y <= MAG_SIZE + 10
      drawMagnifiedPoints(ctx, datasets, toMag, inView)
      drawMagnifiedCalibration(ctx, calibration, toMag, inView)

      ctx.strokeStyle = 'rgba(255, 107, 107, 0.9)'
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.moveTo(MAG_SIZE / 2, 0)
      ctx.lineTo(MAG_SIZE / 2, MAG_SIZE)
      ctx.moveTo(0, MAG_SIZE / 2)
      ctx.lineTo(MAG_SIZE, MAG_SIZE / 2)
      ctx.stroke()
    },
    [image, datasets, calibration],
  )

  useEffect(() => drawMagnifier(cursor), [cursor, drawMagnifier])

  // --- interactions

  const eventToImagePx = (e: React.MouseEvent<HTMLCanvasElement>): DigPoint => {
    const rect = (e.target as HTMLCanvasElement).getBoundingClientRect()
    return {
      x: (e.clientX - rect.left) / scale,
      y: (e.clientY - rect.top) / scale,
    }
  }

  const nearestPoint = (pt: DigPoint): { datasetId: number; index: number } | null => {
    let best: { datasetId: number; index: number } | null = null
    let bestDist = 10 / scale
    for (const ds of datasets) {
      for (let i = 0; i < ds.points.length; i++) {
        const d = Math.hypot(ds.points[i].x - pt.x, ds.points[i].y - pt.y)
        if (d < bestDist) {
          bestDist = d
          best = { datasetId: ds.id, index: i }
        }
      }
    }
    return best
  }

  const updateDataset = (id: number, update: (ds: Dataset) => Dataset) => {
    setDatasets((all) => all.map((d) => (d.id === id ? update(d) : d)))
  }

  /** Samples a 3x3 region average — a single pixel on an anti-aliased
   * curve often lands on a blended edge color and mismatches the line. */
  const sampleColor = (pt: DigPoint): [number, number, number] | null => {
    if (!image) return null
    const off = document.createElement('canvas')
    off.width = image.width
    off.height = image.height
    const ctx = off.getContext('2d')
    if (!ctx) return null
    ctx.drawImage(image, 0, 0)
    const x = Math.max(1, Math.min(image.width - 2, Math.round(pt.x)))
    const y = Math.max(1, Math.min(image.height - 2, Math.round(pt.y)))
    const d = ctx.getImageData(x - 1, y - 1, 3, 3).data
    let r = 0
    let g = 0
    let b = 0
    for (let i = 0; i < 9; i++) {
      r += d[i * 4]
      g += d[i * 4 + 1]
      b += d[i * 4 + 2]
    }
    return [Math.round(r / 9), Math.round(g / 9), Math.round(b / 9)]
  }

  const removePointAt = (pt: { x: number; y: number }) => {
    const hit = nearestPoint(pt)
    if (hit) {
      updateDataset(hit.datasetId, (ds) => ({ ...ds, points: removeAt(ds.points, hit.index) }))
      if (selected?.datasetId === hit.datasetId) setSelected(null)
    }
  }

  const placeCalibration = (key: 'x1' | 'x2' | 'y1' | 'y2', pt: { x: number; y: number }) => {
    setCalibration((c) => ({ ...c, [key]: { px: pt.x, py: pt.y, raw: c[key]?.raw ?? '' } }))
    const order: Mode[] = ['x1', 'x2', 'y1', 'y2']
    const idx = order.indexOf(key)
    setMode(idx < 3 ? order[idx + 1] : 'add')
  }

  const pickColorAt = (pt: { x: number; y: number }) => {
    const c = sampleColor(pt)
    if (!c) return
    if (activeDataset) {
      updateDataset(activeDataset.id, (ds) => ({ ...ds, color: rgbToHex(c) }))
    } else {
      setNotice('Add a dataset first, then pick its curve color.')
    }
    setMode('add')
  }

  const onCanvasMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const pt = eventToImagePx(e)
    // Ctrl+Click removes the point under the cursor in any mode — the
    // quick fix for stray auto-extracted points.
    if (e.ctrlKey || e.metaKey) {
      removePointAt(pt)
      return
    }
    if (mode === 'x1' || mode === 'x2' || mode === 'y1' || mode === 'y2') {
      placeCalibration(mode, pt)
      return
    }
    if (mode === 'pick') {
      pickColorAt(pt)
      return
    }
    if (mode === 'mask') {
      setMaskDraft({ x0: pt.x, y0: pt.y, x1: pt.x, y1: pt.y })
      return
    }
    if (mode === 'select') {
      const hit = nearestPoint(pt)
      setSelected(hit)
      dragRef.current = hit
      return
    }
    // add mode
    if (!activeDataset) {
      setNotice('Add a dataset first — every curve on the plot gets its own dataset.')
      return
    }
    updateDataset(activeDataset.id, (ds) => ({ ...ds, points: [...ds.points, pt] }))
  }

  const onCanvasMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const pt = eventToImagePx(e)
    setCursor(pt)
    if (mode === 'mask' && maskDraft) {
      setMaskDraft({ ...maskDraft, x1: pt.x, y1: pt.y })
      return
    }
    if (mode === 'select' && dragRef.current && e.buttons === 1) {
      const { datasetId, index } = dragRef.current
      updateDataset(datasetId, (ds) => {
        const points = [...ds.points]
        points[index] = pt
        return { ...ds, points }
      })
    }
  }

  const onCanvasMouseUp = () => {
    dragRef.current = null
    if (mode === 'mask' && maskDraft) {
      const normalized = {
        x0: Math.min(maskDraft.x0, maskDraft.x1),
        y0: Math.min(maskDraft.y0, maskDraft.y1),
        x1: Math.max(maskDraft.x0, maskDraft.x1),
        y1: Math.max(maskDraft.y0, maskDraft.y1),
      }
      setMaskRect(normalized.x1 - normalized.x0 > 4 ? normalized : null)
      setMaskDraft(null)
      setMode('add')
    }
  }

  // Delete key removes the selected point.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && selected) {
        const target = e.target as HTMLElement
        if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return
        updateDataset(selected.datasetId, (ds) => ({
          ...ds,
          points: removeAt(ds.points, selected.index),
        }))
        setSelected(null)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [selected])

  // --- auto extraction

  const runAutoExtract = () => {
    if (!image || !activeDataset) {
      setNotice(image ? 'Add a dataset first.' : 'Load a graph image first.')
      return
    }
    if (!targetColor) {
      setNotice(`Pick ${activeDataset.name}'s curve color from the image first.`)
      setMode('pick')
      return
    }
    const target = targetColor
    setExtracting(true)
    // Defer so the spinner renders before the synchronous pixel scan.
    setTimeout(() => {
      try {
        const off = document.createElement('canvas')
        off.width = image.width
        off.height = image.height
        const ctx = off.getContext('2d')
        if (!ctx) return
        ctx.drawImage(image, 0, 0)
        const data = ctx.getImageData(0, 0, image.width, image.height)
        let points = extractByColor(data, {
          kind: algorithm,
          target,
          threshold,
          minDiameterPx: minDia,
          maxDiameterPx: maxDia,
          mask: maskRect,
        })
        if (algorithm === 'line') {
          points = resampleByX(filterByContinuity(points), resampleCount)
        }
        if (points.length === 0) {
          setNotice(
            'No matching pixels found — pick the curve color from the image and loosen the threshold.',
          )
        } else {
          setNotice(null)
          updateDataset(activeDataset.id, (ds) => ({ ...ds, points }))
        }
      } finally {
        setExtracting(false)
      }
    }, 30)
  }

  // --- export

  /** Calibrated values sorted by x, each keeping its point index so list
   * rows can select and delete the underlying point. */
  const datasetEntries = (ds: Dataset): { x: number; y: number; index: number }[] => {
    if (!resolved) return []
    return ds.points
      .map((p, index) => {
        const v = pxToValue(resolved, p.x, p.y)
        return v ? { x: v.x, y: v.y, index } : null
      })
      .filter((v): v is { x: number; y: number; index: number } => v !== null)
      .sort((p, q) => p.x - q.x)
  }

  const datasetValues = (ds: Dataset): { x: number; y: number }[] =>
    datasetEntries(ds).map(({ x, y }) => ({ x, y }))

  const download = (filename: string, content: string, type: string) => {
    const blob = new Blob([content], { type })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
  }

  const exportCsv = () => {
    if (!resolved) {
      setNotice('Calibrate all four axis points before exporting.')
      return
    }
    const lines: string[] = []
    for (const ds of datasets) {
      lines.push(`# ${ds.name}${ds.param ? ` (param = ${ds.param})` : ''}`)
      lines.push(`${calibration.xName},${calibration.yName}`)
      for (const v of datasetValues(ds)) {
        lines.push(`${fmt(v.x)},${fmt(v.y)}`)
      }
      lines.push('')
    }
    download('digitized-curves.csv', lines.join('\n'), 'text/csv')
  }

  const exportJson = () => {
    if (!resolved) {
      setNotice('Calibrate all four axis points before exporting.')
      return
    }
    const payload = {
      xName: calibration.xName,
      yName: calibration.yName,
      xLog: calibration.xLog,
      yLog: calibration.yLog,
      curves: datasets.map((ds) => ({
        name: ds.name,
        param: ds.param === '' ? null : evalMathExpr(ds.param),
        points: datasetValues(ds).map((v) => [v.x, v.y]),
      })),
    }
    download('digitized-curves.json', JSON.stringify(payload, null, 2), 'application/json')
  }

  const sendToFunctionTable = () => {
    if (!resolved) {
      setNotice('Calibrate all four axis points before sending to a Function Table.')
      return
    }
    const curves = datasets
      .map((ds) => ({ param: ds.param, points: datasetValues(ds) }))
      .filter((c) => c.points.length > 0)
    if (curves.length === 0) {
      setNotice('Digitize at least one curve first.')
      return
    }
    onSendToFunctionTable?.({
      xName: calibration.xName,
      yName: calibration.yName,
      xLog: calibration.xLog,
      yLog: calibration.yLog,
      curves,
    })
  }

  const openFitCurve = () => {
    if (!resolved) {
      setNotice('Calibrate all four axis points before fitting a curve.')
      return
    }
    if (!activeDataset) {
      setNotice('Select a dataset to fit — the fit uses the active dataset’s points.')
      return
    }
    const points = datasetValues(activeDataset)
    if (points.length < 2) {
      setNotice(`Digitize at least two points of ${activeDataset.name} first.`)
      return
    }
    setFitTarget({ datasetName: activeDataset.name, points })
  }

  const clearAll = () => {
    setImage(null)
    setImageDataUrl(null)
    setCalibration({ xLog: false, yLog: false, xName: 'X', yName: 'Y' })
    setDatasets([])
    setActiveDatasetId(null)
    setSelected(null)
    setMaskRect(null)
    setNotice(null)
    try {
      localStorage.removeItem(STORAGE_KEY)
    } catch {
      // ignore
    }
  }

  const cursorValue = resolved && cursor ? pxToValue(resolved, cursor.x, cursor.y) : null

  const modeOptions = [
    { label: 'X1', value: 'x1' },
    { label: 'X2', value: 'x2' },
    { label: 'Y1', value: 'y1' },
    { label: 'Y2', value: 'y2' },
    { label: 'Add', value: 'add' },
    { label: 'Select', value: 'select' },
    { label: 'Mask', value: 'mask' },
  ]

  return (
    <Group align="stretch" gap="sm" style={{ flex: 1, minHeight: 0, overflow: 'hidden' }} wrap="nowrap">
      {/* --- canvas area --- */}
      <Stack gap="xs" style={{ flex: 1, minWidth: 0, minHeight: 0 }}>
        <Group justify="space-between" wrap="nowrap">
          <Group gap="xs" wrap="nowrap">
            <FileButton onChange={loadImageFile} accept="image/*">
              {(props) => (
                <Button size="xs" leftSection={<IconPhotoUp size={14} />} {...props}>
                  Load Image
                </Button>
              )}
            </FileButton>
            <Tooltip label="Zoom out">
              <ActionIcon variant="default" size="sm" onClick={() => setScale((s) => Math.max(0.25, s / 1.25))}>
                <IconZoomOut size={14} />
              </ActionIcon>
            </Tooltip>
            <Text size="xs" c="dimmed" w={36} ta="center">
              {Math.round(scale * 100)}%
            </Text>
            <Tooltip label="Zoom in">
              <ActionIcon variant="default" size="sm" onClick={() => setScale((s) => Math.min(8, s * 1.25))}>
                <IconZoomIn size={14} />
              </ActionIcon>
            </Tooltip>
            <SegmentedControl
              size="xs"
              value={mode === 'pick' ? 'add' : mode}
              onChange={(v) => setMode(v as Mode)}
              data={modeOptions}
            />
          </Group>
          <Button size="xs" variant="subtle" color="red" leftSection={<IconEraser size={14} />} onClick={clearAll}>
            Reset
          </Button>
        </Group>

        {notice && (
          <Alert color="yellow" p="xs" withCloseButton onClose={() => setNotice(null)}>
            <Text size="xs">{notice}</Text>
          </Alert>
        )}

        {image ? (
          <ScrollArea style={{ flex: 1, border: '1px solid var(--mantine-color-default-border)', borderRadius: 4 }}>
            <canvas
              ref={canvasRef}
              style={{ display: 'block', cursor: 'crosshair' }}
              onMouseDown={onCanvasMouseDown}
              onMouseMove={onCanvasMouseMove}
              onMouseUp={onCanvasMouseUp}
              onMouseLeave={() => setCursor(null)}
            />
          </ScrollArea>
        ) : (
          <Paper
            withBorder
            style={{
              flex: 1,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              borderStyle: 'dashed',
            }}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault()
              loadImageFile(e.dataTransfer.files?.[0] ?? null)
            }}
          >
            <Stack align="center" gap="xs">
              <IconCrosshair size={40} stroke={1.2} color="var(--mantine-color-dimmed)" />
              <Text c="dimmed" size="sm" ta="center">
                Load a graph image, drop it here, or paste it from the clipboard (Ctrl+V).
                <br />
                Then place X1, X2, Y1, Y2 on the axes and start digitizing curves.
                <br />
                Ctrl+Click removes a point; Select mode drags points; Delete removes the selection.
              </Text>
            </Stack>
          </Paper>
        )}
      </Stack>

      {/* --- side panel --- */}
      <Stack w={340} style={{ flexShrink: 0, height: '100%', overflow: 'hidden' }} gap="sm">
        <Paper withBorder p="xs" style={{ flexShrink: 0 }}>
          <Group justify="space-between" mb={4}>
            <Title order={6}>Magnifier</Title>
            {cursorValue && (
              <Text size="xs" c="dimmed" ff="monospace">
                {fmt(cursorValue.x)}, {fmt(cursorValue.y)}
              </Text>
            )}
          </Group>
          <canvas
            ref={magRef}
            width={MAG_SIZE}
            height={MAG_SIZE}
            style={{ width: '100%', borderRadius: 4, background: 'light-dark(var(--mantine-color-gray-1), var(--mantine-color-dark-8))' }}
          />
        </Paper>

        <ScrollArea style={{ flex: 1, minHeight: 0 }}>
          <Stack gap="sm" pr={6}>
            <Paper withBorder p="xs">
            <Title order={6} mb={6}>
              Axes
            </Title>
            <Group gap={6} grow mb={6}>
              <TextInput
                size="xs"
                label="X axis"
                value={calibration.xName}
                onChange={(e) => {
                  const xName = e.currentTarget.value
                  setCalibration((c) => ({ ...c, xName }))
                }}
              />
              <TextInput
                size="xs"
                label="Y axis"
                value={calibration.yName}
                onChange={(e) => {
                  const yName = e.currentTarget.value
                  setCalibration((c) => ({ ...c, yName }))
                }}
              />
            </Group>
            {(['x1', 'x2', 'y1', 'y2'] as const).map((key) => (
              <Group key={key} gap={6} mb={4} wrap="nowrap">
                <Button
                  size="compact-xs"
                  variant={mode === key ? 'filled' : 'default'}
                  color={calibration[key] ? 'teal' : 'blue'}
                  w={42}
                  onClick={() => setMode(key)}
                >
                  {CAL_LABELS[key]}
                </Button>
                <TextInput
                  size="xs"
                  placeholder={`${CAL_LABELS[key]} value, e.g. 2*10^4`}
                  value={calibration[key]?.raw ?? ''}
                  error={
                    calibration[key]?.raw && !Number.isFinite(evalMathExpr(calibration[key]?.raw ?? ''))
                  }
                  style={{ flex: 1 }}
                  onChange={(e) => {
                    const raw = e.currentTarget.value
                    setCalibration((c) => {
                      const cp = c[key]
                      return { ...c, [key]: cp ? { ...cp, raw } : { px: 0, py: 0, raw } }
                    })
                  }}
                  disabled={!calibration[key]}
                />
              </Group>
            ))}
            <Group gap="md" mt={6}>
              <Checkbox
                size="xs"
                label="log X"
                checked={calibration.xLog}
                onChange={(e) => {
                  const xLog = e.currentTarget.checked
                  setCalibration((c) => ({ ...c, xLog }))
                }}
              />
              <Checkbox
                size="xs"
                label="log Y"
                checked={calibration.yLog}
                onChange={(e) => {
                  const yLog = e.currentTarget.checked
                  setCalibration((c) => ({ ...c, yLog }))
                }}
              />
              <Badge size="xs" color={resolved ? 'teal' : 'gray'} variant="light">
                {resolved ? 'Calibrated' : 'Not calibrated'}
              </Badge>
            </Group>
          </Paper>

          <Paper withBorder p="xs">
            <Group justify="space-between" mb={6}>
              <Title order={6}>Datasets</Title>
              <Button
                size="compact-xs"
                leftSection={<IconPlus size={12} />}
                onClick={() => {
                  const ds = newDataset(datasets)
                  setDatasets((all) => [...all, ds])
                  setActiveDatasetId(ds.id)
                  setMode('add')
                }}
              >
                Add
              </Button>
            </Group>
            <Stack gap={4}>
              {datasets.map((ds) => (
                <Paper
                  key={ds.id}
                  p={4}
                  withBorder
                  style={{
                    borderColor: ds.id === activeDatasetId ? 'var(--mantine-color-teal-7)' : undefined,
                    cursor: 'pointer',
                  }}
                  onClick={() => setActiveDatasetId(ds.id)}
                >
                  <Group gap={6} wrap="nowrap">
                    {ds.color ? (
                      <Tooltip label="Curve color picked from the image">
                        <ColorSwatch color={ds.color} size={14} />
                      </Tooltip>
                    ) : (
                      <Tooltip label="No color yet — use the picker with this dataset active">
                        <IconColorPicker
                          size={14}
                          color="var(--mantine-color-dimmed)"
                          style={{ flexShrink: 0 }}
                        />
                      </Tooltip>
                    )}
                    <TextInput
                      size="xs"
                      variant="unstyled"
                      value={ds.name}
                      style={{ flex: 1 }}
                      onChange={(e) => {
                        const name = e.currentTarget.value
                        updateDataset(ds.id, (d) => ({ ...d, name }))
                      }}
                    />
                    <TextInput
                      size="xs"
                      w={70}
                      placeholder="param"
                      title="Curve parameter value (e.g. the temperature of this curve)"
                      value={ds.param}
                      onChange={(e) => {
                        const param = e.currentTarget.value
                        updateDataset(ds.id, (d) => ({ ...d, param }))
                      }}
                    />
                    <Badge size="xs" variant="light">
                      {ds.points.length}
                    </Badge>
                    <Tooltip label="Reset points (keep the dataset)">
                      <ActionIcon
                        size="xs"
                        variant="subtle"
                        color="yellow"
                        aria-label={`Reset points of ${ds.name}`}
                        onClick={(e) => {
                          e.stopPropagation()
                          updateDataset(ds.id, (d) => ({ ...d, points: [] }))
                          if (selected?.datasetId === ds.id) setSelected(null)
                        }}
                      >
                        <IconEraser size={12} />
                      </ActionIcon>
                    </Tooltip>
                    <ActionIcon
                      size="xs"
                      variant="subtle"
                      color="red"
                      aria-label={`Delete ${ds.name}`}
                      onClick={(e) => {
                        e.stopPropagation()
                        setDatasets((all) => removeById(all, ds.id))
                        if (activeDatasetId === ds.id) setActiveDatasetId(null)
                      }}
                    >
                      <IconTrash size={12} />
                    </ActionIcon>
                  </Group>
                </Paper>
              ))}
              {datasets.length === 0 && (
                <Text size="xs" c="dimmed">
                  One dataset per curve. The param field holds the curve's family value (e.g. T =
                  100) for graphfunc(x, param) lookups later.
                </Text>
              )}
            </Stack>
          </Paper>

          <Paper withBorder p="xs">
            <Title order={6} mb={6}>
              Auto extract
            </Title>
            <Group gap={6} mb={6} wrap="nowrap">
              <Tooltip
                label={
                  activeDataset
                    ? `Pick ${activeDataset.name}'s curve color from the image`
                    : 'Add a dataset first'
                }
              >
                <Button
                  size="compact-xs"
                  variant={mode === 'pick' ? 'filled' : 'default'}
                  leftSection={
                    activeDataset?.color ? (
                      <ColorSwatch color={activeDataset.color} size={12} />
                    ) : undefined
                  }
                  rightSection={<IconColorPicker size={12} />}
                  onClick={() => setMode('pick')}
                >
                  {activeDataset?.color ? 'Color' : 'Pick color'}
                </Button>
              </Tooltip>
              <SegmentedControl
                size="xs"
                value={algorithm}
                onChange={(v) => setAlgorithm(v as 'line' | 'symbol')}
                data={[
                  { label: 'Line trace', value: 'line' },
                  { label: 'Symbols', value: 'symbol' },
                ]}
              />
            </Group>
            <Text size="xs" c="dimmed">
              Color threshold: {threshold}
            </Text>
            <Slider size="xs" min={5} max={150} value={threshold} onChange={setThreshold} mb={6} />
            <Checkbox
              size="xs"
              label="Highlight matching pixels"
              description="Tune color + threshold until only the curve glows"
              checked={showPreview}
              onChange={(e) => {
                const on = e.currentTarget.checked
                setShowPreview(on)
              }}
              mb={6}
            />
            {algorithm === 'line' ? (
              <NumberInput
                size="xs"
                label="Resample to N points (0 = keep all)"
                value={resampleCount}
                onChange={(v) => setResampleCount(Number(v) || 0)}
                min={0}
                max={500}
                mb={6}
              />
            ) : (
              <Group gap={6} grow mb={6}>
                <NumberInput size="xs" label="Min Ø px" value={minDia} onChange={(v) => setMinDia(Number(v) || 1)} min={1} />
                <NumberInput size="xs" label="Max Ø px" value={maxDia} onChange={(v) => setMaxDia(Number(v) || 1)} min={1} />
              </Group>
            )}
            <Group gap={6}>
              <Button
                size="xs"
                leftSection={<IconWand size={14} />}
                loading={extracting}
                onClick={runAutoExtract}
              >
                Extract into {activeDataset?.name ?? 'dataset'}
              </Button>
              {maskRect && (
                <Button size="compact-xs" variant="default" onClick={() => setMaskRect(null)}>
                  Clear mask
                </Button>
              )}
            </Group>
          </Paper>

          <Paper withBorder p="xs">
            <Group justify="space-between" mb={6}>
              <Title order={6}>Points {activeDataset ? `— ${activeDataset.name}` : ''}</Title>
              <Group gap={4}>
                <Button size="compact-xs" variant="default" leftSection={<IconDownload size={12} />} onClick={exportCsv}>
                  CSV
                </Button>
                <Button size="compact-xs" variant="default" leftSection={<IconDownload size={12} />} onClick={exportJson}>
                  JSON
                </Button>
              </Group>
            </Group>
            {onSendToFunctionTable && (
              <Button size="xs" fullWidth mb={6} leftSection={<IconTable size={14} />} onClick={sendToFunctionTable}>
                Send to Function Table
              </Button>
            )}
            {(onInsertEquation || onCreateFunctionTable) && (
              <Tooltip label="Fit a model curve to the active dataset, then insert the equation or send the fit as a Function Table">
                <Button
                  size="xs"
                  fullWidth
                  mb={6}
                  variant="default"
                  leftSection={<IconMathFunction size={14} />}
                  onClick={openFitCurve}
                >
                  Fit curve…
                </Button>
              </Tooltip>
            )}
            {activeDataset && resolved ? (
              <Table withTableBorder striped highlightOnHover stickyHeader>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>
                      <Text size="xs">{calibration.xName}</Text>
                    </Table.Th>
                    <Table.Th>
                      <Text size="xs">{calibration.yName}</Text>
                    </Table.Th>
                    <Table.Th w={26} />
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {datasetEntries(activeDataset).map((v) => (
                    <Table.Tr
                      key={`pt-${v.index}`}
                      bg={
                        selected?.datasetId === activeDataset.id && selected.index === v.index
                          ? 'dark.5'
                          : undefined
                      }
                      style={{ cursor: 'pointer' }}
                      onClick={() =>
                        setSelected({ datasetId: activeDataset.id, index: v.index })
                      }
                    >
                      <Table.Td>
                        <Text size="xs" ff="monospace">
                          {fmt(v.x)}
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        <Text size="xs" ff="monospace">
                          {fmt(v.y)}
                        </Text>
                      </Table.Td>
                      <Table.Td p={0} w={26}>
                        <ActionIcon
                          size="xs"
                          variant="subtle"
                          color="red"
                          aria-label="Delete point"
                          onClick={(e) => {
                            e.stopPropagation()
                            updateDataset(activeDataset.id, (ds) => ({
                              ...ds,
                              points: removeAt(ds.points, v.index),
                            }))
                            setSelected(null)
                          }}
                        >
                          <IconTrash size={11} />
                        </ActionIcon>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            ) : (
              <Text size="xs" c="dimmed">
                {resolved
                  ? 'Select a dataset to list its calibrated values.'
                  : 'Values appear once all four axis points are calibrated.'}
              </Text>
            )}
          </Paper>
        </Stack>
      </ScrollArea>
    </Stack>

    {fitTarget && (
      <Suspense fallback={null}>
        <DigitizerFitModal
          datasetName={fitTarget.datasetName}
          points={fitTarget.points}
          xName={calibration.xName}
          yName={calibration.yName}
          xLog={calibration.xLog}
          yLog={calibration.yLog}
          tables={tables ?? []}
          onClose={() => setFitTarget(null)}
          onInsertEquation={onInsertEquation}
          onCreateFunctionTable={onCreateFunctionTable}
        />
      </Suspense>
    )}
  </Group>
  )
}
