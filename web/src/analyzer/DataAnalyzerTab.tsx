// Data Analyzer window (todo.md Phases 1–5): CSV import → signal browser
// (SignalBrowser.tsx, also hosted by the Inspector) → multi-strip oscilloscope
// (uPlot) with synced hover cursor, A/B measurement cursors with per-signal
// value readout (§2.5e), Table / Statistics / Event List / Scatter / Histogram
// instruments, calculated signals (Phase 4), per-file time offsets (Phase 5a),
// CSV export, and template-mode file relocation (§2.5b). Mirrors the
// same pattern as spreadsheets: App owns the AnalyzerSpec[] slice; bulk samples live in
// the module-level ChannelStore.

import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
} from 'react'
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Group,
  Menu,
  ScrollArea,
  Stack,
  Switch,
  Tabs,
  Text,
  Tooltip,
  useComputedColorScheme,
} from '@mantine/core'
import {
  IconAlertTriangle,
  IconArrowsDiff,
  IconChartDots,
  IconChartHistogram,
  IconChevronLeft,
  IconChevronRight,
  IconDotsVertical,
  IconDownload,
  IconGripVertical,
  IconHandMove,
  IconListSearch,
  IconMathFunction,
  IconPlus,
  IconSum,
  IconTable,
  IconTrash,
  IconWaveSine,
  IconWaveSquare,
  IconX,
  IconZoomIn,
  IconZoomInArea,
  IconZoomOut,
  IconZoomReset,
} from '@tabler/icons-react'
import uPlot from 'uplot'
import UPlotChart, { type AbCursors, type MouseMode } from './UPlotChart'
import { channelStore } from './channelStore'
import { type CalcResultDto } from './measurementApi'
import { calcResultToMeasurement } from './calc'
import CalcSignalModal from './CalcSignalModal'
import SignalBrowser from './SignalBrowser'
import { lowerBound } from './decimate'
import {
  offsetExactValueAt,
  offsetNearestTime,
  offsetRawRange,
  offsetWindow,
  offsetsOf,
} from './offsets'
import { moveSignal as moveSignalOp, reorderStrip as reorderStripOp } from './stripOps'
import { formatValue } from '../format'
import { buildCsv, downloadCsv, type ExportSignal } from './exportCsv'
import TableInstrument from './instruments/TableInstrument'
import StatisticsInstrument from './instruments/StatisticsInstrument'
import EventListInstrument from './instruments/EventListInstrument'
import ScatterInstrument from './instruments/ScatterInstrument'
import HistogramInstrument from './instruments/HistogramInstrument'
import CompareInstrument from './instruments/CompareInstrument'
import { signalColor } from './palette'
import {
  newStrip,
  type AnalyzerSignal,
  type AnalyzerSpec,
  type AnalyzerStrip,
} from './types'
import type { TableSpec } from '../tables'

/** Decimation budget per strip (≈2 samples per px at typical tile widths). */
const MAX_POINTS = 2400
/** Reference px height a legacy strip.height is normalized against. */
const STRIP_HEIGHT = 200
/** Flex-weight clamp for the per-strip share of the oscilloscope area. */
const WEIGHT_MIN = 0.25
const WEIGHT_MAX = 6
/** A strip never compresses below this (the container scrolls instead). */
const STRIP_MIN_PX = 120
/** Signal-list column width: default and drag clamp. */
const LIST_WIDTH_DEFAULT = 392
const LIST_WIDTH_MIN = 180
const LIST_WIDTH_MAX = 760
/** DnD payload mime for signal-row and strip-reorder drags. */
const DND_MIME = 'application/x-frees-analyzer'

/** Effective flex weight of a strip (migrates the legacy px height). */
function stripWeight(strip: AnalyzerStrip): number {
  const w = strip.weight ?? (strip.height !== undefined ? strip.height / STRIP_HEIGHT : 1)
  return Math.max(WEIGHT_MIN, Math.min(WEIGHT_MAX, w))
}

// ---------------------------------------------------------------------------
// View state (per-window, non-persisted): shared x range, A/B cursors (§2.5e),
// snap mode, selected strip, and the active instrument tab.
// ---------------------------------------------------------------------------

type Instrument = 'scope' | 'table' | 'stats' | 'events' | 'scatter' | 'histogram' | 'compare'

interface ViewState {
  /** null = the full recording. Shared by every strip (linked time axes). */
  xRange: [number, number] | null
  cursorA: number | null
  cursorB: number | null
  /** Sample-snap (true) vs continuous (false) cursor placement. */
  snap: boolean
  /** What a plain mouse drag does on a strip: box zoom or pan. */
  mouseMode: MouseMode
  /** Signal highlighted in the list + emphasized in the plot (oscilloscope-tool selection). */
  selectedSignal: SignalKey | null
  instrument: Instrument
}

type ViewAction =
  | { type: 'zoom'; min: number; max: number }
  | { type: 'reset-zoom' }
  | { type: 'set-cursor'; which: 'a' | 'b'; t: number | null }
  | { type: 'clear-cursors' }
  | { type: 'toggle-snap' }
  | { type: 'set-mouse-mode'; mode: MouseMode }
  | { type: 'select-signal'; sig: SignalKey | null }
  | { type: 'set-instrument'; instrument: Instrument }

function viewReducer(state: ViewState, action: ViewAction): ViewState {
  switch (action.type) {
    case 'zoom': {
      if (!(action.max > action.min)) return state
      const [curMin, curMax] = state.xRange ?? [Number.NaN, Number.NaN]
      if (action.min === curMin && action.max === curMax) return state
      return { ...state, xRange: [action.min, action.max] }
    }
    case 'reset-zoom':
      return state.xRange === null ? state : { ...state, xRange: null }
    case 'set-cursor':
      return action.which === 'a' ? { ...state, cursorA: action.t } : { ...state, cursorB: action.t }
    case 'clear-cursors':
      return { ...state, cursorA: null, cursorB: null }
    case 'toggle-snap':
      return { ...state, snap: !state.snap }
    case 'set-mouse-mode':
      return { ...state, mouseMode: action.mode }
    case 'select-signal':
      return { ...state, selectedSignal: action.sig }
    case 'set-instrument':
      return { ...state, instrument: action.instrument }
  }
}

// ---------------------------------------------------------------------------
// Per-strip chart assembly
// ---------------------------------------------------------------------------

interface LoadedSignal {
  measurementId: string
  channel: string
  color: string
  kind: 'analog' | 'boolean'
}

/** Identity of a selected/highlighted signal. */
interface SignalKey {
  measurementId: string
  channel: string
}

