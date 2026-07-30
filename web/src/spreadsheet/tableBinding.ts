// spreadsheet/tableBinding.ts
//
// Pure projections between a FunctionTableSpec and a bound sheet (contracts
// a/f of the table-spreadsheet unification plan, todo.md). The spec stays the
// persisted source of truth; the sheet is a materialized view. No Univer
// imports — everything speaks the stored celldata format (univerAdapter's
// isolation principle), so all mapping rules are unit-testable without the
// grid engine.
//
// Region layout (anchored at A1):
//   row 1        header — A1 = argument name; B1.. = curve-parameter values
//                (2-D) or the 'y' label (1-D)
//   rows 2..N+1  data — col A = x, cols B.. = ys
//
// The mapper is the *guarantee* behind protection (contract a): it scans only
// the schema's bounding box, so content outside the region can never reach a
// spec even if a mutation slips past the UI-level protection rules.

import { FunctionTableSpec, newParamRow, ParamRow, ParamTableSpec, TableSpec } from '../tables'
import { colName, StoredCell, StoredCellValue, StoredSheet } from './univerAdapter'

/** Hard cap on data rows (contract a): a runaway 50k-row paste truncates here
 * instead of bloating the .frees file and swamping the solver. */
export const TABLE_MAX_ROWS = 5000
export const HEADER_ROW_COUNT = 1

/** Univer formula error literals (#REF! after a referenced sheet is deleted,
 * #DIV/0!, …). Contract f: these map to omitted spec values — never a
 * crash-risk string in a solver DTO — while the formula overlay entry is
 * retained so the user can fix the formula. */
const ERROR_VALUE = /^#(?:REF!|DIV\/0!|VALUE!|NAME\?|N\/A|NUM!|NULL!|CALC!|SPILL!|ERROR!|CYCLE!)$/i

export function isErrorValue(v: unknown): boolean {
  return typeof v === 'string' && ERROR_VALUE.test(v.trim())
}

/** Bound columns: function tables = x plus one per curve; parametric tables
 * = the Run column plus one per table variable. */
export function boundColumnCount(spec: TableSpec): number {
  return spec.kind === 'function' ? 1 + spec.columns.length : 1 + spec.vars.length
}

export { isHostedTable } from './tablesWorkbookBridge'

/** The computed (solver-written) value shown in a parametric cell: only when
 * the row solved successfully AND the user left the input draft blank. */
export function paramComputedValue(
  spec: ParamTableSpec,
  rowIndex: number,
  varName: string,
): number | undefined {
  const res = spec.results[rowIndex]
  if (!res?.success) return undefined
  if ((spec.rows[rowIndex]?.values[varName] ?? '').trim() !== '') return undefined
  return res.values[varName]
}

function cellValue(raw: string): StoredCellValue {
  const trimmed = raw.trim()
  const num = Number(trimmed)
  const v: string | number = trimmed !== '' && Number.isFinite(num) ? num : raw
  return { v, m: raw }
}

/** A1 ref for 0-based sheet coordinates. */
export function a1(r: number, c: number): string {
  return `${colName(c)}${r + 1}`
}

// ---------------------------------------------------------------------------
// spec -> sheet (materialization)

export function specToSheetData(spec: TableSpec): StoredSheet {
  return spec.kind === 'function' ? functionSpecToSheetData(spec) : paramSpecToSheetData(spec)
}

/** Green used for solver-computed cells (matches the old grid's green.4). */
const COMPUTED_CSS = 'color: #69db7c; '
const FAILED_CSS = 'color: #fa5252; '
/** Every in-region cell gets a border so the bound table reads as a real
 * grid — and a freshly added run/row is visibly part of it even while its
 * input cells are still empty ("these cells should exist"). */
const BORDER_CSS = 'border: 1px solid #64686f;'

/** Grid headroom for bound sheets, so Add Row and ordinary pastes don't hit
 * Univer's hard "Range is out of bounds" error at the default 100×26. */
export function sheetDimsFor(spec: TableSpec): { rowCount: number; columnCount: number } {
  return {
    rowCount: Math.max(spec.rows.length + 200, 1000),
    columnCount: Math.max(boundColumnCount(spec) + 10, 26),
  }
}

