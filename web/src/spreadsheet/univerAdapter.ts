// spreadsheet/univerAdapter.ts
//
// Conversions between the canonical stored format — the `celldata` array
// ({ r, c, v: { v, m, f? } }) plus a CSS-string `styles` map keyed by A1 —
// and Univer's IWorkbookData snapshot. Everything outside the spreadsheet
// module (App auto-sync, ssheetResolver, bindings, CSV, linked tables,
// .frees files) speaks the stored format, so the grid library stays
// isolated to SpreadsheetTab + this module.

import { LocaleType } from '@univerjs/presets'
import type { ICellData, IStyleData, IWorkbookData, IWorksheetData } from '@univerjs/presets'

export interface StoredCellValue {
  v: string | number
  m: string
  f?: string
}

export interface StoredCell {
  r: number
  c: number
  v: StoredCellValue
}

export interface StoredSheet {
  name: string
  id: string
  status?: number
  order?: number
  celldata: StoredCell[]
  /** CSS strings keyed by A1 ref (e.g. { A1: 'font-weight:bold;' }). */
  styles?: Record<string, string>
  config?: unknown
  /** Optional minimum grid dimensions (bound table sheets ask for headroom
   * so Add Row / pastes don't hit Univer's "Range is out of bounds"). */
  rowCount?: number
  columnCount?: number
}

const MIN_ROWS = 100
const MIN_COLS = 26

export function colName(c: number): string {
  let s = ''
  let t = c
  while (t >= 0) {
    s = String.fromCodePoint(65 + (t % 26)) + s
    t = Math.floor(t / 26) - 1
  }
  return s
}

export function parseA1(ref: string): { r: number; c: number } | null {
  const m = ref.toUpperCase().match(/^([A-Z]+)(\d+)$/)
  if (!m) return null
  let c = 0
  for (let i = 0; i < m[1].length; i++) c = c * 26 + ((m[1].codePointAt(i) ?? 0) - 64)
  return { r: Number.parseInt(m[2], 10) - 1, c: c - 1 }
}

// ---------------------------------------------------------------------------
// CSS string <-> Univer IStyleData
//
// The stored `styles` map keeps CSS strings (the shape the old grids emitted
// and .frees files already contain). Only the properties the formatting
// toolbars ever produced are mapped; unknown properties are dropped.

const HT = { left: 1, center: 2, right: 3 } as const
const VT = { top: 1, middle: 2, bottom: 3 } as const

