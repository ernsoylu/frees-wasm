// CSV/TSV ingestion for the Data Analyzer (design contract §2.5c).
//
// All detection and column-building logic is pure and lives here so vitest can
// exercise the full time-column matrix in Node; csvImport.worker.ts is a thin
// shell that streams a File through papaparse into a CsvIngest and hands the
// finished Float64Array buffers back to the main thread as Transferables.
//
// Contract (§2.5c, load-bearing for Phase 2 step-hold fill and the Phase 4
// merged raster):
//  - emits a sorted, strictly monotonic Float64Array of *seconds* per file;
//  - time detection order: column-name match → monotonicity scan → format
//    sniffing (ISO-8601, epoch s vs ms by magnitude, relative seconds, index);
//  - ambiguous or absent time column → the caller must ask the user (status
//    'ask'), never a silent guess;
//  - duplicate timestamps / non-monotonic rows → hard import error naming the
//    offending row numbers (strict-over-warn).

import Papa from 'papaparse'
import type { ChannelKind } from './types'

export type TimeKind = 'iso' | 'epoch-s' | 'epoch-ms' | 'relative' | 'index'

/** A column that could plausibly serve as the time base. */
export interface TimeCandidate {
  column: number
  name: string
  kind: Exclude<TimeKind, 'index'>
}

/** User (or auto-detection) decision on the time base. */
export type TimeChoice =
  | { mode: 'column'; column: number; kind?: TimeKind }
  | { mode: 'dt'; dt: number }

export type CsvErrorCode = 'EMPTY' | 'NO_CHANNELS' | 'NON_MONOTONIC' | 'BAD_TIME'

export class CsvImportError extends Error {
  readonly code: CsvErrorCode
  /** 1-based file row numbers of offending rows (capped), when applicable. */
  readonly rows?: number[]
  constructor(code: CsvErrorCode, message: string, rows?: number[]) {
    super(message)
    this.name = 'CsvImportError'
    this.code = code
    this.rows = rows
  }
}

export interface IngestedChannel {
  name: string
  unit?: string
  kind: ChannelKind
  /** null for string-valued channels — listed but unplottable in Phase 1 (§2.5d). */
  values: Float64Array | null
  min: number
  max: number
}

export interface IngestResult {
  /** Strictly monotonic seconds. */
  time: Float64Array
  channels: IngestedChannel[]
  rowCount: number
  columnNames: string[]
  timeSource: { mode: 'column'; column: number; kind: TimeKind } | { mode: 'dt'; dt: number }
}

export type FinishOutcome =
  | { status: 'ok'; result: IngestResult }
  | { status: 'ask'; candidates: TimeCandidate[] }

// ---------------------------------------------------------------------------
// Growable Float64 column (chunked double-on-full strategy, trimmed at end).
// ---------------------------------------------------------------------------

export class GrowableFloat64 {
  private buf = new Float64Array(1024)
  private n = 0

  push(x: number) {
    if (this.n === this.buf.length) {
      const next = new Float64Array(this.buf.length * 2)
      next.set(this.buf)
      this.buf = next
    }
    this.buf[this.n++] = x
  }

  get length(): number {
    return this.n
  }

  /** Trimmed copy (the backing buffer is over-allocated by up to 2×). */
  finish(): Float64Array {
    return this.buf.slice(0, this.n)
  }
}

// ---------------------------------------------------------------------------
// Cell / header helpers
// ---------------------------------------------------------------------------

function isNumericCell(s: string): boolean {
  const trimmed = s.trim()
  if (trimmed === '') return false
  return !Number.isNaN(Number(trimmed))
}

/** Parse one data cell: '' → NaN gap, true/false → 1/0, else numeric or NaN. */
export function cellToNumber(s: string): number {
  const trimmed = s.trim()
  if (trimmed === '') return Number.NaN
  const lower = trimmed.toLowerCase()
  if (lower === 'true') return 1
  if (lower === 'false') return 0
  return Number(trimmed)
}

const UNIT_CELL = /^\[?[^\s,;0-9][^\s,;]{0,15}\]?$/

