import { useMemo, useState } from 'react'
import {
  Alert,
  Button,
  Group,
  Modal,
  NumberInput,
  ScrollArea,
  Stack,
  Table,
  Text,
} from '@mantine/core'
import type { PlotlyFigure } from 'plotly.js/lib/core'
import PlotlyChart from './plots/PlotlyChart'
import type { MonteCarloResult } from './api'

function fmt(v: number): string {
  if (!Number.isFinite(v)) return '—'
  const a = Math.abs(v)
  return a !== 0 && (a < 1e-3 || a >= 1e5) ? v.toExponential(3) : v.toPrecision(5)
}

/**
 * Monte Carlo uncertainty: run configuration (sample count, seed), the
 * per-variable statistics against the first-order sigmas, and a histogram of
 * the selected variable's sampled distribution.
 */
export default function MonteCarloModal({
  opened,
  onClose,
  onRun,
}: Readonly<{
  opened: boolean
  onClose: () => void
  onRun: (samples: number, seed: number) => Promise<MonteCarloResult>
}>) {
  const [samples, setSamples] = useState<number>(200)
  const [seed, setSeed] = useState<number>(42)
  const [running, setRunning] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<MonteCarloResult | null>(null)
  const [selected, setSelected] = useState<string | null>(null)

  const run = () => {
    setRunning(true)
    setError(null)
    onRun(samples, seed)
      .then((r) => {
        setResult(r)
        setSelected((prev) =>
          prev && r.stats.some((s) => s.variable === prev) ? prev : (r.stats[0]?.variable ?? null))
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setRunning(false))
  }

  const histogram: PlotlyFigure | null = useMemo(() => {
    if (!result || !selected) return null
    const values = result.samples
      .filter((s) => s.success)
      .map((s) => s.values[selected])
      .filter((v) => typeof v === 'number' && Number.isFinite(v))
    if (values.length < 2) return null
    return {
      data: [
        {
          type: 'histogram',
          x: values,
          opacity: 0.85,
          marker: { color: '#4dabf7' },
        },
      ],
      layout: {
        margin: { l: 50, r: 16, t: 8, b: 40 },
        xaxis: { title: { text: selected } },
        yaxis: { title: { text: 'Frequency' } },
        paper_bgcolor: 'rgba(0,0,0,0)',
        plot_bgcolor: 'rgba(0,0,0,0)',
        font: { color: 'var(--mantine-color-text)' },
      },
    } as PlotlyFigure
  }, [result, selected])

  const okSamples = result ? result.samples.length - result.failedSamples : 0

  return (
    <Modal opened={opened} onClose={onClose} title="Monte Carlo Uncertainty" size="xl" centered>
      <Stack gap="sm">
        <Text size="sm" c="dimmed">
          Samples every variable with a declared uncertainty (Variable Information window) around
          its solved value, re-solves, and aggregates the distributions — the sampling counterpart
          of the first-order propagation shown in the Solution window.
        </Text>
        <Group align="end" gap="sm">
          <NumberInput
            label="Samples"
            value={samples}
            onChange={(v) => setSamples(typeof v === 'number' ? v : 200)}
            min={20}
            max={1000}
            step={50}
            w={120}
          />
          <NumberInput
            label="Seed"
            value={seed}
            onChange={(v) => setSeed(typeof v === 'number' ? v : 42)}
            min={0}
            w={120}
          />
          <Button onClick={run} loading={running}>
            Run
          </Button>
        </Group>
        {error && (
          <Alert color="red" title="Monte Carlo failed">
            {error}
          </Alert>
        )}
        {result && (
          <>
            <Text size="sm">
              {okSamples} of {result.requestedSamples} samples solved
              {result.failedSamples > 0 ? ` (${result.failedSamples} failed)` : ''}
              {result.truncated ? ' — stopped at the time budget' : ''}. Sources:{' '}
              {result.sources.join(', ')}.
            </Text>
            <ScrollArea.Autosize mah={260}>
              <Table striped highlightOnHover withTableBorder fz="sm">
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>Variable</Table.Th>
                    <Table.Th>Mean</Table.Th>
                    <Table.Th>&sigma; (MC)</Table.Th>
                    <Table.Th>&sigma; (first-order)</Table.Th>
                    <Table.Th>P5</Table.Th>
                    <Table.Th>P50</Table.Th>
                    <Table.Th>P95</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {result.stats.map((s) => (
                    <Table.Tr
                      key={s.variable}
                      onClick={() => setSelected(s.variable)}
                      style={{ cursor: 'pointer' }}
                      bg={s.variable === selected ? 'var(--mantine-color-dark-5)' : undefined}
                    >
                      <Table.Td ff="monospace">{s.variable}</Table.Td>
                      <Table.Td>{fmt(s.mean)}</Table.Td>
                      <Table.Td>{fmt(s.sigma)}</Table.Td>
                      <Table.Td>{fmt(s.firstOrderSigma)}</Table.Td>
                      <Table.Td>{fmt(s.p5)}</Table.Td>
                      <Table.Td>{fmt(s.p50)}</Table.Td>
                      <Table.Td>{fmt(s.p95)}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </ScrollArea.Autosize>
            {histogram && <PlotlyChart figure={histogram} minHeight={280} />}
          </>
        )}
      </Stack>
    </Modal>
  )
}