function paramSpecToSheetData(spec: ParamTableSpec): StoredSheet {
  const celldata: StoredCell[] = []
  const styles: Record<string, string> = {}

  // Header: Run column + one column per table variable (units appended for
  // code/ODE display tables that carry them).
  celldata.push({ r: 0, c: 0, v: cellValue('Run') })
  styles.A1 = 'font-weight: bold; ' + BORDER_CSS
  spec.vars.forEach((name, j) => {
    const unit = spec.columnUnits?.[name]
    celldata.push({ r: 0, c: j + 1, v: cellValue(unit ? `${name} [${unit}]` : name) })
    styles[a1(0, j + 1)] = 'font-weight: bold; ' + BORDER_CSS
  })

  const formulas = spec.formulas ?? {}
  spec.rows.forEach((row, i) => {
    const r = i + HEADER_ROW_COUNT
    const res = spec.results[i]
    const failed = res && !res.success
    celldata.push({ r, c: 0, v: cellValue(failed ? `${i + 1} ✗` : String(i + 1)) })
    styles[a1(r, 0)] = (failed ? FAILED_CSS : '') + BORDER_CSS
    spec.vars.forEach((name, j) => {
      const ref = a1(r, j + 1)
      const draft = row.values[name] ?? ''
      const computed = paramComputedValue(spec, i, name)
      const f = formulas[ref]
      if (computed !== undefined) {
        celldata.push({ r, c: j + 1, v: { v: computed, m: String(computed) } })
        styles[ref] = COMPUTED_CSS + BORDER_CSS
      } else {
        // Blank input cells are materialized too — an empty cell with a
        // border is still a visible part of the table region.
        const v = cellValue(draft)
        if (f) v.f = f
        celldata.push({ r, c: j + 1, v })
        styles[ref] = BORDER_CSS
      }
    })
  })

  const dims = sheetDimsFor(spec)
  return { name: spec.name, id: spec.id, celldata, styles, config: {}, ...dims }
}

function functionSpecToSheetData(spec: FunctionTableSpec): StoredSheet {
  const celldata: StoredCell[] = []
  const styles: Record<string, string> = {}

  // Header row (bold). A1 = argument name; curve headers carry the raw
  // family-parameter strings (editable for 2-D tables, mapped back to
  // spec.columns), or a fixed 'y' label for 1-D.
  celldata.push({ r: 0, c: 0, v: cellValue(spec.argName || 'x') })
  styles.A1 = 'font-weight: bold; ' + BORDER_CSS
  spec.columns.forEach((param, j) => {
    celldata.push({ r: 0, c: j + 1, v: cellValue(spec.is1D ? 'y' : param) })
    styles[a1(0, j + 1)] = 'font-weight: bold; ' + BORDER_CSS
  })

  // Data rows — every in-region cell is materialized (blank ones as empty
  // bordered cells) so the table reads as a bounded grid. Formula-overlay
  // cells are written with `f` so the adapter's load path (f-only, no cached
  // v) makes Univer recompute them.
  const formulas = spec.formulas ?? {}
  const pushCell = (r: number, c: number, raw: string) => {
    const ref = a1(r, c)
    const f = formulas[ref]
    const v = cellValue(raw)
    if (f) v.f = f
    celldata.push({ r, c, v })
    styles[ref] = BORDER_CSS
  }
  spec.rows.forEach((row, i) => {
    pushCell(i + HEADER_ROW_COUNT, 0, row.x)
    spec.columns.forEach((_, j) => pushCell(i + HEADER_ROW_COUNT, j + 1, row.ys[j] ?? ''))
  })

  const dims = sheetDimsFor(spec)
  return { name: spec.name, id: spec.id, celldata, styles, config: {}, ...dims }
}

/** A1 ranges the host must protect (protect() + setPoint(Edit, false) per the
 * Phase 0 spike — a bare protect() does not restrict the local session).
 * Code-sourced tables are fully protected (the editor text is their source of
 * truth); GUI tables protect the schema header cells only: the argument name
 * (toolbar-edited) and, for 1-D, the fixed 'y' label. 2-D curve-parameter
 * headers stay editable — they map back to spec.columns. */
export function protectedRangesFor(spec: TableSpec): string[] {
  if (spec.source === 'code') {
    const lastCol = colName(Math.max(boundColumnCount(spec) - 1, 1))
    return [`A1:${lastCol}${TABLE_MAX_ROWS + HEADER_ROW_COUNT}`]
  }
  if (spec.kind === 'parametric') {
    // Header row (column schema lives in ConfigureTableModal) + the Run
    // column. Computed result cells are NOT range-protected — they are a
    // scattered per-cell set; typing over one turns it into an input (an
    // override), and the mapper handles that (invalidating the results).
    const lastCol = colName(spec.vars.length)
    return [`A1:${lastCol}1`, `A1:A${TABLE_MAX_ROWS + HEADER_ROW_COUNT}`]
  }
  return spec.is1D ? ['A1:B1'] : ['A1:A1']
}

