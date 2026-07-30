// Scatter instrument (todo.md Phase 5b): signal-vs-signal correlation over
// the cursor-bounded range (A–B, else the visible window, else everything),
// paired on the merged raster with step-hold fill. Plotly, per plan — the
// scatter path is not perf-critical.

import { useMemo, useState } from 'react'
import { Group, Select, Text, useComputedColorScheme } from '@mantine/core'
import type { PlotlyFigure } from 'plotly.js-dist-min'
import PlotlyChart from '../../plots/PlotlyChart'
import type { AbCursors } from '../UPlotChart'
import { mergeTimestamps, stepHoldAt } from '../stats'
import { offsetRawRange } from '../offsets'
import type { AnalyzerSignal } from '../types'

const MAX_POINTS = 10_000

interface Props {
  signals: AnalyzerSignal[]
  offsets: Map<string, number>
  xRange: [number, number] | null
  cursors: AbCursors
  storeVersion: number
}

export default function ScatterInstrument({ signals, offsets, xRange, cursors, storeVersion }: Readonly<Props>) {
  const dark = useComputedColorScheme('dark') === 'dark'
  const [xSel, setXSel] = useState<string | null>(
    signals.length > 0 ? `${signals[0].measurementId}|${signals[0].channel}` : null,
  )
  const [ySel, setYSel] = useState<string | null>(
    signals.length > 1 ? `${signals[1].measurementId}|${signals[1].channel}` : null,
  )

  const range: [number, number] | null =
    cursors.a !== null && cursors.b !== null
      ? [Math.min(cursors.a, cursors.b), Math.max(cursors.a, cursors.b)]
      : xRange

  const figure = useMemo<PlotlyFigure | null>(() => {
    if (xSel === null || ySel === null) return null
    const [xm, xc] = xSel.split('|')
    const [ym, yc] = ySel.split('|')
    const rawX = offsetRawRange({ measurementId: xm, channel: xc }, offsets.get(xm) ?? 0, range?.[0] ?? null, range?.[1] ?? null)
    const rawY = offsetRawRange({ measurementId: ym, channel: yc }, offsets.get(ym) ?? 0, range?.[0] ?? null, range?.[1] ?? null)
    if (!rawX || !rawY) return null
    const raster = mergeTimestamps([rawX.t, rawY.t])
    const stride = Math.max(1, Math.ceil(raster.length / MAX_POINTS))
    const xs: (number | null)[] = []
    const ys: (number | null)[] = []
    for (let i = 0; i < raster.length; i += stride) {
      const x = stepHoldAt(rawX.t, rawX.v, raster[i])
      const y = stepHoldAt(rawY.t, rawY.v, raster[i])
      if (Number.isNaN(x) || Number.isNaN(y)) continue
      xs.push(x)
      ys.push(y)
    }
    const axisColor = dark ? '#909296' : '#495057'
    const gridColor = dark ? 'rgba(134,142,150,0.15)' : 'rgba(134,142,150,0.3)'
    return {
      data: [
        {
          type: 'scatter',
          mode: 'markers',
          name: `${yc} vs ${xc}`,
          x: xs,
          y: ys,
          marker: { color: '#4dabf7', size: 3, opacity: 0.6 },
        },
      ],
      layout: {
        paper_bgcolor: 'rgba(0,0,0,0)',
        plot_bgcolor: 'rgba(0,0,0,0)',
        font: { color: axisColor, size: 11 },
        margin: { l: 55, r: 12, t: 12, b: 42 },
        showlegend: false,
        xaxis: { title: { text: rawX.unit ? `${xc} [${rawX.unit}]` : xc }, gridcolor: gridColor, zerolinecolor: gridColor },
        yaxis: { title: { text: rawY.unit ? `${yc} [${rawY.unit}]` : yc }, gridcolor: gridColor, zerolinecolor: gridColor },
      },
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [xSel, ySel, range?.[0], range?.[1], offsets, storeVersion, dark])

  if (signals.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign signals to strips to correlate them.
      </Text>
    )
  }

  const options = signals.map((s) => ({ value: `${s.measurementId}|${s.channel}`, label: s.channel }))

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <Group gap="sm" mb={4}>
        <Select size="xs" w={220} label="X signal" searchable data={options} value={xSel} onChange={setXSel} />
        <Select size="xs" w={220} label="Y signal" searchable data={options} value={ySel} onChange={setYSel} />
        <Text size="xs" c="dimmed" mt={22}>
          Range: {range ? `${range[0].toPrecision(6)} s → ${range[1].toPrecision(6)} s` : 'full recording'}
        </Text>
      </Group>
      <div style={{ flex: 1, minHeight: 0 }}>{figure && <PlotlyChart figure={figure} minHeight={260} />}</div>
    </div>
  )
}
