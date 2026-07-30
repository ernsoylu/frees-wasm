import { SpreadsheetSpec } from './types'

export interface SsheetReference {
  match: string
  spreadsheetName?: string
  sheetName?: string
  range: string
}

function parseCell(ref: string): { r: number; c: number } | null {
  const match = ref.toUpperCase().match(/^([A-Z]+)(\d+)$/)
  if (!match) return null
  const [, colStr, rowStr] = match
  let c = 0
  for (let i = 0; i < colStr.length; i++) {
    c = c * 26 + ((colStr.codePointAt(i) ?? 0) - 64)
  }
  c -= 1 // 0-indexed
  const r = Number.parseInt(rowStr, 10) - 1
  return { r, c }
}

export function parseSsheetReferences(text: string): SsheetReference[] {
  const refs: SsheetReference[] = []
  // Match `ssheet(...)` capturing everything between the parens, then parse the
  // argument list ourselves. Doing it this way (rather than one mega-regex) lets
  // quoted names contain spaces — e.g. ssheet('Spreadsheet 1', 'Sheet1!B2'),
  // which is exactly what GUI bindings generate.
  // Supports: ssheet(A1), ssheet('A1'), ssheet(Sheet1, A1), ssheet('Sheet1', 'A1'),
  //           ssheet('Spreadsheet 1', 'Sheet1!A1:B2')
  const regex = /ssheet\s*\(([^)]*)\)/g
  let match
  while ((match = regex.exec(text)) !== null) {
    const args = match[1]
      .split(',')
      .map((a) => a.trim().replace(/^['"]|['"]$/g, '').trim())
      .filter((a) => a.length > 0)
    if (args.length === 0) continue

    let spreadsheetName: string | undefined
    let sheetName: string | undefined
    let rangePart: string

    if (args.length >= 2) {
      // First arg is a workbook (or sheet) name; second is `[sheet!]range`.
      spreadsheetName = args[0]
      rangePart = args[1]
    } else {
      rangePart = args[0]
    }

    const bang = rangePart.indexOf('!')
    if (bang >= 0) {
      sheetName = rangePart.slice(0, bang).trim()
      rangePart = rangePart.slice(bang + 1)
    }

    refs.push({
      match: match[0],
      spreadsheetName,
      sheetName,
      range: rangePart.trim(),
    })
  }
  return refs
}

type LooseSheet = {
  name?: string
  celldata?: Array<{ r: number; c: number; v?: { v?: string | number } }>
  data?: Record<number, Record<number, { v?: string | number }>>
}

function getCellValue(sheet: LooseSheet, r: number, c: number): number {
  if (sheet.celldata) {
    const cell = sheet.celldata.find((cd) => cd.r === r && cd.c === c)
    if (cell && cell.v && typeof cell.v.v !== 'undefined') {
      const val = Number(cell.v.v)
      return Number.isNaN(val) ? 0 : val
    }
  } else if (sheet.data && sheet.data[r] && sheet.data[r][c]) {
    const val = Number(sheet.data[r][c].v)
    return Number.isNaN(val) ? 0 : val
  }
  return 0
}

export function resolveSsheetValues(
  refs: SsheetReference[],
  spreadsheets: SpreadsheetSpec[]
): Map<string, string> {
  const map = new Map<string, string>()

  for (const ref of refs) {
    let spec: SpreadsheetSpec | undefined
    let sheetName = ref.sheetName
    let spreadsheetName = ref.spreadsheetName

    if (spreadsheetName) {
      // First try to match by workbook (spreadsheet) name
      spec = spreadsheets.find((s) => s.name.toLowerCase() === spreadsheetName!.toLowerCase())
      
      // If no workbook matched, maybe it was a Sheet name (e.g. ssheet('Sheet1', 'A1'))
      if (!spec) {
        const matchingWorkbooks = spreadsheets.filter((s) => 
          (s.sheets as LooseSheet[]).some(sh => sh.name && sh.name.toLowerCase() === spreadsheetName!.toLowerCase())
        )
        if (matchingWorkbooks.length > 1) {
          throw new Error(`Ambiguous spreadsheet reference: Multiple workbooks contain a sheet named '${spreadsheetName}'. Please specify the workbook explicitly.`)
        } else if (matchingWorkbooks.length === 1) {
          spec = matchingWorkbooks[0]
          sheetName = spreadsheetName
          spreadsheetName = undefined
        }
      }
    }

    // Fallback to the first available spreadsheet if still not found
    if (!spec) {
      spec = spreadsheets[0]
    }

    if (!spec || !spec.sheets || spec.sheets.length === 0) {
      map.set(ref.match, '[0]') // fallback
      continue
    }

    let sheet: LooseSheet = spec.sheets[0] as LooseSheet
    if (sheetName) {
      const found = (spec.sheets as LooseSheet[]).find(
        (sh) => sh.name && sh.name.toLowerCase() === sheetName!.toLowerCase()
      )
      if (found) sheet = found
    }

    const parts = ref.range.split(':')
    if (parts.length === 1) {
      // Scalar
      const cell = parseCell(parts[0])
      if (cell) {
        map.set(ref.match, String(getCellValue(sheet, cell.r, cell.c)))
      } else {
        map.set(ref.match, '0')
      }
    } else if (parts.length === 2) {
      // Range
      const start = parseCell(parts[0])
      const end = parseCell(parts[1])
      if (!start || !end) {
        map.set(ref.match, '[0]')
        continue
      }

      const minR = Math.min(start.r, end.r)
      const maxR = Math.max(start.r, end.r)
      const minC = Math.min(start.c, end.c)
      const maxC = Math.max(start.c, end.c)

      if (minR === maxR) {
        // Row vector
        const vals: number[] = []
        for (let c = minC; c <= maxC; c++) vals.push(getCellValue(sheet, minR, c))
        map.set(ref.match, `[${vals.join(', ')}]`)
      } else if (minC === maxC) {
        // Column vector
        const vals: string[] = []
        for (let r = minR; r <= maxR; r++) vals.push(String(getCellValue(sheet, r, minC)))
        map.set(ref.match, `[${vals.join('; ')}]`)
      } else {
        // Matrix
        const rows: string[] = []
        for (let r = minR; r <= maxR; r++) {
          const rowVals: number[] = []
          for (let c = minC; c <= maxC; c++) {
            rowVals.push(getCellValue(sheet, r, c))
          }
          rows.push(rowVals.join(', '))
        }
        map.set(ref.match, `[${rows.join('; ')}]`)
      }
    }
  }

  return map
}

export function substituteSsheetRefs(text: string, spreadsheets: SpreadsheetSpec[]): string {
  const refs = parseSsheetReferences(text)
  if (refs.length === 0) return text

  const replacements = resolveSsheetValues(refs, spreadsheets)
  let result = text
  // Replace each literal match. Because identical matches yield the same replacement,
  // simple global replacement per unique match is safe.
  const uniqueMatches = Array.from(new Set(refs.map(r => r.match)))
  for (const match of uniqueMatches) {
    const val = replacements.get(match)
    if (val !== undefined) {
      // Escape special characters in match for regex
      const escapedMatch = match.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
      result = result.replace(new RegExp(escapedMatch, 'g'), val)
    }
  }
  
  return result
}