const sameSignal = (a: SignalKey | null, m: string, c: string): boolean =>
  a !== null && a.measurementId === m && a.channel === c

/** Fade a #rrggbb signal color for non-selected curves (blend toward mid-gray). */
function dimColor(hex: string): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex)
  if (m === null) return hex
  const mix = (v: number) => Math.round(v * 0.4 + 128 * 0.6)
  const r = mix(parseInt(m[1], 16))
  const g = mix(parseInt(m[2], 16))
  const b = mix(parseInt(m[3], 16))
  return `rgb(${r}, ${g}, ${b})`
}

/**
 * Every signal always contributes TWO series (envelope min + max). When the
 * window is raw (not decimated) both series reference the SAME value array —
 * no copy, and the series composition stays stable across zoom levels so the
 * chart instance (and its cursor) survives data updates.
 */
function buildStripData(
  strip: AnalyzerStrip,
  xRange: [number, number] | null,
  offsets: Map<string, number>,
): { data: uPlot.AlignedData; loaded: LoadedSignal[]; missing: string[]; decimated: boolean } {
  const tables: uPlot.AlignedData[] = []
  const loaded: LoadedSignal[] = []
  const missing: string[] = []
  let decimated = false
  for (const sig of strip.signals) {
    const win = offsetWindow(
      sig,
      offsets.get(sig.measurementId) ?? 0,
      xRange?.[0] ?? null,
      xRange?.[1] ?? null,
      MAX_POINTS,
    )
    if (win === null) {
      // null = evicted/unknown → the "Locate file…" banner.
      if (!channelStore.isLoaded(sig.measurementId)) missing.push(sig.channel)
      continue
    }
    if (win.decimated) {
      decimated = true
      tables.push([win.t, win.min, win.max] as unknown as uPlot.AlignedData)
    } else {
      tables.push([win.t, win.v, win.v] as unknown as uPlot.AlignedData)
    }
    loaded.push({
      measurementId: sig.measurementId,
      channel: sig.channel,
      color: sig.color,
      // "Treat As Boolean/Analog Signal": the per-signal override wins
      // over the imported channel kind (rendering only — stepped + 0/1 band).
      kind: sig.kindOverride ?? (win.kind === 'boolean' ? 'boolean' : 'analog'),
    })
  }
  const data: uPlot.AlignedData =
    tables.length === 0
      ? ([[], []] as unknown as uPlot.AlignedData)
      : tables.length === 1
        ? tables[0]
        : uPlot.join(tables)
  return { data, loaded, missing, decimated }
}

function stripOptions(
  loaded: LoadedSignal[],
  syncKey: string,
  dark: boolean,
  selected: SignalKey | null,
): Omit<uPlot.Options, 'width' | 'height'> {
  const axisColor = dark ? '#909296' : '#495057'
  const gridColor = dark ? 'rgba(134,142,150,0.15)' : 'rgba(134,142,150,0.25)'
  // When a signal is selected, its curve is bold and the others dim
  // (selection emphasis). With nothing selected, every curve is at its normal weight.
  const anySelected = loaded.some((s) => sameSignal(selected, s.measurementId, s.channel))
  const series: uPlot.Series[] = [{}]
  for (const sig of loaded) {
    const stepped =
      sig.kind === 'boolean' ? uPlot.paths?.stepped?.({ align: 1 }) : undefined
    const isSel = sameSignal(selected, sig.measurementId, sig.channel)
    const width = isSel ? 2.5 : 1
    const stroke = anySelected && !isSel ? dimColor(sig.color) : sig.color
    // Envelope pair: min line + max line in the signal color (identical when raw).
    series.push({ label: `${sig.channel} (env)`, stroke, width, spanGaps: true, paths: stepped })
    series.push({ label: sig.channel, stroke, width, spanGaps: true, paths: stepped })
  }
  const allBoolean = loaded.length > 0 && loaded.every((s) => s.kind === 'boolean')
  return {
    series,
    legend: { show: false },
    cursor: {
      sync: { key: syncKey },
      drag: { x: true, y: false },
    },
    scales: {
      x: { time: false },
      // Fixed headroom band for pure boolean strips so pulses read as pulses.
      y: allBoolean ? { range: [-0.15, 1.15] } : { auto: true },
    },
    axes: [
      { stroke: axisColor, grid: { stroke: gridColor, width: 1 }, ticks: { stroke: gridColor } },
      { stroke: axisColor, grid: { stroke: gridColor, width: 1 }, ticks: { stroke: gridColor }, size: 56 },
    ],
  }
}

// ---------------------------------------------------------------------------
// Per-strip signal list (oscilloscope-tool parity): Style | Name | Unit | Value | A | B | Δ.
// Value follows the hover cursor live; A/B/Δ read the measurement cursors.
// ---------------------------------------------------------------------------

const SIGNAL_LIST_GRID = '10px minmax(60px, 1fr) 30px 58px 58px 58px 58px 18px'

function fmtHit(hit: { v: number } | null): string {
  if (hit === null) return '—'
  return formatValue(hit.v)
}

const NUM_CELL: CSSProperties = {
  textAlign: 'right',
  fontFamily: 'var(--mantine-font-family-monospace)',
  fontSize: 10.5,
  whiteSpace: 'nowrap',
  overflow: 'hidden',
}

interface SignalRowProps {
  sig: AnalyzerSignal
  stripId: string
  offset: number
  hoverT: number | null
  cursors: AbCursors
  selected: boolean
  onSelect: () => void
  onRemoveSignal: (channel: string, measurementId: string) => void
  onSetKind: (channel: string, measurementId: string, kind: 'analog' | 'boolean' | undefined) => void
  onMoveToNewStrip: (channel: string, measurementId: string) => void
}

/** One signal-list row: draggable, selectable (click / right-click highlights
 *  it and its curve), with a measurement-tool-style context menu reachable from BOTH the ⋮
 *  icon and a right-click anywhere on the row. */
