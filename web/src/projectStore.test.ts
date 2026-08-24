// Phase 11: the IndexedDB project library (projectStore.ts, decision D4).
//
// fake-indexeddb provides a real (structured-clone-faithful) IndexedDB per
// test, so these are behavioural tests of the storage contract, not mocks of
// it. Each test gets a fresh database via a fresh IDBFactory.

import { beforeEach, describe, expect, it } from 'vitest'
import { IDBFactory } from 'fake-indexeddb'
import { buildProject } from './project'
import type { FreesProject, ProjectSlices } from './project'
import {
  __resetProjectStoreForTests,
  clearAutosaveMirror,
  copyName,
  deleteStoredProject,
  listStoredProjects,
  loadStoredProject,
  loadStoredProjectRev,
  mirrorIsNewer,
  readAutosaveMirror,
  renameStoredProject,
  saveStoredProject,
  subscribeLibraryChanges,
  writeAutosaveMirror,
} from './projectStore'
import type { ExpectedRev, LibraryChange, SaveOutcome } from './projectStore'

/** Save and assert it was written, returning the new revision. */
async function saveOk(name: string, p: FreesProject, expected: ExpectedRev = 'overwrite'): Promise<number> {
  const outcome = await saveStoredProject(name, p, expected)
  expect(outcome.status).toBe('saved')
  return (outcome as Extract<SaveOutcome, { status: 'saved' }>).meta.rev
}

const SLICES: ProjectSlices = {
  text: 'x = 2 [m]',
  varDrafts: {},
  stopCriteria: {
    maxIterations: 100,
    relativeResiduals: 1e-6,
    changeInVariables: 1e-9,
    elapsedTimeSeconds: 30,
  },
  unitSystem: 'SI',
  fillMissing: false,
  stateUnitIds: {},
  tables: [],
  plots: [],
  spreadsheets: [],
  analyzers: [],
  sliders: [],
  schematic: {},
}

function project(text = SLICES.text, savedAt?: string): FreesProject {
  const p = buildProject({ ...SLICES, text })
  return savedAt ? { ...p, savedAt } : p
}

beforeEach(() => {
  localStorage.clear()
  globalThis.indexedDB = new IDBFactory()
  __resetProjectStoreForTests()
})

describe('the project library', () => {
  it('round-trips a saved project through the list and back', async () => {
    const outcome = await saveStoredProject('pump-sizing', project('Q = 3 [m^3/s]'), 'new')
    expect(outcome.status).toBe('saved')
    const meta = (outcome as Extract<SaveOutcome, { status: 'saved' }>).meta
    expect(meta.name).toBe('pump-sizing')
    expect(meta.size).toBeGreaterThan(0)
    expect(meta.rev).toBe(1)

    const listed = await listStoredProjects()
    expect(listed.map((m) => m.name)).toEqual(['pump-sizing'])
    expect(listed[0].rev).toBe(1)

    const loaded = await loadStoredProject('pump-sizing')
    expect(loaded?.text).toBe('Q = 3 [m^3/s]')
  })

  it('lists newest first', async () => {
    await saveOk('older', project('a = 1', '2026-08-01T00:00:00.000Z'))
    await saveOk('newer', project('b = 2', '2026-08-02T00:00:00.000Z'))
    const listed = await listStoredProjects()
    expect(listed.map((m) => m.name)).toEqual(['newer', 'older'])
  })

  it('overwrites on the same name, like saving a file', async () => {
    const rev = await saveOk('model', project('v1 = 1'), 'new')
    await saveOk('model', project('v2 = 2'), rev)
    expect((await listStoredProjects()).length).toBe(1)
    expect((await loadStoredProject('model'))?.text).toBe('v2 = 2')
  })

  it('treats "model" and "model.frees" as the same project', async () => {
    await saveOk('model.frees', project('a = 1'))
    expect((await loadStoredProject('model'))?.text).toBe('a = 1')
  })

  it('re-validates a stored project on read, exactly like a localStorage read', async () => {
    // A hostile or stale row: wrong types survive the write only if the read
    // path fails to normalize. unitSystem is the allowlisted field.
    const hostile = { ...project('a = 1'), unitSystem: 'CUBITS' } as unknown as FreesProject
    await saveOk('sus', hostile)
    const loaded = await loadStoredProject('sus')
    expect(loaded?.unitSystem).toBe('SI')
  })

  it('deletes, and deleting an absent project is a no-op', async () => {
    await saveOk('doomed', project())
    await deleteStoredProject('doomed')
    await deleteStoredProject('doomed')
    expect(await listStoredProjects()).toEqual([])
  })

  it('renames by re-keying', async () => {
    await saveOk('draft', project('a = 1'))
    expect(await renameStoredProject('draft', 'final')).toBe(true)
    expect(await loadStoredProject('draft')).toBeNull()
    expect((await loadStoredProject('final'))?.text).toBe('a = 1')
  })

  it('refuses to rename onto an existing project', async () => {
    await saveOk('a', project('a = 1'))
    await saveOk('b', project('b = 2'))
    expect(await renameStoredProject('a', 'b')).toBe(false)
    expect((await loadStoredProject('b'))?.text).toBe('b = 2')
    expect((await loadStoredProject('a'))?.text).toBe('a = 1')
  })

  it('refuses to rename a project that does not exist', async () => {
    expect(await renameStoredProject('ghost', 'anything')).toBe(false)
  })

  it('rename to the same name is a successful no-op', async () => {
    await saveOk('same', project())
    expect(await renameStoredProject('same', 'same')).toBe(true)
    expect(await loadStoredProject('same')).not.toBeNull()
  })
})