export function cssToStyleData(css: string): IStyleData | undefined {
  const s: IStyleData = {}
  let any = false
  for (const decl of css.split(';')) {
    const i = decl.indexOf(':')
    if (i < 0) continue
    const prop = decl.slice(0, i).trim().toLowerCase()
    const val = decl.slice(i + 1).trim()
    if (!val) continue
    switch (prop) {
      case 'font-weight':
        if (val === 'bold' || Number(val) >= 600) { s.bl = 1; any = true }
        break
      case 'font-style':
        if (val === 'italic') { s.it = 1; any = true }
        break
      case 'text-decoration':
        if (val.includes('underline')) { s.ul = { s: 1 }; any = true }
        if (val.includes('line-through')) { s.st = { s: 1 }; any = true }
        break
      case 'color':
        s.cl = { rgb: val }; any = true
        break
      case 'background-color':
        s.bg = { rgb: val }; any = true
        break
      case 'text-align': {
        const ht = HT[val.toLowerCase() as keyof typeof HT]
        if (ht) { s.ht = ht; any = true }
        break
      }
      case 'vertical-align': {
        const vt = VT[val.toLowerCase() as keyof typeof VT]
        if (vt) { s.vt = vt; any = true }
        break
      }
      case 'font-size': {
        const n = Number.parseFloat(val)
        if (Number.isFinite(n) && n > 0) {
          // Univer font size is pt; stored CSS was px.
          s.fs = val.endsWith('pt') ? n : Math.round(n * 0.75 * 100) / 100
          any = true
        }
        break
      }
      case 'font-family':
        s.ff = val.replace(/^['"]|['"]$/g, '')
        any = true
        break
      case 'border': {
        // "1px solid #rrggbb" — used by the Tables workbook to draw the bound
        // region as a real grid. Width/style are fixed thin lines.
        const color = val.split(/\s+/).find((p) => p.startsWith('#'))
        if (color) {
          const side = { s: 1, cl: { rgb: color } }
          s.bd = { t: side, b: side, l: side, r: side }
          any = true
        }
        break
      }
    }
  }
  return any ? s : undefined
}

export function styleDataToCss(s: IStyleData | null | undefined): string {
  if (!s) return ''
  const out: string[] = []
  if (s.bl === 1) out.push('font-weight: bold')
  if (s.it === 1) out.push('font-style: italic')
  const deco: string[] = []
  if (s.ul?.s === 1) deco.push('underline')
  if (s.st?.s === 1) deco.push('line-through')
  if (deco.length) out.push(`text-decoration: ${deco.join(' ')}`)
  if (s.cl?.rgb) out.push(`color: ${s.cl.rgb}`)
  if (s.bg?.rgb) out.push(`background-color: ${s.bg.rgb}`)
  const htName = (Object.keys(HT) as (keyof typeof HT)[]).find((k) => HT[k] === s.ht)
  if (htName) out.push(`text-align: ${htName}`)
  const vtName = (Object.keys(VT) as (keyof typeof VT)[]).find((k) => VT[k] === s.vt)
  if (vtName) out.push(`vertical-align: ${vtName}`)
  if (typeof s.fs === 'number' && s.fs > 0) out.push(`font-size: ${Math.round((s.fs / 0.75) * 100) / 100}px`)
  if (s.ff) out.push(`font-family: ${s.ff}`)
  const bdColor = s.bd?.t?.cl?.rgb ?? s.bd?.b?.cl?.rgb ?? s.bd?.l?.cl?.rgb ?? s.bd?.r?.cl?.rgb
  if (bdColor) out.push(`border: 1px solid ${bdColor}`)
  return out.length ? out.join('; ') + ';' : ''
}

// ---------------------------------------------------------------------------
// Stored sheets -> IWorkbookData (load path)

export function sheetsToWorkbookData(id: string, name: string, sheets: StoredSheet[]): IWorkbookData {
  const wsheets: IWorkbookData['sheets'] = {}
  const sheetOrder: string[] = []

  sheets.forEach((sh, i) => {
    const sid = sh.id || String(i)
    sheetOrder.push(sid)

    let maxR = 0
    let maxC = 0
    const cellData: Record<number, Record<number, ICellData>> = {}
    for (const cd of sh.celldata ?? []) {
      const src = cd.v ?? ({} as StoredCellValue)
      const cell: ICellData = {}
      if (src.f) {
        // Formula cells get only `f`, never the cached `v`: Univer's default
        // load mode computes (and dependency-registers) only formula cells
        // with an empty value — a cached `v` would load as a frozen number
        // that no longer recalculates when its inputs change.
        cell.f = src.f
      } else if (src.v !== undefined && src.v !== null && src.v !== '') {
        cell.v = src.v
      } else {
        continue
      }
      ;(cellData[cd.r] ??= {})[cd.c] = cell
      if (cd.r > maxR) maxR = cd.r
      if (cd.c > maxC) maxC = cd.c
    }

    for (const [ref, css] of Object.entries(sh.styles ?? {})) {
      const pos = parseA1(ref)
      if (!pos) continue
      const style = cssToStyleData(css)
      if (!style) continue
      const cell = ((cellData[pos.r] ??= {})[pos.c] ??= {})
      cell.s = style
      if (pos.r > maxR) maxR = pos.r
      if (pos.c > maxC) maxC = pos.c
    }

    wsheets[sid] = {
      id: sid,
      name: sh.name || `Sheet${i + 1}`,
      rowCount: Math.max(MIN_ROWS, maxR + 20, sh.rowCount ?? 0),
      columnCount: Math.max(MIN_COLS, maxC + 5, sh.columnCount ?? 0),
      cellData,
    } as Partial<IWorksheetData>
  })

  return {
    id,
    name,
    appVersion: '',
    locale: LocaleType.EN_US,
    sheetOrder,
    styles: {},
    sheets: wsheets,
  } as IWorkbookData
}

// ---------------------------------------------------------------------------
// Univer workbook -> stored sheets (persist path)
//
// Reads through the Facade so formulas come back per-cell with shared-formula
// (`si`) references resolved/offset (the snapshot alone keeps only the master
// cell's `f`), and values come back computed. The snapshot supplies sheet
// order/names and the style table.

interface FRangeLike {
  getValues(): (string | number | boolean | null)[][]
  getFormulas(): string[][]
}
interface FWorksheetLike {
  getSheetId(): string
  getRange(row: number, column: number, numRows: number, numColumns: number): FRangeLike
}
export interface FWorkbookLike {
  save(): IWorkbookData
  getSheets(): FWorksheetLike[]
  getActiveSheet(): FWorksheetLike | null
}

export function extractSheets(fwb: FWorkbookLike, prevSheets: StoredSheet[]): StoredSheet[] {
  const snapshot = fwb.save()
  const styleTable = (snapshot.styles ?? {}) as Record<string, IStyleData>
  const facadeById = new Map(fwb.getSheets().map((ws) => [ws.getSheetId(), ws]))
  const activeId = fwb.getActiveSheet()?.getSheetId()
  const prevById = new Map(prevSheets.map((p) => [p.id, p]))

  return snapshot.sheetOrder.map((sid, i) => {
    const sh = snapshot.sheets[sid]
    const prev = prevById.get(sid)
    const cellData = (sh?.cellData ?? {}) as Record<number, Record<number, ICellData>>

    let maxR = -1
    let maxC = -1
    for (const r of Object.keys(cellData)) {
      const row = cellData[Number(r)]
      if (!row) continue
      if (Number(r) > maxR) maxR = Number(r)
      for (const c of Object.keys(row)) if (Number(c) > maxC) maxC = Number(c)
    }

    const celldata: StoredCell[] = []
    const styles: Record<string, string> = {}

    if (maxR >= 0 && maxC >= 0) {
      const fws = facadeById.get(sid)
      const range = fws?.getRange(0, 0, maxR + 1, maxC + 1)
      const values = range?.getValues() ?? []
      const formulas = range?.getFormulas() ?? []

      for (let r = 0; r <= maxR; r++) {
        for (let c = 0; c <= maxC; c++) {
          const raw = cellData[r]?.[c]
          const f = formulas[r]?.[c] || (typeof raw?.f === 'string' ? raw.f : '')
          let v = values[r]?.[c]
          if (v === undefined || v === null) v = raw?.v ?? null

          if (raw?.s) {
            const style = typeof raw.s === 'string' ? styleTable[raw.s] : (raw.s as IStyleData)
            const css = styleDataToCss(style)
            if (css) styles[`${colName(c)}${r + 1}`] = css
          }

          if (!f && (v === null || v === '')) continue
          const str = v === null || v === undefined ? '' : String(v)
          const num = typeof v === 'number' ? v : Number(str)
          const computed: string | number = str.trim() !== '' && !Number.isNaN(num) ? num : str
          const cell: StoredCell = { r, c, v: { v: computed, m: str } }
          if (f) cell.v.f = f
          celldata.push(cell)
        }
      }
    }

    return {
      name: sh?.name || `Sheet${i + 1}`,
      id: sid,
      status: sid === activeId ? 1 : 0,
      order: i,
      celldata,
      styles,
      config: prev?.config ?? {},
    }
  })
}
