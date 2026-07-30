import { useCallback, useEffect, useLayoutEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { createUniver, defaultTheme, LocaleType, mergeLocales } from '@univerjs/presets'
import type { FUniver } from '@univerjs/presets'
import { UniverSheetsCorePreset } from '@univerjs/preset-sheets-core'
import UniverPresetSheetsCoreEnUS from '@univerjs/preset-sheets-core/locales/en-US'
import '@univerjs/preset-sheets-core/lib/index.css'
import './theme.css'
import { type SpreadsheetSpec, emptySpreadsheetData } from './types'
import { colName, extractSheets, sheetsToWorkbookData, type FWorkbookLike, type StoredSheet } from './univerAdapter'
import { Button, Group, Modal, TextInput, Switch, Text, Select, Tooltip, useComputedColorScheme } from '@mantine/core'
import { IconTablePlus, IconLink, IconDownload } from '@tabler/icons-react'
import { newParamRow, ParamTableSpec } from '../tables'
import { downloadValuesAsCsv } from './csv'

interface Props {
  singleSpreadsheetId: string
  spreadsheets: SpreadsheetSpec[]
  // Functional updater form: a debounced syncOut can fire with a stale
  // `spreadsheets` snapshot and would otherwise clobber concurrent changes
  // (e.g. a result binding added between the schedule and the flush).
  onSpreadsheetsChange: Dispatch<SetStateAction<SpreadsheetSpec[]>>
  onCreateTable?: (table: ParamTableSpec) => void
  availableVariables?: string[]
  onInsertText?: (text: string) => void
}

// Univer theme with the primary scale swapped for the Mantine blue palette so
// selection/toolbar accents match the rest of the app. Dark mode itself is
// native (createUniver darkMode + univerAPI.toggleDarkMode), no CSS hacks.
const freesTheme = {
  ...defaultTheme,
  primary: {
    50: '#e7f5ff',
    100: '#d0ebff',
    200: '#a5d8ff',
    300: '#74c0fc',
    400: '#4dabf7',
    500: '#339af0',
    600: '#228be6',
    700: '#1c7ed6',
    800: '#1971c2',
    900: '#1864ab',
  },
}

export default function SpreadsheetTab({ singleSpreadsheetId, spreadsheets, onSpreadsheetsChange, onCreateTable, availableVariables = [], onInsertText }: Props) {
  const spec = spreadsheets.find((s) => s.id === singleSpreadsheetId)
  const containerRef = useRef<HTMLDivElement>(null)
  const apiRef = useRef<FUniver | null>(null)
  const [ready, setReady] = useState(false)
  const [showBindModal, setShowBindModal] = useState(false)
  const [bindVarName, setBindVarName] = useState('')
  const [bindType, setBindType] = useState<'input' | 'result'>('input')
  const [hasSelection, setHasSelection] = useState(false)
  const [selectedCell, setSelectedCell] = useState<string>('')
  const scheme = useComputedColorScheme('dark')

  // Last selection reported by Univer's SelectionChanged — kept in a ref so the
  // Mantine toolbar buttons (which live outside the grid) can act on it even
  // after the grid blurs.
  const selRef = useRef<{ sheetName: string; x1: number; y1: number; x2: number; y2: number } | null>(null)

  // Reference to the exact sheets array we last wrote, so the external-update
  // effect can tell our own writes apart from App's auto-sync / linked-table writes.
  const selfWriteRef = useRef<unknown[] | null>(null)
  const syncTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Latest spec, readable from stable callbacks without re-subscribing events.
  const specRef = useRef(spec)
  useLayoutEffect(() => {
    specRef.current = spec
  })

  const syncOut = useCallback(() => {
    const api = apiRef.current
    const fwb = api?.getActiveWorkbook() as unknown as FWorkbookLike | null
    if (!fwb) return
    const prevSheets = (specRef.current?.sheets ?? []) as StoredSheet[]
    const newSheets = extractSheets(fwb, prevSheets)
    selfWriteRef.current = newSheets
    onSpreadsheetsChange((prev) =>
      prev.map((s) => (s.id === singleSpreadsheetId ? { ...s, sheets: newSheets } : s))
    )
  }, [singleSpreadsheetId, onSpreadsheetsChange])

  const scheduleSync = useCallback(() => {
    if (syncTimer.current) clearTimeout(syncTimer.current)
    syncTimer.current = setTimeout(syncOut, 300)
  }, [syncOut])

  // Create the Univer instance once per mount; dispose on unmount (flushing any
  // pending edits first — dock close / tab switch).
  useEffect(() => {
    const container = containerRef.current
    if (!container || !specRef.current) return

    const { univerAPI } = createUniver({
      locale: LocaleType.EN_US,
      locales: {
        [LocaleType.EN_US]: mergeLocales(UniverPresetSheetsCoreEnUS),
      },
      theme: freesTheme,
      darkMode: document.documentElement.getAttribute('data-mantine-color-scheme') !== 'light',
      presets: [
        UniverSheetsCorePreset({
          container,
        }),
      ],
    })
    apiRef.current = univerAPI

    const s = specRef.current
    const sheets = (!s.sheets || s.sheets.length === 0 ? emptySpreadsheetData() : s.sheets) as StoredSheet[]
    univerAPI.createWorkbook(sheetsToWorkbookData(s.id, s.name, sheets))

    // Any sheet mutation (values, styles, merges, renames, add/remove sheet)
    // schedules a debounced persist back into App state.
    const cmdSub = univerAPI.addEvent(univerAPI.Event.CommandExecuted, (e: { id?: string }) => {
      if (String(e?.id ?? '').startsWith('sheet.mutation.')) scheduleSync()
    })

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const selSub = univerAPI.addEvent(univerAPI.Event.SelectionChanged, (params: any) => {
      const sel = params?.selections?.[0]
      const ws = params?.worksheet
      if (!sel || !ws) return
      const sheetName: string = ws.getSheetName()
      selRef.current = {
        sheetName,
        x1: sel.startColumn,
        y1: sel.startRow,
        x2: sel.endColumn,
        y2: sel.endRow,
      }
      setHasSelection(true)
      setSelectedCell(`${sheetName}!${colName(sel.startColumn)}${sel.startRow + 1}`)
    })

    setReady(true)

    return () => {
      if (syncTimer.current) {
        clearTimeout(syncTimer.current)
        syncTimer.current = null
        syncOut()
      }
      cmdSub?.dispose?.()
      selSub?.dispose?.()
      apiRef.current = null
      setReady(false)
      univerAPI.dispose()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Track the Mantine color scheme (same pattern as the whiteboard).
  useEffect(() => {
    if (ready) apiRef.current?.toggleDarkMode(scheme === 'dark')
  }, [scheme, ready])

  // Push external celldata changes (auto-sync results, linked-table sync) into
  // the live grid. Skips our own writes by reference identity.
  useEffect(() => {
    const api = apiRef.current
    if (!api || !spec || !ready) return
    if (spec.sheets === selfWriteRef.current) return
    const fwb = api.getActiveWorkbook()
    if (!fwb) return
    const facadeSheets = fwb.getSheets()
    ;(spec.sheets as StoredSheet[]).forEach((sh, i) => {
      const fws = fwb.getSheetByName(sh.name) ?? facadeSheets[i]
      if (!fws) return
      for (const cd of sh.celldata ?? []) {
        const target = cd.v?.f ?? cd.v?.m ?? cd.v?.v ?? ''
        const range = fws.getRange(cd.r, cd.c)
        const current = range.getFormula() || range.getValue()
        if (String(target ?? '') !== String(current ?? '')) {
          range.setValue(cd.v?.f ? { f: cd.v.f } : (cd.v?.v ?? ''))
        }
      }
    })
  }, [spec, ready])

  if (!spec) {
    return (
      <div style={{ padding: 20, color: 'var(--mantine-color-dimmed)' }}>
        Spreadsheet not found.
      </div>
    )
  }

  const handleCreateTable = () => {
    const sel = selRef.current
    const fwb = apiRef.current?.getActiveWorkbook()
    const fws = sel && fwb ? fwb.getSheetByName(sel.sheetName) : null
    if (!sel || !fws) return
    if (sel.y1 >= sel.y2) return // need header + at least one row

    const values = fws.getRange(sel.y1, sel.x1, sel.y2 - sel.y1 + 1, sel.x2 - sel.x1 + 1).getValues()

    const vars: string[] = values[0].map((v, j) => String(v ?? '') || `Var${j + 1}`)

    const rows = []
    for (let r = 1; r < values.length; r++) {
      const paramRow = newParamRow()
      for (let c = 0; c < vars.length; c++) {
        const val = values[r][c]
        if (val !== '' && val !== null && val !== undefined) {
          paramRow.values[vars[c]] = String(val)
        }
      }
      rows.push(paramRow)
    }

    const newTable: ParamTableSpec = {
      id: crypto.randomUUID(),
      kind: 'parametric',
      name: `Table from ${spec.name}`,
      vars,
      rows,
      results: [],
      stats: null,
      checkResult: null,
      checkMessage: '',
      source: 'gui'
    }
    onCreateTable?.(newTable)
  }

  const handleBindVariable = (type: 'input' | 'result') => {
    if (!selRef.current) return
    setBindType(type)
    setShowBindModal(true)
  }

  const confirmBind = () => {
    const sel = selRef.current
    if (!sel || !spec) return
    const refStr = `${sel.sheetName}!${colName(sel.x1)}${sel.y1 + 1}`

    if (bindType === 'input') {
      // Input bindings: insert the ssheet equation directly into the editor text
      onInsertText?.(`${bindVarName} = ssheet('${spec.name}', '${refStr}')`)
    } else {
      // Result bindings: save in spreadsheet metadata to drive post-solve writebacks
      const key = 'resultBindings'
      onSpreadsheetsChange((prev) =>
        prev.map((s) => (s.id === singleSpreadsheetId ? { ...s, [key]: { ...s[key], [bindVarName]: refStr } } : s))
      )
    }

    setShowBindModal(false)
    setBindVarName('')
  }

  const handleExportCSV = () => {
    const fws = apiRef.current?.getActiveWorkbook()?.getActiveSheet()
    if (!fws) return
    downloadValuesAsCsv(fws.getDataRange().getValues(), `${spec.name || 'spreadsheet'}.csv`)
  }

  return (
    <div style={{ height: '100%', width: '100%', display: 'flex', flexDirection: 'column' }}>
      <Group p="xs" gap="xs" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
        <Button size="xs" variant="light" leftSection={<IconTablePlus size={14} />} onClick={handleCreateTable} disabled={!onCreateTable || !hasSelection}>
          Create Table from Selection
        </Button>
        <Button size="xs" variant="light" leftSection={<IconLink size={14} />} color="orange" onClick={() => handleBindVariable('input')} disabled={!hasSelection}>
          Bind as Input
        </Button>
        {availableVariables.length > 0 && (
          <Button size="xs" variant="light" leftSection={<IconLink size={14} />} color="teal" onClick={() => handleBindVariable('result')} disabled={!hasSelection}>
            Bind as Result
          </Button>
        )}
        <Button size="xs" variant="light" leftSection={<IconDownload size={14} />} color="blue" onClick={handleExportCSV}>
          Export CSV
        </Button>
        <Switch
          label="Auto-sync Results"
          size="sm"
          checked={spec.autoSync || false}
          onChange={(e) => {
            const val = e.currentTarget.checked
            onSpreadsheetsChange((prev) => prev.map((s) => (s.id === spec.id ? { ...s, autoSync: val } : s)))
          }}
        />
        {spec.linkedTableId && (
          // Legacy linked-table view: superseded by the Tables workbook. The
          // field stays inert in the file (downgrade-safe) until the user
          // explicitly unlinks (contract d of the unification plan).
          <Tooltip label="This spreadsheet was linked to a parametric table (old mechanism). The link is inactive — the table lives in the Tables window now. Unlink removes the leftover marker.">
            <Button
              size="xs"
              variant="light"
              color="gray"
              onClick={() =>
                onSpreadsheetsChange((prev) =>
                  prev.map((s) => (s.id === spec.id ? { ...s, linkedTableId: undefined } : s)),
                )
              }
            >
              Unlink table (legacy)
            </Button>
          </Tooltip>
        )}
      </Group>

      <div ref={containerRef} className="frees-univer" style={{ flex: 1, minHeight: 0 }} />

      <Modal opened={showBindModal} onClose={() => setShowBindModal(false)} title={`Bind Cell as ${bindType === 'input' ? 'Input' : 'Result'}`} size="sm">
        <Text size="sm" mb="xs" c="dimmed">
          Selected Cell: <strong>{selectedCell}</strong>
        </Text>
        {bindType === 'input' ? (
          <TextInput
            label="Variable Name"
            placeholder="e.g. T_in"
            value={bindVarName}
            onChange={(e) => setBindVarName(e.currentTarget.value)}
            data-autofocus
          />
        ) : (
          <Select
            label="Variable Name"
            placeholder="Select a variable"
            data={availableVariables}
            value={bindVarName}
            onChange={(val) => setBindVarName(val ?? '')}
            searchable
            clearable
            data-autofocus
          />
        )}
        <Group justify="flex-end" mt="md">
          <Button variant="default" onClick={() => setShowBindModal(false)}>Cancel</Button>
          <Button onClick={confirmBind} disabled={!bindVarName.trim()}>Bind</Button>
        </Group>
      </Modal>
    </div>
  )
}