function SignalRow({
  sig,
  stripId,
  offset,
  hoverT,
  cursors,
  selected,
  onSelect,
  onRemoveSignal,
  onSetKind,
  onMoveToNewStrip,
}: Readonly<SignalRowProps>) {
  const [menuOpened, setMenuOpened] = useState(false)
  const chMeta = channelStore
    .getMeta(sig.measurementId)
    ?.channels.find((c) => c.name === sig.channel)
  const unit = chMeta?.unit
  const storeKind = chMeta?.kind === 'boolean' ? 'boolean' : 'analog'
  const effKind = sig.kindOverride ?? storeKind
  const otherKind = effKind === 'boolean' ? 'analog' : 'boolean'
  const hover = hoverT === null ? null : offsetExactValueAt(sig, offset, hoverT)
  const a = cursors.a === null ? null : offsetExactValueAt(sig, offset, cursors.a)
  const b = cursors.b === null ? null : offsetExactValueAt(sig, offset, cursors.b)
  const delta = a !== null && b !== null ? { v: b.v - a.v } : null
  return (
    <Box
      px={6}
      py={1}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(
          DND_MIME,
          JSON.stringify({
            type: 'signal',
            stripId,
            measurementId: sig.measurementId,
            channel: sig.channel,
          }),
        )
        e.dataTransfer.effectAllowed = 'move'
      }}
      onClick={(e) => {
        e.stopPropagation()
        onSelect()
      }}
      onContextMenu={(e) => {
        e.preventDefault()
        e.stopPropagation()
        onSelect()
        setMenuOpened(true)
      }}
      style={{
        display: 'grid',
        gridTemplateColumns: SIGNAL_LIST_GRID,
        gap: 4,
        alignItems: 'center',
        cursor: 'grab',
        borderRadius: 3,
        background: selected ? 'var(--mantine-color-teal-light)' : undefined,
        boxShadow: selected ? 'inset 2px 0 0 var(--mantine-color-teal-6)' : undefined,
      }}
    >
      <Box w={10} h={10} style={{ background: sig.color, borderRadius: 2 }} />
      <Text size="xs" truncate title={`${sig.channel} — drag onto another strip to move it`}>
        {sig.channel}
      </Text>
      <Text size="xs" c="dimmed" truncate>
        {unit ?? ''}
      </Text>
      <span style={NUM_CELL}>{fmtHit(hover)}</span>
      <span style={NUM_CELL}>{fmtHit(a)}</span>
      <span style={NUM_CELL}>{fmtHit(b)}</span>
      <span style={NUM_CELL}>{fmtHit(delta)}</span>
      <Menu
        withinPortal
        position="bottom-end"
        shadow="md"
        width={200}
        opened={menuOpened}
        onChange={setMenuOpened}
      >
        <Menu.Target>
          <ActionIcon
            size={14}
            variant="subtle"
            color="gray"
            aria-label={`Options for ${sig.channel}`}
            onClick={(e) => {
              e.stopPropagation()
              setMenuOpened((o) => !o)
            }}
          >
            <IconDotsVertical size={10} />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item
            leftSection={
              effKind === 'boolean' ? <IconWaveSine size={13} /> : <IconWaveSquare size={13} />
            }
            onClick={() =>
              // Back to auto when the requested kind IS the imported one.
              onSetKind(
                sig.channel,
                sig.measurementId,
                otherKind === storeKind ? undefined : otherKind,
              )
            }
          >
            Treat as {otherKind} signal
          </Menu.Item>
          <Menu.Item
            leftSection={<IconPlus size={13} />}
            onClick={() => onMoveToNewStrip(sig.channel, sig.measurementId)}
          >
            Move to new strip
          </Menu.Item>
          <Menu.Divider />
          <Menu.Item
            color="red"
            leftSection={<IconX size={13} />}
            onClick={() => onRemoveSignal(sig.channel, sig.measurementId)}
          >
            Remove from strip
          </Menu.Item>
        </Menu.Dropdown>
      </Menu>
    </Box>
  )
}

interface SignalListProps {
  strip: AnalyzerStrip
  width: number
  offsets: Map<string, number>
  hoverT: number | null
  cursors: AbCursors
  selectedSignal: SignalKey | null
  onSelectSignal: (sig: SignalKey) => void
  onRemoveSignal: (channel: string, measurementId: string) => void
  /** "Treat As Boolean/Analog Signal"; undefined = back to auto. */
  onSetKind: (channel: string, measurementId: string, kind: 'analog' | 'boolean' | undefined) => void
  onMoveToNewStrip: (channel: string, measurementId: string) => void
}

function SignalList({
  strip,
  width,
  offsets,
  hoverT,
  cursors,
  selectedSignal,
  onSelectSignal,
  onRemoveSignal,
  onSetKind,
  onMoveToNewStrip,
}: Readonly<SignalListProps>) {
  return (
    <Box
      w={width}
      style={{
        flexShrink: 0,
        borderLeft: '1px solid var(--mantine-color-default-border)',
        overflowY: 'auto',
        overflowX: 'hidden',
      }}
    >
      <Box
        px={6}
        py={2}
        style={{
          display: 'grid',
          gridTemplateColumns: SIGNAL_LIST_GRID,
          gap: 4,
          position: 'sticky',
          top: 0,
          background: 'var(--mantine-color-body)',
          borderBottom: '1px solid var(--mantine-color-default-border)',
        }}
      >
        <span />
        <Text size="xs" c="dimmed">
          Signal
        </Text>
        <Text size="xs" c="dimmed">
          Unit
        </Text>
        <Text size="xs" c="dimmed" ta="right">
          Value
        </Text>
        <Text size="xs" c="yellow" ta="right">
          A
        </Text>
        <Text size="xs" c="cyan" ta="right">
          B
        </Text>
        <Text size="xs" c="dimmed" ta="right">
          Δ (B−A)
        </Text>
        <span />
      </Box>
      {strip.signals.map((sig) => (
        <SignalRow
          key={`${sig.measurementId}:${sig.channel}`}
          sig={sig}
          stripId={strip.id}
          offset={offsets.get(sig.measurementId) ?? 0}
          hoverT={hoverT}
          cursors={cursors}
          selected={sameSignal(selectedSignal, sig.measurementId, sig.channel)}
          onSelect={() => onSelectSignal({ measurementId: sig.measurementId, channel: sig.channel })}
          onRemoveSignal={onRemoveSignal}
          onSetKind={onSetKind}
          onMoveToNewStrip={onMoveToNewStrip}
        />
      ))}
      {strip.signals.length === 0 && (
        <Text size="xs" c="dimmed" p={6}>
          No signals — add from the browser or drop a row here.
        </Text>
      )}
    </Box>
  )
}