export interface HeaderInfo {
  names: string[]
  units: (string | undefined)[]
  /** Index of the first data row within the parsed row array. */
  dataStart: number
  columnCount: number
}

/**
 * Detect the header row plus up to two unit rows on lines 2–3 (§1: "unit-header
 * rows on lines 2–3 are consumed by the header/unit-row detection"). A unit row
 * has every non-empty cell non-numeric and unit-shaped (short, no spaces,
 * optionally bracketed) — a real data row always carries numbers.
 */
export function detectHeader(rows: string[][]): HeaderInfo {
  let first = 0
  while (first < rows.length && rows[first].every((c) => c.trim() === '')) first++
  if (first >= rows.length) throw new CsvImportError('EMPTY', 'The file contains no data.')

  const firstRow = rows[first]
  const columnCount = firstRow.length
  const headerless = firstRow.some((c) => isNumericCell(c))
  if (headerless) {
    return {
      names: firstRow.map((_, i) => `col${i + 1}`),
      units: [],
      dataStart: first,
      columnCount,
    }
  }

  const names = firstRow.map((c, i) => (c.trim() === '' ? `col${i + 1}` : c.trim()))
  let dataStart = first + 1
  let units: (string | undefined)[] = []
  for (let k = 0; k < 2 && dataStart < rows.length; k++) {
    const row = rows[dataStart]
    const nonEmpty = row.filter((c) => c.trim() !== '')
    const isUnitRow =
      nonEmpty.length > 0 &&
      nonEmpty.every((c) => !isNumericCell(c) && UNIT_CELL.test(c.trim()))
    if (!isUnitRow) break
    if (k === 0) {
      units = row.map((c) => {
        const u = c.trim().replace(/^\[|\]$/g, '')
        return u === '' ? undefined : u
      })
    }
    dataStart++
  }
  return { names, units, dataStart, columnCount }
}

// ---------------------------------------------------------------------------
// Time-column detection (§2.5c order: name match → monotonicity → sniffing)
// ---------------------------------------------------------------------------

const TIME_NAMES = new Set(['time', 't', 'timestamp', 'zeit', 'sec', 'secs', 'seconds', 'ms', 'millis'])

function isTimeName(name: string): boolean {
  // Cut at the first "[" or "(" (unit suffix) — an index scan, not a regex,
  // because /\s*[[(].*$/ backtracks super-linearly on pathological headers.
  const lower = name.toLowerCase()
  let cut = lower.length
  for (const ch of ['[', '(']) {
    const i = lower.indexOf(ch)
    if (i >= 0 && i < cut) cut = i
  }
  const norm = lower.slice(0, cut).trim()
  return TIME_NAMES.has(norm) || norm.startsWith('time')
}

interface ColumnSniff {
  kind: Exclude<TimeKind, 'index'> | null
  monotonic: boolean
}

function sniffColumn(sample: string[][], col: number): ColumnSniff {
  let prev = Number.NEGATIVE_INFINITY
  let monotonic = true
  let count = 0
  let allNumeric = true
  let allIso = true
  let magSum = 0
  for (const row of sample) {
    const cell = (row[col] ?? '').trim()
    if (cell === '') continue
    count++
    const num = Number(cell)
    if (Number.isNaN(num)) {
      allNumeric = false
      const parsed = Date.parse(cell)
      if (Number.isNaN(parsed) || !/[-:]/.test(cell)) {
        allIso = false
        monotonic = false
      } else {
        const sec = parsed / 1000
        if (sec <= prev) monotonic = false
        prev = sec
      }
    } else {
      allIso = false
      if (num <= prev) monotonic = false
      prev = num
      magSum += Math.abs(num)
    }
  }
  if (count === 0) return { kind: null, monotonic: false }
  if (allNumeric) {
    const mean = magSum / count
    const kind: Exclude<TimeKind, 'index'> = mean > 1e11 ? 'epoch-ms' : mean > 1e8 ? 'epoch-s' : 'relative'
    return { kind, monotonic }
  }
  if (allIso) return { kind: 'iso', monotonic }
  return { kind: null, monotonic: false }
}

export type TimeDetection =
  | { status: 'ok'; candidate: TimeCandidate }
  | { status: 'ask'; candidates: TimeCandidate[] }

