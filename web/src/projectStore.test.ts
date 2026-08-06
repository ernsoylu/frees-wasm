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
  deleteStoredProject,
  listStoredProjects,
  loadStoredProject,
  mirrorIsNewer,
  readAutosaveMirror,
  renameStoredProject,
  saveStoredProject,
  writeAutosaveMirror,
} from './projectStore'

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
    const meta = await saveStoredProject('pump-sizing', project('Q = 3 [m^3/s]'))
    expect(meta).not.toBeNull()
    expect(meta!.name).toBe('pump-sizing')
    expect(meta!.size).toBeGreaterThan(0)

    const listed = await listStoredProjects()
    expect(listed.map((m) => m.name)).toEqual(['pump-sizing'])

    const loaded = await loadStoredProject('pump-sizing')
    expect(loaded?.text).toBe('Q = 3 [m^3/s]')
  })

  it('lists newest first', async () => {
    await saveStoredProject('older', project('a = 1', '2026-08-01T00:00:00.000Z'))
    await saveStoredProject('newer', project('b = 2', '2026-08-02T00:00:00.000Z'))
    const listed = await listStoredProjects()
    expect(listed.map((m) => m.name)).toEqual(['newer', 'older'])
  })

  it('overwrites on the same name, like saving a file', async () => {
    await saveStoredProject('model', project('v1 = 1'))
    await saveStoredProject('model', project('v2 = 2'))
    expect((await listStoredProjects()).length).toBe(1)
    expect((await loadStoredProject('model'))?.text).toBe('v2 = 2')
  })

  it('treats "model" and "model.frees" as the same project', async () => {
    await saveStoredProject('model.frees', project('a = 1'))
    expect((await loadStoredProject('model'))?.text).toBe('a = 1')
  })

  it('re-validates a stored project on read, exactly like a localStorage read', async () => {
    // A hostile or stale row: wrong types survive the write only if the read
    // path fails to normalize. unitSystem is the allowlisted field.
    const hostile = { ...project('a = 1'), unitSystem: 'CUBITS' } as unknown as FreesProject
    await saveStoredProject('sus', hostile)
    const loaded = await loadStoredProject('sus')
    expect(loaded?.unitSystem).toBe('SI')
  })

  it('deletes, and deleting an absent project is a no-op', async () => {
    await saveStoredProject('doomed', project())
    await deleteStoredProject('doomed')
    await deleteStoredProject('doomed')
    expect(await listStoredProjects()).toEqual([])
  })

  it('renames by re-keying', async () => {
    await saveStoredProject('draft', project('a = 1'))
    expect(await renameStoredProject('draft', 'final')).toBe(true)
    expect(await loadStoredProject('draft')).toBeNull()
    expect((await loadStoredProject('final'))?.text).toBe('a = 1')
  })

  it('refuses to rename onto an existing project', async () => {
    await saveStoredProject('a', project('a = 1'))
    await saveStoredProject('b', project('b = 2'))
    expect(await renameStoredProject('a', 'b')).toBe(false)
    expect((await loadStoredProject('b'))?.text).toBe('b = 2')
    expect((await loadStoredProject('a'))?.text).toBe('a = 1')
  })

  it('refuses to rename a project that does not exist', async () => {
    expect(await renameStoredProject('ghost', 'anything')).toBe(false)
  })

  it('rename to the same name is a successful no-op', async () => {
    await saveStoredProject('same', project())
    expect(await renameStoredProject('same', 'same')).toBe(true)
    expect(await loadStoredProject('same')).not.toBeNull()
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
    expect(await saveStoredProject('x', project())).toBeNull()
    expect(await renameStoredProject('x', 'y')).toBe(false)
    await deleteStoredProject('x')
  })

  it('the autosave mirror degrades silently', async () => {
    await writeAutosaveMirror(project())
    expect(await readAutosaveMirror()).toBeNull()
    await clearAutosaveMirror()
  })
})
