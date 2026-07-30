// Parse pasted/uploaded performance-map data and emit a frees TABLE block for the
// wizard's map-ingestion flow. The TABLE grammar (Frees.g4) is:
//   1-D:  TABLE name(arg [unit]) [outUnit]            rows: <arg> <value>
//   2-D:  TABLE name(arg [unit] : fam = v1, v2) [out] rows: <arg> <v_fam1> <v_fam2>
// Body rows are whitespace-separated signed numbers; the first column is the
// lookup argument, each further column is one curve of the family.

export interface ParsedMap {
  kind: '1d' | '2d'
  rows: number[][] // each row: [arg, value...] (already header-stripped)
  family: number[] // 2-D family values (length = cols-1); [] for 1-D
}

const isNum = (t: string) => t !== '' && Number.isFinite(Number(t))

/** Parse a whitespace/comma-separated grid. A non-numeric leading first cell marks
 *  the first line as a family-value header (2-D). Otherwise 2 columns ⇒ 1-D,
 *  more ⇒ 2-D with family values defaulting to 1..N. Throws on malformed input. */
export function parseMapData(text: string): ParsedMap {
  const lines = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l !== '' && !l.startsWith('#'))
  if (lines.length === 0) throw new Error('No data rows found.')

  const tokens = lines.map((l) => l.split(/[\s,]+/).filter((t) => t !== ''))

  // Header detection: a first row whose leading cell is non-numeric supplies the
  // family values (its remaining numeric tokens).
  let header: number[] | null = null
  let body = tokens
  if (!tokens[0].every(isNum)) {
    header = tokens[0].slice(1).filter(isNum).map(Number)
    body = tokens.slice(1)
    if (body.length === 0) throw new Error('Header row present but no data rows.')
  }

  const ncols = body[0].length
  if (ncols < 2) throw new Error('Each row needs an argument column plus at least one value column.')
  const rows: number[][] = body.map((r, i) => {
    if (r.length !== ncols) throw new Error(`Row ${i + 1} has ${r.length} columns, expected ${ncols}.`)
    return r.map((t) => {
      if (!isNum(t)) throw new Error(`Non-numeric value "${t}" in row ${i + 1}.`)
      return Number(t)
    })
  })

  if (ncols === 2 && !header) return { kind: '1d', rows, family: [] }
  const family = header ?? Array.from({ length: ncols - 1 }, (_, i) => i + 1)
  if (family.length !== ncols - 1) {
    throw new Error(`Header has ${family.length} family values but data has ${ncols - 1} value columns.`)
  }
  return { kind: '2d', rows, family }
}

export interface TableConfig {
  name: string
  argName: string
  argUnit?: string
  outUnit?: string
  famName?: string // 2-D only
  family?: number[] // 2-D only
  rows: number[][]
}

const unitTag = (u?: string) => (u && u.trim() !== '' ? ` [${u.trim()}]` : '')
const numList = (xs: number[]) => xs.join(', ')

/** Emit a TABLE … END block from a config (1-D when family is empty/absent). */
export function buildTableBlock(cfg: TableConfig): string {
  const fam = cfg.family ?? []
  const argDecl = fam.length > 0
    ? `${cfg.argName}${unitTag(cfg.argUnit)} : ${cfg.famName || 'param'} = ${numList(fam)}`
    : `${cfg.argName}${unitTag(cfg.argUnit)}`
  const head = `TABLE ${cfg.name}(${argDecl})${unitTag(cfg.outUnit)}`
  const body = cfg.rows.map((r) => '  ' + r.map((n) => String(n)).join('  ')).join('\n')
  return `${head}\n${body}\nEND`
}

/** Legal frees identifier (table name / axis name). */
export function isValidTableName(name: string): boolean {
  return /^[A-Za-z_]\w*$/.test(name.trim())
}