// Two tabs, one row. Before revisions, the second tab's save silently replaced
// the first's — the BroadcastChannel notice told you afterwards and that was
// all. These pin that a stale save now writes NOTHING and reports what it
// found, and that each of the three ways out actually ends the conflict.
describe('revision-checked saves', () => {
  it('stamps a monotonic revision the caller can hand back', async () => {
    expect(await saveOk('m', project('a = 1'), 'new')).toBe(1)
    expect(await saveOk('m', project('a = 2'), 1)).toBe(2)
    expect(await saveOk('m', project('a = 3'), 2)).toBe(3)
    expect((await loadStoredProjectRev('m'))?.rev).toBe(3)
  })

  it('refuses a save whose revision another tab has moved past — and writes nothing', async () => {
    // Both tabs open revision 1.
    await saveOk('shared', project('mine = 0'), 'new')
    const tabA = (await loadStoredProjectRev('shared'))!.rev
    const tabB = (await loadStoredProjectRev('shared'))!.rev
    expect(tabA).toBe(tabB)

    // Tab A saves first and wins.
    await saveOk('shared', project('from = A'), tabA)

    // Tab B still believes in revision 1.
    const outcome = await saveStoredProject('shared', project('from = B'), tabB)
    expect(outcome.status).toBe('conflict')
    expect((outcome as Extract<SaveOutcome, { status: 'conflict' }>).theirs.rev).toBe(2)
    // The decisive assertion: A's work is still there.
    expect((await loadStoredProject('shared'))?.text).toBe('from = A')
  })

  it("'new' refuses to write over a name that already exists", async () => {
    await saveOk('taken', project('theirs = 1'), 'new')
    const outcome = await saveStoredProject('taken', project('mine = 1'), 'new')
    expect(outcome.status).toBe('conflict')
    expect((await loadStoredProject('taken'))?.text).toBe('theirs = 1')
  })

  it("'overwrite' is the explicit escape hatch, and takes over the revision line", async () => {
    await saveOk('shared', project('theirs = 1'), 'new')
    await saveOk('shared', project('theirs = 2'), 1)
    const rev = await saveOk('shared', project('mine = 1'), 'overwrite')
    expect(rev).toBe(3)
    expect((await loadStoredProject('shared'))?.text).toBe('mine = 1')
    // And the overwriting tab is now in step: its next ordinary save is clean.
    await saveOk('shared', project('mine = 2'), rev)
  })

  it('re-creates a project another tab deleted rather than refusing the save', async () => {
    // Losing this tab's work to protect a row that no longer exists would be
    // the wrong trade — nobody's changes are at risk here.
    await saveOk('gone', project('a = 1'), 'new')
    const rev = (await loadStoredProjectRev('gone'))!.rev
    await deleteStoredProject('gone')
    const outcome = await saveStoredProject('gone', project('a = 2'), rev)
    expect(outcome.status).toBe('saved')
    expect((await loadStoredProject('gone'))?.text).toBe('a = 2')
  })

  it('treats a row written before revisions existed as revision 0', async () => {
    // Exactly what a Phase-11 library holds: a row with no `rev` field. A tab
    // that loads it reads 0, and saving with 0 must succeed, not conflict.
    const db = await new Promise<IDBDatabase>((resolve) => {
      const request = indexedDB.open('frees', 1)
      request.onupgradeneeded = () => {
        request.result.createObjectStore('projects', { keyPath: 'name' })
        request.result.createObjectStore('autosave')
      }
      request.onsuccess = () => resolve(request.result)
    })
    const legacy = project('legacy = 1')
    await new Promise<void>((resolve) => {
      const tx = db.transaction('projects', 'readwrite')
      tx.objectStore('projects').put({
        name: 'old',
        savedAt: legacy.savedAt,
        size: 10,
        project: legacy,
      })
      tx.oncomplete = () => resolve()
    })
    db.close()
    __resetProjectStoreForTests()

    expect((await loadStoredProjectRev('old'))?.rev).toBe(0)
    expect((await listStoredProjects())[0].rev).toBe(0)
    expect(await saveOk('old', project('legacy = 2'), 0)).toBe(1)
    expect((await loadStoredProject('old'))?.text).toBe('legacy = 2')
  })

  it('reports unavailable (never conflict) when there is no library at all', async () => {
    globalThis.indexedDB = {
      open() {
        throw new DOMException('denied', 'SecurityError')
      },
    } as unknown as IDBFactory
    __resetProjectStoreForTests()
    expect((await saveStoredProject('x', project(), 'new')).status).toBe('unavailable')
  })
})

