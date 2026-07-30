// Event List instrument (todo.md Phase 5a): a simple condition on a signal
// (or a boolean calc channel — complex conditions come from Phase 4) →
// rising-edge timestamps; clicking an event jumps every synced instrument by
// setting the shared cursor. Offsets are applied first (cross-file events
// need synchronized time).

import { useMemo, useState } from 'react'
import { Alert, Group, NumberInput, Select, Table, Text } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import { formatValue } from '../../format'
import { offsetRawRange } from '../offsets'
import type { AnalyzerSignal } from '../types'

const MAX_EVENTS = 5000

type Operator = '>' | '>=' | '<' | '<=' | 'rising'

interface Props {
  signals: AnalyzerSignal[]
  offsets: Map<string, number>
  storeVersion: number
  /** Jump all synced instruments: sets the shared cursor A (and recenters). */
  onJump: (t: number) => void
}

export default function EventListInstrument({ signals, offsets, storeVersion, onJump }: Readonly<Props>) {
  const [selection, setSelection] = useState<string | null>(
    signals.length > 0 ? `${signals[0].measurementId}|${signals[0].channel}` : null,
  )
  const [operator, setOperator] = useState<Operator>('rising')
  const [threshold, setThreshold] = useState<number | string>(0.5)

  const events = useMemo(() => {
    if (selection === null) return { rows: [] as { t: number; v: number }[], truncated: false }
    const [measurementId, channel] = selection.split('|')
    const raw = offsetRawRange(
      { measurementId, channel },
      offsets.get(measurementId) ?? 0,
      null,
      null,
    )
    if (!raw) return { rows: [], truncated: false }
    const thr =
      operator === 'rising' ? 0.5 : typeof threshold === 'number' ? threshold : Number(threshold)
    const test = (x: number): boolean => {
      if (Number.isNaN(x)) return false
      switch (operator) {
        case '>':
        case 'rising':
          return x > thr
        case '>=':
          return x >= thr
        case '<':
          return x < thr
        case '<=':
          return x <= thr
      }
    }
    const rows: { t: number; v: number }[] = []
    let prev = false
    let truncated = false
    for (let i = 0; i < raw.t.length; i++) {
      const now = test(raw.v[i])
      if (now && !prev) {
        if (rows.length >= MAX_EVENTS) {
          truncated = true
          break
        }
        rows.push({ t: raw.t[i], v: raw.v[i] })
      }
      prev = now
    }
    return { rows, truncated }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, operator, threshold, offsets, storeVersion])

  if (signals.length === 0) {
    return (
      <Text size="sm" c="dimmed" ta="center" mt="xl">
        Assign signals to strips to search for events.
      </Text>
    )
  }

  return (
    <div>
      <Group gap="sm" mb="xs" align="flex-end">
        <Select
          size="xs"
          w={260}
          label="Signal"
          searchable
          data={signals.map((s) => ({
            value: `${s.measurementId}|${s.channel}`,
            label: s.channel,
          }))}
          value={selection}
          onChange={setSelection}
        />
        <Select
          size="xs"
          w={150}
          label="Condition"
          data={[
            { value: 'rising', label: 'rising edge (bool)' },
            { value: '>', label: '> threshold' },
            { value: '>=', label: '≥ threshold' },
            { value: '<', label: '< threshold' },
            { value: '<=', label: '≤ threshold' },
          ]}
          value={operator}
          onChange={(v) => setOperator((v ?? 'rising') as Operator)}
        />
        {operator !== 'rising' && (
          <NumberInput size="xs" w={140} label="Threshold" value={threshold} onChange={setThreshold} />
        )}
        <Text size="xs" c="dimmed">
          {events.rows.length.toLocaleString()} event{events.rows.length === 1 ? '' : 's'} (condition
          becomes true) — click one to move cursor A there.
        </Text>
      </Group>
      {events.truncated && (
        <Alert color="yellow" p="xs" mb="xs" icon={<IconAlertTriangle size={14} />}>
          <Text size="xs">Only the first {MAX_EVENTS.toLocaleString()} events are listed.</Text>
        </Alert>
      )}
      <Table.ScrollContainer minWidth={360}>
        <Table striped highlightOnHover withTableBorder>
          <Table.Thead>
            <Table.Tr>
              <Table.Th w={60}>#</Table.Th>
              <Table.Th ta="right">t [s]</Table.Th>
              <Table.Th ta="right">value</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {events.rows.map((e, i) => (
              <Table.Tr key={i} style={{ cursor: 'pointer' }} onClick={() => onJump(e.t)}>
                <Table.Td>{i + 1}</Table.Td>
                <Table.Td ta="right" ff="monospace">{formatValue(e.t)}</Table.Td>
                <Table.Td ta="right" ff="monospace">{formatValue(e.v)}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
    </div>
  )
}