/**
 * Detect the time column over a sample of data rows. Name match wins when the
 * named column is usable; otherwise a single strictly monotonic column is
 * accepted; anything else (zero or several candidates) must go to the user —
 * no silent guess (§2.5c).
 */
export function detectTimeColumn(names: string[], sample: string[][]): TimeDetection {
  const sniffs = names.map((_, col) => sniffColumn(sample, col))

  for (let col = 0; col < names.length; col++) {
    const s = sniffs[col]
    if (isTimeName(names[col]) && s.kind !== null && s.monotonic) {
      return { status: 'ok', candidate: { column: col, name: names[col], kind: s.kind } }
    }
  }

  const candidates: TimeCandidate[] = []
  for (let col = 0; col < names.length; col++) {
    const s = sniffs[col]
    if (s.kind !== null && s.monotonic) {
      candidates.push({ column: col, name: names[col], kind: s.kind })
    }
  }
  if (candidates.length === 1) return { status: 'ok', candidate: candidates[0] }
  return { status: 'ask', candidates }
}

// ---------------------------------------------------------------------------
// Streaming ingest
// ---------------------------------------------------------------------------

/** Data rows sampled for detection before streaming begins. */
const SAMPLE_ROWS = 200
const MAX_REPORTED_ROWS = 10

interface ResolvedTime {
  mode: 'column' | 'dt'
  column: number
  kind: TimeKind
  dt: number
}

/**
 * Incremental CSV ingester. Feed parsed rows via push() (buffered until enough
 * rows exist to resolve header + time base), then call tryFinish(). If the
 * time base is ambiguous and no TimeChoice was supplied, `ask` is set and
 * ingestion stops so the caller can prompt the user and retry with a choice.
 */
export class CsvIngest {
  ask: TimeCandidate[] | null = null

  private readonly choice?: TimeChoice
  private pending: string[][] = []
  private header: HeaderInfo | null = null
  private resolved: ResolvedTime | null = null

  private time = new GrowableFloat64()
  private cols: GrowableFloat64[] = []
  private nonEmpty: number[] = []
  private nonNumeric: number[] = []
  private mins: number[] = []
  private maxs: number[] = []
  private boolish: boolean[] = []

  private prevT = Number.NEGATIVE_INFINITY
  private violations: number[] = []
  private dataRow = 0

  constructor(choice?: TimeChoice) {
    this.choice = choice
  }

  push(rows: string[][]) {
    if (this.ask) return
    let i = 0
    if (this.resolved === null) {
      // No spread here: a papaparse chunk can carry hundreds of thousands of
      // rows and Array.push(...rows) passes each row as a call argument —
      // instant stack overflow. Buffer one by one, resolve the header + time
      // base as soon as the detection sample is full, then stream the rest.
      for (; i < rows.length; i++) {
        this.pending.push(rows[i])
        if (this.pending.length >= SAMPLE_ROWS + 3) {
          this.resolve()
          i++
          break
        }
      }
      if (this.resolved === null || this.ask !== null) return
    }
    for (; i < rows.length; i++) this.ingestRow(rows[i])
  }

