// Compare instrument (roadmap item 7): pair a measured channel with a
// simulated one (an imported ODE/parametric-table signal), resample the
// simulation onto the measurement raster, and reduce the residuals to live
// error metrics. Range binding per §2.5e: the A–B cursor range when both
// cursors are placed, else the visible window, else the full recording —
// always intersected with the simulated series' own time span.

import { useMemo, useState } from 'react'
import { Badge, Group, Select, Stack, Table, Text } from '@mantine/core'
import { formatValue } from '../../format'
import type { AbCursors } from '../UPlotChart'
import { channelStore } from '../channelStore'
import { compareStats, linearAt } from '../compare'
import { offsetExactValueAt, offsetRawRange } from '../offsets'
import type { AnalyzerSignal } from '../types'

interface Props {
  signals: AnalyzerSignal[]
  offsets: Map<string, number>
  xRange: [number, number] | null
  cursors: AbCursors
  storeVersion: number
}

const keyOf = (s: AnalyzerSignal) => `${s.measurementId}|${s.channel}`

/** Whether the signal comes from an imported solver table (ODE or parametric). */
function isSimulated(s: AnalyzerSignal): boolean {
  const meta = channelStore.getMeta(s.measurementId)
  return Boolean(meta?.signature?.headerHash?.startsWith('table:'))
}

export default function CompareInstrument({ signals, offsets, xRange, cursors, storeVersion }: Readonly<Props>) {
  const firstMeasured = signals.find((s) => !isSimulated(s))
  const firstSimulated = signals.find(isSimulated)
  const [measSel, setMeasSel] = useState<string | null>(firstMeasured ? keyOf(firstMeasured) : null)
  const [simSel, setSimSel] = useState<string | null>(firstSimulated ? keyOf(firstSimulated) : null)

  const bothCursors = cursors.a !== null && cursors.b !== null
  const range: [number, number] | null = bothCursors
    ? [Math.min(cursors.a as number, cursors.b as number), Math.max(cursors.a as number, cursors.b as number)]
    : xRange

  const model = useMemo(() => {
    if (measSel === null || simSel === null || measSel === simSel) return null
    const [mm, mc] = measSel.split('|')
    const [sm, sc] = simSel.split('|')
    const measRef = { measurementId: mm, channel: mc }
    const simRef = { measurementId: sm, channel: sc }
    const measOff = offsets.get(mm) ?? 0
    const simOff = offsets.get(sm) ?? 0
    // offsetRawRange returns t already shifted into display time, so both
    // series live on one clock; the sim series is fetched unbounded and the
    // metric loop intersects the ranges itself.
    const meas = offsetRawRange(measRef, measOff, range?.[0] ?? null, range?.[1] ?? null)
    const sim = offsetRawRange(simRef, simOff, null, null)
    if (!meas || !sim) return null
    const stats = compareStats(meas.t, meas.v, sim.t, sim.v, range?.[0] ?? null, range?.[1] ?? null)
    const at = (x: number | null) => {
      if (x === null) return null
      const m = offsetExactValueAt(measRef, measOff, x)?.v ?? Number.NaN
      const s = linearAt(sim.t, sim.v, x)
      return { m, s, d: s - m }
    }
    return {
      measUnit: meas.unit,
      simUnit: sim.unit,
      stats,
      atA: at(cursors.a),
      atB: at(cursors.b),
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [measSel, simSel, range?.[0], range?.[1], cursors.a, cursors.b, offsets, storeVersion])

  if (signals.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign a measured channel and an imported solver table (Signals → Table) to strips to
        compare them.
      </Text>
    )
  }

  const options = signals.map((s) => ({
    value: keyOf(s),
    label: isSimulated(s) ? `⌗ ${s.channel}` : s.channel,
  }))
  const unitMismatch =
    model && model.measUnit && model.simUnit && model.measUnit !== model.simUnit

  const fmt = (x: number | null | undefined) =>
    x === null || x === undefined || Number.isNaN(x) ? '—' : formatValue(x)

  return (
    <Stack gap="xs" p={4}>
      <Group gap="sm" align="end">
        <Select
          size="xs"
          w={230}
          label="Measured"
          searchable
          data={options}
          value={measSel}
          onChange={setMeasSel}
        />
        <Select
          size="xs"
          w={230}
          label="Simulated"
          searchable
          data={options}
          value={simSel}
          onChange={setSimSel}
        />
        <Badge size="sm" variant="light" color={bothCursors ? 'yellow' : 'teal'} mb={4}>
          {bothCursors
            ? `A–B (${formatValue(range?.[0])} s → ${formatValue(range?.[1])} s)`
            : range
              ? `visible window (${formatValue(range[0])} s → ${formatValue(range[1])} s)`
              : 'full overlap'}
        </Badge>
      </Group>
      {unitMismatch && (
        <Text size="xs" c="orange">
          Units differ ({model?.measUnit} vs {model?.simUnit}) — the residuals mix quantities.
        </Text>
      )}
      {model === null || model.stats === null ? (
        <Text size="sm" c="dimmed" mt="md">
          {measSel !== null && simSel !== null && measSel === simSel
            ? 'Pick two different signals.'
            : 'No overlapping samples in the bound range — the simulated series and the measurement do not share a time window here.'}
        </Text>
      ) : (
        <>
          <Group gap="xl" wrap="wrap">
            <Metric label="RMSE" value={fmt(model.stats.rmse)} unit={model.measUnit} />
            <Metric
              label="Max |error|"
              value={`${fmt(model.stats.maxAbsError)} @ ${fmt(model.stats.maxAbsErrorAt)} s`}
              unit={model.measUnit}
            />
            <Metric label="Bias (sim − meas)" value={fmt(model.stats.bias)} unit={model.measUnit} />
            <Metric label="Mean |error|" value={fmt(model.stats.meanAbsError)} unit={model.measUnit} />
            <Metric label="Samples" value={model.stats.n.toLocaleString()} />
          </Group>
          <Table withTableBorder maw={560} fz="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Cursor</Table.Th>
                <Table.Th ta="right">Measured</Table.Th>
                <Table.Th ta="right">Simulated</Table.Th>
                <Table.Th ta="right">Δ (sim − meas)</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              <Table.Tr>
                <Table.Td>A</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atA?.m)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atA?.s)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atA?.d)}</Table.Td>
              </Table.Tr>
              <Table.Tr>
                <Table.Td>B</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atB?.m)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atB?.s)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(model.atB?.d)}</Table.Td>
              </Table.Tr>
            </Table.Tbody>
          </Table>
          {!bothCursors && (
            <Text size="xs" c="dimmed">
              Place cursors A (click) and B (Shift+click) on a strip for point-wise deltas and to
              bind the metrics to the A–B range.
            </Text>
          )}
        </>
      )}
    </Stack>
  )
}

function Metric({ label, value, unit }: Readonly<{ label: string; value: string; unit?: string }>) {
  return (
    <Stack gap={0}>
      <Text size="xs" c="dimmed">
        {label}
        {unit ? ` [${unit}]` : ''}
      </Text>
      <Text size="sm" ff="monospace" fw={600}>
        {value}
      </Text>
    </Stack>
  )
}