// ---------------------------------------------------------------------------
// sheet -> spec (user edits, read through the facade)

/** Facade reads over the bound region, row-major from A1 (header row
 * included). `formulas` uses '' for non-formula cells. Values must come from
 * the facade *after* the calculation-complete gate (contract b), never from
 * raw celldata. */
export interface RegionRead {
  values: (string | number | boolean | null)[][]
  formulas: string[][]
}

export interface SheetEditsResult {
  spec: TableSpec
  /** Data rows exceeded TABLE_MAX_ROWS and were cut (toast the user). */
  truncated: boolean
  /** A1 refs whose value was a formula error, mapped to blank (flag them). */
  errorCells: string[]
  /** Content existed beyond the bound columns / row cap (host clears it). */
  outOfRegion: boolean
  /** Parametric only: the display-only Run column no longer matches the
   * materialized labels — e.g. the user drag-filled run numbers into it
   * (Univer's fill-drag can write through range protection). The host must
   * repaint the sheet to repair it. */
  runColumnDrift?: boolean
}

export function sheetEditsToSpec(spec: TableSpec, read: RegionRead): SheetEditsResult {
  // Code tables are never writable through the sheet; the mapper is the
  // last line of defense if a mutation slips past protection.
  if (spec.source === 'code') {
    return { spec, truncated: false, errorCells: [], outOfRegion: false }
  }
  return spec.kind === 'function'
    ? functionSheetEditsToSpec(spec, read)
    : paramSheetEditsToSpec(spec, read)
}

/** Parametric sheet → spec. Cells showing solver-computed values (blank
 * input + successful run, detected against the PREVIOUS spec) are not inputs
 * — unless the user typed over one, which turns it into an input override.
 * Any input change (or new rows) invalidates results/stats/check, exactly
 * like the old grid's invalidateActiveParam. Row identities are preserved.
 * Rows are never trimmed below the previous count — blank rows are runs;
 * Remove Row is the way to drop them (matches the old UI). */
function paramSheetEditsToSpec(prev: ParamTableSpec, read: RegionRead): SheetEditsResult {
  const cols = boundColumnCount(prev)
  const errorCells: string[] = []
  const formulas: Record<string, string> = {}

  const cellStr = (r: number, c: number): string => {
    const f = read.formulas[r]?.[c] ?? ''
    const raw = read.values[r]?.[c]
    if (f) formulas[a1(r, c)] = f
    if (isErrorValue(raw)) {
      errorCells.push(a1(r, c))
      return ''
    }
    return raw === null || raw === undefined ? '' : String(raw)
  }

  // Last data row carrying content in the bound variable columns.
  let lastDataRow = prev.rows.length + HEADER_ROW_COUNT - 1
  for (let r = HEADER_ROW_COUNT; r < read.values.length; r++) {
    for (let c = 1; c < cols; c++) {
      const raw = read.values[r]?.[c]
      if ((raw !== null && raw !== undefined && String(raw).trim() !== '') || read.formulas[r]?.[c]) {
        if (r > lastDataRow) lastDataRow = r
        break
      }
    }
  }
  let truncated = false
  if (lastDataRow > TABLE_MAX_ROWS) {
    lastDataRow = TABLE_MAX_ROWS
    truncated = true
  }

  let inputChanged = false
  const rows: ParamRow[] = []
  for (let r = HEADER_ROW_COUNT; r <= lastDataRow; r++) {
    const i = r - HEADER_ROW_COUNT
    const prevRow = prev.rows[i]
    const values: Record<string, string> = {}
    prev.vars.forEach((name, j) => {
      const raw = cellStr(r, j + 1)
      const computed = paramComputedValue(prev, i, name)
      // A cell still showing the solver's value is not an input.
      const isUntouchedComputed =
        computed !== undefined && raw.trim() !== '' && Number(raw) === computed
      const draft = isUntouchedComputed ? '' : raw
      values[name] = draft
      if ((prevRow?.values[name] ?? '') !== draft) inputChanged = true
    })
    if (!prevRow) inputChanged = true
    rows.push(prevRow ? { ...prevRow, values } : { ...newParamRow(), values })
  }

  // Deleting runs by clearing their cells: trailing rows whose inputs just
  // transitioned from filled (in prev) to fully blank are dropped — the user
  // deleted them by hand. Rows that were ALREADY blank (starter rows, Add
  // Row) are kept: blank runs are legitimate, and Add Row must survive
  // unrelated edits. Explicit shrinking beyond that stays on Remove Row.
  const isBlank = (values: Record<string, string>) =>
    prev.vars.every((name) => (values[name] ?? '').trim() === '')
  while (rows.length > 0) {
    const i = rows.length - 1
    const prevRow = prev.rows[i]
    if (!prevRow || isBlank(prevRow.values) || !isBlank(rows[i].values)) break
    rows.pop()
    inputChanged = true
  }

  let outOfRegion = truncated
  for (let r = 0; r < read.values.length && !outOfRegion; r++) {
    const rowVals = read.values[r] ?? []
    for (let c = cols; c < rowVals.length; c++) {
      const raw = rowVals[c]
      if ((raw !== null && raw !== undefined && String(raw).trim() !== '') || read.formulas[r]?.[c]) {
        outOfRegion = true
        break
      }
    }
  }

  // The Run column is display-only, but Univer's fill-drag can write through
  // range protection. Detect drift from the materialized labels — content
  // beyond the run count, or a PRESENT label with the wrong text — so the
  // host repaints the sheet instead of leaving half-stale run numbers
  // behind. A missing in-range label is deliberately NOT drift: freshly
  // grown rows are blank until the materialize-on-spec-change repaint lands
  // (treating that write race as tampering fired spurious warnings).
  let runColumnDrift = false
  for (let r = HEADER_ROW_COUNT; r < read.values.length; r++) {
    const raw = read.values[r]?.[0]
    const text = raw === null || raw === undefined ? '' : String(raw).trim()
    if (text === '') continue
    const runIndex = r - HEADER_ROW_COUNT
    if (runIndex >= rows.length || (text !== `${runIndex + 1}` && text !== `${runIndex + 1} ✗`)) {
      runColumnDrift = true
      break
    }
  }

  const spec: ParamTableSpec = {
    ...prev,
    rows,
    formulas: Object.keys(formulas).length > 0 ? formulas : undefined,
    ...(inputChanged
      ? { results: [], stats: null, checkResult: null, checkMessage: '' }
      : {}),
  }
  return { spec, truncated, errorCells, outOfRegion, runColumnDrift }
}