  tryFinish(): FinishOutcome {
    if (this.resolved === null && this.ask === null) this.resolve()
    if (this.ask !== null) return { status: 'ask', candidates: this.ask }
    const header = this.header
    const resolved = this.resolved
    if (header === null || resolved === null) {
      throw new CsvImportError('EMPTY', 'The file contains no data.')
    }
    if (this.dataRow === 0) throw new CsvImportError('EMPTY', 'The file contains no data rows.')
    if (this.violations.length > 0) {
      const shown = this.violations.slice(0, MAX_REPORTED_ROWS)
      const suffix = this.violations.length > shown.length ? ', …' : ''
      throw new CsvImportError(
        'NON_MONOTONIC',
        `The time column must be strictly increasing — duplicate or out-of-order timestamps at row${
          this.violations.length > 1 ? 's' : ''
        } ${shown.join(', ')}${suffix}.`,
        shown,
      )
    }

    const channels: IngestedChannel[] = []
    const names: string[] = []
    for (let c = 0, k = 0; c < header.columnCount; c++) {
      if (resolved.mode === 'column' && c === resolved.column) continue
      const idx = k++
      const name = header.names[c] ?? `col${c + 1}`
      names.push(name)
      const stringy = this.nonEmpty[idx] > 0 && this.nonNumeric[idx] > this.nonEmpty[idx] / 2
      const kind: ChannelKind = stringy ? 'string' : this.boolish[idx] ? 'boolean' : 'analog'
      channels.push({
        name,
        unit: header.units[c],
        kind,
        values: stringy ? null : this.cols[idx].finish(),
        min: this.mins[idx] === Number.POSITIVE_INFINITY ? Number.NaN : this.mins[idx],
        max: this.maxs[idx] === Number.NEGATIVE_INFINITY ? Number.NaN : this.maxs[idx],
      })
    }
    if (channels.length === 0) {
      throw new CsvImportError('NO_CHANNELS', 'The file has no data columns besides the time base.')
    }

    const timeSource =
      resolved.mode === 'dt'
        ? ({ mode: 'dt', dt: resolved.dt } as const)
        : ({ mode: 'column', column: resolved.column, kind: resolved.kind } as const)
    return {
      status: 'ok',
      result: {
        time: this.time.finish(),
        channels,
        rowCount: this.dataRow,
        columnNames: names,
        timeSource,
      },
    }
  }

  private resolve() {
    const header = detectHeader(this.pending)
    this.header = header
    const sample = this.pending.slice(header.dataStart, header.dataStart + SAMPLE_ROWS)

    if (this.choice?.mode === 'dt') {
      if (!(this.choice.dt > 0)) {
        throw new CsvImportError('BAD_TIME', 'The sample interval dt must be a positive number of seconds.')
      }
      this.resolved = { mode: 'dt', column: -1, kind: 'index', dt: this.choice.dt }
    } else if (this.choice?.mode === 'column') {
      const col = this.choice.column
      if (col < 0 || col >= header.columnCount) {
        throw new CsvImportError('BAD_TIME', `Time column index ${col} is out of range.`)
      }
      const kind = this.choice.kind ?? sniffColumn(sample, col).kind ?? 'relative'
      this.resolved = { mode: 'column', column: col, kind, dt: 0 }
    } else {
      const detection = detectTimeColumn(header.names, sample)
      if (detection.status === 'ask') {
        this.ask = detection.candidates
        this.pending = []
        return
      }
      this.resolved = {
        mode: 'column',
        column: detection.candidate.column,
        kind: detection.candidate.kind,
        dt: 0,
      }
    }

    const dataColumns = header.columnCount - (this.resolved.mode === 'column' ? 1 : 0)
    for (let i = 0; i < dataColumns; i++) {
      this.cols.push(new GrowableFloat64())
      this.nonEmpty.push(0)
      this.nonNumeric.push(0)
      this.mins.push(Number.POSITIVE_INFINITY)
      this.maxs.push(Number.NEGATIVE_INFINITY)
      this.boolish.push(true)
    }

    const buffered = this.pending
    this.pending = []
    for (let i = header.dataStart; i < buffered.length; i++) this.ingestRow(buffered[i])
  }

  private ingestRow(row: string[]) {
    const header = this.header
    const resolved = this.resolved
    if (header === null || resolved === null) return
    if (row.every((c) => c.trim() === '')) return

    let t: number
    if (resolved.mode === 'dt') {
      t = this.dataRow * resolved.dt
    } else {
      const cell = (row[resolved.column] ?? '').trim()
      if (resolved.kind === 'iso') {
        t = Date.parse(cell) / 1000
      } else {
        t = Number(cell)
        if (resolved.kind === 'epoch-ms') t /= 1000
      }
    }
    // 1-based file row number (header + unit rows included) for error reports.
    const fileRow = header.dataStart + this.dataRow + 1
    if (Number.isNaN(t) || t <= this.prevT) {
      if (this.violations.length <= MAX_REPORTED_ROWS) this.violations.push(fileRow)
    }
    this.prevT = Number.isNaN(t) ? this.prevT : t
    this.time.push(t)

    for (let c = 0, k = 0; c < header.columnCount; c++) {
      if (resolved.mode === 'column' && c === resolved.column) continue
      const idx = k++
      const cell = row[c] ?? ''
      const x = cellToNumber(cell)
      this.cols[idx].push(x)
      if (cell.trim() !== '') {
        this.nonEmpty[idx]++
        if (Number.isNaN(x) && cell.trim().toLowerCase() !== 'nan') this.nonNumeric[idx]++
      }
      if (!Number.isNaN(x)) {
        if (x < this.mins[idx]) this.mins[idx] = x
        if (x > this.maxs[idx]) this.maxs[idx] = x
        if (x !== 0 && x !== 1) this.boolish[idx] = false
      }
    }
    this.dataRow++
  }
}

