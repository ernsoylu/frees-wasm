// spreadsheet/TablesWorkbookTab.tsx
//
// The Tables workbook (Phase 1 of the table-spreadsheet unification plan,
// todo.md): ONE Univer engine hosting every function/lookup table as a bound
// sheet (decision 2 — never an engine per table). TableSpec stays the source
// of truth; sheets are materialized projections (tableBinding.ts) and are
// never persisted as celldata (decision 3).
//
// Spike-verified mechanics (todo.md § Phase 0 result):
// - protection = protect() + rule.setPoint(Edit, false) — bare protect()
//   does not restrict the local session (owner)
// - materializer writes go through the MUTATION level
//   ('sheet.mutation.set-range-values'), which is permission-exempt and adds
//   no undo entry (solver/external write-back must not be user-undoable)
// - sheet→spec reads gate on getFormula().onCalculationResultApplied() and
//   are also triggered by 'formula.mutation.set-formula-calculation-result'
//   (dependent recalcs do NOT emit sheet.mutation.* — the P4 finding)

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react'
import { createUniver, defaultTheme, LocaleType, mergeLocales } from '@univerjs/presets'
import type { FUniver } from '@univerjs/presets'
import { UniverSheetsCorePreset } from '@univerjs/preset-sheets-core'
import UniverPresetSheetsCoreEnUS from '@univerjs/preset-sheets-core/locales/en-US'
import '@univerjs/preset-sheets-core/lib/index.css'
import './theme.css'
import {
  ActionIcon,
  Badge,
  Button,
  Checkbox,
  Code,
  Group,
  Menu,
  Stack,
  Text,
  TextInput,
  Tooltip,
  useComputedColorScheme,
} from '@mantine/core'
import {
  IconArrowsSort,
  IconChartGridDots,
  IconDownload,
  IconPlus,
  IconSparkles,
  IconTable,
  IconTrash,
} from '@tabler/icons-react'
import {
  fillMissingCells,
  FunctionTableSpec,
  newFunctionTable,
  newParamRow,
  newParamTable,
  ParamTableSpec,
  sortFunctionRows,
  TableSpec,
} from '../tables'
import {
  boundColumnCount,
  isHostedTable,
  protectedRangesFor,
  sheetDimsFor,
  sheetEditsToSpec,
  specToSheetData,
  TABLE_MAX_ROWS,
} from './tableBinding'
import { colName, cssToStyleData, sheetsToWorkbookData, type StoredSheet } from './univerAdapter'
import { downloadValuesAsCsv } from './csv'

/** A1 ref for 0-based coordinates (style lookup on the mutation path). */
function a1Ref(r: number, c: number): string {
  return `${colName(c)}${r + 1}`
}
import { initialSyncMachine, transition, type SyncEvent, type SyncMachine } from './tableSyncMachine'

import { tablesWorkbookSync } from './tablesWorkbookBridge'

const WORKBOOK_ID = 'frees-tables-workbook'

/** Structural commands that are schema operations on bound sheets — vetoed at
 * the command level (contract a; the veto path is undo-clean per the spike). */
const VETOED_COMMANDS = [
  'sheet.command.insert-col',
  'sheet.command.insert-col-before',
  'sheet.command.insert-col-after',
  'sheet.command.remove-col',
  'sheet.command.insert-sheet',
  'sheet.command.remove-sheet',
  'sheet.command.copy-sheet',
  'sheet.command.move-sheet',
]

interface Props {
  tables: TableSpec[]
  activeTableId: string | null
  onTablesChange: Dispatch<SetStateAction<TableSpec[]>>
  onActiveTableIdChange: (id: string | null) => void
  /** Opens ConfigureTableModal for a parametric table (App-level modal). */
  onConfigureTable?: (tableId: string) => void
  /** Opens the fill-column-values modal for a parametric table variable. */
  onAlterColumn?: (tableId: string, varName: string) => void
}

function hostedOf(tables: TableSpec[]): TableSpec[] {
  return tables.filter(isHostedTable)
}

