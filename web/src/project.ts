// Story 10.10: Unified project file (`.frees` JSON).
//
// A single document capturing the entire workspace — equation text, Variable
// Information, parametric/function tables, plots, digitizer state, and all
// diagrams — so a model can be saved to and opened from one file, and
// autosaved/restored across reloads. This supersedes the scattered per-feature
// localStorage keys: on save everything is collected into one object written to
// `frees.project`; the legacy keys remain only as a one-time migration source.

import { DEFAULT_STOP_CRITERIA } from './api'
import type { StopCriteria, UnitSystem } from './api'
import type { VariableDraft } from './VariableInfoModal'
import type { TableSpec } from './tables'
import type { PlotSpec } from './plots/types'
import type { SpreadsheetSpec } from './spreadsheet/types'
import type { PinnedSlider } from './sliders'
import type { AnalyzerSpec } from './analyzer/types'
import type { SchematicOffsets } from './schematic/layout'

// v2 (Data Analyzer Phase 2): + `analyzers` slice — layout, signal
// assignments and measurement file REFS only ("template mode", §2.5b in
// todo.md); bulk samples never enter the project file. v1 files migrate by
// defaulting the slice to [].
// v3: + `schematic` slice — where the user has dragged each block on the
// rendered schematic. Earlier files migrate by defaulting it to {}, which is
// exactly "nothing dragged yet".
const PROJECT_VERSION = 3
const PROJECT_KEY = 'frees.project'

// Child-owned localStorage keys bridged into the project file. These mirror the
// literals used inside DigitizerTab.tsx and WorkspaceDock.tsx; the project
// file is the source of truth, those keys act as local caches.
const DIGITIZER_KEY = 'frees-digitizer'
const DOCK_LAYOUT_KEY = 'frees-dock-layout-v3'

/** The in-memory workspace slices owned by App.tsx that make up a project. */
export interface ProjectSlices {
  text: string
  varDrafts: Record<string, VariableDraft>
  stopCriteria: StopCriteria
  unitSystem: UnitSystem
  fillMissing: boolean
  stateUnitIds: Record<string, string>
  tables: TableSpec[]
  plots: PlotSpec[]
  spreadsheets: SpreadsheetSpec[]
  analyzers: AnalyzerSpec[]
  /** Parameters pinned to the workspace slider strip. */
  sliders?: PinnedSlider[]
  /** Blocks the user has moved on the schematic, as offsets from the
   *  auto-layout. The drawing itself is always derived from the document, so
   *  this is the only part of it worth saving. */
  schematic?: SchematicOffsets
}

export interface FreesProject extends ProjectSlices {
  version: number
  savedAt: string
  // Bridged from child-owned localStorage; opaque to App.
  digitizer: unknown
  dockLayout: unknown
}

function readJson(key: string): unknown {
  try {
    const raw = localStorage.getItem(key)
    return raw ? JSON.parse(raw) : null
  } catch {
    return null
  }
}

/** Assemble a complete project from App's slices plus the bridged child state. */
export function buildProject(slices: ProjectSlices): FreesProject {
  return {
    version: PROJECT_VERSION,
    savedAt: new Date().toISOString(),
    ...slices,
    digitizer: readJson(DIGITIZER_KEY),
    dockLayout: readJson(DOCK_LAYOUT_KEY),
  }
}

/**
 * Write the child-owned slices back to their localStorage caches so that
 * remounting DigitizerTab / DiagramTab restores them from an opened project.
 */
export function writeBridgedKeys(project: FreesProject) {
  try {
    if (project.digitizer != null) {
      localStorage.setItem(DIGITIZER_KEY, JSON.stringify(project.digitizer))
    } else {
      localStorage.removeItem(DIGITIZER_KEY)
    }
    if (project.dockLayout != null) {
      localStorage.setItem(DOCK_LAYOUT_KEY, JSON.stringify(project.dockLayout))
    } else {
      localStorage.removeItem(DOCK_LAYOUT_KEY)
    }
  } catch {
    // Quota or serialization failures are non-fatal; the in-memory state still loads.
  }
}

const ALLOWED_UNIT_SYSTEMS: readonly UnitSystem[] = ['SI', 'ENG_SI', 'ENGLISH']

