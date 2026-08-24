// Data saver: the persisted opt-out from the full precache (dataSaver.ts).
//
// The decision and the teardown are the whole feature — everything else is one
// `if` in setupPwa and one Switch in Preferences. These tests pin the parts
// that must not drift: the default is OFF (the offline guarantee is the
// headline feature), unavailable storage reads as OFF rather than throwing, and
// the teardown removes *every* registration and *every* cache while surviving
// each of them failing individually.

import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  DATA_SAVER_KEY,
  decidePrecache,
  dropPrecache,
  readDataSaver,
  writeDataSaver,
} from './dataSaver'

beforeEach(() => {
  localStorage.clear()
})

describe('the persisted setting', () => {
  it('defaults to off — full precache stays the default', () => {
    expect(readDataSaver()).toBe(false)
  })

  it('round-trips on and back off', () => {
    writeDataSaver(true)
    expect(localStorage.getItem(DATA_SAVER_KEY)).toBe('1')
    expect(readDataSaver()).toBe(true)
    writeDataSaver(false)
    expect(localStorage.getItem(DATA_SAVER_KEY)).toBeNull()
    expect(readDataSaver()).toBe(false)
  })

  it('reads anything other than the exact stored flag as off', () => {
    // A stale value from a future format, or a hostile one, must not turn the
    // offline guarantee off by accident.
    for (const value of ['true', 'yes', '0', '', '{"on":true}']) {
      localStorage.setItem(DATA_SAVER_KEY, value)
      expect(readDataSaver()).toBe(false)
    }
  })

  it('reads as off, and writes without throwing, when storage refuses', () => {
    // A private mode / partitioned context where every access throws.
    const denied = () => {
      throw new DOMException('denied', 'SecurityError')
    }
    const spyGet = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(denied)
    const spySet = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(denied)
    const spyRemove = vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(denied)
    try {
      expect(readDataSaver()).toBe(false)
      expect(() => writeDataSaver(true)).not.toThrow()
      expect(() => writeDataSaver(false)).not.toThrow()
    } finally {
      spyGet.mockRestore()
      spySet.mockRestore()
      spyRemove.mockRestore()
    }
  })
})

describe('decidePrecache', () => {
  it('registers by default', () => {
    expect(decidePrecache(false, true)).toBe('register')
  })

  it('drops when data saver is on', () => {
    expect(decidePrecache(true, true)).toBe('drop')
  })

  it('does nothing at all without service-worker support', () => {
    // An insecure origin has neither a worker to register nor one to drop —
    // and must not be told it is "saving data" when nothing was cached.
    expect(decidePrecache(true, false)).toBe('unsupported')
    expect(decidePrecache(false, false)).toBe('unsupported')
  })
})

/** A registration double whose unregister() outcome is scripted. */
function registration(result: boolean | Error) {
  return {
    unregister: vi.fn(async () => {
      if (result instanceof Error) throw result
      return result
    }),
  } as unknown as ServiceWorkerRegistration
}

describe('dropPrecache', () => {
  it('unregisters every worker and deletes every cache', async () => {
    const a = registration(true)
    const b = registration(true)
    const deleted: string[] = []
    const result = await dropPrecache({
      serviceWorker: { getRegistrations: async () => [a, b] },
      caches: {
        keys: async () => ['frees-precache-v2-http://x/', 'frees-runtime'],
        delete: async (key: string) => {
          deleted.push(key)
          return true
        },
      },
    })
    expect(result).toEqual({ unregistered: 2, cachesDeleted: 2 })
    expect(deleted).toEqual(['frees-precache-v2-http://x/', 'frees-runtime'])
  })

  it('keeps going when one registration and one cache refuse', async () => {
    const ok = registration(true)
    const angry = registration(new Error('nope'))
    const result = await dropPrecache({
      serviceWorker: { getRegistrations: async () => [angry, ok] },
      caches: {
        keys: async () => ['a', 'b'],
        delete: async (key: string) => {
          if (key === 'a') throw new Error('locked')
          return true
        },
      },
    })
    expect(result).toEqual({ unregistered: 1, cachesDeleted: 1 })
  })

  it('counts a registration that reports nothing was removed as not removed', async () => {
    const result = await dropPrecache({
      serviceWorker: { getRegistrations: async () => [registration(false)] },
      caches: { keys: async () => ['a'], delete: async () => false },
    })
    expect(result).toEqual({ unregistered: 0, cachesDeleted: 0 })
  })

  it('is a no-op where neither surface exists', async () => {
    expect(await dropPrecache({})).toEqual({ unregistered: 0, cachesDeleted: 0 })
  })

  it('survives getRegistrations() and caches.keys() rejecting', async () => {
    const result = await dropPrecache({
      serviceWorker: {
        getRegistrations: async () => {
          throw new DOMException('partitioned', 'SecurityError')
        },
      },
      caches: {
        keys: async () => {
          throw new DOMException('partitioned', 'SecurityError')
        },
        delete: async () => true,
      },
    })
    expect(result).toEqual({ unregistered: 0, cachesDeleted: 0 })
  })
})