export default function TablesWorkbookTab({
  tables,
  activeTableId,
  onTablesChange,
  onActiveTableIdChange,
  onConfigureTable,
  onAlterColumn,
}: Readonly<Props>) {
  const hosted = hostedOf(tables)
  const active =
    hosted.find((t) => t.id === activeTableId) ?? hosted[0] ?? null

  const containerRef = useRef<HTMLDivElement>(null)
  const apiRef = useRef<FUniver | null>(null)
  const [ready, setReady] = useState(false)
  const [warnings, setWarnings] = useState<Record<string, string>>({})
  const scheme = useComputedColorScheme('dark')

  const tablesRef = useRef(tables)
  const activeIdRef = useRef<string | null>(active?.id ?? null)

  useLayoutEffect(() => {
    tablesRef.current = tables
    activeIdRef.current = active?.id ?? null
  })

  // spec.id -> Univer sheet id (boot sheets reuse spec ids; later fwb.create
  // mints its own, so the mapping is explicit).
  const sheetIdBySpec = useRef(new Map<string, string>())
  // spec.id -> the exact spec object last written to / read from the sheet;
  // the external-change effect skips specs whose identity matches (the
  // selfWriteRef pattern, per spec instead of per array).
  const lastWritten = useRef(new Map<string, TableSpec>())
  // Non-zero while our own materializer writes are executing, so the
  // CommandExecuted listener doesn't schedule a sheet→spec sync for them.
  const suppressSync = useRef(0)
  const internalOps = useRef(0)
  const machine = useRef<SyncMachine>(initialSyncMachine)
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dirtySheets = useRef(new Set<string>())

  const fwbOf = (api: FUniver) =>
    (api as unknown as { getUniverSheet?: (id: string) => unknown }).getUniverSheet?.(WORKBOOK_ID) ??
    api.getActiveWorkbook()

  const findSheet = useCallback((api: FUniver, specId: string) => {
    const fwb = fwbOf(api) as { getSheets(): { getSheetId(): string }[] } | null
    const sid = sheetIdBySpec.current.get(specId)
    if (!fwb || !sid) return null
    return (fwb.getSheets() as unknown as { getSheetId(): string }[]).find(
      (ws) => ws.getSheetId() === sid,
    ) as unknown as UniverSheetLike | null
  }, [])

  // -------------------------------------------------------------------------
  // spec -> sheet (materialization; mutation-level, permission-exempt)

  const materializeSheet = useCallback((api: FUniver, spec: TableSpec, prev?: TableSpec) => {
    const sid = sheetIdBySpec.current.get(spec.id)
    if (!sid) return
    const stored = specToSheetData(spec)
    const cols = boundColumnCount(spec)
    const prevCols = prev ? boundColumnCount(prev) : cols
    const rows = spec.rows.length + 1
    const prevRows = prev ? prev.rows.length + 1 : rows
    // The clear window must also cover the sheet's USED range, not just the
    // spec's extent — stray content (e.g. run numbers drag-filled through
    // range protection far below the table) would otherwise survive a
    // repaint as half-stale leftovers.
    const fws = findSheet(api, spec.id)
    const usedVals = fws?.getDataRange()?.getValues() ?? []
    const usedRows = usedVals.length
    const usedCols = usedVals.reduce((m, row) => Math.max(m, row.length), 0)
    let clearRows = Math.max(rows, prevRows, usedRows) + 5
    let clearCols = Math.max(cols, prevCols, usedCols) + 2

    // Univer hard-throws "Range is out of bounds" on writes past the sheet's
    // grid (the crash a user hit after growing a table). Grow the grid when
    // the table outgrew it, then clamp the write window to what exists.
    if (fws) {
      try {
        const maxR = fws.getMaxRows?.() ?? 0
        const maxC = fws.getMaxColumns?.() ?? 0
        internalOps.current++
        suppressSync.current++
        try {
          if (maxR > 0 && clearRows > maxR && typeof fws.insertRowsAfter === 'function') {
            fws.insertRowsAfter(maxR - 1, clearRows - maxR + 200)
          }
          if (maxC > 0 && clearCols > maxC && typeof fws.insertColumnsAfter === 'function') {
            fws.insertColumnsAfter(maxC - 1, clearCols - maxC + 5)
          }
        } finally {
          internalOps.current--
          suppressSync.current--
        }
        const mr = fws.getMaxRows?.() ?? clearRows
        const mc = fws.getMaxColumns?.() ?? clearCols
        clearRows = Math.min(clearRows, Math.max(mr, 1))
        clearCols = Math.min(clearCols, Math.max(mc, 1))
      } catch {
        // capacity probing must never take the workbook down
      }
    }

    // Blank-out matrix, then lay the spec cells over it. Cleared cells are
    // written as NULL (cell removed) — an empty-string cell still counts as
    // content for Univer's used range, which made every materialize see a
    // bigger range, pad it, and grow the sheet (+2 cols/+5 rows per Add Row:
    // a runaway that scrolled the grid out to column BL).
    const cellValue: Record<number, Record<number, unknown>> = {}
    for (let r = 0; r < clearRows; r++) {
      const row: Record<number, unknown> = {}
      for (let c = 0; c < clearCols; c++) row[c] = null
      cellValue[r] = row
    }
    for (const cd of stored.celldata) {
      const css = stored.styles?.[a1Ref(cd.r, cd.c)]
      const s = css ? (cssToStyleData(css) ?? null) : null
      cellValue[cd.r] ??= {}
      cellValue[cd.r][cd.c] = cd.v.f ? { f: cd.v.f, v: null, m: null, s } : { v: cd.v.v, m: cd.v.m, s }
    }

    suppressSync.current++
    void Promise.resolve()
      .then(() =>
        (api as unknown as { executeCommand(id: string, p: unknown): Promise<unknown> }).executeCommand(
          'sheet.mutation.set-range-values',
          { unitId: WORKBOOK_ID, subUnitId: sid, cellValue },
        ),
      )
      .catch(() => {
        // A failed repaint must degrade to a warning, never the app's error
        // boundary — the spec is still the source of truth.
        setWarnings((prev) => ({
          ...prev,
          [spec.id]: 'The sheet could not be fully redrawn — reopen the Tables window to repaint it.',
        }))
      })
      .finally(() => {
        suppressSync.current--
      })

    // Keep the sheet tab name in step with the spec (rename via toolbar).
    if (fws && typeof fws.setName === 'function' && fws.getSheetName?.() !== spec.name) {
      internalOps.current++
      try {
        fws.setName(spec.name)
      } finally {
        internalOps.current--
      }
    }
    lastWritten.current.set(spec.id, spec)
  }, [findSheet])

  const applyProtection = useCallback(async (api: FUniver, spec: TableSpec) => {
    const fws = findSheet(api, spec.id)
    if (!fws) return
    for (const rangeRef of protectedRangesFor(spec)) {
      try {
        const perm = fws.getRange(rangeRef).getRangePermission?.()
        if (!perm) return
        const rule = await perm.protect({ name: `frees:${spec.id}:${rangeRef}` })
        const editPoint = (api as unknown as { Enum?: { RangePermissionPoint?: { Edit: unknown } } })
          .Enum?.RangePermissionPoint?.Edit
        if (rule && editPoint !== undefined) await rule.setPoint(editPoint, false)
      } catch {
        // Protection is UX; the mapper's spatial filter is the guarantee
        // (contract a) — a protection failure must not kill the window.
      }
    }
  }, [findSheet])

  const runMaterializePass = useCallback(() => {
    const api = apiRef.current
    if (!api) return
    const fwb = fwbOf(api) as {
      create(name: string, rows: number, cols: number): UniverSheetLike
      deleteSheet(ws: unknown): unknown
      setActiveSheet?(ws: unknown): void
    } | null
    if (!fwb) return
    const specs = hostedOf(tablesRef.current)
    const specIds = new Set(specs.map((s) => s.id))

    // Remove sheets whose spec is gone.
    for (const [specId] of [...sheetIdBySpec.current]) {
      if (specIds.has(specId)) continue
      const fws = findSheet(api, specId)
      sheetIdBySpec.current.delete(specId)
      lastWritten.current.delete(specId)
      if (fws) {
        internalOps.current++
        try {
          fwb.deleteSheet(fws)
        } finally {
          internalOps.current--
        }
      }
    }

    // Create + materialize new specs; rewrite changed ones.
    for (const spec of specs) {
      if (!sheetIdBySpec.current.has(spec.id)) {
        internalOps.current++
        try {
          const dims = sheetDimsFor(spec)
          const fws = fwb.create(spec.name, dims.rowCount, dims.columnCount)
          sheetIdBySpec.current.set(spec.id, fws.getSheetId())
        } finally {
          internalOps.current--
        }
        materializeSheet(api, spec)
        void applyProtection(api, spec)
      } else if (lastWritten.current.get(spec.id) !== spec) {
        materializeSheet(api, spec, lastWritten.current.get(spec.id))
        if (spec.source === 'code') void applyProtection(api, spec)
      }
    }
  }, [applyProtection, findSheet, materializeSheet])

  const dispatch = (ev: SyncEvent) => {
    const t = transition(machine.current, ev)
    machine.current = t.machine
    if (t.runMaterialize) {
      runMaterializePass()
      machine.current = transition(machine.current, 'MATERIALIZE_DONE').machine
    }
  }

  // -------------------------------------------------------------------------
  // sheet -> spec (user edits)

  const syncSheetOut = useCallback(
    (specId: string): TableSpec | null => {
      const api = apiRef.current
      if (!api) return null
      const spec = hostedOf(tablesRef.current).find((t) => t.id === specId)
      if (!spec || spec.source === 'code') return null
      const fws = findSheet(api, specId)
      if (!fws) return null

      const dataRange = fws.getDataRange()
      const values = dataRange?.getValues() ?? []
      const formulas = dataRange?.getFormulas() ?? []
      const result = sheetEditsToSpec(spec, { values, formulas })

      let warning: string | null = null
      if (result.truncated) {
        warning = `Table row limit reached — data past ${TABLE_MAX_ROWS} rows was dropped.`
      } else if (result.errorCells.length > 0) {
        warning = `Formula error in ${result.errorCells.join(', ')} — those cells are excluded from the solver.`
      } else if (result.outOfRegion) {
        warning = 'Content outside the table columns was cleared (columns are schema — use the toolbar).'
      } else if (result.runColumnDrift) {
        warning = 'The Run column is managed by the table — hand-typed run numbers were reset (use Add Row to add runs).'
      }
      // Warnings are per sheet: showing another table's message on the
      // active one (e.g. a Run-column notice on a function table) reads as
      // nonsense.
      setWarnings((prev) => {
        if (warning) return { ...prev, [specId]: warning }
        if (!(specId in prev)) return prev
        const next = { ...prev }
        delete next[specId]
        return next
      })

      // Clear anything beyond the bound region (contract a: rejected, not
      // silently kept). Suppress the resulting mutations from re-syncing.
      if (result.outOfRegion) {
        const cols = boundColumnCount(spec)
        const usedCols = values.reduce((m, row) => Math.max(m, row.length), 0)
        const usedRows = values.length
        suppressSync.current++
        try {
          if (usedCols > cols) {
            fws
              .getRange(0, cols, usedRows, usedCols - cols)
              .setValues(Array.from({ length: usedRows }, () => Array<string>(usedCols - cols).fill('')))
          }
          const maxDataRows = TABLE_MAX_ROWS + 1
          if (usedRows > maxDataRows) {
            fws
              .getRange(maxDataRows, 0, usedRows - maxDataRows, cols)
              .setValues(Array.from({ length: usedRows - maxDataRows }, () => Array<string>(cols).fill('')))
          }
        } finally {
          suppressSync.current--
        }
      }

      lastWritten.current.set(specId, result.spec)
      onTablesChange((prev) => prev.map((t) => (t.id === specId ? result.spec : t)))

      // Run-column drift (fill-drag writes through range protection): repaint
      // the whole sheet from the fresh spec — the materializer's clear window
      // covers the used range, so stray labels are wiped completely instead
      // of leaving half-stale leftovers.
      if (result.runColumnDrift) {
        materializeSheet(api, result.spec, spec)
      }
      return result.spec
    },
    [findSheet, materializeSheet, onTablesChange],
  )

  /** Flush pending syncs and return the up-to-date hosted specs (the pre-DTO
   * scrape entry point — React state lands a render later, too late for the
   * solve handler's closure). */
  const flushSync = useCallback((): TableSpec[] => {
    if (syncTimer.current) {
      clearTimeout(syncTimer.current)
      syncTimer.current = null
    }
    const ids = [...dirtySheets.current]
    dirtySheets.current.clear()
    const freshById = new Map<string, TableSpec>()
    for (const id of ids) {
      const fresh = syncSheetOut(id)
      if (fresh) freshById.set(id, fresh)
    }
    dispatch('EDIT_SETTLE')
    // tablesRef is the latest state; freshById overlays syncs from THIS flush
    // (their setState lands a render later).
    return hostedOf(tablesRef.current).map((t) => freshById.get(t.id) ?? t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [syncSheetOut])

  const scheduleSync = useCallback(
    (specId: string) => {
      dirtySheets.current.add(specId)
      dispatch('EDIT_START')
      if (syncTimer.current) clearTimeout(syncTimer.current)
      syncTimer.current = setTimeout(flushSync, 300)
    },
    [flushSync, dispatch],
  )

  // -------------------------------------------------------------------------
  // Engine lifecycle: boot once there is at least one hosted table.

  const hasTables = hosted.length > 0
  useEffect(() => {
    const container = containerRef.current
    if (!container || !hasTables) return

    const { univerAPI } = createUniver({
      locale: LocaleType.EN_US,
      locales: { [LocaleType.EN_US]: mergeLocales(UniverPresetSheetsCoreEnUS) },
      theme: defaultTheme,
      darkMode: document.documentElement.getAttribute('data-mantine-color-scheme') !== 'light',
      presets: [
        UniverSheetsCorePreset({
          container,
          // v1: the grid context menu is disabled wholesale on the Tables
          // workbook — its structural items (insert/delete column, delete
          // sheet) are schema operations here, and a visible item that gets
          // vetoed reads as a broken app (contract a). Normal spreadsheet
          // windows keep their menu; per-item filtering is a Phase 2 polish.
          contextMenu: false,
        }),
      ],
    })
    apiRef.current = univerAPI

    const specs = hostedOf(tablesRef.current)
    const storedSheets: StoredSheet[] = specs.map(specToSheetData)
    sheetIdBySpec.current = new Map(specs.map((s) => [s.id, s.id]))
    univerAPI.createWorkbook(sheetsToWorkbookData(WORKBOOK_ID, 'Tables', storedSheets))
    for (const s of specs) {
      lastWritten.current.set(s.id, s)
      void applyProtection(univerAPI, s)
    }

    // User edits: value mutations schedule the debounced sheet→spec sync;
    // formula recalcs (formula.mutation.*, NOT sheet.mutation.*) must trigger
    // it too — dependent cells recalc without any sheet mutation (spike P4).
    const cmdSub = univerAPI.addEvent(univerAPI.Event.CommandExecuted, (e: { id?: string; params?: unknown }) => {
      try {
      const id = String(e?.id ?? '')
      const params = (e?.params ?? {}) as { unitId?: string; subUnitId?: string; name?: string }
      if (params.unitId && params.unitId !== WORKBOOK_ID) return
      if (suppressSync.current > 0) return
      if (id === 'sheet.mutation.set-range-values' && params.subUnitId) {
        for (const [specId, sid] of sheetIdBySpec.current) {
          if (sid === params.subUnitId) scheduleSync(specId)
        }
      } else if (id === 'formula.mutation.set-formula-calculation-result') {
        // Refresh every hosted sheet that carries formulas (cheap reads).
        for (const spec of hostedOf(tablesRef.current)) {
          if (spec.formulas && Object.keys(spec.formulas).length > 0) scheduleSync(spec.id)
        }
      } else if (id === 'sheet.command.set-worksheet-name' && params.subUnitId && internalOps.current === 0) {
        for (const [specId, sid] of sheetIdBySpec.current) {
          if (sid === params.subUnitId && params.name) {
            onTablesChange((prev) => prev.map((t) => (t.id === specId ? { ...t, name: params.name! } : t)))
          }
        }
      }
      } catch {
        // Listener errors propagate into Univer's command pipeline and can
        // take the whole workspace down — contain them here.
      }
    })

    // Structural column/sheet operations are schema-only (contract a); the
    // command-level veto is undo-clean (spike P2).
    const vetoSub = univerAPI.onBeforeCommandExecute((cmd: { id?: string; params?: { unitId?: string } }) => {
      if (internalOps.current > 0) return
      const id = String(cmd?.id ?? '')
      if (cmd?.params?.unitId && cmd.params.unitId !== WORKBOOK_ID) return
      if (VETOED_COMMANDS.includes(id)) {
        throw new Error('frees: table columns/sheets are schema — use the table toolbar instead')
      }
    })

    tablesWorkbookSync.flush = flushSync
    setReady(true)

    const lastWrittenMap = lastWritten.current

    return () => {
      tablesWorkbookSync.flush = null
      if (syncTimer.current) {
        clearTimeout(syncTimer.current)
        syncTimer.current = null
      }
      flushSync()
      cmdSub?.dispose?.()
      ;(vetoSub as { dispose?: () => void })?.dispose?.()
      apiRef.current = null
      sheetIdBySpec.current.clear()
      lastWrittenMap.clear()
      machine.current = initialSyncMachine
      setReady(false)
      univerAPI.dispose()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasTables])

  useEffect(() => {
    if (ready) apiRef.current?.toggleDarkMode(scheme === 'dark')
  }, [scheme, ready])

  // External spec changes (digitizer sends, code-table refresh after a solve,
  // toolbar schema ops) — queued through the state machine so they can never
  // land mid-keystroke (contract b).
  useEffect(() => {
    if (!ready) return
    const changed = hostedOf(tables).some((s) => lastWritten.current.get(s.id) !== s)
    const removed = [...sheetIdBySpec.current.keys()].some((id) => !tables.some((t) => t.id === id))
    if (changed || removed) dispatch('MATERIALIZE_REQUEST')
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tables, ready])

  // Activate the sheet backing the requested table.
  useEffect(() => {
    const api = apiRef.current
    if (!api || !ready || !active) return
    const fws = findSheet(api, active.id)
    const fwb = fwbOf(api) as { setActiveSheet?(ws: unknown): void } | null
    if (fws && fwb?.setActiveSheet) fwb.setActiveSheet(fws)
  }, [active, ready, findSheet])

  // ---------------------------------------------------------------------------
  // Toolbar actions (schema lives here, not in cells — contract a)

  /** Downloads the active sheet as CSV — the direct export the two-step
   *  detour through a Spreadsheet used to be needed for. */
  const handleExportCsv = () => {
    const fws = apiRef.current?.getActiveWorkbook()?.getActiveSheet()
    if (!fws) return
    downloadValuesAsCsv(fws.getDataRange().getValues(), `${active?.name || 'table'}.csv`)
  }

  const activeFn: FunctionTableSpec | null = active?.kind === 'function' ? active : null
  const activeParam: ParamTableSpec | null = active?.kind === 'parametric' ? active : null

  const updateActiveFn = (patch: Partial<FunctionTableSpec>) => {
    if (!activeFn) return
    onTablesChange((prev) =>
      prev.map((t) => (t.id === activeFn.id && t.kind === 'function' ? { ...t, ...patch } : t)),
    )
  }

  const updateActiveParam = (update: (t: ParamTableSpec) => ParamTableSpec) => {
    if (!activeParam) return
    onTablesChange((prev) =>
      prev.map((t) => (t.id === activeParam.id && t.kind === 'parametric' ? update(t) : t)),
    )
  }

  /** Any row-count change invalidates results/check, mirroring the old
   * grid's invalidateActiveParam. */
  const invalidated = (t: ParamTableSpec): ParamTableSpec => ({
    ...t,
    results: [],
    stats: null,
    checkResult: null,
    checkMessage: '',
  })

  const addTable = (kind: 'function-1d' | 'function-2d' | 'parametric') => {
    let created: TableSpec | null = null
    onTablesChange((prev) => {
      created =
        kind === 'parametric' ? newParamTable(prev) : newFunctionTable(prev, kind === 'function-1d')
      return [...prev, created]
    })
    requestAnimationFrame(() => {
      if (created) onActiveTableIdChange(created.id)
    })
  }

  const removeTable = (id: string) => {
    onTablesChange((prev) => prev.filter((t) => t.id !== id))
    if (activeTableId === id) onActiveTableIdChange(hosted.find((t) => t.id !== id)?.id ?? null)
  }

  const readOnly = active?.source === 'code'
  const multiCurve = (activeFn?.columns.length ?? 0) > 1
  const callSignature = activeFn
    ? multiCurve
      ? `${activeFn.name || 'name'}(${activeFn.argName || 'x'}, ${activeFn.paramName || 'param'})`
      : `${activeFn.name || 'name'}(${activeFn.argName || 'x'})`
    : ''

  return (
    <Group
      align="stretch"
      gap={0}
      style={{ flex: 1, minHeight: 0, height: '100%' }}
      wrap="nowrap"
      // Edit-settle rule (contract b): focus leaving the workbook flushes the
      // pending sync and any queued materialization.
      onBlurCapture={() => {
        if (syncTimer.current) flushSync()
      }}
    >
      {/* Navigation list (decision 2: not Univer's bottom sheet tabs). */}
      <Stack
        gap={4}
        p="xs"
        style={{ width: 180, borderRight: '1px solid var(--mantine-color-default-border)', overflowY: 'auto' }}
      >
        <Group justify="space-between" wrap="nowrap">
          <Text size="xs" fw={700} c="dimmed">
            Tables
          </Text>
          <Tooltip label="Export the active table as CSV">
            <ActionIcon size="sm" variant="light" aria-label="Export active table as CSV" onClick={handleExportCsv} disabled={!active}>
              <IconDownload size={14} />
            </ActionIcon>
          </Tooltip>
          <Menu position="bottom-start" shadow="md">
            <Menu.Target>
              <ActionIcon size="sm" variant="light" aria-label="Add table">
                <IconPlus size={14} />
              </ActionIcon>
            </Menu.Target>
            <Menu.Dropdown>
              <Menu.Item leftSection={<IconTable size={14} />} onClick={() => addTable('parametric')}>
                Parametric Table — solve the system once per row
              </Menu.Item>
              <Menu.Item leftSection={<IconChartGridDots size={14} />} onClick={() => addTable('function-1d')}>
                Function Table (without Curve)
              </Menu.Item>
              <Menu.Item leftSection={<IconChartGridDots size={14} />} onClick={() => addTable('function-2d')}>
                Function Table (with Curve family)
              </Menu.Item>
            </Menu.Dropdown>
          </Menu>
        </Group>
        {hosted.map((t) => (
          <Group
            key={t.id}
            gap={4}
            px={6}
            py={2}
            wrap="nowrap"
            style={{
              cursor: 'pointer',
              borderRadius: 4,
              background: active?.id === t.id ? 'var(--mantine-color-default-hover)' : undefined,
            }}
            onClick={() => onActiveTableIdChange(t.id)}
          >
            {t.kind === 'parametric' ? (
              <IconTable size={13} color="var(--mantine-color-teal-4)" />
            ) : (
              <IconChartGridDots size={13} color="var(--mantine-color-teal-4)" />
            )}
            <Text size="xs" style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {t.name}
            </Text>
            {t.source === 'code' ? (
              <Tooltip label="Defined by a TABLE block in the editor (read-only)">
                <Badge size="xs" variant="light" color="grape">
                  code
                </Badge>
              </Tooltip>
            ) : (
              <ActionIcon
                size="xs"
                variant="subtle"
                color="red"
                aria-label={`Delete ${t.name}`}
                onClick={(e) => {
                  e.stopPropagation()
                  removeTable(t.id)
                }}
              >
                <IconTrash size={11} />
              </ActionIcon>
            )}
          </Group>
        ))}
        {hosted.length === 0 && (
          <Text size="xs" c="dimmed">
            No tables yet. Add a Parametric Table to run the system over value sets, or a Function
            Table to turn tabulated data into a function callable from the equations.
          </Text>
        )}
      </Stack>

      <Stack gap={4} p="xs" style={{ flex: 1, minWidth: 0 }}>
        {activeFn && (
          <Group gap="xs" align="flex-end" wrap="wrap">
            <TextInput
              size="xs"
              label="Function name"
              value={activeFn.name}
              onChange={(e) => updateActiveFn({ name: e.currentTarget.value })}
              readOnly={readOnly}
              w={130}
            />
            <TextInput
              size="xs"
              label="Argument (X column)"
              value={activeFn.argName}
              onChange={(e) => updateActiveFn({ argName: e.currentTarget.value })}
              readOnly={readOnly}
              w={130}
            />
            {!activeFn.is1D && (
              <TextInput
                size="xs"
                label="Curve parameter"
                placeholder="e.g. T"
                value={activeFn.paramName}
                onChange={(e) => updateActiveFn({ paramName: e.currentTarget.value })}
                readOnly={readOnly}
                disabled={!multiCurve}
                w={110}
              />
            )}
            <Checkbox
              size="xs"
              label="log X"
              checked={activeFn.xLog}
              onChange={(e) => updateActiveFn({ xLog: e.currentTarget.checked })}
              disabled={readOnly}
              mb={6}
            />
            <Checkbox
              size="xs"
              label="log Y"
              checked={activeFn.yLog}
              onChange={(e) => updateActiveFn({ yLog: e.currentTarget.checked })}
              disabled={readOnly}
              mb={6}
            />
            {!readOnly && (
              <>
                {!activeFn.is1D && (
                  <Button
                    size="compact-xs"
                    variant="default"
                    mb={4}
                    onClick={() =>
                      updateActiveFn({
                        columns: [...activeFn.columns, ''],
                        rows: activeFn.rows.map((r) => ({ ...r, ys: [...r.ys, ''] })),
                      })
                    }
                  >
                    Add curve
                  </Button>
                )}
                {!activeFn.is1D && (
                  <Button
                    size="compact-xs"
                    variant="default"
                    mb={4}
                    disabled={activeFn.columns.length <= 1}
                    onClick={() =>
                      updateActiveFn({
                        columns: activeFn.columns.slice(0, -1),
                        rows: activeFn.rows.map((r) => ({ ...r, ys: r.ys.slice(0, -1) })),
                      })
                    }
                  >
                    Remove curve
                  </Button>
                )}
                <Button
                  size="compact-xs"
                  variant="default"
                  mb={4}
                  leftSection={<IconArrowsSort size={13} />}
                  onClick={() => onTablesChange((prev) => prev.map((t) => (t.id === activeFn.id ? sortFunctionRows(activeFn) : t)))}
                >
                  Sort
                </Button>
                <Tooltip label="Interpolate blank cells from each curve's known points (log-aware)">
                  <Button
                    size="compact-xs"
                    mb={4}
                    leftSection={<IconSparkles size={13} />}
                    onClick={() => onTablesChange((prev) => prev.map((t) => (t.id === activeFn.id ? fillMissingCells(activeFn) : t)))}
                  >
                    Fill missing
                  </Button>
                </Tooltip>
              </>
            )}
            <Text size="xs" c="dimmed" mb={6}>
              Use in equations: <Code>U = {callSignature}</Code>
            </Text>
          </Group>
        )}
        {activeParam && (
          <Group gap="xs" align="center" wrap="wrap">
            <Button size="xs" variant="default" onClick={() => onConfigureTable?.(activeParam.id)}>
              Configure Columns
            </Button>
            <Button
              size="xs"
              variant="default"
              onClick={() => updateActiveParam((t) => invalidated({ ...t, rows: [...t.rows, newParamRow()] }))}
            >
              Add Row
            </Button>
            <Button
              size="xs"
              variant="default"
              disabled={activeParam.rows.length <= 1}
              onClick={() => updateActiveParam((t) => invalidated({ ...t, rows: t.rows.slice(0, -1) }))}
            >
              Remove Row
            </Button>
            {activeParam.vars.length > 0 && (
              <Menu position="bottom-start" shadow="md">
                <Menu.Target>
                  <Button size="xs" variant="default">
                    Fill Column…
                  </Button>
                </Menu.Target>
                <Menu.Dropdown>
                  {activeParam.vars.map((name) => (
                    <Menu.Item key={name} onClick={() => onAlterColumn?.(activeParam.id, name)}>
                      {name}
                    </Menu.Item>
                  ))}
                </Menu.Dropdown>
              </Menu>
            )}
            {activeParam.results.length > 0 && (
              <Button
                size="xs"
                variant="subtle"
                onClick={() => updateActiveParam((t) => ({ ...t, results: [] }))}
              >
                Clear Results
              </Button>
            )}
            <Text size="xs" c="dimmed">
              {activeParam.vars.length === 0
                ? 'Run Check first, then Configure Columns to choose the table variables. Blank cells are solved per run (shown green).'
                : activeParam.results.length > 0
                  ? `${activeParam.results.filter((r) => r.success).length}/${activeParam.results.length} runs solved — computed cells are green; typing over one makes it an input.`
                  : 'Fill cells for independent variables; blank cells are solved for each run (Run Table in the top bar).'}
            </Text>
          </Group>
        )}
        {active && warnings[active.id] && (
          <Text size="xs" c="yellow.5">
            {warnings[active.id]}
          </Text>
        )}
        <div ref={containerRef} style={{ flex: 1, minHeight: 0 }} />
      </Stack>
    </Group>
  )
}

interface UniverSheetLike {
  getSheetId(): string
  getSheetName?(): string
  setName?(name: string): void
  getMaxRows?(): number
  getMaxColumns?(): number
  insertRowsAfter?(afterIndex: number, howMany: number): unknown
  insertColumnsAfter?(afterIndex: number, howMany: number): unknown
  getDataRange(): {
    getValues(): (string | number | boolean | null)[][]
    getFormulas(): string[][]
  } | null
  getRange(rowOrA1: number | string, col?: number, rows?: number, cols?: number): {
    setValues(values: unknown[][]): void
    getRangePermission?(): {
      protect(opts: { name?: string }): Promise<{ setPoint(point: unknown, value: boolean): Promise<unknown> } | null>
    }
  }
}
