import { useCallback, useMemo } from 'react'
import { DataEditor, GridCell, GridCellKind, GridColumn, Item } from '@glideapps/glide-data-grid'
import '@glideapps/glide-data-grid/dist/index.css'
import { useElementSize } from '@mantine/hooks'
import { useGlideTheme } from './DataGridReadOnly'
import type { ArrayGroup } from './workspaceData'
import { VariableResult } from './api'
import { formatValue } from './format'

// ---------------------------------------------------------------------------
// Virtualized (canvas) fallbacks for the Variable Explorer, lazy-loaded by
// Workspace.tsx only once a solve produces more rows/cells than the Mantine
// tables can render without jank. Rendering cost is bounded by the visible
// window, not the system size. Same glide theme as the ODE/code tables.
// ---------------------------------------------------------------------------

// glide defaults: 34 px rows below a 36 px header (+1 px border).
const GRID_MAX_HEIGHT = 440
function gridHeight(rowCount: number): number {
  return Math.min(rowCount * 34 + 37, GRID_MAX_HEIGHT)
}

const readOnlyText = (value: string, align: 'left' | 'right' = 'left'): GridCell => ({
  kind: GridCellKind.Text,
  data: value,
  displayData: value,
  allowOverlay: false,
  contentAlign: align,
})

/** Scalar list (Name/Value/Type/Units/Uncertainty) as a virtualized grid. */
export function ScalarGrid({ scalars, replNames }: Readonly<{ scalars: VariableResult[]; replNames: Set<string> }>) {
  const { ref, width } = useElementSize()
  const gridTheme = useGlideTheme()

  const columns = useMemo<GridColumn[]>(
    () => [
      { id: 'name', title: 'Name', width: 180 },
      { id: 'value', title: 'Value', width: 130 },
      { id: 'type', title: 'Type', width: 100 },
      { id: 'units', title: 'Units', width: 110 },
      { id: 'unc', title: 'Uncertainty', width: 120 },
    ],
    [],
  )

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const v = scalars[row]
      if (!v) return readOnlyText('')
      switch (col) {
        case 0:
          return readOnlyText(v.name)
        case 1:
          return readOnlyText(formatValue(v.value), 'right')
        case 2:
          return readOnlyText(replNames.has(v.name.toLowerCase()) ? 'Scalar (repl)' : 'Scalar')
        case 3:
          return readOnlyText(v.units || '—')
        default:
          return readOnlyText(
            v.uncertainty != null && v.uncertainty !== 0 ? `± ${formatValue(v.uncertainty)}` : '—',
            'right',
          )
      }
    },
    [scalars, replNames],
  )

  const height = gridHeight(scalars.length)
  return (
    <div ref={ref} style={{ width: '100%', height }}>
      {width > 0 && (
        <DataEditor
          theme={gridTheme}
          columns={columns}
          rows={scalars.length}
          getCellContent={getCellContent}
          width={width}
          height={height}
          rowMarkers="none"
          smoothScrollY
        />
      )}
    </div>
  )
}

/** Expanded vector/matrix body as a virtualized grid (rows AND columns windowed). */
export function MatrixGrid({ g }: Readonly<{ g: ArrayGroup }>) {
  const { ref, width } = useElementSize()
  const gridTheme = useGlideTheme()

  const columns = useMemo<GridColumn[]>(
    () =>
      g.is2D
        ? [
            { id: 'r', title: 'r\\c', width: 60 },
            ...g.cols.map((c) => ({ id: `c${c}`, title: String(c), width: 110 })),
          ]
        : [
            { id: 'r', title: 'Index', width: 80 },
            { id: 'v', title: 'Value', width: 140 },
          ],
    [g],
  )

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const r = g.rows[row]
      if (r === undefined) return readOnlyText('')
      if (col === 0) return readOnlyText(String(r))
      const cell = g.is2D ? g.cells.get(`${r},${g.cols[col - 1]}`) : g.cells.get(`${r}`)
      return readOnlyText(cell ? formatValue(cell.value) : '—', 'right')
    },
    [g],
  )

  const height = gridHeight(g.rows.length)
  return (
    <div ref={ref} style={{ width: '100%', height, padding: 0 }}>
      {width > 0 && (
        <DataEditor
          theme={gridTheme}
          columns={columns}
          rows={g.rows.length}
          getCellContent={getCellContent}
          width={width}
          height={height}
          rowMarkers="none"
          smoothScrollX
          smoothScrollY
        />
      )}
    </div>
  )
}
