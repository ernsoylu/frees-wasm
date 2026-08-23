import { Badge, Button, Group, Select, Table, Text, Stack } from '@mantine/core'
import { StateTableDto, VariableResult } from './api'
import { detectStates, detectStateTables, StateTable } from './plots/stateTable'
import { PROPERTY_UNITS, resolveUnit, unitIdsFor } from './plots/units'
import { formatValue } from './format'

const PROPERTY_NAMES: Record<string, string> = {
  T: 'T',
  P: 'P',
  v: 'v',
  u: 'u',
  h: 'h',
  s: 's',
  x: 'x',
  rho: 'ρ',
  w: 'ω',
  Twb: 'T_wb',
  Tdp: 'T_dp',
  rh: 'φ',
}

function ColumnHeader({
  property,
  unitId,
  onUnitChange,
}: Readonly<{
  property: string
  unitId: string
  onUnitChange: (unitId: string) => void
}>) {
  const choices = unitIdsFor(property)
  return (
    <Group gap={6} justify="flex-end" wrap="nowrap">
      <Text size="sm" fw={700}>
        {PROPERTY_NAMES[property] ?? property}
      </Text>
      {choices.length > 1 ? (
        <Select
          size="xs"
          w={110}
          data={choices}
          value={unitId}
          onChange={(id) => id && onUnitChange(id)}
        />
      ) : (
        <Text size="xs" c="dimmed">
          [{choices[0] ?? '-'}]
        </Text>
      )}
    </Group>
  )
}

/**
 * The State Points window: solved variables named h1, s[2], T_3, T_wb1, ...
 * are grouped into numbered thermodynamic states (rows) by property
 * (columns), with a display unit selector per column. These states can be
 * overlaid on property diagrams and psychrometric charts in the Plots tab.
 */
/** One state table rendered as a grid: states are rows, properties columns. */
function StateGrid({
  state,
  unitIds,
  onUnitIdsChange,
}: Readonly<{
  state: StateTable
  unitIds: Record<string, string>
  onUnitIdsChange: (updater: (prev: Record<string, string>) => Record<string, string>) => void
}>) {
  const unitIdOf = (property: string): string =>
    unitIds[property] ?? PROPERTY_UNITS[property]?.[0]?.id ?? '-'
  return (
    <Table striped highlightOnHover withTableBorder stickyHeader>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>State</Table.Th>
          {state.columns.map((p) => (
            <Table.Th key={p}>
              <ColumnHeader
                property={p}
                unitId={unitIdOf(p)}
                onUnitChange={(id) => onUnitIdsChange((u) => ({ ...u, [p]: id }))}
              />
            </Table.Th>
          ))}
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {state.indices.map((index) => (
          <Table.Tr key={index}>
            <Table.Td>{index}</Table.Td>
            {state.columns.map((p) => {
              const value = state.values[index][p]
              const unit = resolveUnit(p, unitIdOf(p), false)
              return (
                <Table.Td
                  key={p}
                  style={{
                    textAlign: 'right',
                    fontFamily: 'var(--mantine-font-family-monospace)',
                  }}
                >
                  {value === undefined
                    ? '—'
                    : formatValue(value * unit.scale + unit.offset)}
                </Table.Td>
              )
            })}
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  )
}

interface StatesTabProps {
  solvedVariables: VariableResult[]
  /** Explicit STATE TABLE blocks declared in the editor; when present, states
   * are grouped and labelled per block (fluid-aware) instead of implicitly. */
  stateTableDefs?: StateTableDto[]
  unitIds: Record<string, string>
  onUnitIdsChange: (unitIds: Record<string, string> | ((prev: Record<string, string>) => Record<string, string>)) => void
  onFillMissing?: () => void
  solving?: boolean
  solvable?: boolean
}

export default function StatesTab({
  solvedVariables,
  stateTableDefs,
  unitIds,
  onUnitIdsChange,
  onFillMissing,
  solving = false,
  solvable = false,
}: Readonly<StatesTabProps>) {
  const declared = detectStateTables(solvedVariables, stateTableDefs)
  const implicit = declared.length === 0 ? detectStates(solvedVariables) : null

  const fillButton = onFillMissing && (
    <Button
      size="xs"
      variant="light"
      color="teal"
      onClick={onFillMissing}
      loading={solving}
      disabled={!solvable}
    >
      Fill Missing Values
    </Button>
  )

  if (implicit && implicit.indices.length === 0) {
    return (
      <Text size="sm" c="dimmed">
        No state points detected. Declare a STATE TABLE block (e.g.
        {' '}<code>STATE TABLE Cycle(P1, T1, h2) ... FLUID = Water ... END</code>),
        or name variables with a property and a state number — h1, s1, T[2],
        P_3, T_wb1, w1 — and solve; each numbered state becomes a row here and
        can be drawn on the diagrams in the Plots tab.
      </Text>
    )
  }

  return (
    <Stack gap="md" style={{ flex: 1, minHeight: 0 }}>
      <Group justify="space-between" align="center">
        <Text size="sm" c="dimmed">
          Each numbered state is displayed as a row. Missing variables can be solved using the button.
        </Text>
        {fillButton}
      </Group>

      <div style={{ overflow: 'auto', flex: 1 }}>
        {implicit ? (
          <StateGrid state={implicit} unitIds={unitIds} onUnitIdsChange={onUnitIdsChange} />
        ) : (
          <Stack gap="lg">
            {declared.map((st) => (
              <Stack key={st.name} gap={6}>
                <Group gap="xs" align="center" wrap="nowrap">
                  <Text size="sm" fw={700}>{st.name}</Text>
                  {st.fluid && (
                    <Badge size="sm" variant="light" color="teal">{st.fluid}</Badge>
                  )}
                </Group>
                {st.indices.length === 0 ? (
                  <Text size="xs" c="dimmed">No solved states yet for this table.</Text>
                ) : (
                  <StateGrid state={st} unitIds={unitIds} onUnitIdsChange={onUnitIdsChange} />
                )}
              </Stack>
            ))}
          </Stack>
        )}
      </div>
    </Stack>
  )
}