/**
 * Parse a whole CSV/TSV text in one go (test + small-input convenience; the
 * worker streams a File through the same CsvIngest instead). Delimiter is
 * auto-detected by papaparse (comma, tab, semicolon, pipe).
 */
export function ingestCsvText(text: string, choice?: TimeChoice): FinishOutcome {
  const parsed = Papa.parse<string[]>(text, { skipEmptyLines: 'greedy' })
  const ingest = new CsvIngest(choice)
  ingest.push(parsed.data)
  return ingest.tryFinish()
}

// ---------------------------------------------------------------------------
// File signature (§2.5a): FNV-1a over the first 64 KB + the column-name list.
// ---------------------------------------------------------------------------

export function fnv1a(s: string): string {
  let h = 0x811c9dc5
  for (const ch of s) {
    h ^= ch.codePointAt(0) ?? 0
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

export function headerHash(head64k: string, columnNames: string[]): string {
  return fnv1a(`${head64k} ${columnNames.join(',')}`)
}

// ---------------------------------------------------------------------------
// Main-thread worker driver
// ---------------------------------------------------------------------------

/** One imported measurement, buffers already transferred to the main thread. */
export interface ImportedMeasurement {
  signatureName: string
  size: number
  headerHash: string
  time: Float64Array
  channels: IngestedChannel[]
  rowCount: number
}

export type ImportOutcome =
  | { status: 'ok'; measurement: ImportedMeasurement }
  | { status: 'needs-time'; candidates: TimeCandidate[] }

export interface WorkerRequest {
  file: File
  choice?: TimeChoice
}

export type WorkerResponse =
  | { type: 'needs-time'; candidates: TimeCandidate[] }
  | { type: 'error'; message: string; code?: CsvErrorCode; rows?: number[] }
  | {
      type: 'done'
      time: Float64Array
      channels: {
        name: string
        unit?: string
        kind: ChannelKind
        values: Float64Array | null
        min: number
        max: number
      }[]
      rowCount: number
      headerHash: string
    }

/**
 * Parse a CSV/TSV File off the main thread. The worker streams the file
 * through papaparse in chunks (no whole-file string in memory), builds
 * Float64Array columns, and transfers ownership of the finished buffers back
 * (zero-copy — a structured-clone copy of hundreds of MB would freeze the
 * main thread and defeat the worker, per todo.md §2).
 */
export function importCsvFile(file: File, choice?: TimeChoice): Promise<ImportOutcome> {
  return new Promise((resolvePromise, rejectPromise) => {
    const worker = new Worker(new URL('./csvImport.worker.ts', import.meta.url), {
      type: 'module',
    })
    worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const msg = e.data
      worker.terminate()
      if (msg.type === 'needs-time') {
        resolvePromise({ status: 'needs-time', candidates: msg.candidates })
      } else if (msg.type === 'error') {
        rejectPromise(new CsvImportError(msg.code ?? 'BAD_TIME', msg.message, msg.rows))
      } else {
        resolvePromise({
          status: 'ok',
          measurement: {
            signatureName: file.name,
            size: file.size,
            headerHash: msg.headerHash,
            time: msg.time,
            channels: msg.channels,
            rowCount: msg.rowCount,
          },
        })
      }
    }
    worker.onerror = (e) => {
      worker.terminate()
      rejectPromise(new Error(e.message || 'CSV import worker failed.'))
    }
    const request: WorkerRequest = { file, choice }
    worker.postMessage(request)
  })
}
