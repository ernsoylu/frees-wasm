// tablesGrid/csv.ts
//
// CSV in and out for the Tables workbook.
//
// OUT — `downloadValuesAsCsv`: the active table as a .csv download.
//
// IN — `parseCsvTable`: the small reader that took over from papaparse when
// the Data Analyzer left (decision D11). Import CSV… needs one thing the
// analyzer's streaming ingest never got to be: simple. There is no time base
// to detect, no strictly-monotonic contract to enforce and no worker — a
// function table is a header row plus numeric columns, so that is all this
// parses. Quoted fields (with embedded delimiters, quotes and newlines), CRLF,
// a UTF-8 BOM and `;`/tab/`|` delimiters are handled because real exports have
// them. Everything else degrades instead of failing: a blank, a stray unit row
// or any other non-numeric cell becomes NaN and is skipped downstream by
// `functionSpecFromXY`, which counts what it dropped so the dialog can show it.

/** CSV download for the Tables workbook: quote-double `"` and wrap any cell
 *  containing a comma, quote, or newline. */
export function downloadValuesAsCsv(values: unknown[][], filename: string): void {
  const csvStr = values
    .map((row) =>
      row
        .map((cell) => {
          let val = String(cell ?? '').replaceAll('"', '""')
          if (val.includes(',') || val.includes('"') || val.includes('\n')) val = `"${val}"`
          return val
        })
        .join(','),
    )
    .join('\n')
  const blob = new Blob([csvStr], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}

// ---------------------------------------------------------------------------
// Reading

/** Delimiters sniffed from the header line, in no particular order — the one
 *  that splits it into the most fields wins, comma on a tie. */
const DELIMITERS = [',', ';', '\t', '|'] as const

/** One parsed column: the numbers are positional (one entry per data row) so
 *  any two columns pair up by row index without re-walking the file. */
export interface CsvColumn {
  /** Header text, de-duplicated and never blank (`col3` when unnamed). */
  name: string
  /** Zero-based position in the file. */
  index: number
  /** One entry per data row; blank and non-numeric cells are NaN. */
  values: Float64Array
  /** How many of `values` are finite — 0 marks a text column. */
  numericCount: number
}

export interface CsvTable {
  columns: CsvColumn[]
  /** Data rows (the header row, if any, excluded). */
  rowCount: number
  /** True when the file opened straight into data: columns are named colN. */
  headerless: boolean
  /** The delimiter actually used. */
  delimiter: string
}

/** A cell's numeric value; blank, text and `NaN` all read as NaN. */
export function cellToNumber(cell: string): number {
  const trimmed = cell.trim()
  if (trimmed === '') return Number.NaN
  return Number(trimmed)
}

function isNumericCell(cell: string): boolean {
  const trimmed = cell.trim()
  return trimmed !== '' && !Number.isNaN(Number(trimmed))
}

/**
 * Splits CSV text into raw cells, RFC 4180 style: `"` opens a quoted field at
 * its start (leading spaces tolerated), `""` is a literal quote inside one,
 * and CR / LF / CRLF all end a row. A UTF-8 BOM is stripped. Rows are ragged
 * exactly as the file is — squaring them up is `parseCsvTable`'s job.
 */
export function splitCsvRows(text: string, delimiter: string): string[][] {
  const rows: string[][] = []
  let row: string[] = []
  let field = ''
  let quoted = false
  let started = false // this row has content (guards the trailing newline)
  let i = text.charCodeAt(0) === 0xfeff ? 1 : 0

  const endField = () => {
    row.push(field)
    field = ''
    started = true
  }
  const endRow = () => {
    endField()
    rows.push(row)
    row = []
    started = false
  }

  while (i < text.length) {
    const ch = text[i]
    if (quoted) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          field += '"'
          i += 2
          continue
        }
        quoted = false
        i++
        continue
      }
      field += ch
      i++
      continue
    }
    if (ch === '"' && field.trim() === '') {
      // A quote only opens a field at its start; anywhere else it is literal.
      quoted = true
      field = ''
      i++
      continue
    }
    if (ch === delimiter) {
      endField()
      i++
      continue
    }
    if (ch === '\r' || ch === '\n') {
      if (ch === '\r' && text[i + 1] === '\n') i++
      endRow()
      i++
      continue
    }
    field += ch
    i++
  }
  // A file ending in a newline must not produce a phantom last row.
  if (field !== '' || started) endRow()
  return rows
}

