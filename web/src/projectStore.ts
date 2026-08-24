// Phase 11: the browser-resident project library (decision D4).
//
// IndexedDB holds two things localStorage cannot: a *library* of named
// projects (localStorage has one slot) and a *durable* autosave mirror
// (localStorage's ~5 MB quota makes the existing autosave silently stop
// updating once the workspace grows past it). The
// localStorage autosave in project.ts is untouched — it remains what App.tsx
// boots from synchronously; everything here is post-boot and async.
//
// Projects are keyed by their display name, deliberately: the app's only
// naming concept is `projectName`, and name-keyed storage gives "Save to
// browser" the same overwrite semantics as saving a file. See D4 for the
// OPFS-vs-IndexedDB call.

import type { FreesProject } from './project'
import { normalizeStoredProject } from './project'

const DB_NAME = 'frees'
const DB_VERSION = 1
/** Named projects, keyed by display name. */
const PROJECTS_STORE = 'projects'
/**
 * Out-of-line-keyed store holding the autosave mirror and (Wave I) the file
 * link — the FileSystemFileHandle behind a file-provenance project, persisted
 * beside the workspace it belongs to so a reload keeps Save pickerless.
 */
const AUTOSAVE_STORE = 'autosave'
const AUTOSAVE_KEY = 'workspace'
const FILE_LINK_KEY = 'fileLink'

/** What the library list shows; the document itself stays on disk until opened. */
export interface StoredProjectMeta {
  name: string
  savedAt: string
  /** Serialized size in bytes — the list UI's honesty about quota. */
  size: number
  /**
   * Our own write counter for this row, bumped on every save (Wave: multi-tab
   * safety). NOT `savedAt`: two tabs can produce the same millisecond, clocks
   * move backwards, and a project loaded from a file carries a `savedAt` from
   * another machine entirely. A counter we mint on write is the only thing in
   * the row we can compare and trust. Rows written before revisions existed
   * read back as `LEGACY_REV` (0), which compares correctly against a tab that
   * loaded them.
   */
  rev: number
}

/** The revision of a row stored before this file stamped revisions. */
const LEGACY_REV = 0

interface ProjectRow extends StoredProjectMeta {
  project: FreesProject
}

function metaOf(row: ProjectRow): StoredProjectMeta {
  return { name: row.name, savedAt: row.savedAt, size: row.size, rev: row.rev ?? LEGACY_REV }
}

let dbPromise: Promise<IDBDatabase | null> | null = null

// ---------------------------------------------------------------------------
// Multi-tab coordination (Wave E, narrowing Phase 11's gap 5). BroadcastChannel
// posts never echo to the sending tab, so every received event IS another tab:
// the library modal refreshes its listing live, and App warns when the project
// it has open was just overwritten elsewhere. That is the *visibility* half.
//
// The *safety* half is `rev` above: a save states the revision it is replacing,
// and a save whose revision no longer matches what is on disk writes NOTHING
// and returns `conflict` for the caller to resolve (overwrite / save a copy /
// take theirs). The notices below stay exactly as they were — a warning you
// can miss is a fine complement to a write that cannot silently lose work, and
// a poor substitute for one.
// ---------------------------------------------------------------------------

/** One library mutation as seen from another tab. */
export interface LibraryChange {
  kind: 'saved' | 'deleted' | 'renamed'
  name: string
  /** The new name, for renames. */
  to?: string
}

const LIBRARY_CHANNEL = 'frees-project-library'

function postLibraryChange(change: LibraryChange): void {
  try {
    // A throwaway channel per post: cheap, and avoids holding a handle that
    // some test environments never close.
    const channel = new BroadcastChannel(LIBRARY_CHANNEL)
    channel.postMessage(change)
    channel.close()
  } catch {
    // No BroadcastChannel (old jsdom, exotic embeds): writes still work,
    // other tabs just refresh on their own next open.
  }
}

/** Subscribe to other tabs' library mutations; returns the unsubscriber. */
export function subscribeLibraryChanges(listener: (change: LibraryChange) => void): () => void {
  try {
    const channel = new BroadcastChannel(LIBRARY_CHANNEL)
    channel.onmessage = (event: MessageEvent<LibraryChange>) => listener(event.data)
    return () => channel.close()
  } catch {
    return () => {}
  }
}