describe('copyName', () => {
  it('is the plain copy when nothing is in the way', () => {
    expect(copyName('model', [])).toBe('model (copy)')
  })

  it('counts up past every copy already taken', () => {
    expect(copyName('model', ['model'])).toBe('model (copy)')
    expect(copyName('model', ['model', 'model (copy)'])).toBe('model (copy 2)')
    expect(copyName('model', ['model', 'model (copy)', 'model (copy 2)'])).toBe('model (copy 3)')
  })

  it('matches case-insensitively — the library is name-keyed, not case-keyed', () => {
    expect(copyName('Model', ['MODEL (COPY)'])).toBe('Model (copy 2)')
  })

  it('normalizes the stem and the taken names the way the store keys them', () => {
    expect(copyName('model.frees', ['model (copy).frees'])).toBe('model (copy 2)')
    expect(copyName('  spaced  ', [])).toBe('spaced (copy)')
    expect(copyName('', [])).toBe('untitled (copy)')
  })

  it('produces a name that then saves cleanly as new', async () => {
    await saveOk('busy', project('a = 1'), 'new')
    const copy = copyName('busy', (await listStoredProjects()).map((p) => p.name))
    expect((await saveStoredProject(copy, project('a = 2'), 'new')).status).toBe('saved')
    expect((await loadStoredProject('busy'))?.text).toBe('a = 1')
    expect((await loadStoredProject(copy))?.text).toBe('a = 2')
  })
})

