// Table instrument (todo.md Phase 2): values of every assigned
// signal by timestamp over the visible window, empty cells filled by
// step-hold (§ stats.ts). Rendered through glide-data-grid (canvas,
// virtualized) with O(log n) cell lookup — the merged raster is the only
// materialized array, so 200k-row windows stay smooth.

import { useCallback, useMemo } from 'react'
import { DataEditor, GridCellKind, type GridCell, type GridColumn, type Item } from '@glideapps/glide-data-grid'
import '@glideapps/glide-data-grid/dist/index.css'
import { Alert, Stack, Text } from '@mantine/core'
import { useElementSize } from '@mantine/hooks'
import { IconAlertTriangle } from '@tabler/icons-react'
import { useGlideTheme } from '../../DataGridReadOnly'
import { formatValue } from '../../format'
import { offsetRawRange } from '../offsets'
import { mergeTimestamps, stepHoldAt } from '../stats'
import type { AnalyzerSignal } from '../types'

const MAX_ROWS = 200_000

interface Props {
  /** All assigned signals, in strip order. */
  signals: AnalyzerSignal[]
  /** Per-file display-time offsets (Phase 5a). */
  offsets: Map<string, number>
  xRange: [number, number] | null
  /** ChannelStore version — invalidates the model on register/evict. */
  storeVersion: number
}

export default function TableInstrument({ signals, offsets, xRange, storeVersion }: Readonly<Props>) {
  const gridTheme = useGlideTheme()
  const { ref, width, height } = useElementSize()

  const model = useMemo(() => {
    const from = xRange?.[0] ?? null
    const to = xRange?.[1] ?? null
    const cols: {
      signal: AnalyzerSignal
      unit?: string
      /** Full-resolution arrays: step-hold must reach BEFORE the window. */
      t: Float64Array
      v: Float64Array
    }[] = []
    const windowT: Float64Array[] = []
    for (const sig of signals) {
      const off = offsets.get(sig.measurementId) ?? 0
      const full = offsetRawRange(sig, off, null, null)
      const windowed = offsetRawRange(sig, off, from, to)
      if (!full || !windowed) continue
      cols.push({ signal: sig, unit: full.unit, t: full.t, v: full.v })
      windowT.push(windowed.t)
    }
    const raster = mergeTimestamps(windowT)
    const truncated = raster.length > MAX_ROWS
    return { cols, raster: truncated ? raster.subarray(0, MAX_ROWS) : raster, truncated }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signals, offsets, xRange, storeVersion])

  const columns = useMemo<GridColumn[]>(
    () => [
      { id: 'time', title: 'time [s]', width: 140 },
      ...model.cols.map((c) => ({
        id: `${c.signal.measurementId}:${c.signal.channel}`,
        title: c.unit ? `${c.signal.channel} [${c.unit}]` : c.signal.channel,
        width: Math.max(110, c.signal.channel.length * 8 + 40),
        themeOverride: { textDark: c.signal.color },
      })),
    ],
    [model],
  )

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const ts = model.raster[row]
      let text: string
      if (col === 0) {
        text = ts === undefined ? '' : String(Number(ts.toPrecision(10)))
      } else {
        const c = model.cols[col - 1]
        const x = c === undefined || ts === undefined ? Number.NaN : stepHoldAt(c.t, c.v, ts)
        text = Number.isNaN(x) ? '' : formatValue(x)
      }
      return {
        kind: GridCellKind.Text,
        data: text,
        displayData: text,
        allowOverlay: false,
        contentAlign: 'right',
      }
    },
    [model],
  )

  if (model.cols.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign signals to strips to populate the table.
      </Text>
    )
  }

  return (
    <Stack gap={6} style={{ flex: 1, minHeight: 0 }} h="100%">
      {model.truncated && (
        <Alert color="yellow" p="xs" icon={<IconAlertTriangle size={14} />}>
          <Text size="xs">
            Showing the first {MAX_ROWS.toLocaleString()} rows of the window — zoom in to narrow
            the range.
          </Text>
        </Alert>
      )}
      <div ref={ref} style={{ flex: 1, minHeight: 0, width: '100%', position: 'relative' }}>
        {width > 0 && height > 0 && (
          <DataEditor
            theme={gridTheme}
            columns={columns}
            rows={model.raster.length}
            getCellContent={getCellContent}
            width={width}
            height={height}
            rowMarkers="number"
            smoothScrollX
            smoothScrollY
          />
        )}
      </div>
    </Stack>
  )
}
