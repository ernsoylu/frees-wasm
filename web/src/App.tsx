import { ChangeEvent, lazy, startTransition, Suspense, useCallback, useEffect, useMemo, useState, useRef, type ReactNode } from 'react'
import {
  Alert,
  Anchor,
  Badge,
  Button,
  Center,
  Flex,
  Group,
  Loader,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useMediaQuery } from '@mantine/hooks'
import { Spotlight, SpotlightActionGroupData } from '@mantine/spotlight'
import {
  IconChartGridDots,
  IconSitemap,
  IconChartLine,
  IconChecks,
  IconCode,
  IconDeviceFloppy,
  IconBrush,
  IconFilePlus,
  IconFolderOpen,
  IconHelp,
  IconInfoCircle,
  IconKeyboard,
  IconLayoutGrid,
  IconMathFunction,
  IconPlayerPlayFilled,
  IconSearch,
  IconSettings,
  IconTable,
  IconTemperature,
  IconVariable,
  IconWaveSine,
  IconGrid4x4,
  IconLink,
  IconPrinter,
  IconDatabase,
} from '@tabler/icons-react'
import { notifications } from '@mantine/notifications'
import { buildShareUrl, clearShareHash, extractSharedText } from './share'
import { openPrintReport } from './report'
import {
  check,
  CheckResponse,
  DEFAULT_STOP_CRITERIA,
  getFluids,
  solve,
  replClear,
  solveTable,
  SolveResponse,
  StopCriteria,
  UnitSystem,
  VariableInfo,
  VariableResult,
} from './api'
import { findPin, pinnableParameters, sliderOverrideEquation, sliderRange, type PinnedSlider } from './sliders'
const PreferencesModal = lazy(() => import('./PreferencesModal'))
const AboutModal = lazy(() => import('./AboutModal'))
import VariableInfoModal, {
  DEFAULT_DRAFT,
  parseBound,
  VariableDraft,
} from './VariableInfoModal'

const ConfigureTableModal = lazy(() => import('./ConfigureTableModal'))
const AlterValuesModal = lazy(() => import('./AlterValuesModal'))
const TablesTab = lazy(() => import('./TablesTab'))
import {
  functionTableFromDigitizer,
  loadTables,
  mergeCodeTables,
  newFunctionTable,
  newParamRow,
  newParamTable,
  ParamTableSpec,
  saveTables,
  TableSpec,
  toFunctionTableDtos,
  paramTableFromDto,
} from './tables'
const StatesTab = lazy(() => import('./StatesTab'))
import type { DigitizedExport } from './DigitizerTab'
import { loadWhiteboards, newWhiteboard, saveWhiteboards } from './whiteboard/whiteboardStorage'
import { WhiteboardSpec } from './whiteboard/types'
import { loadSpreadsheets, newSpreadsheet, saveSpreadsheets } from './spreadsheet/spreadsheetStorage'
import { emptySpreadsheetData, type SpreadsheetSpec } from './spreadsheet/types'
import {
  flushTablesWorkbook,
  isHostedTable,
  TABLES_WORKBOOK_WINDOW_ID,
} from './spreadsheet/tablesWorkbookBridge'
import { buildSnapshotSheet, type SnapshotInput } from './spreadsheet/snapshot'
import { newAnalyzer, type AnalyzerSpec } from './analyzer/types'
import { channelStore } from './analyzer/channelStore'
import { substituteSsheetRefs } from './spreadsheet/ssheetResolver'
import { group } from './workspaceData'

// The Digitizer tab is a large, self-contained editor that most
// sessions never open, so they are code-split and only fetched when their tab
// is first shown (wrapped in <Suspense> at their render sites below).
const SchematicTab = lazy(() => import('./schematic/SchematicTab'))
import type { SchematicOffsets } from './schematic/layout'
const DigitizerTab = lazy(() =>
  import('./DigitizerTab').then((m) => ({ default: m.DigitizerTab })),
)
// The Excalidraw whiteboard editor is large and self-contained; code-split it
// so the Excalidraw bundle is only fetched when a whiteboard window opens.
const WhiteboardTab = lazy(() => import('./whiteboard/WhiteboardTab'))
const SpreadsheetTab = lazy(() => import('./spreadsheet/SpreadsheetTab'))
// The Univer Tables workbook (function/lookup tables as bound sheets) shares
// the Univer chunk with SpreadsheetTab; lazy so tables-only sessions without
// it opened never fetch the engine.
const TablesWorkbookTab = lazy(() => import('./spreadsheet/TablesWorkbookTab'))
// The Data Analyzer (uPlot + papaparse) is code-split so the measurement
// tooling is only fetched when an analyzer window opens.
const DataAnalyzerTab = lazy(() => import('./analyzer/DataAnalyzerTab'))
// The analyzer's variable/signal browser, hosted by the Inspector when an
// analyzer window is focused (a measurement-suite-style dockable
// variable-selection window).
const SignalBrowser = lazy(() => import('./analyzer/SignalBrowser'))
// Lazy: pulls the full 58 KB example catalog only when the picker opens.
const ExamplesModal = lazy(() => import('./ExamplesModal'))

// The Plot tab (and its Plotly figure builders) plus the optimization and
// plot-config modals are also code-split: the Plotly figure machinery is large
// and only needed once a plot window is opened or a modal is invoked.
const PlotTab = lazy(() => import('./PlotTab'))
const ComponentWizardModal = lazy(() => import('./ComponentWizardModal'))
const SliderStrip = lazy(() => import('./SliderStrip'))
const PlotConfigModal = lazy(() => import('./plots/PlotConfigModal'))
// Clipped (decision D5): the Min/Max, Curve Fit, PID Tuner, Monte Carlo and
// Parameter Fit modals launched engine features that only exist as
// NOT_IN_BROWSER_ENGINE stubs in api.ts. The stubs (and the pidLoop /
// pidGainRewrite helpers) remain as the wiring seam; the UI stops promising
// what the engine cannot do.

const lazyTabFallback = (
  <Center h="100%">
    <Loader color="teal" />
  </Center>
)
import { PlotSpec, PlotKind } from './plots/types'
import { plotDefToSpec } from './plots/fromCode'
const Workspace = lazy(() => import('./Workspace'))
const ReplTerminal = lazy(() => import('./ReplTerminal'))
const MobileLayout = lazy(() => import('./MobileLayout'))
import { DOCS_TOPICS } from './docsTopics'
const ShortcutsModal = lazy(() => import('./ShortcutsModal'))
const GettingStartedModal = lazy(() => import('./GettingStartedModal'))
import { DEFAULT_EXAMPLE_TEXT } from './defaultExample'
import type { Example } from './examples'
import type { EquationEditorHandle } from './EquationEditor'
const EquationEditor = lazy(() => import('./EquationEditor'))
import { MessageModal, SaveCheckModal, TextPromptModal } from './dialogs'
import { Rail, TopBar } from './WorkspaceChrome'
import type { WorkspaceDockHandle, OpenWindow } from './workspace/WorkspaceDock'
const WorkspaceDock = lazy(() => import('./workspace/WorkspaceDock').then(m => ({ default: m.WorkspaceDock })))
import { detectStates } from './plots/stateTable'
import { withStableKeys } from './format'
import { FUNCTION_CATEGORIES, catalogFunctionNames } from './functionCatalog'
import {
  buildProject,
  clearProjectLocal,
  FreesProject,
  loadProjectLocal,
  ProjectSlices,
  readProjectFile,
  saveProject,
  saveProjectLocal,
  writeBridgedKeys,
} from './project'
import {
  clearAutosaveMirror,
  loadStoredProject,
  mirrorIsNewer,
  readAutosaveMirror,
  saveStoredProject,
  writeAutosaveMirror,
} from './projectStore'
import { ProjectLibraryModal } from './ProjectLibraryModal'

const STOP_CRITERIA_KEY = 'frees.stopCriteria'
const UNIT_SYSTEM_KEY = 'frees.unitSystem'
const FIRST_RUN_KEY = 'frees.firstRunDismissed'
const GETTING_STARTED_KEY = 'frees.gettingStartedSeen'

function loadUnitSystem(): UnitSystem {
  const raw = localStorage.getItem(UNIT_SYSTEM_KEY)
  return raw === 'ENG_SI' || raw === 'ENGLISH' ? raw : 'SI'
}

const EXAMPLE = DEFAULT_EXAMPLE_TEXT

function loadStopCriteria(): StopCriteria {
  try {
    const raw = localStorage.getItem(STOP_CRITERIA_KEY)
    if (raw) {
      const { complexMode: _ignored, ...rest } = JSON.parse(raw)
      return { ...DEFAULT_STOP_CRITERIA, ...rest }
    }
  } catch {
    // Corrupt storage falls back to defaults.
  }
  return DEFAULT_STOP_CRITERIA
}


/** Returns a copy of {@code items} with the matching id's name replaced. */
function renameById<T extends { id: string; name: string }>(items: T[], id: string, name: string): T[] {
  return items.map((x) => (x.id === id ? { ...x, name } : x))
}

/** First finite value supplied for each table input column (table-check semantics). */
function firstFilledValues(tVars: string[], tRows: { values: Record<string, string> }[]): Map<string, number> {
  const filled = new Map<string, number>()
  for (const name of tVars) {
    for (const row of tRows) {
      const raw = (row.values[name] ?? '').trim()
      if (raw !== '' && Number.isFinite(Number(raw))) {
        filled.set(name, Number(raw))
        break
      }
    }
  }
  return filled
}

/** Merges backend-inferred units into the variable drafts, never overriding a
 *  unit the user set explicitly. */
function mergeInferredUnits(
  drafts: Record<string, VariableDraft>,
  variables: string[],
  inferredUnits: Record<string, string>,
): Record<string, VariableDraft> {
  const next: Record<string, VariableDraft> = { ...drafts }
  for (const name of variables) {
    const existing = next[name] ?? { ...DEFAULT_DRAFT }
    next[name] = { ...existing }
    if (!existing.isUnitsUserSet) {
      next[name].units = inferredUnits[name] ?? existing.units ?? ''
    }
  }
  return next
}

/** A REPL-defined/changed variable as a frees equation string for the solve
 *  override list, e.g. {@code {name:'eta',value:0.75,units:''}} → "eta = 0.75",
 *  {@code {name:'P',value:250000,units:'Pa'}} → "P = 250000 [Pa]". */
function replOverrideEquation(v: VariableResult): string {
  const unit = v.units && v.units !== '-' ? ` [${v.units}]` : ''
  return `${v.name} = ${v.value}${unit}`
}

