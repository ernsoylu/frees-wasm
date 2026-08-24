// Wave I: the pickerless-re-save decision matrix (Phase 11 gap 2, the
// FileSystemFileHandle half). `saveTarget` is pure; the two async helpers are
// graded against mock handles, since jsdom has no File System Access API —
// which is itself one of the cases (no API => picker, i.e. pre-Wave-I
// behavior).
import { describe, expect, it, vi } from 'vitest'
import { queryWritePermission, saveTarget, writeToHandle, type HandlePermission } from './saveTarget'

describe('saveTarget', () => {
  it('a browser-library project always re-saves to the library', () => {
    const perms: HandlePermission[] = ['granted', 'prompt', 'denied', 'unsupported']
    for (const p of perms) {
      expect(saveTarget('browser', true, p)).toBe('library')
      expect(saveTarget('browser', false, p)).toBe('library')
    }
  })

  it('a file project with a permitted handle writes back to the file', () => {
    expect(saveTarget('file', true, 'granted')).toBe('handle')
    // 'prompt' is still a handle save: requestPermission runs inside the
    // Save click and the flow falls back to the picker only on refusal.
    expect(saveTarget('file', true, 'prompt')).toBe('handle')
  })

  it('denied or unsupported permission degrades a file project to the picker', () => {
    expect(saveTarget('file', true, 'denied')).toBe('picker')
    expect(saveTarget('file', true, 'unsupported')).toBe('picker')
  })

  it('a file project without a handle keeps meaning the picker', () => {
    expect(saveTarget('file', false, 'granted')).toBe('picker')
    expect(saveTarget('file', false, 'unsupported')).toBe('picker')
  })

  it('a never-saved project picks, even if a stale handle lingers', () => {
    expect(saveTarget(null, false, 'unsupported')).toBe('picker')
    expect(saveTarget(null, true, 'granted')).toBe('picker')
  })
})

// A structurally-complete fake: only the members the helpers touch.
function fakeHandle(overrides: Record<string, unknown>): FileSystemFileHandle {
  return overrides as unknown as FileSystemFileHandle
}

describe('queryWritePermission', () => {
  it('returns the handle-reported state', async () => {
    for (const state of ['granted', 'prompt', 'denied'] as const) {
      const handle = fakeHandle({ queryPermission: vi.fn().mockResolvedValue(state) })
      await expect(queryWritePermission(handle)).resolves.toBe(state)
    }
  })

  it('asks for readwrite, not read', async () => {
    const queryPermission = vi.fn().mockResolvedValue('granted')
    await queryWritePermission(fakeHandle({ queryPermission }))
    expect(queryPermission).toHaveBeenCalledWith({ mode: 'readwrite' })
  })

  it('reports unsupported when the API is missing (Firefox/Safari) or throws', async () => {
    await expect(queryWritePermission(fakeHandle({}))).resolves.toBe('unsupported')
    const throwing = fakeHandle({ queryPermission: vi.fn().mockRejectedValue(new Error('nope')) })
    await expect(queryWritePermission(throwing)).resolves.toBe('unsupported')
  })
})

describe('writeToHandle', () => {
  function writable() {
    const write = vi.fn().mockResolvedValue(undefined)
    const close = vi.fn().mockResolvedValue(undefined)
    return { write, close, createWritable: vi.fn().mockResolvedValue({ write, close }) }
  }

  it('writes straight through when permission is already granted', async () => {
    const w = writable()
    const handle = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('granted'),
      requestPermission: vi.fn(),
      createWritable: w.createWritable,
    })
    await expect(writeToHandle(handle, '{"a":1}')).resolves.toBe('saved')
    expect(w.write).toHaveBeenCalledWith('{"a":1}')
    expect(w.close).toHaveBeenCalled()
    expect((handle as unknown as { requestPermission: unknown }).requestPermission).not.toHaveBeenCalled()
  })

  it("requests permission on 'prompt' and saves when the user grants", async () => {
    const w = writable()
    const requestPermission = vi.fn().mockResolvedValue('granted')
    const handle = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('prompt'),
      requestPermission,
      createWritable: w.createWritable,
    })
    await expect(writeToHandle(handle, 'x')).resolves.toBe('saved')
    expect(requestPermission).toHaveBeenCalledWith({ mode: 'readwrite' })
  })

  it('reports denied — and never opens a writable — when the user refuses', async () => {
    const w = writable()
    const handle = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('prompt'),
      requestPermission: vi.fn().mockResolvedValue('denied'),
      createWritable: w.createWritable,
    })
    await expect(writeToHandle(handle, 'x')).resolves.toBe('denied')
    expect(w.createWritable).not.toHaveBeenCalled()
  })

  it('reports denied when permission was already denied, without re-prompting', async () => {
    const requestPermission = vi.fn()
    const handle = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('denied'),
      requestPermission,
      createWritable: vi.fn(),
    })
    await expect(writeToHandle(handle, 'x')).resolves.toBe('denied')
    expect(requestPermission).not.toHaveBeenCalled()
  })

  it('reports denied when the permission API is absent or requestPermission throws', async () => {
    // Absent entirely (a foreign object out of IndexedDB, an OPFS handle):
    await expect(writeToHandle(fakeHandle({ createWritable: vi.fn() }), 'x')).resolves.toBe('denied')
    const throwing = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('prompt'),
      requestPermission: vi.fn().mockRejectedValue(new Error('gesture required')),
      createWritable: vi.fn(),
    })
    await expect(writeToHandle(throwing, 'x')).resolves.toBe('denied')
  })

  it('reports failed when the write itself throws (file moved or deleted)', async () => {
    const handle = fakeHandle({
      queryPermission: vi.fn().mockResolvedValue('granted'),
      createWritable: vi.fn().mockRejectedValue(new DOMException('gone', 'NotFoundError')),
    })
    await expect(writeToHandle(handle, 'x')).resolves.toBe('failed')
  })
})