/** Deep-copy to plain JSON data, dropping anything non-serializable. */
function plainJson<T>(value: T): T {
  try {
    return JSON.parse(JSON.stringify(value ?? null)) as T
  } catch {
    return null as T
  }
}

function finiteNumber(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

/** How far from its auto-layout position a block may be saved, and how many
 *  blocks may carry an offset. A schematic is bounded by the network it draws;
 *  anything past these is a malformed or hostile file, not a real drawing. */
const MAX_OFFSET = 100_000
const MAX_OFFSET_ENTRIES = 5_000

/**
 * Validate the schematic's drag offsets. Coordinates from a project file reach
 * an SVG viewBox and the export's bounding box, so a non-finite or absurd value
 * would render the drawing unusable rather than merely wrong — each is required
 * to be a finite number and clamped to a plausible canvas.
 */
function sanitizeOffsets(value: unknown): SchematicOffsets {
  if (value == null || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }
  const out: SchematicOffsets = {}
  const clamp = (n: number) => Math.min(MAX_OFFSET, Math.max(-MAX_OFFSET, n))
  for (const [key, raw] of Object.entries(value as Record<string, unknown>)) {
    if (Object.keys(out).length >= MAX_OFFSET_ENTRIES) {
      break
    }
    if (raw == null || typeof raw !== 'object') {
      continue
    }
    const { dx, dy } = raw as { dx?: unknown; dy?: unknown }
    if (typeof dx !== 'number' || typeof dy !== 'number' || !Number.isFinite(dx) || !Number.isFinite(dy)) {
      continue
    }
    out[key] = { dx: clamp(dx), dy: clamp(dy) }
  }
  return out
}

/**
 * Validate and normalize a project into the plain, schema-shaped payload that is
 * safe to persist. Every field is checked against its expected type — and the
 * unit system against an allowlist — before it can reach browser storage, so a
 * project can never poison localStorage with unvalidated, externally influenced
 * values (tssecurity:S8475). Sanitizing here, at write time, keeps the trust
 * boundary independent of whatever code later reads the value back. Returns
 * null for non-object input.
 */
function sanitizeProject(project: FreesProject): FreesProject | null {
  if (project == null || typeof project !== 'object') return null
  const sc = (project.stopCriteria ?? {}) as Partial<StopCriteria>
  return {
    version: PROJECT_VERSION,
    savedAt: typeof project.savedAt === 'string' ? project.savedAt : new Date().toISOString(),
    text: typeof project.text === 'string' ? project.text : '',
    varDrafts: plainJson(project.varDrafts) ?? {},
    stopCriteria: {
      maxIterations: finiteNumber(sc.maxIterations, DEFAULT_STOP_CRITERIA.maxIterations),
      relativeResiduals: finiteNumber(sc.relativeResiduals, DEFAULT_STOP_CRITERIA.relativeResiduals),
      changeInVariables: finiteNumber(sc.changeInVariables, DEFAULT_STOP_CRITERIA.changeInVariables),
      elapsedTimeSeconds: finiteNumber(sc.elapsedTimeSeconds, DEFAULT_STOP_CRITERIA.elapsedTimeSeconds),
      ...(typeof sc.complexMode === 'boolean' ? { complexMode: sc.complexMode } : {}),
    },
    unitSystem: ALLOWED_UNIT_SYSTEMS.includes(project.unitSystem) ? project.unitSystem : 'SI',
    fillMissing: Boolean(project.fillMissing),
    stateUnitIds: plainJson(project.stateUnitIds) ?? {},
    tables: Array.isArray(project.tables) ? plainJson(project.tables) : [],
    plots: Array.isArray(project.plots) ? plainJson(project.plots) : [],
    spreadsheets: Array.isArray(project.spreadsheets) ? plainJson(project.spreadsheets) : [],
    analyzers: Array.isArray(project.analyzers) ? plainJson(project.analyzers) : [],
    sliders: Array.isArray(project.sliders) ? plainJson(project.sliders) : [],
    schematic: sanitizeOffsets(project.schematic),
    digitizer: plainJson(project.digitizer),
    dockLayout: plainJson(project.dockLayout),
  }
}

/**
 * Normalize a project read back from any browser storage (localStorage,
 * IndexedDB) to the current version and schema. Storage is outside the app's
 * trust boundary regardless of which API it hides behind, so reads go through
 * the same migrate-then-sanitize path as writes.
 */
export function normalizeStoredProject(raw: unknown): FreesProject | null {
  if (raw == null || typeof raw !== 'object') return null
  return sanitizeProject(migrate(raw as FreesProject))
}

export function saveProjectLocal(project: FreesProject) {
  const safe = sanitizeProject(project)
  if (safe == null) return
  try {
    localStorage.setItem(PROJECT_KEY, JSON.stringify(safe))
  } catch {
    // Autosave is best-effort; ignore quota errors.
  }
}

export function loadProjectLocal(): FreesProject | null {
  const raw = readJson(PROJECT_KEY)
  return raw ? migrate(raw as FreesProject) : null
}

export function clearProjectLocal() {
  try {
    localStorage.removeItem(PROJECT_KEY)
  } catch {
    // ignore
  }
}

/** Normalize a parsed project to the current version, filling missing slices. */
function migrate(p: FreesProject): FreesProject {
  return {
    version: PROJECT_VERSION,
    savedAt: p.savedAt ?? new Date().toISOString(),
    text: p.text ?? '',
    varDrafts: p.varDrafts ?? {},
    stopCriteria: p.stopCriteria,
    unitSystem: p.unitSystem ?? 'SI',
    fillMissing: Boolean(p.fillMissing),
    stateUnitIds: p.stateUnitIds ?? {},
    tables: p.tables ?? [],
    plots: p.plots ?? [],
    spreadsheets: p.spreadsheets ?? [],
    analyzers: p.analyzers ?? [],
    sliders: p.sliders ?? [],
    // Pre-v3 files predate saved schematic positions; nothing dragged.
    schematic: sanitizeOffsets(p.schematic),
    digitizer: p.digitizer ?? null,
    dockLayout: p.dockLayout ?? null,
  }
}

function sanitizeFilename(name: string): string {
  const base = name.trim().replace(/\.frees$/i, '').replace(/[^\w.-]+/g, '_')
  return `${base || 'untitled'}.frees`
}

/** Trigger a browser download of the project as a `.frees` JSON file. */
function downloadProject(project: FreesProject, filename: string) {
  const blob = new Blob([JSON.stringify(project, null, 2)], {
    type: 'application/json',
  })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = sanitizeFilename(filename)
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}

/**
 * Save the project, letting the user choose the destination via the File System
 * Access API (showSaveFilePicker) where supported. Falls back to a plain browser
 * download (fixed Downloads folder) on browsers without the API (e.g. Firefox).
 *
 * Returns true if the project was saved (or a download was triggered), false if
 * the user cancelled the picker — so callers can keep the dirty flag set.
 */
export async function saveProject(project: FreesProject, filename: string): Promise<boolean> {
  const json = JSON.stringify(project, null, 2)
  const suggestedName = sanitizeFilename(filename)
  const picker = (window as unknown as {
    showSaveFilePicker?: (opts: unknown) => Promise<{
      createWritable: () => Promise<{ write: (data: string) => Promise<void>; close: () => Promise<void> }>
    }>
  }).showSaveFilePicker

  if (typeof picker === 'function') {
    try {
      const handle = await picker({
        suggestedName,
        types: [{ description: 'frees project', accept: { 'application/json': ['.frees'] } }],
      })
      const writable = await handle.createWritable()
      await writable.write(json)
      await writable.close()
      return true
    } catch (err) {
      // The user dismissed the picker — leave the project unsaved (and dirty).
      if (err instanceof DOMException && err.name === 'AbortError') return false
      // Any other failure (permissions, unsupported) falls back to a download.
    }
  }

  downloadProject(project, filename)
  return true
}

/** Read and validate an opened `.frees` file. */
export async function readProjectFile(file: File): Promise<FreesProject> {
  const raw = await file.text()
  const parsed = JSON.parse(raw)
  if (!parsed || typeof parsed !== 'object' || !('version' in parsed)) {
    throw new Error('Not a valid .frees project file.')
  }
  if ((parsed as FreesProject).version > PROJECT_VERSION) {
    throw new Error(
      `This project was saved by a newer version of frees (v${(parsed as FreesProject).version}).`,
    )
  }
  return migrate(parsed as FreesProject)
}