function functionSheetEditsToSpec(spec: FunctionTableSpec, read: RegionRead): SheetEditsResult {

  const cols = boundColumnCount(spec)
  const errorCells: string[] = []
  const formulas: Record<string, string> = {}

  const cellStr = (r: number, c: number): string => {
    const f = read.formulas[r]?.[c] ?? ''
    const raw = read.values[r]?.[c]
    if (f) formulas[a1(r, c)] = f
    if (isErrorValue(raw)) {
      errorCells.push(a1(r, c))
      return ''
    }
    return raw === null || raw === undefined ? '' : String(raw)
  }

  // Header: 2-D curve-parameter values map back to spec.columns; the column
  // *count* is schema and never changes from sheet edits (contract a).
  const columns = spec.is1D
    ? [...spec.columns]
    : spec.columns.map((_, j) => cellStr(0, j + 1))

  // Data region: bounded scan (never the sheet's full used range).
  let lastDataRow = 0 // sheet row index of the last non-blank data row
  const scanLimit = read.values.length
  for (let r = HEADER_ROW_COUNT; r < scanLimit; r++) {
    for (let c = 0; c < cols; c++) {
      const raw = read.values[r]?.[c]
      const f = read.formulas[r]?.[c]
      if (f || (raw !== null && raw !== undefined && String(raw).trim() !== '')) {
        lastDataRow = r
        break
      }
    }
  }

  let truncated = false
  if (lastDataRow > TABLE_MAX_ROWS) {
    lastDataRow = TABLE_MAX_ROWS
    truncated = true
  }

  const rows: { x: string; ys: string[] }[] = []
  for (let r = HEADER_ROW_COUNT; r <= lastDataRow; r++) {
    rows.push({ x: cellStr(r, 0), ys: columns.map((_, j) => cellStr(r, j + 1)) })
  }

  // Out-of-region detection: anything beyond the bound columns within the
  // read window, or rows past the cap — the host clears these (contract a).
  let outOfRegion = truncated
  for (let r = 0; r < scanLimit && !outOfRegion; r++) {
    const rowVals = read.values[r] ?? []
    for (let c = cols; c < rowVals.length; c++) {
      const raw = rowVals[c]
      if ((raw !== null && raw !== undefined && String(raw).trim() !== '') || read.formulas[r]?.[c]) {
        outOfRegion = true
        break
      }
    }
  }

  return {
    spec: {
      ...spec,
      columns,
      rows,
      formulas: Object.keys(formulas).length > 0 ? formulas : undefined,
    },
    truncated,
    errorCells,
    outOfRegion,
  }
}
