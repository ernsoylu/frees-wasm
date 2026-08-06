// Phase 11: the browser-resident project library (decision D4).
//
// IndexedDB holds two things localStorage cannot: a *library* of named
// projects (localStorage has one slot) and a *durable* autosave mirror
// (localStorage's ~5 MB quota makes the existing autosave silently stop
// updating once spreadsheets grow past it). The
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
/** Single-row store for the autosave mirror. */
const AUTOSAVE_STORE = 'autosave'
const AUTOSAVE_KEY = 'workspace'

/** What the library list shows; the document itself stays on disk until opened. */
export interface StoredProjectMeta {
  name: string
  savedAt: string
  /** Serialized size in bytes — the list UI's honesty about quota. */
  size: number
}

interface ProjectRow extends StoredProjectMeta {
  project: FreesProject
}

let dbPromise: Promise<IDBDatabase | null> | null = null

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
      .map(({ name, savedAt, size }) => ({ name, savedAt, size }))
      .sort((a, b) => (a.savedAt < b.savedAt ? 1 : a.savedAt > b.savedAt ? -1 : 0))
  } catch {
    return []
  }
}

/** Read one project back, re-validated exactly like a localStorage read. */
export async function loadStoredProject(name: string): Promise<FreesProject | null> {
  const db = await openDb()
  if (!db) return null
  try {
    const row = await await_(
      db.transaction(PROJECTS_STORE, 'readonly').objectStore(PROJECTS_STORE).get(normalizeName(name)) as IDBRequest<
        ProjectRow | undefined
      >,
    )
    return row ? normalizeStoredProject(row.project) : null
  } catch {
    return null
  }
}

/**
 * Save under `name`, overwriting any existing project of that name — the same
 * semantics as saving a file. Returns the stored metadata, or null when the
 * library is unavailable (callers surface that; a failed *explicit* save must
 * never be silent).
 */
export async function saveStoredProject(name: string, project: FreesProject): Promise<StoredProjectMeta | null> {
  const db = await openDb()
  if (!db) return null
  const safe = normalizeStoredProject(project)
  if (!safe) return null
  const row: ProjectRow = {
    name: normalizeName(name),
    savedAt: safe.savedAt,
    size: JSON.stringify(safe).length,
    project: safe,
  }
  try {
    await await_(db.transaction(PROJECTS_STORE, 'readwrite').objectStore(PROJECTS_STORE).put(row))
    return { name: row.name, savedAt: row.savedAt, size: row.size }
  } catch {
    return null
  }
}

export async function deleteStoredProject(name: string): Promise<void> {
  const db = await openDb()
  if (!db) return
  try {
    await await_(db.transaction(PROJECTS_STORE, 'readwrite').objectStore(PROJECTS_STORE).delete(normalizeName(name)))
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
    return true
  } catch {
    return false
  }
}

/**
 * The durable half of the debounced autosave. Best-effort by the same rule as
 * the localStorage half: autosave must never interrupt the user.
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
