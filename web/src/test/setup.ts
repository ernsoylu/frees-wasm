// Global test setup: registers @testing-library/jest-dom matchers (toBeInTheDocument,
// toHaveTextContent, …) and cleans up the DOM between tests.
import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'

// Node >=26 ships experimental Web Storage, which puts a `localStorage` GETTER on
// globalThis that returns undefined unless the process was started with
// `--localstorage-file`. Under vitest's jsdom environment `window` *is* globalThis,
// so that getter shadows jsdom's own localStorage and every store access becomes
// "Cannot read properties of undefined" — `window.localStorage` included. Node 20/24
// never define the global, so jsdom's wins there and nothing here fires.
//
// The Dockerfile builds on node:26-alpine, so leaving this unhandled makes the suite
// unrunnable on the runtime that ships the bundle. The descriptor is configurable,
// so replace it with an in-memory Storage when it resolves to undefined.
if (globalThis.localStorage == null) {
  const store = new Map<string, string>()
  const storage: Storage = {
    get length() {
      return store.size
    },
    key: (i: number) => [...store.keys()][i] ?? null,
    getItem: (k: string) => store.get(String(k)) ?? null,
    setItem: (k: string, v: string) => void store.set(String(k), String(v)),
    removeItem: (k: string) => void store.delete(String(k)),
    clear: () => store.clear(),
  }
  Object.defineProperty(globalThis, 'localStorage', {
    value: storage,
    configurable: true,
    writable: true,
  })
}

afterEach(() => {
  cleanup()
})
