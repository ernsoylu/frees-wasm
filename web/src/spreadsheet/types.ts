// spreadsheet/types.ts

export interface SpreadsheetSpec {
  id: string
  name: string
  /** Sheet data array — opaque JSON for persistence. Each entry is
   *  `{ name, id, celldata, styles, … }`; `celldata` keeps the legacy
   *  `{ r, c, v: { v, m, f? } }` cell shape that App/resolver/bindings read. */
  sheets: unknown[]
  /** Bindings mapping variable names to cell references (e.g. "Sheet1!A1") for input */
  bindings?: Record<string, string>
  /** Bindings mapping variable names to cell references for auto-syncing results */
  resultBindings?: Record<string, string>
  /** Whether to auto-sync resultBindings after a successful solve */
  autoSync?: boolean
  /** SUPERSEDED (kept parsed for downgrade safety, contract d of the
   * table-spreadsheet unification plan): the old one-off link syncing a
   * parametric table into sheet 0. The Tables workbook hosts parametric
   * tables as bound sheets now, so this link is inert — never acted on, only
   * stripped by the explicit "Unlink" button in the spreadsheet toolbar. */
  linkedTableId?: string
}

export function emptySpreadsheetData(): unknown[] {
  return [{
    name: 'Sheet1',
    id: '0',
    status: 1,        // active
    order: 0,
    celldata: [],
    /** Cell styles: CSS strings keyed by A1 ref (e.g. { A1: 'font-weight:bold;' }). */
    styles: {},
    config: {},
  }]
}