interface StripViewProps {
  strip: AnalyzerStrip
  syncKey: string
  xRange: [number, number] | null
  cursors: AbCursors
  hoverT: number | null
  mouseMode: MouseMode
  offsets: Map<string, number>
  storeVersion: number
  selected: boolean
  dark: boolean
  onSelect: () => void
  onZoom: (min: number, max: number) => void
  onResetZoom: () => void
  onCursorSet: (t: number, which: 'a' | 'b') => void
  onOffsetDrag: (deltaSeconds: number) => void
  onHover: (t: number | null) => void
  selectedSignal: SignalKey | null
  onSelectSignal: (sig: SignalKey) => void
  /** Width (px) of the signal-list column, shared across strips. */
  signalListWidth: number
  /** Signal-list width during (done=false) / after (done=true) a divider drag. */
  onResizeSignalList: (width: number, done: boolean) => void
  onRemoveSignal: (channel: string, measurementId: string) => void
  onRemoveStrip: () => void
  /** Commit a new flex weight (share of the scope area) after a resize drag. */
  onResizeWeight: (weight: number) => void
  onDropSignal: (fromStripId: string, measurementId: string, channel: string) => void
  onDropStrip: (dragStripId: string) => void
  onSetKind: (channel: string, measurementId: string, kind: 'analog' | 'boolean' | undefined) => void
  onMoveToNewStrip: (channel: string, measurementId: string) => void
}

function StripView({
  strip,
  syncKey,
  xRange,
  cursors,
  hoverT,
  mouseMode,
  offsets,
  storeVersion,
  selected,
  dark,
  onSelect,
  onZoom,
  onResetZoom,
  onCursorSet,
  onOffsetDrag,
  onHover,
  selectedSignal,
  onSelectSignal,
  signalListWidth,
  onResizeSignalList,
  onRemoveSignal,
  onRemoveStrip,
  onResizeWeight,
  onDropSignal,
  onDropStrip,
  onSetKind,
  onMoveToNewStrip,
}: Readonly<StripViewProps>) {
  const built = useMemo(
    () => buildStripData(strip, xRange, offsets),
    // storeVersion invalidates when measurements register/evict.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [strip, xRange, offsets, storeVersion],
  )
  // Options identity only changes when the series composition or the highlight
  // does, so the chart instance — and the synced cursor — survives zoom/data
  // updates. selKey rebuilds only the strip whose selection state changed.
  const seriesKey = built.loaded.map((s) => `${s.channel} ${s.color} ${s.kind}`).join('|')
  const selInStrip = built.loaded.find((s) =>
    sameSignal(selectedSignal, s.measurementId, s.channel),
  )
  const selKey = selInStrip ? `${selInStrip.measurementId}:${selInStrip.channel}` : ''
  const options = useMemo(
    () => stripOptions(built.loaded, syncKey, dark, selectedSignal),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [seriesKey, syncKey, dark, selKey],
  )
  // Live weight override while the bottom handle is being dragged; the final
  // value is committed to the spec (persisted) on release. Flex handles the
  // rebalancing: growing one strip shrinks the others proportionally.
  const [liveWeight, setLiveWeight] = useState<number | null>(null)
  const weight = liveWeight ?? stripWeight(strip)

  return (
    <Box
      onClick={onSelect}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes(DND_MIME)) {
          e.preventDefault()
          e.dataTransfer.dropEffect = 'move'
        }
      }}
      onDrop={(e) => {
        const raw = e.dataTransfer.getData(DND_MIME)
        if (raw === '') return
        e.preventDefault()
        try {
          const payload = JSON.parse(raw) as
            | { type: 'signal'; stripId: string; measurementId: string; channel: string }
            | { type: 'strip'; stripId: string }
          if (payload.type === 'signal') {
            onDropSignal(payload.stripId, payload.measurementId, payload.channel)
          } else {
            onDropStrip(payload.stripId)
          }
        } catch {
          /* foreign drop — ignore */
        }
      }}
      style={{
        border: `1px solid var(--mantine-color-${selected ? 'teal-6' : 'default-border'})`,
        borderRadius: 6,
        overflow: 'hidden',
        // Fill-area layout: strips share the panel by flex weight.
        flex: `${weight} 1 0%`,
        minHeight: STRIP_MIN_PX,
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      <Group justify="space-between" px={6} py={2} gap={4} wrap="nowrap">
        <Group gap={4} style={{ overflow: 'hidden' }} wrap="nowrap">
          <Tooltip label="Drag to reorder strips">
            <span
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData(DND_MIME, JSON.stringify({ type: 'strip', stripId: strip.id }))
                e.dataTransfer.effectAllowed = 'move'
              }}
              style={{ cursor: 'grab', display: 'inline-flex', alignItems: 'center' }}
            >
              <IconGripVertical size={14} color="var(--mantine-color-dimmed)" />
            </span>
          </Tooltip>
          <Text size="xs" c="dimmed">
            {strip.signals.length === 0
              ? 'Empty strip — add signals from the browser or drop one here'
              : `${strip.signals.length} signal${strip.signals.length === 1 ? '' : 's'}`}
          </Text>
          {built.decimated && (
            <Badge size="xs" variant="light" color="gray">
              envelope
            </Badge>
          )}
        </Group>
        <Tooltip label="Remove strip">
          <ActionIcon
            size="xs"
            variant="subtle"
            color="gray"
            onClick={(e) => {
              e.stopPropagation()
              onRemoveStrip()
            }}
          >
            <IconTrash size={12} />
          </ActionIcon>
        </Tooltip>
      </Group>
      {built.missing.length > 0 && (
        <Alert color="orange" p={4} m={4} icon={<IconAlertTriangle size={14} />}>
          <Text size="xs">
            Measurement data for {built.missing.join(', ')} is not in memory — use “Locate file…”
            in the signal browser.
          </Text>
        </Alert>
      )}
      <Group gap={0} align="stretch" wrap="nowrap" style={{ flex: 1, minHeight: 0 }}>
        <Box style={{ flex: 1, minWidth: 0 }}>
          {built.loaded.length > 0 ? (
            <UPlotChart
              data={built.data}
              options={options}
              xRange={xRange}
              cursors={cursors}
              mouseMode={mouseMode}
              onUserZoom={onZoom}
              onResetZoom={onResetZoom}
              onCursorSet={onCursorSet}
              onOffsetDrag={onOffsetDrag}
              onHover={onHover}
            />
          ) : (
            <Group justify="center" h="100%">
              <IconWaveSine size={28} color="var(--mantine-color-dimmed)" stroke={1.2} />
            </Group>
          )}
        </Box>
        {/* Draggable divider: resize the plot vs signal-list split (shared by
            all strips). Dragging left widens the list, right narrows it. */}
        <Box
          aria-label="Resize signal list"
          h="100%"
          w={6}
          style={{ cursor: 'ew-resize', flexShrink: 0, touchAction: 'none' }}
          onClick={(e) => e.stopPropagation()}
          onPointerDown={(e) => {
            e.preventDefault()
            e.stopPropagation()
            const startX = e.clientX
            const startW = signalListWidth
            const target = e.currentTarget
            target.setPointerCapture(e.pointerId)
            const clamp = (w: number) =>
              Math.max(LIST_WIDTH_MIN, Math.min(LIST_WIDTH_MAX, Math.round(w)))
            const onMove = (ev: PointerEvent) =>
              onResizeSignalList(clamp(startW - (ev.clientX - startX)), false)
            const onUp = (ev: PointerEvent) => {
              target.removeEventListener('pointermove', onMove)
              target.removeEventListener('pointerup', onUp)
              onResizeSignalList(clamp(startW - (ev.clientX - startX)), true)
            }
            target.addEventListener('pointermove', onMove)
            target.addEventListener('pointerup', onUp)
          }}
        >
          <Box w={1} h="100%" mx="auto" style={{ background: 'var(--mantine-color-default-border)' }} />
        </Box>
        <SignalList
          strip={strip}
          width={signalListWidth}
          offsets={offsets}
          hoverT={hoverT}
          cursors={cursors}
          selectedSignal={selectedSignal}
          onSelectSignal={onSelectSignal}
          onRemoveSignal={onRemoveSignal}
          onSetKind={onSetKind}
          onMoveToNewStrip={onMoveToNewStrip}
        />
      </Group>
      {/* Bottom resize handle: pixel drag rescales this strip's flex weight
          (share of the panel); flex rebalances the sibling strips live. */}
      <Box
        h={7}
        aria-label="Resize strip"
        style={{ cursor: 'ns-resize', touchAction: 'none', flexShrink: 0 }}
        onPointerDown={(e) => {
          e.preventDefault()
          e.stopPropagation()
          const root = e.currentTarget.parentElement
          if (root === null) return
          const startY = e.clientY
          const startPx = root.getBoundingClientRect().height
          const startWeight = weight
          if (startPx <= 0) return
          e.currentTarget.setPointerCapture(e.pointerId)
          const clamp = (w: number) => Math.max(WEIGHT_MIN, Math.min(WEIGHT_MAX, w))
          const toWeight = (clientY: number) =>
            clamp((startWeight * Math.max(STRIP_MIN_PX, startPx + clientY - startY)) / startPx)
          const target = e.currentTarget
          const onMove = (ev: PointerEvent) => setLiveWeight(toWeight(ev.clientY))
          const onUp = (ev: PointerEvent) => {
            target.removeEventListener('pointermove', onMove)
            target.removeEventListener('pointerup', onUp)
            setLiveWeight(null)
            onResizeWeight(Number(toWeight(ev.clientY).toFixed(3)))
          }
          target.addEventListener('pointermove', onMove)
          target.addEventListener('pointerup', onUp)
        }}
      >
        <Box
          w={36}
          h={3}
          mx="auto"
          mt={2}
          style={{ borderRadius: 2, background: 'var(--mantine-color-default-border)' }}
        />
      </Box>
    </Box>
  )
}