/**
 * Open (and on first use create) the database. Resolves null wherever
 * IndexedDB is unavailable or refuses to open — private modes, partitioned
 * iframes, jsdom — so every caller degrades to "the library is absent"
 * rather than throwing during boot-adjacent code.
 */
function openDb(): Promise<IDBDatabase | null> {
  if (dbPromise) return dbPromise
  dbPromise = new Promise((resolve) => {
    let request: IDBOpenDBRequest
    try {
      request = indexedDB.open(DB_NAME, DB_VERSION)
    } catch {
      resolve(null)
      return
    }
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains(PROJECTS_STORE)) {
        db.createObjectStore(PROJECTS_STORE, { keyPath: 'name' })
      }
      if (!db.objectStoreNames.contains(AUTOSAVE_STORE)) {
        db.createObjectStore(AUTOSAVE_STORE)
      }
    }
    request.onsuccess = () => {
      const db = request.result
      // A versionchange fires when a newer tab upgrades the schema; close so
      // that tab's upgrade is not blocked forever by this one.
      db.onversionchange = () => db.close()
      resolve(db)
    }
    request.onerror = () => resolve(null)
    request.onblocked = () => resolve(null)
  })
  return dbPromise
}

/** Promise wrapper for a single IDB request. */
function await_<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'))
  })
}

function normalizeName(name: string): string {
  return name.trim().replace(/\.frees$/i, '') || 'untitled'
}

/** All saved projects, newest first. Empty when the library is unavailable. */
export async function listStoredProjects(): Promise<StoredProjectMeta[]> {
  const db = await openDb()
  if (!db) return []
  try {
    const rows = await await_(
      db.transaction(PROJECTS_STORE, 'readonly').objectStore(PROJECTS_STORE).getAll() as IDBRequest<ProjectRow[]>,
    )
    return rows
      .map(metaOf)
      .sort((a, b) => (a.savedAt < b.savedAt ? 1 : a.savedAt > b.savedAt ? -1 : 0))
  } catch {
    return []
  }
}

/** A project read out of the library, with the revision it was read at. */
export interface LoadedProject {
  project: FreesProject
  /** Hand this back as `expectedRev` on the next save of this name. */
  rev: number
}

/**
 * Read one project back with its revision, re-validated exactly like a
 * localStorage read. The revision is the whole point: a tab that saves without
 * remembering what it loaded cannot be told apart from a tab that never looked.
 */
export async function loadStoredProjectRev(name: string): Promise<LoadedProject | null> {
  const db = await openDb()
  if (!db) return null
  try {
    const row = await await_(
      db.transaction(PROJECTS_STORE, 'readonly').objectStore(PROJECTS_STORE).get(normalizeName(name)) as IDBRequest<
        ProjectRow | undefined
      >,
    )
    if (!row) return null
    const project = normalizeStoredProject(row.project)
    return project ? { project, rev: row.rev ?? LEGACY_REV } : null
  } catch {
    return null
  }
}

/** Read one project back, discarding the revision. */
export async function loadStoredProject(name: string): Promise<FreesProject | null> {
  return (await loadStoredProjectRev(name))?.project ?? null
}

/**
 * What a tab believes about the row it is about to write.
 *
 * `number` — "I loaded (or last wrote) this revision"; anything else on disk is
 *            another tab's work and the save is refused.
 * `'new'`  — "I have never read this name"; any existing row is refused.
 * `'overwrite'` — the deliberate last-write-wins escape hatch, which is what
 *            the conflict dialog's Overwrite button chooses.
 */
export type ExpectedRev = number | 'new' | 'overwrite'

export type SaveOutcome =
  /** Written. `meta.rev` is the revision to remember for the next save. */
  | { status: 'saved'; meta: StoredProjectMeta }
  /** Nothing was written: the row on disk is not the one this tab loaded. */
  | { status: 'conflict'; theirs: StoredProjectMeta }
  /** No library here (private mode, partitioned context), or an invalid project. */
  | { status: 'unavailable' }