export default function App() {
  const isMobile = useMediaQuery('(max-width: 768px)')

  // Story 10.10: restore the whole workspace from the unified `.frees` project
  // (autosaved to localStorage). Computed once before any state initializer so
  // every slice below can seed from it, falling back to the legacy per-feature
  // keys when no unified project exists (one-time migration). Child-owned slices
  // (digitizer, custom components) self-restore from their own keys, so they are
  // intentionally not written back here on reload.
  // A #share= link carries a whole document in the URL fragment (share.ts).
  // Opening one replaces the workspace — the same semantics as loading an
  // example — so when an autosaved project exists the user must confirm.
  // The hash is stripped immediately either way: a reload must return to the
  // user's own work, not re-import the link.
  const sharedBootRef = useRef<string | null | undefined>(undefined)
  if (sharedBootRef.current === undefined) {
    const shared = extractSharedText(globalThis.location.hash)
    if (shared === null) {
      sharedBootRef.current = null
    } else {
      clearShareHash()
      const saved = loadProjectLocal()
      const accept = saved?.text == null || saved.text === shared
        || globalThis.confirm('Open the shared document from this link? It replaces your current autosaved workspace.')
      sharedBootRef.current = accept ? shared : null
    }
  }
  const sharedBoot = sharedBootRef.current

  const bootRef = useRef<FreesProject | null | undefined>(undefined)
  if (bootRef.current === undefined) bootRef.current = sharedBoot !== null ? null : loadProjectLocal()
  const boot = bootRef.current

  const [projectName, setProjectName] = useState('untitled')
  const [workspaceEpoch, setWorkspaceEpoch] = useState(0)
  const projectFileRef = useRef<HTMLInputElement>(null)

  const [text, setText] = useState(sharedBoot ?? boot?.text ?? EXAMPLE)
  // Always-current editor document. The editor is uncontrolled after mount, so
  // every keystroke lands here synchronously while the `text` state above (which
  // drives autosave/dirty-tracking/modals) trails behind in a low-priority
  // transition, keeping the full App re-render off the typing critical path.
  // Event-time readers (solve/check/save) must use this ref, not `text`.
  const textRef = useRef(text)
  // Live-lint plumbing: debounce timer + a ref to the latest idle checker
  // (assigned each render, next to onCheck) so the timer never runs stale.
  const idleCheckTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const idleCheckRef = useRef<() => void>(() => {})
  useEffect(() => () => {
    if (idleCheckTimer.current) clearTimeout(idleCheckTimer.current)
  }, [])

  // Share-by-URL: compress the current document into a self-contained link
  // and put it on the clipboard. Refuses documents whose link would be too
  // long to survive chat apps and proxies (share.ts sets the ceiling).
  const handleShareLink = useCallback(() => {
    const url = buildShareUrl(textRef.current)
    if (url === null) {
      notifications.show({
        color: 'yellow',
        title: 'Document too large to share by URL',
        message: 'The compressed link would be too long to travel reliably — save the .frees file and send that instead.',
      })
      return
    }
    navigator.clipboard.writeText(url).then(
      () => notifications.show({
        color: 'teal',
        title: 'Share link copied',
        message: 'Anyone opening it gets this exact document. Nothing is stored on a server.',
      }),
      () => { globalThis.prompt('Copy the share link:', url) },
    )
  }, [])
  useEffect(() => {
    if (sharedBoot !== null) {
      notifications.show({
        color: 'teal',
        title: 'Opened shared document',
        message: 'Loaded from the link — nothing was stored on a server.',
      })
    }
  }, [sharedBoot])
  const [checkResult, setCheckResult] = useState<CheckResponse | null>(null)
  const [checking, setChecking] = useState(false)
  const [result, setResult] = useState<SolveResponse | null>(null)
  // Printable calculation report (browser print-to-PDF): the last successful
  // solve plus the document that produced it, in a self-contained new window.
  const handlePrintReport = useCallback(() => {
    if (!result?.success) return
    if (!openPrintReport(projectName, textRef.current, result)) {
      notifications.show({
        color: 'yellow',
        title: 'Report window blocked',
        message: 'Allow pop-ups for this site to open the printable report.',
      })
    }
  }, [result, projectName])
  // Stable id for this document's solve session: tags solves so their result is
  // cached server-side for the REPL/Workspace, and bottom-terminal visibility.
  const [sessionId] = useState<string>(() => crypto.randomUUID())
  // Variables defined or changed directly in the REPL (keyed by lowercased name),
  // overlaid on the solved variables so the Variable Explorer / Solution reflect
  // them. Cleared on every solve (the backend resets its session overlay too).
  const [replVars, setReplVars] = useState<Record<string, VariableResult>>({})

  // Parameters pinned to the slider strip. They ride the same override path as
  // REPL assignments and are appended AFTER them, so the backend's
  // last-wins collapse by name lets a dragged slider beat a stale REPL value.
  const [pinnedSliders, setPinnedSliders] = useState<PinnedSlider[]>(() => boot?.sliders ?? [])
  const sliderTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const sliderSolveRef = useRef<() => void>(() => {})
  // A release that lands while a solve is in flight must not be dropped: the
  // app's policy elsewhere is "skip rather than queue", which is right for a
  // lint tick (another keystroke follows) but wrong here — the displayed
  // solution would stop matching the handle. Remember it and drain on idle.
  const sliderPendingRef = useRef(false)
  useEffect(() => () => {
    if (sliderTimer.current) clearTimeout(sliderTimer.current)
  }, [])
  const [solving, setSolving] = useState(false)
  const [findAll, setFindAll] = useState(false)
  const [complexMode, setComplexMode] = useState(false)
  const [stopCriteria, setStopCriteria] = useState<StopCriteria>(
    () => boot?.stopCriteria ?? loadStopCriteria(),
  )
  const [unitSystem, setUnitSystem] = useState<UnitSystem>(
    () => boot?.unitSystem ?? loadUnitSystem(),
  )
  const [fillMissing, setFillMissing] = useState<boolean>(() => {
    if (boot) return boot.fillMissing
    return localStorage.getItem('frees.fillMissing') === 'true'
  })
  const [showPreferences, setShowPreferences] = useState(false)
  // Mantine-styled replacements for native prompt()/confirm()/alert().
  const [renameOpen, setRenameOpen] = useState(false)
  const [saveAsOpen, setSaveAsOpen] = useState(false)
  const [showSaveCheck, setShowSaveCheck] = useState(false)
  // Phase 11 (D4): the browser-resident project library.
  const [libraryOpen, setLibraryOpen] = useState(false)
  const [dialogError, setDialogError] = useState<string | null>(null)
  // Tracks unsaved changes; suppressed for one render-cycle after a project
  // load / new / save so the dirty-tracking effect doesn't fire falsely.
  const isDirtyRef = useRef(false)
  const suppressDirtyRef = useRef(false)
  // Stores the action to run once the save-check dialog is resolved.
  const pendingActionRef = useRef<(() => void) | null>(null)
  const [dismissedWarnings, setDismissedWarnings] = useState(false)
  const [showFirstRun, setShowFirstRun] = useState(
    () => localStorage.getItem(FIRST_RUN_KEY) !== 'true',
  )

  const dismissFirstRun = useCallback(() => {
    setShowFirstRun(false)
    localStorage.setItem(FIRST_RUN_KEY, 'true')
  }, [])

  // The app opens straight into the workspace (a deliberate decision — no
  // landing page), so this modal is its welcome mat: auto-opened once per
  // browser on desktop. Share-link visits skip it (the visitor came for a
  // document, not a tour), and isMobile is undefined until the media query
  // resolves, so wait for an explicit false.
  const [showGettingStarted, setShowGettingStarted] = useState(false)
  useEffect(() => {
    if (isMobile === false && sharedBoot === null
        && localStorage.getItem(GETTING_STARTED_KEY) !== 'true') {
      setShowGettingStarted(true)
    }
  }, [isMobile, sharedBoot])
  const closeGettingStarted = useCallback(() => {
    localStorage.setItem(GETTING_STARTED_KEY, 'true')
    setShowGettingStarted(false)
  }, [])
  // Seed from a loaded project's configured drafts so buildVariableInfo() carries
  // their units (display conversion, dimensional grounding) on the very first solve
  // after load — Check/Solve then replaces this with the authoritative variable list.
  const [variables, setVariables] = useState<string[]>(() => Object.keys(boot?.varDrafts ?? {}))
  const [varDrafts, setVarDrafts] = useState<Record<string, VariableDraft>>(
    () => boot?.varDrafts ?? {},
  )
  const [showVariableInfo, setShowVariableInfo] = useState(false)
  const [showAbout, setShowAbout] = useState(false)
  const [showExamples, setShowExamples] = useState(false)
  const [showComponentWizard, setShowComponentWizard] = useState(false)
  const [showShortcuts, setShowShortcuts] = useState(false)
  const [activeTab, setActiveTab] = useState<string>('equations')

  const editorRef = useRef<EquationEditorHandle>(null)

  // Insert a function template at the editor caret (Functions menu). "$0" in
  // the snippet marks where the caret lands; selected text is replaced. The
  // editor must be visible first, so switch to it before inserting.
  const insertFunction = useCallback((snippet: string) => {
    setActiveTab('equations')
    setTimeout(() => editorRef.current?.insertSnippet(snippet), 50)
  }, [])
  // Bound-cell input equations (ssheet(...)) are full statements: append each on
  // its own line so successive binds don't concatenate on one line.
  const insertEquationLine = useCallback((text: string) => {
    setActiveTab('equations')
    setTimeout(() => editorRef.current?.insertStatement(text), 50)
  }, [])
  // Component Browser/Wizard: append the generated `Type NAME(...)` block on its
  // own line in the equations editor (same path as bound-cell statements).
  // Wiring emits a statement while the user stays on the schematic, so unlike
  // the wizard's insertion this must NOT pull focus to the editor. The live
  // lint re-checks shortly after, which is what redraws the canvas.
  const emitFromSchematic = useCallback((statement: string) => {
    editorRef.current?.insertStatement(statement)
  }, [])

  const insertComponentBlock = useCallback((block: string) => {
    setActiveTab('equations')
    setTimeout(() => editorRef.current?.insertStatement(block), 50)
  }, [])
  // Programmatic document replacement (project load, new/example, generated
  // equations): updates the ref + state and pushes the doc into the uncontrolled
  // editor. setDoc does not echo back through onTextChange.
  const applyText = useCallback((next: string) => {
    textRef.current = next
    setText(next)
    editorRef.current?.setDoc(next)
  }, [])

  // Dockview workspace manager: imperative handle + set of currently-open
  // window kinds (drives the sidebar's open-state indicators).
  const dockRef = useRef<WorkspaceDockHandle | null>(null)
  const [openWindows, setOpenWindows] = useState<OpenWindow[]>([])
  // Last-focused non-auxiliary window — drives the content-aware Inspector
  // (focusing the Inspector/Solution panels themselves doesn't change it).
  const [focusedWindow, setFocusedWindow] = useState<OpenWindow | null>(null)
  // Tracks which state-table circuit triggered the last fill-missing solve so
  // only that circuit's button shows the loading spinner.
  const [fillMissingFor, setFillMissingFor] = useState<string | null>(null)
  const openKinds = useMemo(() => openWindows.map((w) => w.kind), [openWindows])
  const openIds = useMemo(() => openWindows.map((w) => w.id), [openWindows])
  // Tables (Epic 8): any number of Parametric and Curve Tables; the active
  // parametric table is the one Check/Solve Table and the plots act on.
  const [tables, setTables] = useState<TableSpec[]>(() => {
    if (boot) return boot.tables
    const raw = localStorage.getItem('frees.tables')
    if (raw) return loadTables()
    return []
  })
  const [activeTableId, setActiveTableId] = useState<string | null>(null)
  const [solvingTableId, setSolvingTableId] = useState<string | null>(null)
  const [showConfigureTable, setShowConfigureTable] = useState(false)
  const [alterColumn, setAlterColumn] = useState<string | null>(null)
  const [checkingTableId, setCheckingTableId] = useState<string | null>(null)
  const [plots, setPlots] = useState<PlotSpec[]>(() => boot?.plots ?? [])
  // Plots are addressed per-window now; only the setter is needed.
  const [, setActivePlotId] = useState<string | null>(null)
  // New plot creation is lifted to App (the sidebar "New …" actions) so the
  // config modal opens even though plots are now per-instance dock windows.
  // The kind decides which plot type the modal creates: xy / property / psychro.
  const [newPlotKind, setNewPlotKind] = useState<PlotKind | null>(null)
  // Seed for a new X-Y plot opened from a table's column selection (x + y vars),
  // applied as the modal's initial XY config.
  const [plotSeed, setPlotSeed] = useState<{ xVar: string; yVars: string[] } | null>(null)
  // Excalidraw whiteboards: managed as App-owned state so they round-trip
  // with the .frees project. The localStorage cache is a fallback when no
  // unified project exists.
  const [whiteboards, setWhiteboards] = useState<WhiteboardSpec[]>(() =>
    boot?.whiteboards ?? loadWhiteboards(),
  )
  const [spreadsheets, setSpreadsheets] = useState<SpreadsheetSpec[]>(() =>
    boot?.spreadsheets ?? loadSpreadsheets(),
  )
  // Data Analyzer windows: the spec slice (layout + signal assignments +
  // measurement file refs, never bulk data) rides the .frees project file
  // ("template mode", §2.5b); samples live in the module-level ChannelStore
  // and are re-picked on load via each window's "Locate file…" banner.
  const [analyzers, setAnalyzers] = useState<AnalyzerSpec[]>(() => boot?.analyzers ?? [])
  // Blocks the user has dragged on the rendered schematic, as offsets from the
  // auto-layout. The drawing itself is always derived from the document, so
  // this is the only part of it that has to be saved.
  const [schematicOffsets, setSchematicOffsets] = useState<SchematicOffsets>(
    () => boot?.schematic ?? {},
  )
  // One-line self-dismissing notice (e.g. "N analyzer window(s) awaiting
  // measurement files" after a project load).
  const [loadNotice, setLoadNotice] = useState<string | null>(null)
  useEffect(() => {
    if (loadNotice === null) return
    const id = setTimeout(() => setLoadNotice(null), 8000)
    return () => clearTimeout(id)
  }, [loadNotice])
  const [fluids, setFluids] = useState<string[]>([])
  const [stateUnitIds, setStateUnitIds] = useState<Record<string, string>>(() => {
    if (boot) return boot.stateUnitIds ?? {}
    try {
      const saved = localStorage.getItem('frees.stateUnitIds')
      return saved ? JSON.parse(saved) : {}
    } catch {
      return {}
    }
  })
  const [lastSolvedWithFillMissing, setLastSolvedWithFillMissing] = useState(false)

  useEffect(() => {
    void getFluids().then(setFluids)
  }, [])

  useEffect(() => {
    saveTables(tables)
  }, [tables])

  // Whiteboards can carry large embedded image data, so debounce the scratch-
  // cache write (the unified project autosave at 800ms is the other path).
  useEffect(() => {
    const id = setTimeout(() => saveWhiteboards(whiteboards), 800)
    return () => clearTimeout(id)
  }, [whiteboards])

  useEffect(() => {
    const id = setTimeout(() => saveSpreadsheets(spreadsheets), 800)
    return () => clearTimeout(id)
  }, [spreadsheets])

  // Story 10.10: the current App-owned slices of the unified project. Child-owned
  // slices (digitizer, custom components) are read from their localStorage caches
  // by buildProject(), so they are captured without lifting them into App.
  const currentSlices = useCallback(
    (): ProjectSlices => ({
      // Ref, not state: an explicit Save fired right after a keystroke must not
      // lose the edits still riding the deferred `text` transition.
      text: textRef.current,
      varDrafts,
      stopCriteria,
      unitSystem,
      fillMissing,
      stateUnitIds,
      tables,
      plots,
      whiteboards,
      spreadsheets,
      analyzers,
      sliders: pinnedSliders,
      schematic: schematicOffsets,
    }),
    // `text` stays a dependency so the autosave effect keyed on this callback
    // still refreshes when the (deferred) editor document state lands.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [text, varDrafts, stopCriteria, unitSystem, fillMissing, stateUnitIds, tables, plots, whiteboards, spreadsheets, analyzers, pinnedSliders, schematicOffsets],
  )

  // Debounced autosave of the entire workspace to a single localStorage key,
  // superseding the scattered per-feature keys as the source of truth on reload.
  // The same document also lands in the IndexedDB mirror (D4): localStorage is
  // what boot reads synchronously, but its ~5 MB quota means this write can
  // silently stop succeeding once whiteboard images or spreadsheets grow — the
  // mirror is the durable copy that keeps updating past that point.
  useEffect(() => {
    const id = setTimeout(() => {
      const project = buildProject(currentSlices())
      saveProjectLocal(project)
      void writeAutosaveMirror(project)
    }, 800)
    return () => clearTimeout(id)
  }, [currentSlices])


  // Clear the per-circuit fill-missing tracker once the solve finishes so the
  // next click correctly identifies which circuit triggered the loading state.
  useEffect(() => {
    if (!solving) setFillMissingFor(null)
  }, [solving])

  // Drain a slider re-solve that the in-flight guard blocked. Without this a
  // handle released mid-solve leaves the solution showing the previous value.
  useEffect(() => {
    if (!solving && !checking && sliderPendingRef.current) {
      sliderPendingRef.current = false
      sliderSolveRef.current()
    }
  }, [solving, checking])

  // Phase 3.3 Auto-Sync Mode: Sync result bindings back to spreadsheets if autoSync is enabled
  useEffect(() => {
    if (!result?.success || !result.variables) return

    setSpreadsheets((prev) => {
      let changed = false
      const next = prev.map((ss) => {
        if (!ss.autoSync || !ss.resultBindings || Object.keys(ss.resultBindings).length === 0) return ss

        let sheetChanged = false
        const newSheets = [...(ss.sheets as any[])]
        
        for (const [varName, refStr] of Object.entries(ss.resultBindings)) {
          const variable = result.variables.find((v) => v.name.toLowerCase() === varName.toLowerCase())
          if (!variable || variable.value === undefined) continue

          const refMatch = refStr.match(/^(?:([^!]+)!)?([A-Za-z]+)(\d+)$/)
          if (!refMatch) continue
          const sheetName = refMatch[1]
          const colStr = refMatch[2]
          const rowStr = refMatch[3]

          let c = 0
          for (let i = 0; i < colStr.length; i++) c = c * 26 + ((colStr.codePointAt(i) ?? 0) - 64)
          c -= 1
          const r = Number.parseInt(rowStr, 10) - 1

          const sheetIndex = newSheets.findIndex((sh) => sh.name && sh.name.toLowerCase() === sheetName.toLowerCase())
          if (sheetIndex === -1) continue

          const targetSheet = { ...newSheets[sheetIndex] }
          targetSheet.celldata = [...(targetSheet.celldata || [])]

          const cellIndex = targetSheet.celldata.findIndex((cd: any) => cd.r === r && cd.c === c)
          if (cellIndex !== -1) {
            targetSheet.celldata[cellIndex] = { ...targetSheet.celldata[cellIndex], v: { ...targetSheet.celldata[cellIndex].v, v: variable.value, m: String(variable.value) } }
          } else {
            targetSheet.celldata.push({ r, c, v: { v: variable.value, m: String(variable.value) } })
          }
          
          newSheets[sheetIndex] = targetSheet
          sheetChanged = true
        }

        if (sheetChanged) {
          changed = true
          return { ...ss, sheets: newSheets }
        }
        return ss
      })

      return changed ? next : prev
    })
  }, [result])

  // Linked Table View (the old one-off parametric<->spreadsheet sync) is
  // superseded by the Tables workbook, which hosts GUI parametric tables as
  // bound sheets. `SpreadsheetSpec.linkedTableId` is kept INERT on load and
  // in saved files (downgrade-safe, contract d of the unification plan) and
  // is only stripped by an explicit Unlink in the spreadsheet toolbar.

  // Mark the project dirty whenever content changes, unless the change came from
  // an explicit load/new/save (suppressDirtyRef lets those operations opt out).
  useEffect(() => {
    if (suppressDirtyRef.current) {
      suppressDirtyRef.current = false
      isDirtyRef.current = false
      return
    }
    isDirtyRef.current = true

  }, [text, tables, plots, whiteboards, spreadsheets, analyzers, varDrafts, schematicOffsets])

  // Apply an opened/loaded project to every workspace slice. Child-owned slices
  // are written back to their caches and the relevant tabs are remounted (epoch
  // bump) so they re-read the restored state.
  const applyProject = useCallback((p: FreesProject) => {
    suppressDirtyRef.current = true
    isDirtyRef.current = false
    applyText(p.text ?? '')
    setVarDrafts(p.varDrafts ?? {})
    setStopCriteria(p.stopCriteria)
    setUnitSystem(p.unitSystem ?? 'SI')
    setFillMissing(Boolean(p.fillMissing))
    setStateUnitIds(p.stateUnitIds ?? {})
    setTables(p.tables)
    setPlots(p.plots ?? [])
    setWhiteboards(p.whiteboards ?? [])
    setSpreadsheets(p.spreadsheets ?? [])
    // Template mode (§2.5b): analyzer layouts load with refs only — the
    // measurement data itself is gone, so each window shows a "Locate file…"
    // banner. Clear any stale samples from a previous project first.
    channelStore.clear()
    setAnalyzers(p.analyzers ?? [])
    setSchematicOffsets(p.schematic ?? {})
    const awaiting = (p.analyzers ?? []).filter((a) => a.files.length > 0).length
    const notices: string[] = []
    if (awaiting > 0) {
      notices.push(
        `${awaiting} analyzer window${awaiting === 1 ? '' : 's'} awaiting measurement files — open them and use “Locate file…”.`,
      )
    }
    // Legacy linked-table views are inert now (superseded by the Tables
    // workbook); the field is preserved for downgrade safety (contract d).
    if ((p.spreadsheets ?? []).some((s) => s.linkedTableId)) {
      notices.push(
        'This project used a linked table view — parametric tables now live in the Tables window; the old link is inactive (Unlink in the spreadsheet toolbar removes it).',
      )
    }
    setLoadNotice(notices.length > 0 ? notices.join(' ') : null)
    setResult(null)
    setCheckResult(null)
    writeBridgedKeys(p)
    saveProjectLocal(p)
    // Keep the durable mirror in step (D4): a mirror left carrying the
    // *previous* workspace would read as "newer" on the next boot and offer a
    // restore of the exact state the user just navigated away from.
    void writeAutosaveMirror(p)
    setWorkspaceEpoch((e) => e + 1)
    requestAnimationFrame(() => {
      dockRef.current?.restore(p.dockLayout)
      // Surface the first whiteboard too, if any.
      const firstWhiteboard = p.whiteboards?.[0]
      if (firstWhiteboard) {
        dockRef.current?.openInstance(`whiteboard:${firstWhiteboard.id}`, 'whiteboard', firstWhiteboard.name)
      }
      // And the first analyzer, so its "Locate file…" banner is discoverable.
      const firstAnalyzer = p.analyzers?.[0]
      if (firstAnalyzer) {
        dockRef.current?.openInstance(`analyzer:${firstAnalyzer.id}`, 'analyzer', firstAnalyzer.name)
      }
    })
  }, [applyText])

  // D4 quota recovery, once per boot: when the IndexedDB mirror is strictly
  // newer than what localStorage booted, the localStorage autosave had started
  // failing — offer the newer copy rather than silently restoring it, because
  // the user may have *wanted* the state they are looking at.
  const mirrorCheckedRef = useRef(false)
  useEffect(() => {
    if (mirrorCheckedRef.current || sharedBoot !== null) return
    mirrorCheckedRef.current = true
    const booted = bootRef.current ?? null
    void readAutosaveMirror().then((mirror) => {
      if (!mirrorIsNewer(mirror, booted)) return
      const when = new Date(Date.parse(mirror.savedAt)).toLocaleString()
      notifications.show({
        id: 'autosave-mirror-restore',
        color: 'yellow',
        autoClose: false,
        title: 'A newer autosave exists in browser storage',
        message: (
          <Stack gap="xs">
            <Text size="sm">
              The quick autosave fell behind (usually a storage-quota limit). A newer copy from {when} is
              available.
            </Text>
            <Button
              size="xs"
              onClick={() => {
                notifications.hide('autosave-mirror-restore')
                applyProject(mirror)
              }}
            >
              Restore newer autosave
            </Button>
          </Stack>
        ),
      })
    })
  }, [sharedBoot, applyProject])

  // If the project is dirty, show the save-check dialog; otherwise run immediately.
  const guardedAction = useCallback((action: () => void) => {
    if (isDirtyRef.current) {
      pendingActionRef.current = action
      setShowSaveCheck(true)
    } else {
      action()
    }
  }, [])

  const onSaveCheckSave = useCallback(async () => {
    const saved = await saveProject(buildProject(currentSlices()), projectName)
    // If the user cancelled the save picker, keep the project (and the pending
    // destructive action, e.g. opening another project) on hold.
    if (!saved) return
    setShowSaveCheck(false)
    isDirtyRef.current = false
    pendingActionRef.current?.()
    pendingActionRef.current = null
  }, [currentSlices, projectName])

  const onSaveCheckDiscard = useCallback(() => {
    setShowSaveCheck(false)
    isDirtyRef.current = false
    pendingActionRef.current?.()
    pendingActionRef.current = null
  }, [])

  const onSaveCheckCancel = useCallback(() => {
    setShowSaveCheck(false)
    pendingActionRef.current = null
  }, [])

  const handleSaveProject = useCallback(async () => {
    const saved = await saveProject(buildProject(currentSlices()), projectName)
    if (saved) isDirtyRef.current = false
  }, [currentSlices, projectName])

  const handleRenameProject = useCallback(() => setRenameOpen(true), [])

  const submitRename = useCallback((name: string) => {
    setProjectName(name.trim() || 'untitled')
    setRenameOpen(false)
  }, [])

  const handleSaveProjectAs = useCallback(() => setSaveAsOpen(true), [])

  const submitSaveAs = useCallback(
    async (name: string) => {
      const clean = name.trim() || 'untitled'
      setProjectName(clean)
      const saved = await saveProject(buildProject(currentSlices()), clean)
      if (saved) isDirtyRef.current = false
      setSaveAsOpen(false)
    },
    [currentSlices],
  )

  const handleOpenProject = useCallback(() => {
    guardedAction(() => projectFileRef.current?.click())
  }, [guardedAction])

  // Phase 11 (D4): the browser-resident project library. Saving is name-keyed
  // with file semantics (same name overwrites); a failed explicit save is
  // surfaced by the modal, never silent.
  const handleSaveToBrowser = useCallback(async () => {
    const meta = await saveStoredProject(projectName, buildProject(currentSlices()))
    if (meta) isDirtyRef.current = false
    return meta !== null
  }, [currentSlices, projectName])

  const handleOpenFromBrowser = useCallback(
    (name: string) => {
      guardedAction(() => {
        void loadStoredProject(name).then((p) => {
          if (p) {
            applyProject(p)
            setProjectName(name)
            setLibraryOpen(false)
          } else {
            notifications.show({
              color: 'yellow',
              title: 'Could not open browser project',
              message: 'It may have been deleted in another tab.',
            })
          }
        })
      })
    },
    [guardedAction, applyProject],
  )

  const onProjectFileSelected = useCallback(
    async (e: ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0]
      e.target.value = '' // allow re-opening the same file
      if (!file) return
      try {
        const p = await readProjectFile(file)
        applyProject(p)
        setProjectName(file.name.replace(/\.frees$/i, ''))
      } catch (err) {
        setDialogError(err instanceof Error ? err.message : 'Could not open project file.')
      }
    },
    [applyProject],
  )

  const performNewProject = useCallback(() => {
    suppressDirtyRef.current = true
    isDirtyRef.current = false
    clearProjectLocal()
    void clearAutosaveMirror()
    writeBridgedKeys({
      version: 1,
      savedAt: '',
      text: '',
      varDrafts: {},
      stopCriteria,
      unitSystem,
      fillMissing,
      stateUnitIds: {},
      tables: [],
      plots: [],
      whiteboards: [],
      spreadsheets: [],
      analyzers: [],
      digitizer: null,
      dockLayout: null,
    })
    applyText(EXAMPLE)
    setVarDrafts({})
    setStateUnitIds({})
    setTables([])
    setPlots([])
    setWhiteboards([])
    setSpreadsheets([])
    setAnalyzers([])
    setSchematicOffsets({})
    channelStore.clear()
    setResult(null)
    setCheckResult(null)
    setProjectName('untitled')
    setWorkspaceEpoch((e) => e + 1)
    requestAnimationFrame(() => dockRef.current?.reset())
  }, [stopCriteria, unitSystem, fillMissing, applyText])

  const handleNewProject = useCallback(() => guardedAction(performNewProject), [guardedAction, performNewProject])

  // The active table, defaulting to the first; the parametric-table solver
  // state below is derived from the active *parametric* table so all the
  // existing single-table wiring (plots, reports, top bar) keeps working.
  const activeTable = tables.find((t) => t.id === activeTableId) ?? tables[0] ?? null
  const activeParam: ParamTableSpec | null =
    activeTable?.kind === 'parametric' ? activeTable : null
  const tableVars = activeParam?.vars ?? []
  const paramRows = activeParam?.rows ?? []
  const tableResults = activeParam?.results ?? []
  const solvedVars = useMemo(() => {
    const names = new Set<string>()
    if (result?.variables) {
      for (const v of result.variables) names.add(v.name)
    }
    if (checkResult?.variables) {
      for (const v of checkResult.variables) names.add(v)
    }
    return Array.from(names).sort((a, b) => a.localeCompare(b))
  }, [result, checkResult])

  // The parametric table window that is currently focused in the dock — the
  // TopBar's Check Table / Run Table buttons and status pill track this table.
  const focusedParam: ParamTableSpec | null = (() => {
    if (focusedWindow?.kind !== 'table') return null
    // The Tables workbook window tracks whichever hosted table is active.
    if (focusedWindow.id === TABLES_WORKBOOK_WINDOW_ID) return activeParam
    const t = tables.find((x) => `table:${x.id}` === focusedWindow.id)
    return t?.kind === 'parametric' ? t : null
  })()
  const tableCheckResult = focusedParam?.checkResult ?? null
  const tableCheckMessage = focusedParam?.checkMessage ?? ''
  // Function-table wire payloads. When the Univer Tables workbook is mounted,
  // flush its pending sheet→spec sync first (contract b's pre-DTO scrape) and
  // build from the returned fresh specs — React state lands a render later,
  // too late for the calling handler's closure.
  const functionTableDtos = () => {
    const fresh = flushTablesWorkbook()
    return fresh ? toFunctionTableDtos(fresh) : toFunctionTableDtos(tables)
  }

  function updateParamTable(id: string, update: (t: ParamTableSpec) => ParamTableSpec) {
    setTables((all) =>
      all.map((t) => (t.id === id && t.kind === 'parametric' ? update(t) : t)),
    )
  }

  function updateActiveParam(update: (t: ParamTableSpec) => ParamTableSpec) {
    if (activeParam) updateParamTable(activeParam.id, update)
  }

  function sendDigitizedToFunctionTable(data: DigitizedExport) {
    const table = functionTableFromDigitizer({ existing: tables, ...data })
    setTables((all) => [...all, table])
    setActiveTableId(table.id)
    setActiveTab('table')
    requestAnimationFrame(() =>
      dockRef.current?.openInstance(TABLES_WORKBOOK_WINDOW_ID, 'table', 'Tables'),
    )
  }

  const handleStateUnitIdsChange = (
    val: Record<string, string> | ((prev: Record<string, string>) => Record<string, string>)
  ) => {
    setStateUnitIds((prev) => {
      const next = typeof val === 'function' ? val(prev) : val
      localStorage.setItem('frees.stateUnitIds', JSON.stringify(next))
      return next
    })
  }

  const solvable = checkResult?.solvable === true

  function savePreferences(criteria: StopCriteria, system: UnitSystem, fill: boolean) {
    // Never persist complexMode inside stopCriteria — it is a separate toggle
    const { complexMode: _ignored, ...persistable } = criteria
    setStopCriteria(persistable)
    setUnitSystem(system)
    setFillMissing(fill)
    localStorage.setItem(STOP_CRITERIA_KEY, JSON.stringify(persistable))
    localStorage.setItem(UNIT_SYSTEM_KEY, system)
    localStorage.setItem('frees.fillMissing', String(fill))
    setShowPreferences(false)
  }

  function buildVariableInfo(): VariableInfo[] {
    return variables.map((name) => {
      const draft = varDrafts[name] ?? DEFAULT_DRAFT
      return {
        name,
        guess:
          draft.guess.trim() !== '' && Number.isFinite(Number(draft.guess))
            ? Number(draft.guess)
            : null,
        lower: parseBound(draft.lower) ?? null,
        upper: parseBound(draft.upper) ?? null,
        units: draft.isUnitsUserSet ? (draft.units.trim() || null) : null,
        uncertainty:
          draft.uncertainty && draft.uncertainty.trim() !== '' && Number.isFinite(Number(draft.uncertainty))
            ? Number(draft.uncertainty)
            : null,
      }
    })
  }

  function onTextChange(value: string) {
    // Keystrokes land in the ref synchronously; the state update that re-renders
    // the rest of the app rides a low-priority (interruptible) transition.
    textRef.current = value
    startTransition(() => setText(value))
    // Any edit invalidates the previous Check; Solve is gated
    // until the system is re-checked. Table checks depend on the same text.
    // Guarded so only the first edit after a check/solve pays an urgent render.
    if (checkResult) setCheckResult(null)
    if (result) setResult(null)
    if (lastSolvedWithFillMissing) setLastSolvedWithFillMissing(false)
    invalidateTable()
    if (idleCheckTimer.current) clearTimeout(idleCheckTimer.current)
    idleCheckTimer.current = setTimeout(() => idleCheckRef.current(), 700)
  }

  // Load a curated example into the editor, replacing the current document and
  // invalidating any stale check/solve so the user can immediately re-Solve.
  function actuallyLoadExample(example: Example) {
    suppressDirtyRef.current = true
    isDirtyRef.current = false
    clearProjectLocal()
    void clearAutosaveMirror()
    writeBridgedKeys({
      version: 1,
      savedAt: '',
      text: example.text,
      varDrafts: {},
      stopCriteria,
      unitSystem,
      fillMissing,
      stateUnitIds: {},
      tables: [],
      plots: [],
      whiteboards: [],
      spreadsheets: [],
      analyzers: [],
      digitizer: null,
      dockLayout: null,
    })
    applyText(example.text)
    setVarDrafts({})
    setStateUnitIds({})
    setTables([])
    setPlots([])
    setWhiteboards([])
    setSpreadsheets([])
    setAnalyzers([])
    setSchematicOffsets({})
    channelStore.clear()
    setResult(null)
    setCheckResult(null)
    setLastSolvedWithFillMissing(false)
    setProjectName(example.title)
    setWorkspaceEpoch((e) => e + 1)
    requestAnimationFrame(() => dockRef.current?.reset())
  }

  function loadExample(example: Example) {
    setShowExamples(false)
    guardedAction(() => actuallyLoadExample(example))
  }

  // "Open in Editor" handoff from the /help docs: the Help page parks a runnable
  // snippet in localStorage and opens this route; consume it exactly once on
  // mount. Stale keys (> 5 min) are dropped — they are leftovers, not intent.
  useEffect(() => {
    const raw = localStorage.getItem('frees.pendingSnippet')
    if (!raw) return
    localStorage.removeItem('frees.pendingSnippet')
    try {
      const snip = JSON.parse(raw)
      if (typeof snip?.text !== 'string' || !snip.text.trim()) return
      if (typeof snip.ts !== 'number' || Date.now() - snip.ts > 5 * 60_000) return
      loadExample({
        id: 'doc-snippet',
        title: String(snip.title || 'Documentation example'),
        description: 'Loaded from the documentation',
        category: 'Documentation',
        text: snip.text,
      })
    } catch {
      // Malformed handoff: nothing to load.
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  /** The equations actually solved: editor text plus spreadsheet bindings.
   *  Reads textRef (not `text`) so a solve fired right after a keystroke sees
   *  the document before the deferred state sync lands. */
  function effectiveText(): string {
    const lines = [textRef.current]

    for (const ss of spreadsheets) {
      if (ss.bindings) {
        for (const [varName, refStr] of Object.entries(ss.bindings)) {
          lines.push(`${varName} = ssheet('${ss.name}', '${refStr}')`)
        }
      }
    }

    // ssheet() pulls a bare number out of a cell, so an input binding like
    // `T_1 = ssheet(...)` loses the variable's unit and the solver would read the
    // value as SI base (150 -> 150 K instead of 150 [C]). Re-attach the unit the
    // user declared in Variable Information, but ONLY for pure `VAR = ssheet(...)`
    // bindings resolving to a scalar — hand-written assignments stay untouched.
    const original = lines.join('\n').split('\n')
    const substituted = substituteSsheetRefs(lines.join('\n'), spreadsheets).split('\n')
    const draftFor = (name: string) =>
      varDrafts[name] ?? varDrafts[Object.keys(varDrafts).find((k) => k.toLowerCase() === name.toLowerCase()) ?? '']
    return substituted
      .map((subLine, i) => {
        const bind = (original[i] ?? '').match(/^\s*([A-Za-z_]\w*)\s*=\s*ssheet\s*\(/i)
        if (!bind) return subLine
        const draft = draftFor(bind[1])
        const unit = draft?.isUnitsUserSet ? draft.units.trim() : ''
        if (!unit) return subLine
        // Only a scalar numeric substitution — never a vector/matrix like [1, 2; 3].
        const scalar = subLine.match(/^(\s*[A-Za-z_]\w*\s*=\s*)(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)\s*$/)
        return scalar ? `${scalar[1]}${scalar[2]} [${unit}]` : subLine
      })
      .join('\n')
  }

  async function onCheck(): Promise<CheckResponse | null> {
    if (checking) return null
    setChecking(true)
    setResult(null)
    setLastSolvedWithFillMissing(false)
    try {
      const response = await check(
        effectiveText(),
        buildVariableInfo(),
        complexMode,
        functionTableDtos(),
        solveOverrides(),
      )
      setCheckResult(response)
      setTables((all) => mergeCodeTables(all, response.codeTables, response.parametricTables))
      // Sync the Variable Information table: keep edited rows for variables
      // that still exist, add defaults for new ones.
      setVariables(response.variables)
      setVarDrafts((drafts) => {
        const next: Record<string, VariableDraft> = {}
        for (const name of response.variables) {
          const existing = drafts[name] ?? { ...DEFAULT_DRAFT }
          next[name] = { ...existing }
          // Automatically inferred units are dynamic: if they are not explicitly
          // configured by the user, we update/sync them with the newly inferred ones.
          if (!existing.isUnitsUserSet) {
            next[name].units = response.inferredUnits[name] ?? ''
          }
        }
        return next
      })
      return response
    } catch (e) {
      const errorResponse: CheckResponse = {
        solvable: false,
        equations: 0,
        unknowns: 0,
        variables: [],
        unitWarnings: [],
        inferredUnits: {},
        message: `Could not reach the solver backend: ${String(e)}`,
      }
      setCheckResult(errorResponse)
      return errorResponse
    } finally {
      setChecking(false)
    }
  }

  // Live lint: run Check automatically once typing pauses, so broken lines are
  // marked (all of them — the multi-error lint) without pressing F4. The timer
  // fires through a ref because its closure would otherwise go stale across
  // renders; anything already in flight skips the tick rather than queueing.
  idleCheckRef.current = () => {
    if (checking || solving || solvingTableId) return
    if (!textRef.current.trim()) return
    void onCheck()
  }

  // Every solve/check override, REPL first then sliders: the backend collapses
  // the list by variable name keeping the last, so a pinned slider wins.
  // Only variables the document assigns a literal may be pinned: an override
  // replaces any line assigning the name, so offering a computed variable
  // would let a drag silently delete the equation defining it.
  const pinnableNames = useMemo(
    () => pinnableParameters(textRef.current),
    // Recomputed when a check/solve lands, which is also when the variable
    // list the affordance decorates is refreshed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [checkResult, result],
  )

  const pinnedSliderNames = useMemo(
    () => new Set(pinnedSliders.map((p) => p.name.toLowerCase())),
    [pinnedSliders],
  )

  const solveOverrides = () => [
    ...Object.values(replVars).map(replOverrideEquation),
    // Only pins that are STILL literal parameters are applied. A document edit
    // can turn a pinned name into a computed variable, and an override on one
    // of those replaces the equation defining it — so a stale pin goes inert
    // rather than silently deleting physics.
    ...pinnedSliders
      .filter((p) => pinnableNames.has(p.name.toLowerCase()))
      .map(sliderOverrideEquation),
  ]

  // Re-solve for the current slider values, honouring the in-flight guard.
  sliderSolveRef.current = () => {
    if (!solvable) return
    if (solving || checking) {
      sliderPendingRef.current = true
      return
    }
    void onSolve()
  }

  function scheduleSliderSolve(delayMs: number) {
    if (sliderTimer.current) clearTimeout(sliderTimer.current)
    sliderTimer.current = setTimeout(() => sliderSolveRef.current(), delayMs)
  }

  function setSliderValue(name: string, value: number, commit: boolean) {
    setPinnedSliders((prev) => prev.map((p) => (p.name === name ? { ...p, value } : p)))
    // Dragging re-solves on a short debounce so the solution tracks the handle;
    // the release commits promptly. Both go through the timer, so a fast drag
    // collapses into one solve rather than a queue of them.
    scheduleSliderSolve(commit ? 30 : 220)
  }

  function pinSlider(v: VariableResult) {
    setPinnedSliders((prev) => {
      if (findPin(prev, v.name)) return prev
      const draft = varDrafts[v.name]
        ?? varDrafts[Object.keys(varDrafts).find((k) => k.toLowerCase() === v.name.toLowerCase()) ?? '']
      const { min, max } = sliderRange(v.value, parseBound(draft?.lower ?? ''), parseBound(draft?.upper ?? ''))
      return [...prev, { name: v.name, value: v.value, units: v.units, min, max }]
    })
  }

  function unpinSlider(name: string) {
    setPinnedSliders((prev) => prev.filter((p) => p.name !== name))
  }

  function invalidateTable() {
    // Text edits invalidate the runs of every parametric table. Return the same
    // array when there is nothing to clear so a plain keystroke doesn't
    // re-render every table consumer.
    setTables((all) => {
      let changed = false
      const next = all.map((t) => {
        if (t.kind !== 'parametric') return t
        if (t.results.length === 0 && !t.stats && !t.checkResult && !t.checkMessage) return t
        changed = true
        return { ...t, results: [], stats: null, checkResult: null, checkMessage: '' }
      })
      return changed ? next : all
    })
  }

  function invalidateActiveParam(t: ParamTableSpec): ParamTableSpec {
    return { ...t, results: [], stats: null, checkResult: null, checkMessage: '' }
  }

  // Fresh hosted spec for a table run (contract b's pre-run scrape): flush
  // the Tables workbook so a just-typed sheet edit is part of THIS run — the
  // React state update from the flush lands a render too late for this
  // handler's closure.
  function freshParamTable(tableId: string): ParamTableSpec | undefined {
    const fresh = flushTablesWorkbook()?.find((t) => t.id === tableId)
    const t = fresh ?? tables.find((x) => x.id === tableId)
    return t?.kind === 'parametric' ? t : undefined
  }

  async function onCheckTable(tableIdArg?: string, overrideTbl?: ParamTableSpec): Promise<CheckResponse | null> {
    const tableId = tableIdArg ?? activeParam?.id
    if (checkingTableId !== null || !tableId) return null
    const tbl = overrideTbl ?? freshParamTable(tableId)
    if (!tbl || tbl.kind !== 'parametric') return null
    const tVars = tbl.vars
    const tRows = tbl.rows
    setCheckingTableId(tableId)
    updateParamTable(tableId, (t) => ({ ...t, results: [] }))
    try {
      // Check the augmented system: the equations plus one representative
      // fixed value per table input column (table semantics).
      const filled = firstFilledValues(tVars, tRows)
      let augmented = effectiveText()
      for (const [name, value] of filled) {
        augmented += `\n${name} = ${value}`
      }
      const response = await check(augmented, buildVariableInfo(), complexMode, functionTableDtos())
      updateParamTable(tableId, (t) => ({ ...t, checkResult: response }))

      // Sync variable list and units so the column headers show units for
      // calculated variables too (inferred + dimensionally derived).
      if (response.variables.length > 0) {
        setVariables(response.variables)
        setVarDrafts((drafts) => mergeInferredUnits(drafts, response.variables, response.inferredUnits))
      }

      if (response.solvable) {
        updateParamTable(tableId, (t) => ({
          ...t,
          checkMessage:
            `Table check passed: ${response.equations} equations and ` +
            `${response.unknowns} variables, with ${filled.size} value(s) ` +
            `supplied by the table.`,
        }))
      } else {
        const unfilledColumns = tVars.filter((v) => !filled.has(v))
        const hint =
          unfilledColumns.length > 0
            ? ` Fill input values for: ${unfilledColumns.join(', ')} (or use the column fill).`
            : ' Add the missing variables as table columns via Configure Columns, or fix the equations.'
        updateParamTable(tableId, (t) => ({ ...t, checkMessage: response.message + hint }))
      }
      return response
    } catch (e) {
      updateParamTable(tableId, (t) => ({
        ...t,
        checkResult: null,
        checkMessage: `Could not reach the solver backend: ${String(e)}`,
      }))
      return null
    } finally {
      setCheckingTableId(null)
    }
  }

  async function onSolveTable(tableIdArg?: string, checkOverride?: CheckResponse, overrideTbl?: ParamTableSpec): Promise<boolean> {
    const tableId = tableIdArg ?? activeParam?.id
    if (solvingTableId !== null || !tableId) return false
    const tbl = overrideTbl ?? freshParamTable(tableId)
    if (!tbl || tbl.kind !== 'parametric' || tbl.vars.length === 0) return false
    // When checkOverride is explicitly provided (from checkThenSolveTable), honour it.
    // When called directly from a per-window "Run Table" button we skip the gate so
    // independent-block equations (e.g. two separate circuits) still solve correctly
    // even when the global underdetermination check fails.
    if (checkOverride !== undefined && !checkOverride.solvable) return false
    setSolvingTableId(tableId)
    try {
      // Non-empty cells become fixed inputs for that run; blank cells are
      // solved per row (Solve Table semantics).
      const rows = tbl.rows.map((row) => {
        const fixed: Record<string, number> = {}
        for (const name of tbl.vars) {
          const raw = (row.values[name] ?? '').trim()
          if (raw !== '') {
            const value = Number(raw)
            if (Number.isFinite(value)) fixed[name] = value
          }
        }
        return fixed
      })
      const response = await solveTable(
        effectiveText(),
        { ...stopCriteria, complexMode },
        buildVariableInfo(),
        unitSystem,
        tbl.vars,
        rows,
        functionTableDtos(),
      )
      updateParamTable(tableId, (t) => ({
        ...t,
        results: response.results,
        stats: response.stats,
      }))
      if (response.variables && response.variables.length > 0) {
        setResult((prev) => ({
          success: true,
          variables: response.variables,
          blocks: prev?.blocks ?? [],
          residuals: prev?.residuals ?? [],
          stats: prev?.stats ?? null,
          solutions: [],
          unitWarnings: prev?.unitWarnings ?? [],
          error: null,
        }))
      }
      return true
    } catch (e) {
      updateParamTable(tableId, (t) => ({
        ...t,
        stats: null,
        results: t.rows.map(() => ({
          success: false,
          values: {},
          error: `Could not reach the solver backend: ${String(e)}`,
        })),
      }))
      return false
    } finally {
      setSolvingTableId(null)
    }
  }

  async function onSolve(
    forceFill: unknown = false,
    overridePlots?: PlotSpec[],
    checkOverride?: CheckResponse,
  ): Promise<boolean> {
    const canRun = checkOverride ? checkOverride.solvable === true : solvable
    if (solving || !canRun) return false
    setSolving(true)
    try {
      const activePlots = overridePlots ?? plots
      const needMissing =
        activePlots.some((p) => p.kind === 'property' && p.property.overlayStates) ||
        // PLOT-block property diagrams (resolved after Check) also need the
        // interpolated cycle path, so request it when one is present.
        codePlots.some((p) => p.kind === 'property' && p.property.overlayStates)
      const shouldFillMissing = (forceFill === true) || fillMissing || needMissing
      const response = await solve(
        effectiveText(),
        { ...stopCriteria, complexMode },
        buildVariableInfo(),
        findAll,
        unitSystem,
        shouldFillMissing,
        functionTableDtos(),
        sessionId,
        // REPL-defined/changed variables take priority over the editor until the
        // user runs `clear` in the terminal.
        solveOverrides(),
      )
      setResult(response)
      // REPL overrides persist across solves (the terminal keeps priority over the
      // editor); they're dropped only by the `clear` command, not by solving.
      // The Variable Explorer lives in the right edge group (expanded by default)
      // and shows the solved variables — it replaces the old Solution panel.
      // Solving updates its contents; the user can collapse it via its edge tab.
      setTables((all) => mergeCodeTables(all, response.codeTables, response.parametricTables, response.odeTables))
      setLastSolvedWithFillMissing(shouldFillMissing && response.success)
      // Once the user has solved successfully, they've learned the core
      // workflow — retire the first-run welcome banner so it stops eating
      // editor space.
      if (response.success && showFirstRun) dismissFirstRun()
      if (response.success && response.variables) {
        setReplVars((prev) => {
          const next = { ...prev }
          for (const v of response.variables) {
            const lower = v.name.toLowerCase()
            if (next[lower]) {
              next[lower] = {
                ...next[lower],
                value: v.value,
                units: v.units || '',
                uncertainty: v.uncertainty,
              }
            }
          }
          return next
        })
        setVarDrafts((drafts) => {
          const next = { ...drafts }
          for (const v of response.variables) {
            const name = v.name
            const existing = next[name] ?? { ...DEFAULT_DRAFT }
            const updated = { ...existing }
            if (!existing.isUnitsUserSet) {
              updated.units = v.units || ''
            }
            if (existing.uncertaintyType === 'relative' && existing.relativeUncertainty.trim() !== '') {
              const relVal = Number(existing.relativeUncertainty)
              if (Number.isFinite(relVal)) {
                updated.uncertainty = String(Number(((relVal / 100) * Math.abs(v.value)).toPrecision(6)))
              }
            }
            next[name] = updated
          }
          return next
        })
      }
      return response.success
    } catch (e) {
      setResult({
        success: false,
        variables: [],
        blocks: [],
        residuals: [],
        stats: null,
        solutions: [],
        unitWarnings: [],
        error: `Could not reach the solver backend: ${String(e)}`,
      })
      setLastSolvedWithFillMissing(false)
      return false
    } finally {
      setSolving(false)
    }
  }

  // "Just solve it": if the system is already checked, solve; otherwise run
  // Check first and chain into Solve when it passes. The fresh CheckResponse is
  // passed through so the solve guard doesn't read stale `solvable` state.
  async function checkThenSolve(): Promise<'workspace' | 'table' | void> {
    if (solving || checking) return
    if (solvable) {
      const ok = await onSolve()
      if (ok) return 'workspace'
      return
    }
    const res = await onCheck()
    if (res?.solvable) {
      const ok = await onSolve(false, undefined, res)
      if (ok) return 'workspace'
    } else if (res && !res.solvable && res.parametricTables && res.parametricTables.length > 0) {
      // Auto-fallback: if the main block is not fully determined but there is a parametric
      // sweep defined, the user likely intended to just "Solve Table".
      const dto = res.parametricTables[0]
      const overrideTbl = paramTableFromDto(dto)
      const tableId = overrideTbl.id
      dockRef.current?.openInstance(`table:${tableId}`, 'table', overrideTbl.name)
      const ok = await checkThenSolveTable(tableId, overrideTbl)
      if (ok) return 'table'
    }
  }

  async function checkWithFallback() {
    if (checking || solving) return
    const res = await onCheck()
    if (res && !res.solvable && res.parametricTables && res.parametricTables.length > 0) {
      const dto = res.parametricTables[0]
      const overrideTbl = paramTableFromDto(dto)
      const tableId = overrideTbl.id
      dockRef.current?.openInstance(`table:${tableId}`, 'table', overrideTbl.name)
      void onCheckTable(tableId, overrideTbl)
    }
  }

  async function checkThenSolveTable(tableIdArg?: string, overrideTbl?: ParamTableSpec): Promise<boolean> {
    const tableId = tableIdArg ?? activeParam?.id
    if (solvingTableId !== null || checkingTableId !== null || !tableId) return false
    const tbl = overrideTbl ?? freshParamTable(tableId)
    if (!tbl || tbl.kind !== 'parametric') return false
    if (tbl.checkResult?.solvable === true) {
      return await onSolveTable(tableId, undefined, overrideTbl)
    }
    const res = await onCheckTable(tableId, overrideTbl)
    if (res?.solvable) {
      return await onSolveTable(tableId, res, overrideTbl)
    }
    return false
  }

  // From a read-only table's column selection: open the New X-Y plot modal
  // pre-filled with the time column as x and the selected columns as y.
  const handlePlotColumns = (xVar: string, yVars: string[]) => {
    setPlotSeed({ xVar, yVars })
    setNewPlotKind('xy')
  }

  const handlePlotsChange = (nextPlots: PlotSpec[]) => {
    // Code-defined plots are derived from the solve response, not persisted, so
    // strip them before saving — they are re-merged on the next solve/check.
    const userPlots = nextPlots.filter((p) => !p.fromCode)
    setPlots(userPlots)
    const needMissing = userPlots.some((p) => p.kind === 'property' && p.property.overlayStates)
    if (needMissing && result?.success && !lastSolvedWithFillMissing && !solving && solvable) {
      void onSolve(true, userPlots)
    }
  }


  // Jump the editor to a 1-based line (selecting it) — the error Alert's "Go to
  // line" and the schematic's click-to-reveal. Opening the dock panel is what
  // actually makes the jump visible: setting the rail tab alone leaves the
  // editor behind whichever window is focused (or closed entirely), so the
  // selection would land somewhere the user cannot see.
  const goToLine = useCallback((lineNo: number) => {
    setActiveTab('equations')
    dockRef.current?.open('equations')
    setTimeout(() => editorRef.current?.goToLine(lineNo), 50)
  }, [])

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      // Shortcuts act on the active section: equations vs parametric table.
      if (e.key === 'F2') {
        e.preventDefault()
        void (activeTab === 'table' ? checkThenSolveTable() : checkThenSolve())
      }
      if (e.key === 'F4') {
        e.preventDefault()
        void (activeTab === 'table' ? onCheckTable() : onCheck())
      }
      // "?" opens the shortcuts overlay, but only when the user isn't typing
      // into the editor, an input, or any text field.
      if (e.key === '?') {
        const el = e.target as HTMLElement | null
        const typing =
          !!el &&
          (el.tagName === 'INPUT' ||
            el.tagName === 'TEXTAREA' ||
            el.isContentEditable ||
            el.closest('.cm-editor') !== null)
        if (!typing) {
          e.preventDefault()
          setShowShortcuts(true)
        }
      }
    }
    globalThis.addEventListener('keydown', onKeyDown)
    return () => globalThis.removeEventListener('keydown', onKeyDown)
  })

  const solutions = result?.solutions ?? []

  // Unit-consistency warnings from the latest Solve (preferred) or Check, shown
  // as a dismissible banner above the editor. Re-shown whenever the set changes.
  const unitWarnings = result?.unitWarnings ?? checkResult?.unitWarnings ?? []
  const warningsKey = unitWarnings.join('|')
  useEffect(() => {
    setDismissedWarnings(false)
  }, [warningsKey])

  // 1-based editor line a syntax error points at (from Solve, then Check), used
  // to mark the gutter and offer a jump-to-line action.
  const errorLine = result?.errorLine ?? checkResult?.errorLine ?? null

  // Plots declared in the editor text with PLOT ... END blocks, regenerated on
  // every solve/check. Merged with GUI plots for display and [Graph] resolution
  // but kept out of the persisted project (handlePlotsChange strips fromCode).
  const codePlots = useMemo<PlotSpec[]>(() => {
    const dtos = result?.definedPlots ?? checkResult?.definedPlots ?? []
    return dtos.map(plotDefToSpec)
  }, [result?.definedPlots, checkResult?.definedPlots])

  const mergedPlots = useMemo<PlotSpec[]>(() => {
    const userNames = new Set(plots.map((p) => p.name.toLowerCase()))
    return [...plots, ...codePlots.filter((c) => !userNames.has(c.name.toLowerCase()))]
  }, [plots, codePlots])

  // Auto-close dock windows whose backing instance no longer exists — e.g. a
  // plot removed from its card, a deleted table, or a stale window
  // restored from a saved layout. Without this they'd render as blank panels.
  useEffect(() => {
    const valid = new Set<string>([
      'equations', 'table', 'plots', 'digitizer', 'schematic', 'workspace', 'terminal', 'states', 'inspector',
      TABLES_WORKBOOK_WINDOW_ID,
      ...mergedPlots.map((p) => `plot:${p.id}`),
      // Hosted tables (function + GUI parametric) live as sheets in the
      // single Tables workbook window; no per-table windows (decision 2).
      ...tables.filter((t) => !isHostedTable(t)).map((t) => `table:${t.id}`),
      ...whiteboards.map((w) => `whiteboard:${w.id}`),
      ...spreadsheets.map((s) => `spreadsheet:${s.id}`),
      ...analyzers.map((a) => `analyzer:${a.id}`),
      ...(result?.stateTableDefs ?? checkResult?.stateTableDefs ?? []).map((s) => `state:${s.name}`),
    ])
    for (const w of openWindows) {
      if (!valid.has(w.id)) dockRef.current?.close(w.id)
    }
  }, [mergedPlots, tables, whiteboards, spreadsheets, analyzers, openWindows, result?.stateTableDefs, checkResult?.stateTableDefs])

  // Keep dock tab titles in sync with instance names (so renames in the
  // Inspector show on the tabs). Deferred out of the commit cycle so dockview's
  // own re-render can't re-enter React mid-update.
  useEffect(() => {
    const raf = requestAnimationFrame(() => {
      for (const p of mergedPlots) dockRef.current?.setTitle(`plot:${p.id}`, p.name)
      for (const t of tables) {
        if (isHostedTable(t)) continue // hosted in the Tables workbook
        dockRef.current?.setTitle(`table:${t.id}`, t.name)
      }
      for (const w of whiteboards) dockRef.current?.setTitle(`whiteboard:${w.id}`, w.name)
      for (const s of spreadsheets) dockRef.current?.setTitle(`spreadsheet:${s.id}`, s.name)
      for (const a of analyzers) dockRef.current?.setTitle(`analyzer:${a.id}`, a.name)
    })
    return () => cancelAnimationFrame(raf)
  }, [mergedPlots, tables, whiteboards, spreadsheets, analyzers])

  // Analyzers with an OPEN dock window are protected from the ChannelStore's
  // over-ceiling LRU eviction (§2.5a) — tell the store which ones those are.
  useEffect(() => {
    channelStore.setOpenAnalyzers(
      openWindows.filter((w) => w.kind === 'analyzer').map((w) => w.id.slice('analyzer:'.length)),
    )
  }, [openWindows])

  const baseVariables =
    solutions.length > 0 ? solutions[0].variables : result?.variables ?? []

  // Solved variables with the REPL overlay applied: REPL-defined names are
  // appended, and REPL-changed names override the solved value. Feeds the
  // Variable Explorer, the Solution rows and the terminal's tab-completion.
  const replOverlay = Object.values(replVars)
  const workspaceVariables: VariableResult[] = replOverlay.length === 0
    ? baseVariables
    : [
        ...baseVariables.map((v) => replVars[v.name.toLowerCase()] ?? v),
        ...replOverlay.filter(
          (v) => !baseVariables.some((b) => b.name.toLowerCase() === v.name.toLowerCase()),
        ),
      ]
  const replNames = new Set(Object.keys(replVars))
  // All callable function names (property functions + built-ins, including CALL
  // library callees) for the terminal's Tab-completion — bare names so a
  // completion inserts `lqr(`, not the menu's descriptive label.
  const replFunctionNames = catalogFunctionNames()

  // Fluid state tables declared with STATE TABLE blocks: surfaced in the left
  // Tables menu (tagged by fluid) and opened in the shared Fluid States window.
  const declaredStateDefs = result?.stateTableDefs ?? checkResult?.stateTableDefs ?? []

  // Reusable window open/create handlers, shared by the left rail and the
  // command palette so both open real dock windows (not just highlight a tab).
  // Opens the dock window backing a table: function tables live as sheets in
  // the single Univer Tables workbook (decision 2), everything else keeps its
  // per-table window.
  const openTableWindow = (t: TableSpec) => {
    if (isHostedTable(t)) {
      setActiveTableId(t.id)
      dockRef.current?.openInstance(TABLES_WORKBOOK_WINDOW_ID, 'table', 'Tables')
    } else {
      dockRef.current?.openInstance(`table:${t.id}`, 'table', t.name)
    }
  }
  const createTable = (kind: 'parametric' | 'function-1d' | 'function-2d') => {
    const t =
      kind === 'parametric'
        ? newParamTable(tables)
        : newFunctionTable(tables, kind === 'function-1d')
    setTables((prev) => [...prev, t])
    setActiveTableId(t.id)
    requestAnimationFrame(() => openTableWindow(t))
  }
  const createWhiteboard = () => {
    const wb = newWhiteboard(whiteboards.length)
    setWhiteboards((prev) => [...prev, wb])
    requestAnimationFrame(() => dockRef.current?.openInstance(`whiteboard:${wb.id}`, 'whiteboard', wb.name))
  }
  const openLatestOrNewWhiteboard = () => {
    const wb = whiteboards[whiteboards.length - 1]
    if (wb) dockRef.current?.openInstance(`whiteboard:${wb.id}`, 'whiteboard', wb.name)
    else createWhiteboard()
  }
  const createSpreadsheet = () => {
    const ss = newSpreadsheet(spreadsheets.length)
    setSpreadsheets((prev) => [...prev, ss])
    requestAnimationFrame(() => dockRef.current?.openInstance(`spreadsheet:${ss.id}`, 'spreadsheet', ss.name))
  }
  const openLatestOrNewSpreadsheet = () => {
    const ss = spreadsheets[spreadsheets.length - 1]
    if (ss) dockRef.current?.openInstance(`spreadsheet:${ss.id}`, 'spreadsheet', ss.name)
    else createSpreadsheet()
  }
  const createAnalyzer = () => {
    const a = newAnalyzer(analyzers.length)
    setAnalyzers((prev) => [...prev, a])
    requestAnimationFrame(() => dockRef.current?.openInstance(`analyzer:${a.id}`, 'analyzer', a.name))
  }
  const openLatestOrNewAnalyzer = () => {
    const a = analyzers[analyzers.length - 1]
    if (a) dockRef.current?.openInstance(`analyzer:${a.id}`, 'analyzer', a.name)
    else createAnalyzer()
  }
  // Release binds to analyzer DELETION, not window close (§2.5a): dropping the
  // spec releases its measurements from the ChannelStore (shared entries are
  // refcounted, so a second analyzer on the same file keeps its data).
  const deleteAnalyzer = (id: string) => {
    const a = analyzers.find((x) => x.id === id)
    if (a) for (const f of a.files) channelStore.release(f.measurementId, a.id)
    setAnalyzers((prev) => prev.filter((x) => x.id !== id))
  }
  const exportToSpreadsheet = (vars: VariableResult[]) => {
    const grouped = group(vars)
    const ss = newSpreadsheet(spreadsheets.length)
    ss.name = `Export ${new Date().toLocaleTimeString()}`

    const celldata: any[] = []
    let currentRow = 0

    // Bold formatting for headers
    const headerStyle = { bl: 1 }

    if (grouped.scalars.length > 0) {
      celldata.push({ r: currentRow, c: 0, v: { v: 'Variable', m: 'Variable', ...headerStyle } })
      celldata.push({ r: currentRow, c: 1, v: { v: 'Value', m: 'Value', ...headerStyle } })
      celldata.push({ r: currentRow, c: 2, v: { v: 'Units', m: 'Units', ...headerStyle } })
      celldata.push({ r: currentRow, c: 3, v: { v: 'Uncertainty', m: 'Uncertainty', ...headerStyle } })
      currentRow++

      for (const s of grouped.scalars) {
        celldata.push({ r: currentRow, c: 0, v: { v: s.name, m: s.name } })
        celldata.push({ r: currentRow, c: 1, v: { v: String(s.value), m: String(s.value) } })
        celldata.push({ r: currentRow, c: 2, v: { v: s.units, m: s.units } })
        if (s.uncertainty != null) {
          celldata.push({ r: currentRow, c: 3, v: { v: String(s.uncertainty), m: String(s.uncertainty) } })
        }
        currentRow++
      }
      currentRow++
    }

    for (const g of grouped.groups) {
      celldata.push({ r: currentRow, c: 0, v: { v: g.name, m: g.name, ...headerStyle } })
      if (g.units) {
        celldata.push({ r: currentRow, c: 1, v: { v: `[${g.units}]`, m: `[${g.units}]` } })
      }
      currentRow++

      if (g.is2D) {
        // Col headers
        for (let j = 0; j < g.cols.length; j++) {
          celldata.push({ r: currentRow, c: j + 1, v: { v: String(g.cols[j]), m: String(g.cols[j]), ...headerStyle } })
        }
        currentRow++
        // Row data
        for (let i = 0; i < g.rows.length; i++) {
          const rId = g.rows[i]
          celldata.push({ r: currentRow, c: 0, v: { v: String(rId), m: String(rId), ...headerStyle } })
          for (let j = 0; j < g.cols.length; j++) {
            const cId = g.cols[j]
            const cell = g.cells.get(`${rId},${cId}`)
            if (cell) {
              celldata.push({ r: currentRow, c: j + 1, v: { v: String(cell.value), m: String(cell.value) } })
            }
          }
          currentRow++
        }
      } else {
        // Index headers
        celldata.push({ r: currentRow, c: 0, v: { v: 'Index', m: 'Index', ...headerStyle } })
        celldata.push({ r: currentRow, c: 1, v: { v: 'Value', m: 'Value', ...headerStyle } })
        currentRow++
        for (let i = 0; i < g.rows.length; i++) {
          const rId = g.rows[i]
          celldata.push({ r: currentRow, c: 0, v: { v: String(rId), m: String(rId), ...headerStyle } })
          const cell = g.cells.get(`${rId}`)
          if (cell) {
            celldata.push({ r: currentRow, c: 1, v: { v: String(cell.value), m: String(cell.value) } })
          }
          currentRow++
        }
      }
      currentRow++
    }

    const sheetData = emptySpreadsheetData()
    ;(sheetData[0] as any).celldata = celldata
    ss.sheets = sheetData

    setSpreadsheets((prev) => [...prev, ss])
    requestAnimationFrame(() => dockRef.current?.openInstance(`spreadsheet:${ss.id}`, 'spreadsheet', ss.name))
  }
  // "Open in Spreadsheet" (Phase 3, contract e): a one-shot, timestamped,
  // cap-guarded snapshot into a NEW spreadsheet window — deliberately
  // decoupled from re-solves. Shared by table exports and the states table.
  const createSnapshotSpreadsheet = (input: SnapshotInput) => {
    const out = buildSnapshotSheet(input)
    if (!out.ok) {
      if (out.reason === 'too-big') setLoadNotice(out.message)
      return
    }
    const ss = newSpreadsheet(spreadsheets.length)
    ss.name = out.name
    const sheetData = emptySpreadsheetData()
    Object.assign(sheetData[0] as object, {
      celldata: out.sheet.celldata,
      styles: out.sheet.styles,
      name: 'Sheet1',
    })
    ss.sheets = sheetData
    setSpreadsheets((prev) => [...prev, ss])
    requestAnimationFrame(() => dockRef.current?.openInstance(`spreadsheet:${ss.id}`, 'spreadsheet', ss.name))
  }

  const exportTableToSpreadsheet = (tableId: string) => {
    const t = tables.find((tbl) => tbl.id === tableId)
    if (!t || t.kind !== 'parametric') return
    createSnapshotSpreadsheet({
      title: `Export ${t.name}`,
      columns: [
        { name: 'Run' },
        ...t.vars.map((v) => ({ name: v, unit: t.columnUnits?.[v] })),
      ],
      rows: t.rows.map((row, i) => [
        t.results[i] && !t.results[i].success ? `${i + 1} ✗` : i + 1,
        ...t.vars.map((v) => {
          const draft = row.values[v]
          if (draft && draft.trim() !== '') return draft
          const computed = t.results[i]?.success ? t.results[i].values[v] : undefined
          return computed !== undefined ? computed : ''
        }),
      ]),
    })
  }
  const openLatestOrNewTable = () => {
    const t = tables[tables.length - 1]
    if (t) openTableWindow(t)
    else createTable('parametric')
  }
  const openLatestOrNewPlot = () => {
    const p = mergedPlots[mergedPlots.length - 1]
    if (p) dockRef.current?.openInstance(`plot:${p.id}`, 'plot', p.name)
    else setNewPlotKind('xy')
  }

  // Command palette (Ctrl/Cmd+K): jump to any view, open any tool window, run
  // Check/Solve, or manage the project — all from one searchable list.
  const spotlightActions: SpotlightActionGroupData[] = [
    {
      group: 'Run',
      actions: [
        {
          id: 'check',
          label: 'Check',
          description: 'Validate the system (F4)',
          leftSection: <IconChecks size={18} />,
          onClick: () => {
            setActiveTab('equations')
            void onCheck()
          },
        },
        {
          id: 'solve',
          label: 'Solve',
          description: 'Check & solve the system (F2)',
          leftSection: <IconPlayerPlayFilled size={16} />,
          onClick: () => {
            setActiveTab('equations')
            void checkThenSolve()
          },
        },
      ],
    },
    {
      group: 'Views',
      actions: [
        { id: 'view-editor', label: 'Editor', leftSection: <IconCode size={18} />, onClick: () => dockRef.current?.open('equations') },
        { id: 'view-table', label: 'Tables', description: 'Open the latest table (or create one)', leftSection: <IconTable size={18} />, onClick: openLatestOrNewTable },
        { id: 'view-plots', label: 'Plots', description: 'Open the latest plot (or create one)', leftSection: <IconChartLine size={18} />, onClick: openLatestOrNewPlot },
        { id: 'view-states', label: 'Fluid States', leftSection: <IconTemperature size={18} />, onClick: () => {
          const first = declaredStateDefs[0]
          if (first) dockRef.current?.openInstance(`state:${first.name}`, 'states', first.name)
          else dockRef.current?.open('states')
        } },
        { id: 'view-digitizer', label: 'Graph Digitizer', leftSection: <IconChartGridDots size={18} />, onClick: () => dockRef.current?.open('digitizer') },
        { id: 'view-schematic', label: 'Schematic', description: 'Auto-rendered component network', leftSection: <IconSitemap size={18} />, onClick: () => dockRef.current?.open('schematic') },
        { id: 'view-whiteboard', label: 'Whiteboard', description: 'Open the latest whiteboard (or create one)', leftSection: <IconBrush size={18} />, onClick: openLatestOrNewWhiteboard },
        { id: 'view-spreadsheet', label: 'Spreadsheet', description: 'Open the latest spreadsheet (or create one)', leftSection: <IconGrid4x4 size={18} />, onClick: openLatestOrNewSpreadsheet },
        { id: 'view-analyzer', label: 'Data Analyzer', description: 'Open the latest analyzer (or create one)', leftSection: <IconWaveSine size={18} />, onClick: openLatestOrNewAnalyzer },
        { id: 'view-inspector', label: 'Inspector', leftSection: <IconSettings size={18} />, onClick: () => dockRef.current?.open('inspector') },
      ],
    },
    {
      group: 'Create',
      actions: [
        { id: 'new-param-table', label: 'Add parametric table', leftSection: <IconTable size={18} />, onClick: () => createTable('parametric') },
        { id: 'new-xy-plot', label: 'Add graph (X-Y)', leftSection: <IconChartLine size={18} />, onClick: () => setNewPlotKind('xy') },
        { id: 'new-property-plot', label: 'Add property graph', leftSection: <IconTemperature size={18} />, onClick: () => setNewPlotKind('property') },
        { id: 'new-psychro-plot', label: 'Add psychrometric graph', leftSection: <IconTemperature size={18} />, onClick: () => setNewPlotKind('psychro') },
        { id: 'new-whiteboard', label: 'Add whiteboard', description: 'New Excalidraw freehand sketch canvas', leftSection: <IconBrush size={18} />, onClick: createWhiteboard },
        { id: 'new-spreadsheet', label: 'Add spreadsheet', description: 'New spreadsheet workbook', leftSection: <IconGrid4x4 size={18} />, onClick: createSpreadsheet },
        { id: 'new-analyzer', label: 'Add data analyzer', description: 'Import and explore measurement data (CSV/TSV)', leftSection: <IconWaveSine size={18} />, onClick: createAnalyzer },
        { id: 'new-state-table', label: 'Add fluid state table', description: 'Insert a STATE TABLE block (fluid-aware circuit) at the caret', leftSection: <IconTemperature size={18} />, onClick: () => insertFunction('STATE TABLE Circuit1(P1, T1, h2)\n  FLUID = Water\nEND\n') },
      ],
    },
    {
      group: 'Tools',
      actions: [
        { id: 'tool-variables', label: 'Variable Information', leftSection: <IconVariable size={18} />, onClick: () => setShowVariableInfo(true) },
        { id: 'tool-preferences', label: 'Preferences', leftSection: <IconSettings size={18} />, onClick: () => setShowPreferences(true) },
        { id: 'tool-about', label: 'About', leftSection: <IconInfoCircle size={18} />, onClick: () => setShowAbout(true) },
      ],
    },
    {
      group: 'Project',
      actions: [
        { id: 'proj-examples', label: 'Open Example…', description: 'Load a ready-to-solve worked example', leftSection: <IconLayoutGrid size={18} />, onClick: () => setShowExamples(true) },
        { id: 'proj-share', label: 'Copy Share Link', description: 'Self-contained URL carrying this document', leftSection: <IconLink size={18} />, onClick: handleShareLink },
        { id: 'proj-report', label: 'Print Report…', description: 'Printable calculation report of the last solve (print to PDF)', leftSection: <IconPrinter size={18} />, onClick: handlePrintReport },
        { id: 'help-getting-started', label: 'Getting Started…', description: 'What frees is, and four one-click ways in', leftSection: <IconHelp size={18} />, onClick: () => setShowGettingStarted(true) },
        { id: 'proj-component', label: 'Component Wizard', description: 'Browse the component library and insert a configured component', leftSection: <IconLayoutGrid size={18} />, onClick: () => setShowComponentWizard(true) },
        { id: 'proj-new', label: 'New Project', leftSection: <IconFilePlus size={18} />, onClick: handleNewProject },
        { id: 'proj-open', label: 'Open Project…', leftSection: <IconFolderOpen size={18} />, onClick: handleOpenProject },
        { id: 'proj-save', label: 'Save Project', leftSection: <IconDeviceFloppy size={18} />, onClick: handleSaveProject },
        { id: 'proj-saveas', label: 'Save Project As…', leftSection: <IconDeviceFloppy size={18} />, onClick: handleSaveProjectAs },
        { id: 'proj-library', label: 'Browser Projects…', description: 'Projects saved in this browser — no server, no files', leftSection: <IconDatabase size={18} />, onClick: () => setLibraryOpen(true) },
        { id: 'proj-save-browser', label: 'Save to Browser', description: 'Keep this project in the browser under its current name', leftSection: <IconDatabase size={18} />, onClick: () => { void handleSaveToBrowser().then((ok) => notifications.show(ok ? { color: 'teal', title: 'Saved to browser', message: `“${projectName}” is stored in this browser.` } : { color: 'yellow', title: 'Could not save to browser', message: 'Browser storage may be unavailable in this browsing mode.' })) } },
      ],
    },
    {
      group: 'Help',
      actions: [
        { id: 'shortcuts', label: 'Keyboard shortcuts', description: 'Show the hotkey reference (?)', leftSection: <IconKeyboard size={18} />, onClick: () => setShowShortcuts(true) },
        { id: 'help', label: 'Help', leftSection: <IconHelp size={18} />, onClick: () => globalThis.open('/help', '_blank') },
      ],
    },
    // Every documentation guide page as a palette entry, deep-linked into the
    // /help portal (the compact id/label list is generated by compile-docs so
    // the full doc catalogs stay code-split with the /help route).
    {
      group: 'Documentation',
      actions: DOCS_TOPICS.map((t) => ({
        id: `doc-${t.id}`,
        label: t.label,
        description: 'Open this guide in the documentation portal',
        keywords: ['docs', 'documentation', 'guide', 'help'],
        leftSection: <IconHelp size={18} />,
        onClick: () => globalThis.open(`/help#${t.id}`, '_blank'),
      })),
    },
    // Every catalog function as a searchable palette entry: explanation plus a
    // sample call in the description, inserting the snippet at the editor caret.
    ...FUNCTION_CATEGORIES.map((cat) => ({
      group: cat.category,
      actions: cat.items.map((item) => ({
        id: `fn-${cat.category}-${item.label}`,
        label: item.label,
        description: item.usage
          ? `${item.description ?? ''}  e.g. ${item.usage}`
          : item.description,
        keywords: [cat.category, 'function'],
        leftSection: <IconMathFunction size={18} />,
        onClick: () => insertFunction(item.snippet),
      })),
    })),
  ]

  // Content for each dockview window kind. Recomputed every render and read by
  // the dock through context; closed panels create the element but never mount.
  const panelPad: React.CSSProperties = {
    height: '100%',
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    padding: 'var(--mantine-spacing-md)',
    overflow: 'auto',
  }
  // On mobile the editor should fill the whole tab with only a hairline of
  // padding for visibility; on desktop it keeps the standard panel padding.
  const editorPanelPad: React.CSSProperties = isMobile
    ? { ...panelPad, padding: 4 }
    : panelPad
  // Plot windows must let the chart fill exactly (no scroll), so the wrapper is
  // a non-scrolling flex column with a tight pad.
  const plotPanelStyle: React.CSSProperties = {
    height: '100%',
    minHeight: 0,
    display: 'flex',
    flexDirection: 'column',
    padding: 'var(--mantine-spacing-xs)',
    overflow: 'hidden',
  }
  const PLOT_KIND_LABEL: Record<PlotKind, string> = {
    xy: 'X-Y',
    property: 'Property',
    psychro: 'Psychrometric',
    bode: 'Bode',
    nyquist: 'Nyquist',
    nichols: 'Nichols',
    polezero: 'Pole-Zero',
    rootlocus: 'Root Locus',
  }
  const panelContent: Record<string, ReactNode> = {
    equations: (
      <div style={editorPanelPad}>
        {errorLine != null && (
          <Alert color="red" variant="light" p="xs" mb={6} title="Syntax error">
            <Group justify="space-between" wrap="nowrap" gap="xs">
              <Text size="xs">Syntax error on line {errorLine}.</Text>
              <Button size="compact-xs" variant="light" color="red" onClick={() => goToLine(errorLine)}>
                Go to line {errorLine}
              </Button>
            </Group>
          </Alert>
        )}
        {showFirstRun && (
          <Alert color="teal" variant="light" p="xs" mb={6} withCloseButton onClose={dismissFirstRun} title="Welcome to frees">
            <Text size="xs">
              Write equations and notes on the left — they can be
              entered in any order. Click <strong>Check</strong> (F4) to
              validate, then <strong>Solve</strong> (F2). Solve also runs
              Check for you automatically.{' '}
              <Anchor size="xs" component="button" type="button" onClick={() => setShowGettingStarted(true)}>
                Getting started guide
              </Anchor>
            </Text>
          </Alert>
        )}
        {unitWarnings.length > 0 && !dismissedWarnings && (
          <Alert
            color="yellow"
            variant="light"
            p="xs"
            mb={6}
            withCloseButton
            onClose={() => setDismissedWarnings(true)}
            title={`${unitWarnings.length} unit consistency warning${unitWarnings.length === 1 ? '' : 's'}`}
          >
            <Stack gap={2} mah={120} style={{ overflowY: 'auto' }}>
              {withStableKeys(unitWarnings).map((w) => (
                <Text size="xs" key={w.key}>
                  ⚠ {w.value}
                </Text>
              ))}
            </Stack>
          </Alert>
        )}
        <div
          style={{
            display: 'flex',
            flex: 1,
            minHeight: 0,
            border: '1px solid var(--mantine-color-default-border)',
            borderRadius: 'var(--mantine-radius-sm)',
          }}
        >
          <Suspense fallback={lazyTabFallback}>
            <EquationEditor
              ref={editorRef}
              initialDoc={() => textRef.current}
              onChange={onTextChange}
              variables={variables}
              errorLine={errorLine}
              errorMessage={result?.error ?? checkResult?.message ?? null}
              errorList={checkResult?.errors ?? null}
              placeholder={'Enter equations and notes, e.g.\n{ Rankine Cycle }\nT1 = 100 [C]\nP1 = 250 [kPa]'}
            />
          </Suspense>
        </div>
      </div>
    ),
    states: (
      <div style={panelPad}>
        <Group justify="space-between" mb="xs" wrap="nowrap" align="center">
          <Title order={5} c="teal.4">Fluid State Table</Title>
          <Text size="xs" c="dimmed">Solved state points</Text>
        </Group>
        <Suspense fallback={lazyTabFallback}>
          <StatesTab
            solvedVariables={result?.variables ?? []}
            stateTableDefs={result?.stateTableDefs ?? checkResult?.stateTableDefs ?? []}
            unitIds={stateUnitIds}
            onUnitIdsChange={handleStateUnitIdsChange}
            onFillMissing={() => onSolve(true)}
            solving={solving}
            solvable={solvable}
            onSnapshot={createSnapshotSpreadsheet}
          />
        </Suspense>
      </div>
    ),
    digitizer: (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <DigitizerTab key={`digitizer-${workspaceEpoch}`} onSendToFunctionTable={sendDigitizedToFunctionTable} />
        </Suspense>
      </div>
    ),
    schematic: (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <SchematicTab
            key={`schematic-${workspaceEpoch}`}
            checkResult={checkResult}
            components={result?.components}
            variables={result?.variables}
            text={textRef.current}
            onRevealLine={goToLine}
            onEmitStatement={emitFromSchematic}
            offsets={schematicOffsets}
            onOffsetsChange={setSchematicOffsets}
          />
        </Suspense>
      </div>
    ),
    inspector: (() => {
      const fw = focusedWindow
      const bodyStyle: React.CSSProperties = { flex: 1, minHeight: 0, overflow: 'auto', padding: 10 }

      // Table: rename + the parametric table's quick actions. The Tables
      // workbook window inspects whichever hosted table is active in it.
      if (fw?.kind === 'table') {
        const t =
          fw.id === TABLES_WORKBOOK_WINDOW_ID
            ? (activeTable && isHostedTable(activeTable) ? activeTable : undefined)
            : tables.find((x) => `table:${x.id}` === fw.id)
        return (
          <div style={{ height: '100%', minHeight: 0, display: 'flex', flexDirection: 'column' }}>
            <div style={{ ...bodyStyle }}>
              <Stack gap="xs">
                <Text size="sm" fw={600} c="teal.4">Table</Text>
                <TextInput
                  size="xs"
                  label="Table name"
                  value={t?.name ?? ''}
                  disabled={!t || t.source === 'code'}
                  onChange={(e) => {
                    const value = e.currentTarget.value
                    if (t) setTables((prev) => renameById(prev, t.id, value))
                  }}
                />
                {t?.kind === 'parametric' ? (
                  <>
                    <Button size="xs" variant="default" onClick={() => { setActiveTableId(t.id); setShowConfigureTable(true) }}>
                      Configure Columns
                    </Button>
                    <Group grow>
                      <Button size="xs" variant="default" onClick={() => updateParamTable(t.id, (pt) => invalidateActiveParam({ ...pt, rows: [...pt.rows, newParamRow()] }))}>
                        Add Row
                      </Button>
                      <Button size="xs" variant="default" onClick={() => updateParamTable(t.id, (pt) => invalidateActiveParam({ ...pt, rows: pt.rows.slice(0, -1) }))}>
                        Remove Row
                      </Button>
                    </Group>
                    <Button size="xs" variant="default" color="gray" onClick={() => updateParamTable(t.id, (pt) => ({ ...pt, results: [] }))}>
                      Clear Results
                    </Button>
                    <Button size="xs" variant="default" onClick={() => exportTableToSpreadsheet(t.id)}>
                      Export to Spreadsheet
                    </Button>
                  </>
                ) : (
                  <Text size="xs" c="dimmed">Function table — edit values in the table window.</Text>
                )}
              </Stack>
            </div>
          </div>
        )
      }

      // Plot: rename (user plots) + delete; configure/export live on the card.
      if (fw?.kind === 'plot') {
        const p = mergedPlots.find((x) => `plot:${x.id}` === fw.id)
        return (
          <div style={bodyStyle}>
            <Stack gap="xs">
              <Text size="sm" fw={600} c="teal.4">{p ? PLOT_KIND_LABEL[p.kind] : 'Plot'}</Text>
              <TextInput
                size="xs"
                label="Plot name"
                value={p?.name ?? ''}
                disabled={!p || p.fromCode}
                onChange={(e) => {
                  const value = e.currentTarget.value
                  if (p) handlePlotsChange(plots.map((x) => (x.id === p.id ? { ...x, name: value } : x)))
                }}
              />
              <Text size="xs" c="dimmed">Configure and Export are on the plot's toolbar.</Text>
              {p && !p.fromCode && (
                <Button size="xs" variant="light" color="red" onClick={() => handlePlotsChange(plots.filter((x) => x.id !== p.id))}>
                  Delete plot
                </Button>
              )}
            </Stack>
          </div>
        )
      }

      // Whiteboard (Excalidraw): rename + delete; drawing tools live in the
      // Excalidraw canvas itself.
      if (fw?.kind === 'whiteboard') {
        const wb = whiteboards.find((x) => `whiteboard:${x.id}` === fw.id)
        return (
          <div style={bodyStyle}>
            <Stack gap="xs">
              <Text size="sm" fw={600} c="teal.4">Whiteboard</Text>
              <TextInput
                size="xs"
                label="Whiteboard name"
                value={wb?.name ?? ''}
                onChange={(e) => {
                  const value = e.currentTarget.value
                  if (wb) setWhiteboards((prev) => renameById(prev, wb.id, value))
                }}
              />
              <Text size="xs" c="dimmed">Freehand sketch canvas (Excalidraw). Export to PNG/SVG from the toolbar above the canvas.</Text>
              {wb && (
                <Button size="xs" variant="light" color="red" onClick={() => setWhiteboards((prev) => prev.filter((x) => x.id !== wb.id))}>
                  Delete whiteboard
                </Button>
              )}
            </Stack>
          </div>
        )
      }

      // Spreadsheet: rename + delete
      if (fw?.kind === 'spreadsheet') {
        const ss = spreadsheets.find((x) => `spreadsheet:${x.id}` === fw.id)
        return (
          <div style={bodyStyle}>
            <Stack gap="xs">
              <Text size="sm" fw={600} c="teal.4">Spreadsheet</Text>
              <TextInput
                size="xs"
                label="Name"
                value={ss?.name ?? ''}
                disabled={!ss}
                onChange={(e) => {
                  const value = e.currentTarget.value
                  if (ss) setSpreadsheets(spreadsheets.map((x) => (x.id === ss.id ? { ...x, name: value } : x)))
                }}
              />
              {ss && (
                <Button size="xs" variant="light" color="red" onClick={() => setSpreadsheets(spreadsheets.filter((x) => x.id !== ss.id))}>
                  Delete spreadsheet
                </Button>
              )}
            </Stack>
          </div>
        )
      }

      // Data Analyzer: rename + the signal browser (a measurement-suite-style
      // dockable variable window) — import CSV/MF4 or solved tables and assign channels to the
      // selected strip without leaving the Inspector.
      if (fw?.kind === 'analyzer') {
        const an = analyzers.find((x) => `analyzer:${x.id}` === fw.id)
        if (an) {
          return (
            <div style={{ height: '100%', minHeight: 0, display: 'flex', flexDirection: 'column' }}>
              <div style={{ padding: 10, paddingBottom: 4 }}>
                <Stack gap="xs">
                  <Text size="sm" fw={600} c="teal.4">Data Analyzer</Text>
                  <TextInput
                    size="xs"
                    label="Analyzer name"
                    value={an.name}
                    onChange={(e) => {
                      const value = e.currentTarget.value
                      setAnalyzers((prev) => renameById(prev, an.id, value))
                    }}
                  />
                </Stack>
              </div>
              <div style={{ flex: 1, minHeight: 0, padding: 10, paddingTop: 4 }}>
                <Suspense fallback={lazyTabFallback}>
                  <SignalBrowser
                    spec={an}
                    updateSpec={(mutate) =>
                      setAnalyzers((prev) => prev.map((a) => (a.id === an.id ? mutate(a) : a)))
                    }
                    tables={tables}
                  />
                </Suspense>
              </div>
              <div style={{ padding: 10, paddingTop: 4 }}>
                <Button
                  size="xs"
                  variant="light"
                  color="red"
                  fullWidth
                  onClick={() => deleteAnalyzer(an.id)}
                >
                  Delete analyzer
                </Button>
              </div>
            </div>
          )
        }
      }

      // Equations: surface the equation tools.
      if (fw?.kind === 'equations') {
        return (
          <div style={bodyStyle}>
            <Stack gap="xs">
              <Text size="sm" fw={600} c="teal.4">Equations</Text>
              <Text size="xs" c="dimmed">Edit equations in the Editor; press Solve (F2) to compute. Results appear in the Variable Explorer on the right.</Text>
              <Text size="xs" c="dimmed">Variable Information, Min / Max and Curve Fit are on the left rail and in the Tools menu.</Text>
            </Stack>
          </div>
        )
      }

      return (
        <div style={bodyStyle}>
          <Text size="xs" c="dimmed">Focus a window (Table, Plot, Whiteboard, Editor) to inspect it here.</Text>
        </div>
      )
    })(),
    workspace: (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <Workspace
            variables={workspaceVariables}
            replNames={replNames}
            components={result?.components}
            diagnostics={result}
            onEdit={() => setShowVariableInfo(true)}
            onExportSpreadsheet={exportToSpreadsheet}
            pinnedNames={pinnedSliderNames}
            pinnableNames={pinnableNames}
            onPin={pinSlider}
            sliderStrip={
              pinnedSliders.length > 0 ? (
                <Suspense fallback={null}>
                  <SliderStrip
                    pins={pinnedSliders}
                    inertNames={pinnedSliders.filter((p) => !pinnableNames.has(p.name.toLowerCase())).map((p) => p.name)}
                    onChange={(name, v) => setSliderValue(name, v, false)}
                    onCommit={(name, v) => setSliderValue(name, v, true)}
                    onUnpin={unpinSlider}
                    solving={solving}
                  />
                </Suspense>
              ) : null
            }
          />
        </Suspense>
      </div>
    ),
    terminal: (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <ReplTerminal
            sessionId={sessionId}
            variables={workspaceVariables}
            replNames={replNames}
            functions={replFunctionNames}
            unitSystem={unitSystem}
            onAssign={(v) => {
              setReplVars((prev) => ({ ...prev, [v.name.toLowerCase()]: v }))
              if (!v.name.includes('[')) {
                setVarDrafts((prev) => {
                  const existing = prev[v.name]
                  if (existing?.isUnitsUserSet) return prev
                  return {
                    ...prev,
                    [v.name]: { ...(existing ?? DEFAULT_DRAFT), units: v.units || '' },
                  }
                })
              }
            }}
            onCheck={() => void onCheck()}
            onSolve={() => void checkThenSolve()}
            onClear={() => {
              setReplVars({})
              setResult(null)
              setCheckResult(null)
              setVariables([])
              void replClear(sessionId)
            }}
            onClearVar={(name) => {
              const lower = name.toLowerCase()
              const prefix = lower + '['
              setReplVars((prev) => {
                const next = { ...prev }
                delete next[lower]
                for (const k of Object.keys(next)) {
                  if (k.startsWith(prefix)) {
                    delete next[k]
                  }
                }
                return next
              })
              void replClear(sessionId, name)
            }}
          />
        </Suspense>
      </div>
    ),
  }
  const panelTitles: Record<string, string> = {
    equations: 'Editor',
    table: 'Tables',
    plots: 'Plots',
    digitizer: 'Digitizer',
    schematic: 'Schematic',
    workspace: 'Variable Explorer',
    terminal: 'Terminal',
    states: 'Fluid States',
    inspector: 'Inspector',
  }

  // Per-instance Whiteboard windows: each Excalidraw whiteboard opens as its
  // own dock window ("whiteboard:<id>"). The
  // scene is persisted through App-owned state → .frees file.
  for (const w of whiteboards) {
    const winId = `whiteboard:${w.id}`
    panelTitles[winId] = w.name
    panelContent[winId] = (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <WhiteboardTab
            key={`whiteboard-${w.id}-${workspaceEpoch}`}
            singleWhiteboardId={w.id}
            whiteboards={whiteboards}
            onWhiteboardsChange={setWhiteboards}
          />
        </Suspense>
      </div>
    )
  }

  // Per-instance Data Analyzer windows ("analyzer:<id>"), mirroring the
  // whiteboard pattern. Bulk samples live in the ChannelStore, never here.
  for (const a of analyzers) {
    const winId = `analyzer:${a.id}`
    panelTitles[winId] = a.name
    panelContent[winId] = (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <DataAnalyzerTab
            key={`analyzer-${a.id}-${workspaceEpoch}`}
            singleAnalyzerId={a.id}
            analyzers={analyzers}
            onAnalyzersChange={setAnalyzers}
            tables={tables}
          />
        </Suspense>
      </div>
    )
  }

  // Per-instance Spreadsheet windows:
  for (const ss of spreadsheets) {
    const winId = `spreadsheet:${ss.id}`
    panelTitles[winId] = ss.name
    panelContent[winId] = (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <SpreadsheetTab
            key={`spreadsheet-${ss.id}-${workspaceEpoch}`}
            singleSpreadsheetId={ss.id}
            spreadsheets={spreadsheets}
            onSpreadsheetsChange={setSpreadsheets}
            availableVariables={solvedVars}
            onInsertText={insertEquationLine}
            onCreateTable={(newTable) => {
              setTables((prev) => [...prev, newTable])
              requestAnimationFrame(() => dockRef.current?.openInstance(`table:${newTable.id}`, 'table', newTable.name))
            }}
          />
        </Suspense>
      </div>
    )
  }

  // Per-instance Plot windows: every plot (X-Y, property diagram, or
  // psychrometric chart) opens as its own dock window ("plot:<id>"). Plot data
  // is global solve output, so these are self-contained. A kind chip
  // distinguishes thermo (property/psychro) windows from X-Y plots.
  for (const pl of mergedPlots) {
    const winId = `plot:${pl.id}`
    const isThermo = pl.kind === 'property' || pl.kind === 'psychro'
    panelTitles[winId] = pl.name
    panelContent[winId] = (
      <div style={plotPanelStyle}>
        <Group justify="space-between" mb={4} wrap="nowrap" align="center" style={{ flexShrink: 0 }}>
          <Badge size="xs" variant="light" color={isThermo ? 'teal' : 'blue'}>
            {PLOT_KIND_LABEL[pl.kind]}
          </Badge>
        </Group>
        <Suspense fallback={lazyTabFallback}>
        <PlotTab
          kinds={[pl.kind]}
          singlePlotId={pl.id}
          emptyHint="This plot was removed."
          plots={mergedPlots}
          onPlotsChange={handlePlotsChange}
          solvedVariables={result?.variables ?? []}
          stateTableDefs={declaredStateDefs}
          cyclePath={result?.cyclePath}
          tableVars={tableVars}
          rows={paramRows}
          results={tableResults}
          tableUnits={activeParam?.columnUnits}
          activePlotId={pl.id}
          onActivePlotIdChange={setActivePlotId}
          spreadsheets={spreadsheets}
        />
        </Suspense>
      </div>
    )
  }

  // Per-instance STATE TABLE windows: each declared STATE TABLE block gets its
  // own dock window ("state:<name>") so Water/R134a circuits sit side by side.
  for (const s of declaredStateDefs) {
    const winId = `state:${s.name}`
    panelTitles[winId] = s.name
    panelContent[winId] = (
      <div style={panelPad}>
        <Group justify="space-between" mb="xs" wrap="nowrap" align="center">
          {s.fluid && (
            <Badge size="xs" variant="light" color="teal">{s.fluid}</Badge>
          )}
          <Text size="xs" c="dimmed">Solved state points</Text>
        </Group>
        <StatesTab
          solvedVariables={result?.variables ?? []}
          stateTableDefs={[s]}
          unitIds={stateUnitIds}
          onUnitIdsChange={handleStateUnitIdsChange}
          onFillMissing={() => {
            setFillMissingFor(s.name)
            onSolve(true)
          }}
          solving={solving && fillMissingFor === s.name}
          solvable={solvable}
          onSnapshot={createSnapshotSpreadsheet}
        />
      </div>
    )
  }

  // The single Univer Tables workbook window hosting every editable table
  // (function/lookup + GUI parametric) as a bound sheet (decision 2 of the
  // unification plan). Code PARAMETRIC / ODE tables keep per-table windows.
  {
    panelTitles[TABLES_WORKBOOK_WINDOW_ID] = 'Tables'
    panelContent[TABLES_WORKBOOK_WINDOW_ID] = (
      <div style={{ height: '100%', minHeight: 0 }}>
        <Suspense fallback={lazyTabFallback}>
          <TablesWorkbookTab
            key={`tables-workbook-${workspaceEpoch}`}
            tables={tables}
            activeTableId={activeTableId}
            onTablesChange={setTables}
            onActiveTableIdChange={setActiveTableId}
            onConfigureTable={(id) => {
              setActiveTableId(id)
              setShowConfigureTable(true)
            }}
            onAlterColumn={(id, name) => {
              setActiveTableId(id)
              setAlterColumn(name)
            }}
          />
        </Suspense>
      </div>
    )
  }

  // Per-instance Table windows: only read-only code PARAMETRIC / ODE tables
  // (each "table:<id>"); editable tables are sheets in the Tables workbook.
  for (const t of tables) {
    if (isHostedTable(t)) continue
    const winId = `table:${t.id}`
    panelTitles[winId] = t.name
    panelContent[winId] = (
      <div style={panelPad}>
        <Suspense fallback={lazyTabFallback}>
          <TablesTab
            tables={tables}
            singleTableId={t.id}
            varDrafts={varDrafts}
            onPlotColumns={handlePlotColumns}
            onExportTable={exportTableToSpreadsheet}
            onCopyToEditable={(copy) => {
              setTables((prev) => [...prev, copy])
              setActiveTableId(copy.id)
              requestAnimationFrame(() => openTableWindow(copy))
            }}
          />
        </Suspense>
      </div>
    )
  }

  return (
    <>
      {isMobile ? (
        <Suspense fallback={lazyTabFallback}>
          <MobileLayout
            panelContent={panelContent}
            tables={tables}
            plots={mergedPlots}
            projectName={projectName}
            checking={checking}
            solving={solving}
            onCheck={checkWithFallback}
            onSolve={checkThenSolve}
            checkingTableId={checkingTableId}
            solvingTableId={solvingTableId}
            onCheckTable={onCheckTable}
            onSolveTable={onSolveTable}
            onSaveProject={handleSaveProject}
            onSaveProjectAs={handleSaveProjectAs}
            onNewProject={handleNewProject}
            onOpenProject={handleOpenProject}
            onOpenLibrary={() => setLibraryOpen(true)}
            onPreferences={() => setShowPreferences(true)}
            onRenameProject={handleRenameProject}
            onOpenExamples={() => setShowExamples(true)}
          />
        </Suspense>
      ) : (
        <Flex h="100vh" style={{ overflow: 'hidden' }}>
      <Rail
        active={activeTab}
        openKinds={openKinds}
        openIds={openIds}
        plots={mergedPlots.map((p) => ({ id: p.id, name: p.name, tag: PLOT_KIND_LABEL[p.kind], deletable: !p.fromCode }))}
        plotCount={mergedPlots.length}
        onOpenPlot={(id) => {
          const p = mergedPlots.find((x) => x.id === id)
          if (p) dockRef.current?.openInstance(`plot:${id}`, 'plot', p.name)
        }}
        onNewPlot={(kind) => setNewPlotKind(kind)}
        onDeletePlot={(id) => handlePlotsChange(plots.filter((p) => p.id !== id))}
        workspaceTables={[
          ...tables.map((t) => ({ id: t.id, name: t.name, deletable: t.source !== 'code' })),
          // Declared STATE TABLE blocks appear as read-only entries that open
          // the shared Fluid States window, tagged with their fluid.
          ...declaredStateDefs.map((s) => ({
            id: `state:${s.name}`,
            name: s.name,
            tag: s.fluid ?? 'States',
            deletable: false,
          })),
        ]}
        tableCount={tables.length + declaredStateDefs.length}
        onOpenTable={(id) => {
          if (id.startsWith('state:')) {
            const name = id.slice('state:'.length)
            dockRef.current?.openInstance(id, 'states', name)
            return
          }
          const t = tables.find((x) => x.id === id)
          if (t) openTableWindow(t)
        }}
        onDeleteTable={(id) => setTables((prev) => prev.filter((t) => t.id !== id))}
        onOpenStates={() => {
          const first = declaredStateDefs[0]
          if (first) dockRef.current?.openInstance(`state:${first.name}`, 'states', first.name)
          else dockRef.current?.open('states')
        }}
        onNewTable={(kind) => createTable(kind)}
        onSelect={(kind) => dockRef.current?.open(kind)}
        onClose={(kind) => dockRef.current?.close(kind)}
        onApplyLayout={(p) => dockRef.current?.applyPerspective(p)}
        whiteboards={whiteboards.map((w) => ({ id: w.id, name: w.name, deletable: true }))}
        whiteboardCount={whiteboards.length}
        onOpenWhiteboard={(id) => {
          const wb = whiteboards.find((x) => x.id === id)
          if (wb) dockRef.current?.openInstance(`whiteboard:${id}`, 'whiteboard', wb.name)
        }}
        onNewWhiteboard={createWhiteboard}
        onDeleteWhiteboard={(id) => setWhiteboards((prev) => prev.filter((w) => w.id !== id))}
        spreadsheets={spreadsheets.map((ss) => ({ id: ss.id, name: ss.name, deletable: true }))}
        spreadsheetCount={spreadsheets.length}
        onOpenSpreadsheet={(id) => {
          const ss = spreadsheets.find((x) => x.id === id)
          if (ss) dockRef.current?.openInstance(`spreadsheet:${id}`, 'spreadsheet', ss.name)
        }}
        onDeleteSpreadsheet={(id) => setSpreadsheets((prev) => prev.filter((s) => s.id !== id))}
        onNewSpreadsheet={createSpreadsheet}
        analyzers={analyzers.map((a) => ({ id: a.id, name: a.name, deletable: true }))}
        analyzerCount={analyzers.length}
        onOpenAnalyzer={(id) => {
          const a = analyzers.find((x) => x.id === id)
          if (a) dockRef.current?.openInstance(`analyzer:${id}`, 'analyzer', a.name)
        }}
        onNewAnalyzer={createAnalyzer}
        onDeleteAnalyzer={deleteAnalyzer}
        onPreferences={() => setShowPreferences(true)}
        onAbout={() => setShowAbout(true)}
      />

      <Flex direction="column" flex={1} miw={0} p="sm" gap="sm">
        <TopBar
          isTable={focusedParam !== null}
          checking={checking}
          solving={solving}
          solvable={solvable}
          findAll={findAll}
          complexMode={complexMode}
          checkResult={checkResult}
          result={result}
          tableChecking={checkingTableId === focusedParam?.id}
          tableSolving={solvingTableId === focusedParam?.id}
          tableCheckResult={tableCheckResult}
          tableCheckMessage={tableCheckMessage}
          tableResults={focusedParam?.results ?? []}
          onCheck={checkWithFallback}
          onSolve={checkThenSolve}
          onCheckTable={() => { if (focusedParam) void onCheckTable(focusedParam.id) }}
          onSolveTable={() => { if (focusedParam) void onSolveTable(focusedParam.id) }}
          onFindAllChange={(checked) => {
            setFindAll(checked)
            setResult(null)
            setLastSolvedWithFillMissing(false)
          }}
          onComplexModeChange={(checked) => {
            setComplexMode(checked)
            setCheckResult(null)
            setResult(null)
            setLastSolvedWithFillMissing(false)
            invalidateTable()
          }}
          projectName={projectName}
          onRenameProject={handleRenameProject}
          onNewProject={handleNewProject}
          onOpenProject={handleOpenProject}
          onOpenLibrary={() => setLibraryOpen(true)}
          onSaveProject={handleSaveProject}
          onSaveProjectAs={handleSaveProjectAs}
          onInsertFunction={insertFunction}
          onInsertComponent={() => setShowComponentWizard(true)}
          onOpenExamples={() => setShowExamples(true)}
          onShareLink={handleShareLink}
          onPrintReport={handlePrintReport}
          canPrintReport={result?.success === true}
          onOpenInspector={() => dockRef.current?.open('inspector')}
          onOpenWorkspace={() => dockRef.current?.open('workspace')}
          onOpenTerminal={() => dockRef.current?.open('terminal')}
          onVariableInfo={() => setShowVariableInfo(true)}
        />
        <input
          ref={projectFileRef}
          type="file"
          accept=".frees,application/json"
          style={{ display: 'none' }}
          onChange={onProjectFileSelected}
        />

        <div style={{ flex: 1, minHeight: 0, display: 'flex' }}>
          <Suspense fallback={lazyTabFallback}>
            <WorkspaceDock
              content={panelContent}
              titles={panelTitles}
              defaultOpen={['equations', 'inspector', 'workspace']}
              edgeKinds={['workspace', 'inspector']}
              onActiveChange={(active) => {
                setActiveTab(active?.kind ?? '')
                // Focusing a table window makes it the "active" table so the
                // shared Solve-Table / Configure / Alter actions target it.
                // The Tables workbook window is excluded: its nav list drives
                // activeTableId itself (one window, many hosted tables).
                if (
                  active?.kind === 'table' &&
                  active.id.startsWith('table:') &&
                  active.id !== TABLES_WORKBOOK_WINDOW_ID
                ) {
                  setActiveTableId(active.id.slice('table:'.length))
                }
                // The Inspector reflects the last-focused main window; focusing
                // the auxiliary Inspector / Variable Explorer edge panels must not change it.
                if (active && active.kind !== 'inspector' && active.kind !== 'workspace') {
                  setFocusedWindow(active)
                }
              }}
              onOpenChange={setOpenWindows}
              handleRef={dockRef}
            />
          </Suspense>
        </div>
      </Flex>

        </Flex>
      )}
      {showAbout && <AboutModal onClose={() => setShowAbout(false)} />}

      {showExamples && (
        <Suspense fallback={null}>
          <ExamplesModal
            opened={showExamples}
            onClose={() => setShowExamples(false)}
            onSelect={loadExample}
          />
        </Suspense>
      )}

      <Suspense fallback={null}>
        {showComponentWizard && (
          <ComponentWizardModal
            opened={showComponentWizard}
            onClose={() => setShowComponentWizard(false)}
            onInsert={insertComponentBlock}
          />
        )}
      </Suspense>

      <ShortcutsModal opened={showShortcuts} onClose={() => setShowShortcuts(false)} />

      <GettingStartedModal
        opened={showGettingStarted}
        onClose={closeGettingStarted}
        onSolveExample={() => {
          void checkThenSolve()
        }}
        onOpenExamples={() => setShowExamples(true)}
      />

      <Spotlight
        actions={spotlightActions}
        nothingFound="Nothing found…"
        highlightQuery
        searchProps={{
          leftSection: <IconSearch size={18} />,
          placeholder: 'Search views, tools, and actions…',
        }}
      />

      <TextPromptModal
        opened={renameOpen}
        title="Rename Project"
        label="Project name"
        defaultValue={projectName}
        confirmLabel="Rename"
        onSubmit={submitRename}
        onClose={() => setRenameOpen(false)}
      />

      <TextPromptModal
        opened={saveAsOpen}
        title="Save Project As"
        label="Project name"
        defaultValue={projectName}
        confirmLabel="Save"
        onSubmit={submitSaveAs}
        onClose={() => setSaveAsOpen(false)}
      />

      <SaveCheckModal
        opened={showSaveCheck}
        projectName={projectName}
        onSave={onSaveCheckSave}
        onDiscard={onSaveCheckDiscard}
        onCancel={onSaveCheckCancel}
      />

      <ProjectLibraryModal
        opened={libraryOpen}
        currentName={projectName}
        onClose={() => setLibraryOpen(false)}
        onSaveCurrent={handleSaveToBrowser}
        onOpenProject={handleOpenFromBrowser}
      />

      <MessageModal
        opened={dialogError !== null}
        title="Could not open project"
        message={dialogError ?? ''}
        onClose={() => setDialogError(null)}
      />

      {/* Self-dismissing project-load summary (template mode, §2.5b). */}
      {loadNotice !== null && (
        <Alert
          icon={<IconWaveSine size={16} />}
          color="teal"
          withCloseButton
          onClose={() => setLoadNotice(null)}
          style={{ position: 'fixed', bottom: 16, right: 16, zIndex: 400, maxWidth: 420 }}
        >
          {loadNotice}
        </Alert>
      )}

      {showPreferences && (
        <PreferencesModal
          criteria={stopCriteria}
          unitSystem={unitSystem}
          fillMissing={fillMissing}
          onSave={savePreferences}
          onClose={() => setShowPreferences(false)}
        />
      )}

      {alterColumn && (
        <AlterValuesModal
          variable={alterColumn}
          rowCount={paramRows.length}
          initialFirst={paramRows[0]?.values[alterColumn] ?? ''}
          initialLast={paramRows[paramRows.length - 1]?.values[alterColumn] ?? ''}
          onApply={(values) => {
            updateActiveParam((t) =>
              invalidateActiveParam({
                ...t,
                rows: t.rows.map((row, i) => ({
                  ...row,
                  values: { ...row.values, [alterColumn]: String(values[i]) },
                })),
              }),
            )
            setAlterColumn(null)
          }}
          onClose={() => setAlterColumn(null)}
        />
      )}

      {showConfigureTable && (
        <ConfigureTableModal
          variables={variables}
          selected={tableVars}
          onSave={(selected) => {
            updateActiveParam((t) => invalidateActiveParam({ ...t, vars: selected }))
            setShowConfigureTable(false)
          }}
          onClose={() => setShowConfigureTable(false)}
        />
      )}

      {showVariableInfo && (
        <VariableInfoModal
          variables={(() => {
            const replScalarNames = Object.values(replVars)
              .map((v) => v.name)
              .filter((n) => !n.includes('['))
              .filter((n) => !variables.some((vn) => vn.toLowerCase() === n.toLowerCase()))
            return [...variables, ...replScalarNames]
          })()}
          drafts={varDrafts}
          solvedValues={(() => {
            const solvedValues: Record<string, number> = {}
            if (result && result.variables) {
              for (const v of result.variables) {
                solvedValues[v.name.toLowerCase()] = v.value
              }
            }
            for (const v of Object.values(replVars)) {
              if (!v.name.includes('[')) solvedValues[v.name.toLowerCase()] = v.value
            }
            return solvedValues
          })()}
          onSave={(drafts) => {
            setVarDrafts(drafts)
            setShowVariableInfo(false)
          }}
          onClose={() => setShowVariableInfo(false)}
          documentText={textRef.current}
          onWriteToDocument={(next) => {
            applyText(next)
            setShowVariableInfo(false)
          }}
        />
      )}

      {newPlotKind && (
        <Suspense fallback={null}>
          <PlotConfigModal
            spec={null}
            allowedKinds={[newPlotKind]}
            defaultName={`${PLOT_KIND_LABEL[newPlotKind]} ${mergedPlots.filter((p) => p.kind === newPlotKind).length + 1}`}
            fluids={fluids}
            tableVars={tableVars}
            initialXy={newPlotKind === 'xy' ? (plotSeed ?? undefined) : undefined}
            hasStates={detectStates(result?.variables ?? []).indices.length > 0}
            onSave={(spec) => {
              handlePlotsChange([...plots, spec])
              setActivePlotId(spec.id)
              setNewPlotKind(null)
              setPlotSeed(null)
              requestAnimationFrame(() => dockRef.current?.openInstance(`plot:${spec.id}`, 'plot', spec.name))
            }}
            onClose={() => { setNewPlotKind(null); setPlotSeed(null) }}
          />
        </Suspense>
      )}
    </>
  )
}
