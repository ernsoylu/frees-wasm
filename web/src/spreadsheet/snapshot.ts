// spreadsheet/snapshot.ts
//
// "Open in Spreadsheet" snapshots (Phase 3, contract e of the unification
// plan): a ONE-SHOT copy of a read-only result grid (ODE/parametric runs,
// fluid states) into a normal persisted spreadsheet sheet — deliberately
// decoupled from re-solves. Univer-free (type-only adapter import): callers
// build stored celldata, never touch the engine.

import type { StoredCell, StoredSheet } from './univerAdapter'

/** Above this the user must confirm — snapshot cells persist as celldata in
 * the .frees file and are parsed by Univer on every load. */
export const SNAPSHOT_WARN_ROWS = 5000
/** Hard cap: bigger data belongs in the CSV export, not the project file. */
export const SNAPSHOT_MAX_ROWS = 10000

export interface SnapshotColumn {
  name: string
  unit?: string
}

export interface SnapshotInput {
  /** Base name; the sheet/spreadsheet name gets a timestamp appended so a
   * static snapshot is never mistaken for a live table (contract e). */
  title: string
  columns: SnapshotColumn[]
  rows: (string | number | null | undefined)[][]
}

export type SnapshotOutcome =
  | { ok: true; name: string; sheet: StoredSheet }
  | { ok: false; reason: 'cancelled' | 'too-big'; message: string }

export function snapshotStamp(): string {
  return new Date().toISOString().slice(0, 16).replace('T', ' ')
}

export function buildSnapshotSheet(
  input: SnapshotInput,
  confirmFn: (message: string) => boolean = (m) => window.confirm(m),
): SnapshotOutcome {
  if (input.rows.length > SNAPSHOT_MAX_ROWS) {
    return {
      ok: false,
      reason: 'too-big',
      message: `${input.rows.length.toLocaleString()} rows exceed the ${SNAPSHOT_MAX_ROWS.toLocaleString()}-row snapshot limit — use the CSV export instead (snapshots persist inside the .frees file).`,
    }
  }
  if (input.rows.length > SNAPSHOT_WARN_ROWS) {
    const go = confirmFn(
      `This snapshot has ${input.rows.length.toLocaleString()} rows; it will be stored inside the .frees project file and parsed on every load. Continue?`,
    )
    if (!go) return { ok: false, reason: 'cancelled', message: '' }
  }

  const name = `${input.title} (${snapshotStamp()})`
  const celldata: StoredCell[] = []
  const styles: Record<string, string> = {}

  // Local A1 column naming (duplicating univerAdapter.colName) keeps this
  // module free of value imports from the Univer chunk.
  const colRef = (c: number): string => {
    let s = ''
    let t = c
    while (t >= 0) {
      s = String.fromCodePoint(65 + (t % 26)) + s
      t = Math.floor(t / 26) - 1
    }
    return s
  }
  input.columns.forEach((col, c) => {
    const label = col.unit ? `${col.name} [${col.unit}]` : col.name
    celldata.push({ r: 0, c, v: { v: label, m: label } })
    styles[`${colRef(c)}1`] = 'font-weight: bold;'
  })

  input.rows.forEach((row, i) => {
    row.forEach((raw, c) => {
      if (raw === null || raw === undefined || raw === '') return
      const str = String(raw)
      const num = typeof raw === 'number' ? raw : Number(str)
      celldata.push({
        r: i + 1,
        c,
        v: { v: str.trim() !== '' && Number.isFinite(num) ? num : str, m: str },
      })
    })
  })

  return {
    ok: true,
    name,
    sheet: { name, id: '0', status: 1, order: 0, celldata, styles, config: {} },
  }
}
