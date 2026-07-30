// Data Analyzer model (oscilloscope-style measurement analysis, todo.md Phase 1).
//
// An analyzer window binds imported measurement files (referenced by
// `measurementId` + a file signature, never bulk data) to a stack of
// oscilloscope strips. Bulk samples live outside React in the module-level
// ChannelStore (channelStore.ts); the spec below is the only part that App
// owns and (from Phase 2 on) persists into the `.frees` project file.

/**
 * Identity of an imported measurement file (design contract §2.5a/b).
 * `headerHash` hashes the first 64 KB plus the parsed column-name list —
 * never the full content (1 GB CSVs). Used in Phase 2 template mode to verify
 * a re-picked file against the stored reference.
 */
export interface FileSignature {
  name: string
  size: number
  headerHash: string
}

/** How a channel's samples may be rendered (design contract §2.5d). */
export type ChannelKind = 'analog' | 'boolean' | 'string'

/** Per-channel metadata surfaced in the signal browser. */
export interface ChannelMeta {
  name: string
  unit?: string
  kind: ChannelKind
  min: number
  max: number
}

/** Metadata for one imported measurement (kept even if samples are evicted). */
export interface MeasurementMeta {
  measurementId: string
  signature: FileSignature
  channels: ChannelMeta[]
  totalSamples: number
}

/** Reference to one channel of one measurement. */
export interface SignalRef {
  measurementId: string
  channel: string
}

/**
 * A signal assigned to a strip. The color is auto-assigned from the fixed
 * categorical palette by assignment slot and persisted here so sessions are
 * color-stable (design contract §2.5e).
 */
export interface AnalyzerSignal extends SignalRef {
  color: string
  /** Rendering override (the "Treat As Boolean/Analog Signal" menu); absent = auto. */
  kindOverride?: 'analog' | 'boolean'
}

/** One oscilloscope strip (a horizontal time-series band). */
export interface AnalyzerStrip {
  id: string
  signals: AnalyzerSignal[]
  /** Legacy chart height in px (pre fill-area layout); migrated to `weight`. */
  height?: number
  /**
   * Relative share of the oscilloscope area (flex weight, default 1): one
   * strip fills the whole panel, added strips split it, and the per-strip
   * resize handle rebalances the shares (conventional oscilloscope strip layout).
   */
  weight?: number
}

/** A measurement file attached to an analyzer window. */
export interface AnalyzerFileRef {
  measurementId: string
  signature: FileSignature
  /**
   * Per-file time offset in seconds (Phase 5a, multi-file compare):
   * displayed time = recorded time + offset. Set numerically or by
   * SHIFT-dragging a strip; persisted with the analyzer.
   */
  offset?: number
}

/** One persisted Data Analyzer document (mirrors WhiteboardSpec/DiagramSpec). */
export interface AnalyzerSpec {
  id: string
  name: string
  files: AnalyzerFileRef[]
  strips: AnalyzerStrip[]
  /**
   * Strip that receives browser “+” assignments. Lives on the spec (not view
   * state) so the Inspector-hosted SignalBrowser targets the same strip the
   * user selected inside the analyzer window.
   */
  selectedStripId?: string
  /** Width (px) of each strip's signal-list column; shared by all strips. */
  signalListWidth?: number
}

/**
 * The shared window DTO all instruments read (todo.md §2 hybrid data-locality
 * architecture): raw `v` when the requested range is small, an M4-style
 * min/max envelope when decimated. Cursor readout always resolves the exact
 * sample lazily via ChannelStore.exactValueAt.
 */
export interface ChannelWindow {
  t: Float64Array
  v?: Float64Array
  min?: Float64Array
  max?: Float64Array
  decimated: boolean
  totalSamples: number
  unit?: string
  kind: ChannelKind
}

/** Construct a new named analyzer with one empty strip. */
export function newAnalyzer(count: number): AnalyzerSpec {
  return {
    id: crypto.randomUUID(),
    name: `Analyzer ${count + 1}`,
    files: [],
    strips: [newStrip()],
  }
}

/** Construct a new empty strip. */
export function newStrip(): AnalyzerStrip {
  return { id: crypto.randomUUID(), signals: [] }
}
