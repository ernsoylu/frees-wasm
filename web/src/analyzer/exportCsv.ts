// CSV export for the Data Analyzer (Phase 2): selected signals × visible time
// range on a merged raster with step-hold fill — the same fill rule the Table
// instrument shows. The emitted layout (header row + bracketed unit row) is
// exactly what csvImport.ts detects, so an exported file re-imports cleanly.

import { lowerBound } from './decimate'
import { mergeTimestamps, stepHoldAt } from './stats'

export interface ExportSignal {
  name: string
  unit?: string
  t: Float64Array
  v: Float64Array
}

/**
 * Build CSV text for the signals over [from, to] (null = full recording).
 * Row raster = union of all signal timestamps within the window; cells are
 * step-hold filled; a cell before a signal's first sample (or a NaN gap)
 * stays empty.
 */
export function buildCsv(
  signals: readonly ExportSignal[],
  from: number | null,
  to: number | null,
): string {
  const windows = signals.map((s) => {
    const i0 = from === null ? 0 : lowerBound(s.t, from)
    let i1 = to === null ? s.t.length : lowerBound(s.t, to)
    if (i1 < s.t.length && s.t[i1] <= (to ?? Infinity)) i1++
    return s.t.subarray(i0, i1)
  })
  const raster = mergeTimestamps(windows)

  const lines: string[] = []
  lines.push(['time', ...signals.map((s) => csvCell(s.name))].join(','))
  lines.push(['[s]', ...signals.map((s) => (s.unit ? `[${s.unit}]` : ''))].join(','))
  for (let r = 0; r < raster.length; r++) {
    const ts = raster[r]
    const cells: string[] = [String(ts)]
    for (const s of signals) {
      const x = stepHoldAt(s.t, s.v, ts)
      cells.push(Number.isNaN(x) ? '' : String(x))
    }
    lines.push(cells.join(','))
  }
  return `${lines.join('\n')}\n`
}

function csvCell(s: string): string {
  return /[",\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s
}

/** Trigger a browser download of CSV text (same pattern as project.ts). */
export function downloadCsv(filename: string, text: string) {
  const blob = new Blob([text], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename.endsWith('.csv') ? filename : `${filename}.csv`
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}