/** Counts `delimiter` outside quoted fields. */
function countOutsideQuotes(line: string, delimiter: string): number {
  let n = 0
  let quoted = false
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]
    if (ch === '"') {
      if (quoted && line[i + 1] === '"') i++
      else quoted = !quoted
    } else if (ch === delimiter && !quoted) n++
  }
  return n
}

/**
 * Sniffs the delimiter from the first line carrying content. Comma wins ties
 * and an undelimited file (one column), which is the honest answer: a
 * single-column CSV has no delimiter to find.
 */
export function detectDelimiter(text: string): string {
  const head = text.slice(0, 1 << 16)
  let line = ''
  for (const candidate of head.split(/\r\n|\r|\n/)) {
    if (candidate.trim() !== '') {
      line = candidate
      break
    }
  }
  let best = ','
  let bestCount = 0
  for (const d of DELIMITERS) {
    const n = countOutsideQuotes(line, d)
    if (n > bestCount) {
      bestCount = n
      best = d
    }
  }
  return best
}

/** Makes every name non-blank and unique (case-insensitively), preserving the
 *  first occurrence and suffixing the rest `name (2)`, `name (3)`, … */
function uniqueNames(raw: string[]): string[] {
  const seen = new Map<string, number>()
  return raw.map((name, i) => {
    const base = name.trim() === '' ? `col${i + 1}` : name.trim()
    const key = base.toLowerCase()
    const count = seen.get(key) ?? 0
    seen.set(key, count + 1)
    return count === 0 ? base : `${base} (${count + 1})`
  })
}

/**
 * Parses CSV/TSV text into named numeric columns.
 *
 * The first content row is the header unless any of its cells is a number, in
 * which case the file is treated as headerless and the columns are named
 * `col1…colN` (the same rule the analyzer's ingest used — a real data row
 * always carries numbers). Ragged rows are squared up: the column count is the
 * widest row, short rows pad with NaN, and columns past the header are named
 * positionally. Blank lines are dropped.
 *
 * `delimiter` overrides the sniffer (tests, and a future "wrong delimiter"
 * escape hatch in the dialog).
 */
export function parseCsvTable(text: string, delimiter?: string): CsvTable {
  const delim = delimiter ?? detectDelimiter(text)
  const rows = splitCsvRows(text, delim).filter((r) => r.some((c) => c.trim() !== ''))
  if (rows.length === 0) {
    return { columns: [], rowCount: 0, headerless: false, delimiter: delim }
  }

  const headerless = rows[0].some((c) => isNumericCell(c))
  const dataRows = headerless ? rows : rows.slice(1)
  const columnCount = rows.reduce((max, r) => Math.max(max, r.length), 0)
  const headerCells = headerless ? [] : rows[0]
  const names = uniqueNames(
    Array.from({ length: columnCount }, (_, c) => headerCells[c] ?? ''),
  )

  const columns: CsvColumn[] = names.map((name, index) => ({
    name,
    index,
    values: new Float64Array(dataRows.length),
    numericCount: 0,
  }))
  for (let r = 0; r < dataRows.length; r++) {
    const row = dataRows[r]
    for (let c = 0; c < columnCount; c++) {
      const value = cellToNumber(row[c] ?? '')
      columns[c].values[r] = value
      if (Number.isFinite(value)) columns[c].numericCount++
    }
  }

  return { columns, rowCount: dataRows.length, headerless, delimiter: delim }
}
