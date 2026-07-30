// Statistics instrument (todo.md Phase 2): per-signal min/max/mean/median/
// stddev over the cursor-bounded range. Binding order per §2.5e: the A–B
// cursor range when both cursors are placed, else the visible window, else
// the full recording. Also carries the Δ readout half of the cursor model:
// value at A, value at B (exact samples, the ~→exact readout pattern) and Δv.

import { useMemo } from 'react'
import { Badge, Group, Stack, Table, Text } from '@mantine/core'
import { formatValue } from '../../format'
import type { AbCursors } from '../UPlotChart'
import { offsetExactValueAt, offsetRawRange } from '../offsets'
import { rangeStats, type RangeStats } from '../stats'
import type { AnalyzerSignal } from '../types'

interface Props {
  /** All assigned signals, in strip order. */
  signals: AnalyzerSignal[]
  /** Per-file display-time offsets (Phase 5a). */
  offsets: Map<string, number>
  xRange: [number, number] | null
  cursors: AbCursors
  /** ChannelStore version — invalidates the model on register/evict. */
  storeVersion: number
}

interface Row {
  signal: AnalyzerSignal
  unit?: string
  vA: number | null
  vB: number | null
  stats: RangeStats | null
}

export default function StatisticsInstrument({ signals, offsets, xRange, cursors, storeVersion }: Readonly<Props>) {
  const bothCursors = cursors.a !== null && cursors.b !== null
  const range: [number, number] | null = bothCursors
    ? [Math.min(cursors.a as number, cursors.b as number), Math.max(cursors.a as number, cursors.b as number)]
    : xRange

  const rows = useMemo<Row[]>(
    () =>
      signals.flatMap((sig) => {
        const off = offsets.get(sig.measurementId) ?? 0
        const raw = offsetRawRange(sig, off, null, null)
        if (!raw) return []
        return [
          {
            signal: sig,
            unit: raw.unit,
            vA: cursors.a === null ? null : (offsetExactValueAt(sig, off, cursors.a)?.v ?? null),
            vB: cursors.b === null ? null : (offsetExactValueAt(sig, off, cursors.b)?.v ?? null),
            stats: rangeStats(raw.t, raw.v, range?.[0] ?? null, range?.[1] ?? null),
          },
        ]
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [signals, offsets, range?.[0], range?.[1], cursors.a, cursors.b, storeVersion],
  )

  if (rows.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign signals to strips to compute statistics.
      </Text>
    )
  }

  const fmt = (x: number | null | undefined) => formatValue(x ?? Number.NaN)

  return (
    <Stack gap="xs" p={4}>
      <Group gap="xs">
        <Text size="xs" c="dimmed">
          Range:
        </Text>
        <Badge size="sm" variant="light" color={bothCursors ? 'yellow' : 'teal'}>
          {bothCursors
            ? `A–B  (${formatValue(range?.[0])} s → ${formatValue(range?.[1])} s)`
            : range
              ? `visible window (${formatValue(range[0])} s → ${formatValue(range[1])} s)`
              : 'full recording'}
        </Badge>
        {!bothCursors && (
          <Text size="xs" c="dimmed">
            Place cursors A (click) and B (Shift+click) on a strip to bind statistics to them.
          </Text>
        )}
      </Group>
      <Table.ScrollContainer minWidth={760}>
        <Table striped highlightOnHover withTableBorder withColumnBorders>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Signal</Table.Th>
              <Table.Th>Unit</Table.Th>
              <Table.Th ta="right">v(A)</Table.Th>
              <Table.Th ta="right">v(B)</Table.Th>
              <Table.Th ta="right">Δv</Table.Th>
              <Table.Th ta="right">Min</Table.Th>
              <Table.Th ta="right">Max</Table.Th>
              <Table.Th ta="right">Mean</Table.Th>
              <Table.Th ta="right">Median</Table.Th>
              <Table.Th ta="right">Std dev</Table.Th>
              <Table.Th ta="right">Samples</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {rows.map((r) => (
              <Table.Tr key={`${r.signal.measurementId}:${r.signal.channel}`}>
                <Table.Td>
                  <Text size="sm" style={{ color: r.signal.color }} fw={600}>
                    {r.signal.channel}
                  </Text>
                </Table.Td>
                <Table.Td>{r.unit ?? ''}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.vA)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.vB)}</Table.Td>
                <Table.Td ta="right" ff="monospace">
                  {r.vA !== null && r.vB !== null ? formatValue(r.vB - r.vA) : '—'}
                </Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.stats?.min)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.stats?.max)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.stats?.mean)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.stats?.median)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{fmt(r.stats?.stddev)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{r.stats?.count?.toLocaleString() ?? '—'}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
    </Stack>
  )
}
