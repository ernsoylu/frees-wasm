// Histogram instrument (todo.md Phase 5b): value distribution of one signal
// over the cursor-bounded range (A–B, else visible window, else everything).

import { useMemo, useState } from 'react'
import { Group, NumberInput, Select, Text, useComputedColorScheme } from '@mantine/core'
import type { PlotlyFigure } from 'plotly.js-dist-min'
import PlotlyChart from '../../plots/PlotlyChart'
import type { AbCursors } from '../UPlotChart'
import { offsetRawRange } from '../offsets'
import type { AnalyzerSignal } from '../types'

const MAX_SAMPLES = 200_000

interface Props {
  signals: AnalyzerSignal[]
  offsets: Map<string, number>
  xRange: [number, number] | null
  cursors: AbCursors
  storeVersion: number
}

export default function HistogramInstrument({ signals, offsets, xRange, cursors, storeVersion }: Readonly<Props>) {
  const dark = useComputedColorScheme('dark') === 'dark'
  const [selection, setSelection] = useState<string | null>(
    signals.length > 0 ? `${signals[0].measurementId}|${signals[0].channel}` : null,
  )
  const [bins, setBins] = useState<number | string>(50)

  const range: [number, number] | null =
    cursors.a !== null && cursors.b !== null
      ? [Math.min(cursors.a, cursors.b), Math.max(cursors.a, cursors.b)]
      : xRange

  const figure = useMemo<PlotlyFigure | null>(() => {
    if (selection === null) return null
    const [measurementId, channel] = selection.split('|')
    const raw = offsetRawRange(
      { measurementId, channel },
      offsets.get(measurementId) ?? 0,
      range?.[0] ?? null,
      range?.[1] ?? null,
    )
    if (!raw) return null
    const stride = Math.max(1, Math.ceil(raw.v.length / MAX_SAMPLES))
    const values: (number | null)[] = []
    for (let i = 0; i < raw.v.length; i += stride) {
      if (!Number.isNaN(raw.v[i])) values.push(raw.v[i])
    }
    const nbins = Math.max(2, Math.min(500, typeof bins === 'number' ? bins : Number(bins) || 50))
    const axisColor = dark ? '#909296' : '#495057'
    const gridColor = dark ? 'rgba(134,142,150,0.15)' : 'rgba(134,142,150,0.3)'
    return {
      data: [
        {
          type: 'histogram',
          name: channel,
          x: values,
          nbinsx: nbins,
          marker: { color: '#4dabf7', opacity: 0.85 },
        },
      ],
      layout: {
        paper_bgcolor: 'rgba(0,0,0,0)',
        plot_bgcolor: 'rgba(0,0,0,0)',
        font: { color: axisColor, size: 11 },
        margin: { l: 55, r: 12, t: 12, b: 42 },
        showlegend: false,
        xaxis: { title: { text: raw.unit ? `${channel} [${raw.unit}]` : channel }, gridcolor: gridColor, zerolinecolor: gridColor },
        yaxis: { title: { text: 'count' }, gridcolor: gridColor, zerolinecolor: gridColor },
      },
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, bins, range?.[0], range?.[1], offsets, storeVersion, dark])

  if (signals.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign signals to strips to see their distribution.
      </Text>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <Group gap="sm" mb={4}>
        <Select
          size="xs"
          w={240}
          label="Signal"
          searchable
          data={signals.map((s) => ({ value: `${s.measurementId}|${s.channel}`, label: s.channel }))}
          value={selection}
          onChange={setSelection}
        />
        <NumberInput size="xs" w={110} label="Bins" value={bins} onChange={setBins} min={2} max={500} />
        <Text size="xs" c="dimmed" mt={22}>
          Range: {range ? `${range[0].toPrecision(6)} s → ${range[1].toPrecision(6)} s` : 'full recording'}
        </Text>
      </Group>
      <div style={{ flex: 1, minHeight: 0 }}>{figure && <PlotlyChart figure={figure} minHeight={260} />}</div>
    </div>
  )
}