/**
 * Save under `name` — the same overwrite semantics as saving a file, but only
 * when `expected` still describes what is on disk. The read-compare-write runs
 * inside one readwrite transaction, so two tabs racing cannot both see the old
 * revision and both write.
 *
 * `expected` is required, deliberately: every call site has to state what it
 * thinks it is replacing, and a caller that genuinely means last-write-wins has
 * to say `'overwrite'` where a reader can see it.
 */
export async function saveStoredProject(
  name: string,
  project: FreesProject,
  expected: ExpectedRev,
): Promise<SaveOutcome> {
  const db = await openDb()
  if (!db) return { status: 'unavailable' }
  const safe = normalizeStoredProject(project)
  if (!safe) return { status: 'unavailable' }
  const key = normalizeName(name)
  try {
    const store = db.transaction(PROJECTS_STORE, 'readwrite').objectStore(PROJECTS_STORE)
    const current = await await_(store.get(key) as IDBRequest<ProjectRow | undefined>)
    if (current && expected !== 'overwrite') {
      const currentRev = current.rev ?? LEGACY_REV
      // 'new' means "I expect nothing here", so any row at all is a conflict.
      if (expected === 'new' || expected !== currentRev) {
        return { status: 'conflict', theirs: metaOf(current) }
      }
    }
    // An absent row is never a conflict, even for a tab that loaded a revision:
    // another tab deleting the project and this one re-creating it loses no
    // work, where refusing the save would lose this tab's.
    const row: ProjectRow = {
      name: key,
      savedAt: safe.savedAt,
      size: JSON.stringify(safe).length,
      rev: (current?.rev ?? LEGACY_REV) + 1,
      project: safe,
    }
    await await_(store.put(row))
    postLibraryChange({ kind: 'saved', name: row.name })
    return { status: 'saved', meta: metaOf(row) }
  } catch {
    return { status: 'unavailable' }
  }
}

/**
 * A free "save as a copy" name: `model` → `model (copy)` → `model (copy 2)`.
 * Pure, and exported because it is the one piece of the conflict resolution
 * that can be wrong in a way no storage test would catch — a colliding copy
 * name would resolve one conflict by creating another.
 */
export function copyName(base: string, taken: Iterable<string>): string {
  const used = new Set<string>()
  for (const name of taken) used.add(normalizeName(name).toLowerCase())
  const stem = normalizeName(base)
  for (let n = 1; ; n += 1) {
    const candidate = n === 1 ? `${stem} (copy)` : `${stem} (copy ${n})`
    if (!used.has(candidate.toLowerCase())) return candidate
  }
}

export async function deleteStoredProject(name: string): Promise<void> {
  const db = await openDb()
  if (!db) return
  try {
    await await_(db.transaction(PROJECTS_STORE, 'readwrite').objectStore(PROJECTS_STORE).delete(normalizeName(name)))
    postLibraryChange({ kind: 'deleted', name: normalizeName(name) })
  } catch {
    // Deleting an absent row is not an error worth surfacing.
  }
}

/**
 * Rename by re-keying. Refuses (returns false) when the target name is
 * already taken — silently merging two projects is the one outcome worse
 * than a failed rename.
 */
export async function renameStoredProject(from: string, to: string): Promise<boolean> {
  const db = await openDb()
  if (!db) return false
  const source = normalizeName(from)
  const target = normalizeName(to)
  if (source === target) return true
  try {
    const tx = db.transaction(PROJECTS_STORE, 'readwrite')
    const store = tx.objectStore(PROJECTS_STORE)
    const row = await await_(store.get(source) as IDBRequest<ProjectRow | undefined>)
    if (!row) return false
    const existing = await await_(store.get(target) as IDBRequest<ProjectRow | undefined>)
    if (existing) return false
    await await_(store.put({ ...row, name: target }))
    await await_(store.delete(source))
    postLibraryChange({ kind: 'renamed', name: source, to: target })
    return true
  } catch {
    return false
  }
}

/**
 * The durable half of the debounced autosave. Best-effort by the same rule as
 * the localStorage half: autosave must never interrupt the user.
 *
 * Deliberately NOT revision-checked, unlike the named library above. This is a
 * single fixed key mirroring one tab's live workspace, and its authority is
 * already single-tab by construction: the localStorage half it mirrors is
 * per-tab-last-writer too, and `mirrorIsNewer` only ever *offers* the mirror at
 * boot rather than restoring it. A conflict dialog on an autosave would
 * interrupt the user on a keystroke timer to ask about a document they never
 * asked to save.
 */