// ---------------------------------------------------------------------------
// The analyzer window
// ---------------------------------------------------------------------------

interface Props {
  singleAnalyzerId: string
  analyzers: AnalyzerSpec[]
  /** setState-compatible so rapid updates can't clobber each other. */
  onAnalyzersChange: (update: (prev: AnalyzerSpec[]) => AnalyzerSpec[]) => void
  /** Solved document tables offered by the browser's “Import table” menu. */
  tables?: TableSpec[]
}

export default function DataAnalyzerTab({
  singleAnalyzerId,
  analyzers,
  onAnalyzersChange,
  tables,
}: Readonly<Props>) {
  const spec = analyzers.find((a) => a.id === singleAnalyzerId)
  const dark = useComputedColorScheme('dark') === 'dark'
  const storeVersion = useSyncExternalStore(channelStore.subscribe, channelStore.version)
  const [view, dispatch] = useReducer(viewReducer, {
    xRange: null,
    cursorA: null,
    cursorB: null,
    snap: true,
    mouseMode: 'zoom',
    selectedSignal: null,
    instrument: 'scope',
  })
  // Hover time (floating-cursor readout), rAF-coalesced: setCursor fires per
  // mousemove and re-renders every strip's signal list.
  const [hoverT, setHoverT] = useState<number | null>(null)
  const hoverPending = useRef<number | null>(null)
  const hoverFrame = useRef(0)
  const handleHover = useCallback((t: number | null) => {
    hoverPending.current = t
    if (hoverFrame.current !== 0) return
    hoverFrame.current = requestAnimationFrame(() => {
      hoverFrame.current = 0
      setHoverT(hoverPending.current)
    })
  }, [])
  useEffect(() => () => cancelAnimationFrame(hoverFrame.current), [])
  // In-tab signal browser visibility. Collapsed by default: the same browser
  // lives in the Inspector (open by default) whenever an analyzer is focused,
  // so the oscilloscope gets the full width until the user expands it here.
  const [browserOpen, setBrowserOpen] = useState(false)
  // Live signal-list width during a divider drag; committed to the spec on release.
  const [liveListWidth, setLiveListWidth] = useState<number | null>(null)

  // Functional update against the LATEST state: two spec changes in the same
  // tick (e.g. two rapid add-signal clicks) must both land, so never derive
  // the next array from this render's (possibly stale) `analyzers` closure.
  const updateSpec = useCallback(
    (mutate: (current: AnalyzerSpec) => AnalyzerSpec) => {
      onAnalyzersChange((prev) =>
        prev.map((a) => (a.id === singleAnalyzerId ? mutate(a) : a)),
      )
    },
    [onAnalyzersChange, singleAnalyzerId],
  )

  // All hooks above; early-out below keeps hook order stable.
  const allSignals = useMemo(() => spec?.strips.flatMap((s) => s.signals) ?? [], [spec])
  const offsets = useMemo(() => (spec ? offsetsOf(spec) : new Map<string, number>()), [spec])
  const [showCalc, setShowCalc] = useState(false)

  if (spec === undefined) return null
  const syncKey = `frees-analyzer-${spec.id}`

  const selectStrip = (id: string | undefined) => {
    updateSpec((cur) => (cur.selectedStripId === id ? cur : { ...cur, selectedStripId: id }))
  }

  const addStrip = () => {
    const strip = newStrip()
    updateSpec((cur) => ({ ...cur, strips: [...cur.strips, strip], selectedStripId: strip.id }))
  }

  const removeStrip = (id: string) => {
    updateSpec((cur) => ({
      ...cur,
      strips: cur.strips.filter((s) => s.id !== id),
      selectedStripId: cur.selectedStripId === id ? undefined : cur.selectedStripId,
    }))
  }

  /** Drop of a signal row onto another strip (drag-between-strips). */
  const dropSignal = (toStripId: string, fromStripId: string, measurementId: string, channel: string) => {
    updateSpec((cur) => ({
      ...cur,
      strips: moveSignalOp(cur.strips, fromStripId, toStripId, measurementId, channel),
    }))
  }

  /** Drop of a strip grip onto another strip: the dragged strip takes its slot. */
  const dropStrip = (targetStripId: string, dragStripId: string) => {
    updateSpec((cur) => ({ ...cur, strips: reorderStripOp(cur.strips, dragStripId, targetStripId) }))
  }

  const setStripWeight = (stripId: string, weight: number) => {
    updateSpec((cur) => ({
      ...cur,
      // Drop the legacy px height once a weight is set (it only feeds the
      // one-time migration in stripWeight()).
      strips: cur.strips.map((s) =>
        s.id === stripId ? { ...s, weight, height: undefined } : s,
      ),
    }))
  }

  /** "Treat As Boolean/Analog Signal" (undefined = back to auto). */
  const setSignalKind = (
    stripId: string,
    channel: string,
    measurementId: string,
    kind: 'analog' | 'boolean' | undefined,
  ) => {
    updateSpec((cur) => ({
      ...cur,
      strips: cur.strips.map((s) =>
        s.id === stripId
          ? {
              ...s,
              signals: s.signals.map((sig) =>
                sig.channel === channel && sig.measurementId === measurementId
                  ? { ...sig, kindOverride: kind }
                  : sig,
              ),
            }
          : s,
      ),
    }))
  }

  /** "Move to New Strip": insert a strip right below and move the signal. */
  const moveToNewStrip = (fromStripId: string, channel: string, measurementId: string) => {
    const strip = newStrip()
    updateSpec((cur) => {
      const strips = [...cur.strips]
      const idx = strips.findIndex((s) => s.id === fromStripId)
      strips.splice(idx < 0 ? strips.length : idx + 1, 0, strip)
      return { ...cur, strips: moveSignalOp(strips, fromStripId, strip.id, measurementId, channel) }
    })
  }

  /** Zoom in/out buttons: scale the window about its center (factor <1 = in).
   *  With no explicit window yet, start from the loaded data extents. */
  const zoomBy = (factor: number) => {
    let range = view.xRange
    if (range === null) {
      let lo = Number.POSITIVE_INFINITY
      let hi = Number.NEGATIVE_INFINITY
      for (const sig of allSignals) {
        const win = offsetWindow(sig, offsets.get(sig.measurementId) ?? 0, null, null, MAX_POINTS)
        if (!win || win.t.length === 0) continue
        lo = Math.min(lo, win.t[0])
        hi = Math.max(hi, win.t[win.t.length - 1])
      }
      if (!(hi > lo)) return
      range = [lo, hi]
    }
    const center = (range[0] + range[1]) / 2
    const half = ((range[1] - range[0]) / 2) * factor
    if (half > 0) dispatch({ type: 'zoom', min: center - half, max: center + half })
  }

  const removeSignal = (stripId: string, channel: string, measurementId: string) => {
    updateSpec((cur) => ({
      ...cur,
      strips: cur.strips.map((s) =>
        s.id === stripId
          ? {
              ...s,
              signals: s.signals.filter(
                (sig) => !(sig.channel === channel && sig.measurementId === measurementId),
              ),
            }
          : s,
      ),
    }))
  }

  /** Cursor placement: snap to the nearest sample of the clicked strip's
   *  first loaded signal when snap mode is on (§2.5e); offset-aware. */
  const placeCursor = (strip: AnalyzerStrip) => (t: number, which: 'a' | 'b') => {
    let snapped = t
    if (view.snap) {
      const first = strip.signals[0]
      if (first) snapped = offsetNearestTime(first, offsets.get(first.measurementId) ?? 0, t) ?? t
    }
    dispatch({ type: 'set-cursor', which, t: snapped })
  }

  /** Per-file time offset (Phase 5a): numeric entry is the precise path;
   *  SHIFT-drag adds a delta on top of the current value. */
  const setFileOffset = (measurementId: string, offset: number) => {
    updateSpec((cur) => ({
      ...cur,
      files: cur.files.map((f) => (f.measurementId === measurementId ? { ...f, offset } : f)),
    }))
  }
  const dragOffset = (strip: AnalyzerStrip) => (deltaSeconds: number) => {
    const first = strip.signals[0]
    if (!first) return
    const current = spec.files.find((f) => f.measurementId === first.measurementId)?.offset ?? 0
    setFileOffset(first.measurementId, Number((current + deltaSeconds).toPrecision(9)))
  }

  /** Calc result (Phase 4) → first-class ChannelStore channel + auto-assign
   *  to the selected strip (mirrors SignalBrowser.addSignal). */
  const handleCalcResult = (result: CalcResultDto) => {
    setShowCalc(false)
    const meta = channelStore.register(calcResultToMeasurement(result.name, result.t, result.v), spec.id)
    const ch = meta.channels[0]
    updateSpec((cur) => {
      const next = {
        ...cur,
        files: [...cur.files, { measurementId: meta.measurementId, signature: meta.signature }],
      }
      if (!ch) return next
      const slot = next.strips.reduce((acc, s) => acc + s.signals.length, 0)
      let strips = next.strips
      let target = strips.find((s) => s.id === next.selectedStripId) ?? strips[strips.length - 1]
      if (target === undefined) {
        target = newStrip()
        strips = [target]
      }
      const signal = { measurementId: meta.measurementId, channel: ch.name, color: signalColor(slot) }
      const targetId = target.id
      return {
        ...next,
        strips: strips.map((s) => (s.id === targetId ? { ...s, signals: [...s.signals, signal] } : s)),
      }
    })
  }

  /** Event List click (Phase 5a): move cursor A there and recenter the view. */
  const eventJump = (t: number) => {
    dispatch({ type: 'set-cursor', which: 'a', t })
    if (view.xRange !== null) {
      const width = view.xRange[1] - view.xRange[0]
      dispatch({ type: 'zoom', min: t - width / 2, max: t + width / 2 })
    }
    dispatch({ type: 'set-instrument', instrument: 'scope' })
  }

  /** Keyboard cursor stepping (Phase 5c): ←/→ steps cursor A one sample of
   *  the first assigned signal; Shift+←/→ steps cursor B. */
  const stepCursor = (which: 'a' | 'b', direction: 1 | -1) => {
    const first = allSignals[0]
    if (!first) return
    const off = offsets.get(first.measurementId) ?? 0
    const raw = offsetRawRange(first, off, null, null)
    if (!raw || raw.t.length === 0) return
    const current = which === 'a' ? view.cursorA : view.cursorB
    if (current === null) {
      dispatch({ type: 'set-cursor', which, t: raw.t[0] })
      return
    }
    let idx = lowerBound(raw.t, current)
    if (idx >= raw.t.length || raw.t[idx] > current) idx--
    const next = Math.max(0, Math.min(raw.t.length - 1, idx + direction))
    dispatch({ type: 'set-cursor', which, t: raw.t[next] })
  }

  const handleExport = () => {
    const exportSignals: ExportSignal[] = []
    for (const sig of allSignals) {
      const raw = offsetRawRange(sig, offsets.get(sig.measurementId) ?? 0, null, null)
      if (raw) exportSignals.push({ name: sig.channel, unit: raw.unit, t: raw.t, v: raw.v })
    }
    if (exportSignals.length === 0) return
    downloadCsv(
      `${spec.name.replace(/\s+/g, '_')}_export`,
      buildCsv(exportSignals, view.xRange?.[0] ?? null, view.xRange?.[1] ?? null),
    )
  }

  const cursors: AbCursors = { a: view.cursorA, b: view.cursorB }
  const dt = view.cursorA !== null && view.cursorB !== null ? view.cursorB - view.cursorA : null
  const listWidth = liveListWidth ?? spec.signalListWidth ?? LIST_WIDTH_DEFAULT
  const resizeSignalList = (width: number, done: boolean) => {
    if (done) {
      setLiveListWidth(null)
      updateSpec((cur) => ({ ...cur, signalListWidth: width }))
    } else {
      setLiveListWidth(width)
    }
  }

  return (
    <Group align="stretch" gap={0} h="100%" wrap="nowrap">
      {/* Signal browser (shared component — the Inspector hosts it too).
          Collapsible so the oscilloscope can take the full width. */}
      {browserOpen ? (
        <Stack
          w={280}
          gap="xs"
          p="xs"
          h="100%"
          style={{ borderRight: '1px solid var(--mantine-color-default-border)', flexShrink: 0 }}
        >
          <Group justify="space-between" gap={4} wrap="nowrap">
            <Text size="xs" fw={600} c="dimmed">
              Signals
            </Text>
            <Tooltip label="Collapse the signal browser (it is also available in the Inspector)">
              <ActionIcon size="xs" variant="subtle" color="gray" onClick={() => setBrowserOpen(false)}>
                <IconChevronLeft size={12} />
              </ActionIcon>
            </Tooltip>
          </Group>
          <Box style={{ flex: 1, minHeight: 0 }}>
            <SignalBrowser
              spec={spec}
              updateSpec={updateSpec}
              tables={tables}
              onAfterImport={() => dispatch({ type: 'reset-zoom' })}
            />
          </Box>
        </Stack>
      ) : (
        <Stack
          w={26}
          p={2}
          h="100%"
          align="center"
          style={{ borderRight: '1px solid var(--mantine-color-default-border)', flexShrink: 0 }}
        >
          <Tooltip label="Expand the signal browser">
            <ActionIcon size="sm" variant="subtle" color="gray" onClick={() => setBrowserOpen(true)}>
              <IconChevronRight size={14} />
            </ActionIcon>
          </Tooltip>
        </Stack>
      )}

      {/* Instruments */}
      <Tabs
        value={view.instrument}
        onChange={(v) => dispatch({ type: 'set-instrument', instrument: (v ?? 'scope') as Instrument })}
        keepMounted={false}
        style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}
        p="xs"
      >
        <Group justify="space-between" wrap="nowrap">
          <Tabs.List>
            <Tabs.Tab value="scope" leftSection={<IconWaveSine size={14} />}>
              Oscilloscope
            </Tabs.Tab>
            <Tabs.Tab value="table" leftSection={<IconTable size={14} />}>
              Table
            </Tabs.Tab>
            <Tabs.Tab value="stats" leftSection={<IconSum size={14} />}>
              Statistics
            </Tabs.Tab>
            <Tabs.Tab value="events" leftSection={<IconListSearch size={14} />}>
              Events
            </Tabs.Tab>
            <Tabs.Tab value="scatter" leftSection={<IconChartDots size={14} />}>
              Scatter
            </Tabs.Tab>
            <Tabs.Tab value="histogram" leftSection={<IconChartHistogram size={14} />}>
              Histogram
            </Tabs.Tab>
            <Tabs.Tab value="compare" leftSection={<IconArrowsDiff size={14} />}>
              Compare
            </Tabs.Tab>
          </Tabs.List>
          <Group gap="xs" wrap="nowrap">
            <Button
              size="compact-xs"
              variant="light"
              leftSection={<IconMathFunction size={13} />}
              disabled={spec.files.length === 0}
              onClick={() => setShowCalc(true)}
            >
              Calc signal
            </Button>
            <Button
              size="compact-xs"
              variant="default"
              leftSection={<IconDownload size={13} />}
              disabled={allSignals.length === 0}
              onClick={handleExport}
            >
              Export CSV
            </Button>
            <Button
              size="compact-xs"
              variant="default"
              leftSection={<IconZoomReset size={13} />}
              onClick={() => dispatch({ type: 'reset-zoom' })}
            >
              Reset zoom
            </Button>
            <Button
              size="compact-xs"
              variant="default"
              leftSection={<IconPlus size={13} />}
              onClick={addStrip}
            >
              Add strip
            </Button>
          </Group>
        </Group>

        <Tabs.Panel
          value="scope"
          style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', outline: 'none' }}
          pt={6}
          tabIndex={0}
          aria-label="Oscilloscope — arrow keys step cursor A one sample, Shift+arrows step cursor B"
          onKeyDown={(e) => {
            if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
              e.preventDefault()
              stepCursor(e.shiftKey ? 'b' : 'a', e.key === 'ArrowRight' ? 1 : -1)
            }
          }}
        >
          {/* Cursor readout bar (§2.5e): A (click), B (Shift+click), Δt, live
              hover time; per-signal values live in each strip's signal list. */}
          <Group gap="md" wrap="nowrap" mb={6}>
            <Group gap={4} wrap="nowrap">
              <Badge size="sm" variant="light" color="yellow">
                A
              </Badge>
              <Text size="xs" ff="monospace">
                {view.cursorA !== null ? `${formatValue(view.cursorA)} s` : '—'}
              </Text>
            </Group>
            <Group gap={4} wrap="nowrap">
              <Badge size="sm" variant="light" color="cyan">
                B
              </Badge>
              <Text size="xs" ff="monospace">
                {view.cursorB !== null ? `${formatValue(view.cursorB)} s` : '—'}
              </Text>
            </Group>
            <Text size="xs" ff="monospace" c={dt !== null ? undefined : 'dimmed'}>
              Δt = {dt !== null ? `${formatValue(dt)} s` : '—'}
              {dt !== null && dt !== 0 ? `  (${formatValue(1 / Math.abs(dt))} Hz)` : ''}
            </Text>
            <Text size="xs" ff="monospace" c="dimmed" w={110}>
              t = {hoverT !== null ? `${formatValue(hoverT)} s` : '—'}
            </Text>
            <Switch
              size="xs"
              label="Snap to samples"
              checked={view.snap}
              onChange={() => dispatch({ type: 'toggle-snap' })}
            />
            <Button
              size="compact-xs"
              variant="subtle"
              color="gray"
              disabled={view.cursorA === null && view.cursorB === null}
              onClick={() => dispatch({ type: 'clear-cursors' })}
            >
              Clear cursors
            </Button>
            {/* Mouse-mode + zoom tools (oscilloscope-tool toolbar parity). */}
            <Group gap={4} wrap="nowrap" ml="auto">
              <Tooltip label="Area zoom — drag a box to zoom in">
                <ActionIcon
                  size="sm"
                  variant={view.mouseMode === 'zoom' ? 'filled' : 'default'}
                  aria-label="Area zoom mode"
                  onClick={() => dispatch({ type: 'set-mouse-mode', mode: 'zoom' })}
                >
                  <IconZoomInArea size={14} />
                </ActionIcon>
              </Tooltip>
              <Tooltip label="Pan — drag to scroll along the time axis">
                <ActionIcon
                  size="sm"
                  variant={view.mouseMode === 'pan' ? 'filled' : 'default'}
                  aria-label="Pan mode"
                  onClick={() => dispatch({ type: 'set-mouse-mode', mode: 'pan' })}
                >
                  <IconHandMove size={14} />
                </ActionIcon>
              </Tooltip>
              <Tooltip label="Zoom in">
                <ActionIcon
                  size="sm"
                  variant="default"
                  aria-label="Zoom in"
                  onClick={() => zoomBy(0.5)}
                >
                  <IconZoomIn size={14} />
                </ActionIcon>
              </Tooltip>
              <Tooltip label="Zoom out">
                <ActionIcon
                  size="sm"
                  variant="default"
                  aria-label="Zoom out"
                  onClick={() => zoomBy(2)}
                >
                  <IconZoomOut size={14} />
                </ActionIcon>
              </Tooltip>
            </Group>
          </Group>
          {/* Fill-area strip stack: one strip covers the whole panel,
              added strips split it by their flex weights; below the minimum
              per-strip height the stack scrolls. */}
          <Box
            style={{
              flex: 1,
              minHeight: 0,
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
              overflowY: 'auto',
            }}
          >
            {spec.strips.map((strip) => (
                <StripView
                  key={strip.id}
                  strip={strip}
                  syncKey={syncKey}
                  xRange={view.xRange}
                  cursors={cursors}
                  hoverT={hoverT}
                  mouseMode={view.mouseMode}
                  offsets={offsets}
                  storeVersion={storeVersion}
                  selected={spec.selectedStripId === strip.id}
                  dark={dark}
                  onSelect={() => selectStrip(strip.id)}
                  onZoom={(min, max) => dispatch({ type: 'zoom', min, max })}
                  onResetZoom={() => dispatch({ type: 'reset-zoom' })}
                  onCursorSet={placeCursor(strip)}
                  onOffsetDrag={dragOffset(strip)}
                  onHover={handleHover}
                  selectedSignal={view.selectedSignal}
                  onSelectSignal={(sig) => dispatch({ type: 'select-signal', sig })}
                  signalListWidth={listWidth}
                  onResizeSignalList={resizeSignalList}
                  onRemoveSignal={(channel, measurementId) =>
                    removeSignal(strip.id, channel, measurementId)
                  }
                  onRemoveStrip={() => removeStrip(strip.id)}
                  onResizeWeight={(weight) => setStripWeight(strip.id, weight)}
                  onDropSignal={(fromStripId, measurementId, channel) =>
                    dropSignal(strip.id, fromStripId, measurementId, channel)
                  }
                  onDropStrip={(dragStripId) => dropStrip(strip.id, dragStripId)}
                  onSetKind={(channel, measurementId, kind) =>
                    setSignalKind(strip.id, channel, measurementId, kind)
                  }
                  onMoveToNewStrip={(channel, measurementId) =>
                    moveToNewStrip(strip.id, channel, measurementId)
                  }
                />
            ))}
            {spec.strips.length === 0 && (
              <Text size="sm" c="dimmed" ta="center" mt="xl">
                No strips — add one to start plotting signals.
              </Text>
            )}
          </Box>
        </Tabs.Panel>

        <Tabs.Panel value="table" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }} pt={6}>
          <TableInstrument signals={allSignals} offsets={offsets} xRange={view.xRange} storeVersion={storeVersion} />
        </Tabs.Panel>

        <Tabs.Panel value="stats" style={{ flex: 1, minHeight: 0 }} pt={6}>
          <ScrollArea h="100%" type="auto">
            <StatisticsInstrument
              signals={allSignals}
              offsets={offsets}
              xRange={view.xRange}
              cursors={cursors}
              storeVersion={storeVersion}
            />
          </ScrollArea>
        </Tabs.Panel>

        <Tabs.Panel value="events" style={{ flex: 1, minHeight: 0 }} pt={6}>
          <ScrollArea h="100%" type="auto">
            <EventListInstrument
              signals={allSignals}
              offsets={offsets}
              storeVersion={storeVersion}
              onJump={eventJump}
            />
          </ScrollArea>
        </Tabs.Panel>

        <Tabs.Panel value="scatter" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }} pt={6}>
          <ScatterInstrument
            signals={allSignals}
            offsets={offsets}
            xRange={view.xRange}
            cursors={cursors}
            storeVersion={storeVersion}
          />
        </Tabs.Panel>

        <Tabs.Panel value="histogram" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }} pt={6}>
          <HistogramInstrument
            signals={allSignals}
            offsets={offsets}
            xRange={view.xRange}
            cursors={cursors}
            storeVersion={storeVersion}
          />
        </Tabs.Panel>

        <Tabs.Panel value="compare" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }} pt={6}>
          <CompareInstrument
            signals={allSignals}
            offsets={offsets}
            xRange={view.xRange}
            cursors={cursors}
            storeVersion={storeVersion}
          />
        </Tabs.Panel>
      </Tabs>

      {showCalc && (
        <CalcSignalModal spec={spec} onResult={handleCalcResult} onClose={() => setShowCalc(false)} />
      )}
    </Group>
  )
}