describe('multi-tab coordination (Wave E)', () => {
  // In a browser a BroadcastChannel post never echoes to the *posting channel*,
  // but it does reach every other channel of the same name in the same context
  // — which is exactly what a subscriber in "another tab" is. Delivery is
  // async, so each assertion drains a macrotask first. Guarded because not
  // every jsdom build ships BroadcastChannel; the store itself degrades to a
  // silent no-op there (covered by the code path, not skipped logic).
  const hasChannel = typeof BroadcastChannel !== 'undefined'
  const settle = () => new Promise((resolve) => setTimeout(resolve, 25))

  it.runIf(hasChannel)('posts saved / deleted / renamed to other subscribers', async () => {
    const events: LibraryChange[] = []
    const unsubscribe = subscribeLibraryChanges((change) => events.push(change))
    try {
      await saveOk('Pump Sizing.frees', project('a = 1'))
      await saveOk('draft', project('b = 2'))
      await renameStoredProject('draft', 'final')
      await deleteStoredProject('final')
      await settle()
      expect(events).toEqual([
        { kind: 'saved', name: 'Pump Sizing' },
        { kind: 'saved', name: 'draft' },
        { kind: 'renamed', name: 'draft', to: 'final' },
        { kind: 'deleted', name: 'final' },
      ])
    } finally {
      unsubscribe()
    }
  })

  it.runIf(hasChannel)('unsubscribing stops delivery', async () => {
    const events: LibraryChange[] = []
    const unsubscribe = subscribeLibraryChanges((change) => events.push(change))
    unsubscribe()
    await saveOk('quiet', project())
    await settle()
    expect(events).toEqual([])
  })
})

describe('the autosave mirror', () => {
  it('round-trips and clears', async () => {
    await writeAutosaveMirror(project('mirror me'))
    expect((await readAutosaveMirror())?.text).toBe('mirror me')
    await clearAutosaveMirror()
    expect(await readAutosaveMirror()).toBeNull()
  })

  it('does not appear in the project list', async () => {
    await writeAutosaveMirror(project())
    expect(await listStoredProjects()).toEqual([])
  })
})

describe('mirrorIsNewer', () => {
  const at = (iso: string) => project('t', iso)

  it('is false with no mirror', () => {
    expect(mirrorIsNewer(null, at('2026-08-01T00:00:00.000Z'))).toBe(false)
  })

  it('is true with a mirror and no booted copy — localStorage was lost entirely', () => {
    expect(mirrorIsNewer(at('2026-08-01T00:00:00.000Z'), null)).toBe(true)
  })

  it('is false when both halves carry the same savedAt (the healthy pair)', () => {
    const iso = '2026-08-01T00:00:00.000Z'
    expect(mirrorIsNewer(at(iso), at(iso))).toBe(false)
  })

  it('is true only when the mirror is strictly newer', () => {
    expect(mirrorIsNewer(at('2026-08-02T00:00:00.000Z'), at('2026-08-01T00:00:00.000Z'))).toBe(true)
    expect(mirrorIsNewer(at('2026-08-01T00:00:00.000Z'), at('2026-08-02T00:00:00.000Z'))).toBe(false)
  })

  it('is false when the mirror timestamp is garbage', () => {
    expect(mirrorIsNewer(at('not a date'), at('2026-08-01T00:00:00.000Z'))).toBe(false)
  })
})

describe('degradation without IndexedDB', () => {
  beforeEach(() => {
    // Simulate a private mode / partitioned context where open() throws.
    globalThis.indexedDB = {
      open() {
        throw new DOMException('denied', 'SecurityError')
      },
    } as unknown as IDBFactory
    __resetProjectStoreForTests()
  })

  it('the library reads as empty and writes report failure without throwing', async () => {
    expect(await listStoredProjects()).toEqual([])
    expect(await loadStoredProject('x')).toBeNull()
    expect(await loadStoredProjectRev('x')).toBeNull()
    expect((await saveStoredProject('x', project(), 'new')).status).toBe('unavailable')
    expect(await renameStoredProject('x', 'y')).toBe(false)
    await deleteStoredProject('x')
  })

  it('the autosave mirror degrades silently', async () => {
    await writeAutosaveMirror(project())
    expect(await readAutosaveMirror()).toBeNull()
    await clearAutosaveMirror()
  })
})