export async function writeAutosaveMirror(project: FreesProject): Promise<void> {
  const db = await openDb()
  if (!db) return
  const safe = normalizeStoredProject(project)
  if (!safe) return
  try {
    await await_(db.transaction(AUTOSAVE_STORE, 'readwrite').objectStore(AUTOSAVE_STORE).put(safe, AUTOSAVE_KEY))
  } catch {
    // Best-effort.
  }
}

export async function readAutosaveMirror(): Promise<FreesProject | null> {
  const db = await openDb()
  if (!db) return null
  try {
    const raw = await await_(
      db.transaction(AUTOSAVE_STORE, 'readonly').objectStore(AUTOSAVE_STORE).get(AUTOSAVE_KEY),
    )
    return normalizeStoredProject(raw)
  } catch {
    return null
  }
}

export async function clearAutosaveMirror(): Promise<void> {
  const db = await openDb()
  if (!db) return
  try {
    await await_(db.transaction(AUTOSAVE_STORE, 'readwrite').objectStore(AUTOSAVE_STORE).delete(AUTOSAVE_KEY))
  } catch {
    // Best-effort.
  }
}

// ---------------------------------------------------------------------------
// Wave I (closing Phase 11's gap 2, the FileSystemFileHandle half): the file
// link. A FileSystemFileHandle is structured-cloneable, so IndexedDB can hold
// it across reloads — which localStorage cannot. All three functions are
// best-effort by the autosave-mirror rule: the in-memory handle already made
// this session's Save pickerless, persistence only extends that across a
// reload, and a browser that refuses to clone the handle just degrades there.
// ---------------------------------------------------------------------------

/** The persisted link between the open workspace and the file it came from. */
export interface StoredFileLink {
  /** The project display name at link time (the handle's name keeps the extension). */
  name: string
  handle: FileSystemFileHandle
}

export async function writeFileLink(name: string, handle: FileSystemFileHandle): Promise<void> {
  const db = await openDb()
  if (!db) return
  try {
    const link: StoredFileLink = { name, handle }
    await await_(db.transaction(AUTOSAVE_STORE, 'readwrite').objectStore(AUTOSAVE_STORE).put(link, FILE_LINK_KEY))
  } catch {
    // Best-effort: a handle that will not clone leaves this session's
    // in-memory handle working and the next session on the picker.
  }
}

export async function readFileLink(): Promise<StoredFileLink | null> {
  const db = await openDb()
  if (!db) return null
  try {
    const raw = await await_(
      db.transaction(AUTOSAVE_STORE, 'readonly').objectStore(AUTOSAVE_STORE).get(FILE_LINK_KEY),
    )
    if (raw == null || typeof raw !== 'object') return null
    const { name, handle } = raw as Partial<StoredFileLink>
    // Storage is outside the trust boundary: require the shape we wrote. The
    // handle's own permission state is the app's problem at Save time.
    if (typeof name !== 'string' || handle == null || typeof handle !== 'object') return null
    return { name, handle }
  } catch {
    return null
  }
}

export async function clearFileLink(): Promise<void> {
  const db = await openDb()
  if (!db) return
  try {
    await await_(db.transaction(AUTOSAVE_STORE, 'readwrite').objectStore(AUTOSAVE_STORE).delete(FILE_LINK_KEY))
  } catch {
    // Best-effort.
  }
}

/**
 * True when the IndexedDB autosave mirror is strictly newer than the
 * localStorage copy the app just booted from — which happens precisely when
 * localStorage writes started failing on quota. Both halves of a healthy
 * autosave share one `buildProject()` output and therefore one `savedAt`, so
 * "strictly newer" can never fire on a healthy pair.
 */
export function mirrorIsNewer(mirror: FreesProject | null, booted: FreesProject | null): mirror is FreesProject {
  if (!mirror) return false
  if (!booted) return true
  const m = Date.parse(mirror.savedAt)
  const b = Date.parse(booted.savedAt)
  return Number.isFinite(m) && (!Number.isFinite(b) || m > b)
}

/** Test seam: forget the cached connection so a fresh database can be opened. */
export function __resetProjectStoreForTests() {
  dbPromise = null
}
