// Table → measurement conversion (oscilloscope-tool parity: analyze solved table results in
// the oscilloscope). An ODE Table (DYNAMIC block trajectory) imports with its
// time column as the time base; a Parametric Table imports on a run-number
// base (1..N). Values prefer the numeric solve results over the formatted
// row strings, so no display-precision is lost where results exist.

import { sniffKind } from './calc'
import { displayVar } from '../varDisplay'
import type { ImportedMeasurement } from './csvImport'
import type { ParamTableSpec, TableSpec } from '../tables'

/** Parametric-kind tables (incl. ODE-origin) with at least one data row. */
export function importableTables(tables: TableSpec[]): ParamTableSpec[] {
  return tables.filter(
    (t): t is ParamTableSpec => t.kind === 'parametric' && t.rows.length > 0,
  )
}

/** Cell value for row i / column v: solved numeric result first, then the
 *  formatted row string; blank/unparsable → NaN. */
function cellValue(t: ParamTableSpec, i: number, v: string): number {
  const solved = t.results[i]
  if (solved?.success && typeof solved.values[v] === 'number') return solved.values[v]
  const raw = t.rows[i]?.values[v]
  if (raw === undefined || raw.trim() === '') return Number.NaN
  return Number(raw)
}

function columnValues(t: ParamTableSpec, v: string, rowIdx: number[]): Float64Array {
  const out = new Float64Array(rowIdx.length)
  for (let k = 0; k < rowIdx.length; k++) out[k] = cellValue(t, rowIdx[k], v)
  return out
}

function minMax(values: Float64Array): { min: number; max: number } {
  let min = Number.POSITIVE_INFINITY
  let max = Number.NEGATIVE_INFINITY
  for (const x of values) {
    if (Number.isNaN(x)) continue
    if (x < min) min = x
    if (x > max) max = x
  }
  return min === Number.POSITIVE_INFINITY
    ? { min: Number.NaN, max: Number.NaN }
    : { min, max }
}

/**
 * Wrap a solved table as an in-memory measurement for ChannelStore.register.
 * ODE-origin tables use their first column (time) as the time base — rows
 * with an unparsable time are dropped and the rest sorted ascending (the
 * store's binary searches need a monotonic axis). Parametric tables use the
 * run number (1..N). Returns null when nothing is plottable.
 */
export function tableToMeasurement(t: ParamTableSpec): ImportedMeasurement | null {
  const isOde = t.origin === 'ode'
  const timeVar = isOde ? t.vars[0] : null
  const channelVars = isOde ? t.vars.slice(1) : t.vars
  if (channelVars.length === 0 || t.rows.length === 0) return null

  // Row order: all rows for parametric; time-valid rows sorted by time for ODE.
  let rowIdx = t.rows.map((_, i) => i)
  let time: Float64Array
  if (timeVar !== null) {
    rowIdx = rowIdx.filter((i) => !Number.isNaN(cellValue(t, i, timeVar)))
    if (rowIdx.length === 0) return null
    rowIdx.sort((a, b) => cellValue(t, a, timeVar) - cellValue(t, b, timeVar))
    time = columnValues(t, timeVar, rowIdx)
  } else {
    time = new Float64Array(rowIdx.map((i) => i + 1))
  }

  const channels: ImportedMeasurement['channels'] = []
  for (const v of channelVars) {
    const values = columnValues(t, v, rowIdx)
    const { min, max } = minMax(values)
    if (Number.isNaN(min)) continue // no finite sample (e.g. string column)
    channels.push({
      // Component-expanded columns are stored flat with '$' (e.g. `ch.ev$hg`);
      // show the dotted form the rest of the app uses (`ch.ev.hg`).
      name: displayVar(v),
      unit: t.columnUnits?.[v] || undefined,
      kind: sniffKind(values),
      values,
      min,
      max,
    })
  }
  if (channels.length === 0) return null

  return {
    signatureName: `⌗ ${t.name}`,
    size: 0,
    headerHash: `table:${t.origin ?? 'parametric'}:${t.id}`,
    time,
    rowCount: rowIdx.length,
    channels,
  }
}
