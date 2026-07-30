import { Button, Group, Stack, Text, Tooltip } from '@mantine/core'
import { lazy, Suspense } from 'react'
import { duplicateAsEditable, TableSpec } from './tables'
import { VariableDraft } from './VariableInfoModal'

// Solver-produced (read-only) tables can be huge; render them through a
// virtualized canvas grid, code-split so glide-data-grid stays out of the
// initial bundle and only loads when such a table is opened.
const DataGridReadOnly = lazy(() => import('./DataGridReadOnly'))

// ---------------------------------------------------------------------------
// Read-only table window: code-defined PARAMETRIC tables and solved ODE
// trajectories, each in its own dock window ("table:<id>"). Editable tables
// (GUI parametric + all function/lookup tables) live as bound sheets in the
// Univer Tables workbook (TablesWorkbookTab) — the Mantine editors this file
// used to host were retired in Phase 4 of the unification plan.
// ---------------------------------------------------------------------------

interface Props {
  tables: TableSpec[]
  /** The one table this window renders. */
  singleTableId: string
  varDrafts: Record<string, VariableDraft>
  /** Open a new X-Y plot from the column selection (x = first column / time,
   * y = the selected columns). */
  onPlotColumns?: (xVar: string, yVars: string[]) => void
  /** "Open in Spreadsheet": one-shot snapshot into a new spreadsheet window
   * (Phase 3, contract e). */
  onExportTable?: (id: string) => void
  /** Make an editable GUI copy (decoupled from the editor text); the copy
   * opens as a bound sheet in the Tables workbook. */
  onCopyToEditable?: (copy: TableSpec) => void
}

export default function TablesTab(props: Readonly<Props>) {
  const { tables, singleTableId } = props
  const active = tables.find((t) => t.id === singleTableId) ?? null

  if (!active) {
    return (
      <Text size="sm" c="dimmed" mt="md">
        Table not found — it may have been removed or renamed by the last solve.
      </Text>
    )
  }

  if (active.kind !== 'parametric' || active.source !== 'code') {
    // Editable tables are hosted in the Tables workbook; this window kind
    // only ever backs read-only code/ODE tables.
    return (
      <Text size="sm" c="dimmed" mt="md">
        This table lives in the Tables window now.
      </Text>
    )
  }

  return (
    <Stack gap="xs" style={{ flex: 1, minHeight: 0 }}>
      <Group gap="xs" wrap="nowrap" align="center">
        <Text size="xs" c="dimmed" style={{ flex: 1 }}>
          {active.origin === 'ode'
            ? 'Solved trajectory from a DYNAMIC (ODE) block — virtualized read-only grid (columns are SI-solver values; drag column edges to resize, click-drag to select/copy).'
            : 'Defined in code (PARAMETRIC … END) — virtualized read-only grid. Run it with Solve Table.'}
        </Text>
        {props.onCopyToEditable && (
          <Tooltip label="Make an editable copy in the Tables workbook (decoupled from the editor text)">
            <Button
              size="compact-xs"
              variant="default"
              onClick={() => props.onCopyToEditable?.(duplicateAsEditable(active))}
            >
              Editable copy
            </Button>
          </Tooltip>
        )}
        {props.onExportTable && (
          <Tooltip label="One-shot snapshot into a new spreadsheet (timestamped; not linked to re-solves)">
            <Button size="compact-xs" variant="default" onClick={() => props.onExportTable?.(active.id)}>
              Open in Spreadsheet
            </Button>
          </Tooltip>
        )}
      </Group>
      <Suspense fallback={<Text size="sm" c="dimmed">Loading grid…</Text>}>
        <DataGridReadOnly
          vars={active.vars}
          rows={active.rows}
          results={active.results}
          varDrafts={props.varDrafts}
          columnUnits={active.columnUnits}
          onPlotColumns={props.onPlotColumns}
        />
      </Suspense>
    </Stack>
  )
}
