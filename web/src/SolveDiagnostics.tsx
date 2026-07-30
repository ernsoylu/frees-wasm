import { useState } from 'react'
import { Accordion, Alert, Badge, Group, Progress, Select, Table, Text } from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import type { SolveResponse, VariableUncertaintyResult } from './api'

/** Compact numeric format for residuals: plain in the readable range,
 *  exponential outside it. */
function formatResidual(v: number): string {
  const a = Math.abs(v)
  if (a === 0) return '0'
  if (a >= 1e-3 && a < 1e4) return v.toPrecision(4)
  return v.toExponential(2)
}

const TOP_RESIDUALS = 12

/** Tornado view: for the selected uncertain variable, each source's signed
 *  contribution as a ranked bar (the RSS of the bars is the variable's
 *  propagated uncertainty). Own component so the selection hook stays below
 *  the parent's early returns. */
function UncertaintyBreakdown({ breakdown }: Readonly<{ breakdown: VariableUncertaintyResult[] }>) {
  const [selected, setSelected] = useState<string | null>(null)
  const effective = breakdown.find((b) => b.variable === selected) ?? breakdown[0]
  const maxAbs = Math.max(...effective.sources.map((s) => Math.abs(s.value)), Number.MIN_VALUE)
  return (
    <>
      <Group gap="xs" mb={6} wrap="nowrap">
        <Select
          size="xs"
          data={breakdown.map((b) => b.variable)}
          value={effective.variable}
          onChange={setSelected}
          allowDeselect={false}
          style={{ flex: '0 0 180px' }}
          aria-label="Uncertain variable"
        />
        <Text size="xs" c="dimmed" truncate>
          σ = {formatResidual(effective.total)} — per-source contributions, RSS-combined
        </Text>
      </Group>
      {effective.sources.map((s) => (
        <Group key={s.source} gap="xs" wrap="nowrap" mb={2}>
          <Text size="xs" ff="monospace" style={{ flex: '0 0 140px' }} truncate>
            {s.source}
          </Text>
          <Progress value={(Math.abs(s.value) / maxAbs) * 100} color="teal" size="sm" style={{ flex: 1 }} />
          <Text size="xs" ff="monospace" style={{ flex: '0 0 84px', textAlign: 'right' }}>
            {formatResidual(s.value)}
          </Text>
        </Group>
      ))}
    </>
  )
}

interface Props {
  response: SolveResponse | null
}

/**
 * Solver diagnostics over the last solve: run stats, per-equation residuals
 * (sorted by magnitude) and the Tarjan block each equation belongs to. Most
 * useful when a solve fails — the payload then carries the residuals at the
 * exact iterate the solver stalled on and names the failing block — so the
 * panel opens itself on failure and stays collapsed on success.
 */
export default function SolveDiagnostics({ response }: Readonly<Props>) {
  if (!response) return null
  const stats = response.stats
  const residuals = response.residuals ?? []
  const blocks = response.blocks ?? []
  const breakdown = response.uncertaintyBreakdown ?? []
  if (!stats && residuals.length === 0 && blocks.length === 0 && breakdown.length === 0) return null

  const failed = !response.success
  const failedIndex = response.failedBlockIndex ?? null
  const blockOf = new Map<string, number>()
  for (const b of blocks) {
    for (const eq of b.equations) blockOf.set(eq, b.index)
  }
  const top = [...residuals]
    .sort((a, b) => Math.abs(b.value) - Math.abs(a.value))
    .slice(0, TOP_RESIDUALS)

  return (
    <Accordion
      variant="contained"
      radius="md"
      mb="sm"
      defaultValue={failed ? 'diagnostics' : null}
    >
      <Accordion.Item value="diagnostics">
        <Accordion.Control
          icon={failed ? <IconAlertTriangle size={16} color="var(--mantine-color-red-6)" /> : undefined}
        >
          <Group gap="xs" wrap="nowrap">
            <Text size="sm" fw={600}>
              Diagnostics
            </Text>
            {stats && (
              <Text size="xs" c="dimmed" truncate>
                {stats.equations} eqns · {stats.blocks} blocks · {stats.iterations} iters · max
                residual {formatResidual(stats.maxResidual)}
              </Text>
            )}
          </Group>
        </Accordion.Control>
        <Accordion.Panel>
          {failed && (
            <Alert
              color="red"
              variant="light"
              p="xs"
              mb="xs"
              title={failedIndex !== null ? `Solve failed in block ${failedIndex}` : 'Solve failed'}
            >
              <Text size="xs">{response.error}</Text>
            </Alert>
          )}
          {top.length > 0 && (
            <Table withTableBorder={false} verticalSpacing={2} horizontalSpacing="xs">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th style={{ width: 52 }}>Block</Table.Th>
                  <Table.Th>Equation</Table.Th>
                  <Table.Th style={{ textAlign: 'right' }}>Residual</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {top.map((r, i) => {
                  const blockIndex = blockOf.get(r.equation)
                  const inFailedBlock = failedIndex !== null && blockIndex === failedIndex
                  return (
                    <Table.Tr key={`${i}:${r.equation.slice(0, 48)}`}>
                      <Table.Td>
                        {blockIndex !== undefined && (
                          <Badge size="xs" variant="light" color={inFailedBlock ? 'red' : 'gray'}>
                            {blockIndex}
                          </Badge>
                        )}
                      </Table.Td>
                      <Table.Td>
                        <Text size="xs" ff="monospace" truncate style={{ maxWidth: 320 }}>
                          {r.equation}
                        </Text>
                      </Table.Td>
                      <Table.Td style={{ textAlign: 'right' }}>
                        <Text size="xs" ff="monospace" c={inFailedBlock ? 'red' : undefined}>
                          {formatResidual(r.value)}
                        </Text>
                      </Table.Td>
                    </Table.Tr>
                  )
                })}
              </Table.Tbody>
            </Table>
          )}
          {residuals.length > top.length && (
            <Text size="xs" c="dimmed" mt={4}>
              …{residuals.length - top.length} more equations, sorted by |residual| — the largest
              are shown.
            </Text>
          )}
        </Accordion.Panel>
      </Accordion.Item>
      {breakdown.length > 0 && (
        <Accordion.Item value="uncertainty">
          <Accordion.Control>
            <Group gap="xs" wrap="nowrap">
              <Text size="sm" fw={600}>
                Uncertainty
              </Text>
              <Text size="xs" c="dimmed" truncate>
                {breakdown.length} variable{breakdown.length === 1 ? '' : 's'} — source breakdown
              </Text>
            </Group>
          </Accordion.Control>
          <Accordion.Panel>
            <UncertaintyBreakdown breakdown={breakdown} />
          </Accordion.Panel>
        </Accordion.Item>
      )}
    </Accordion>
  )
}
