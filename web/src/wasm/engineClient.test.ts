// Phase 12: the worker-death path, exercised for real.
//
// Every other suite that touches engineClient mocks the module away; this one
// drives the real singleton with a fake Worker, because the fail()/respawn
// path is load-bearing for the whole product: the shipped wasm is
// panic = "abort", so an engine defect kills the worker script, and the ONLY
// recovery is that engineClient rejects everything in flight and spawns a
// fresh worker on the next call. Nothing tested that until now.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

class FakeWorker {
  static instances: FakeWorker[] = []
  onmessage: ((e: { data: unknown }) => void) | null = null
  onerror: ((e: { message?: string }) => void) | null = null
  onmessageerror: (() => void) | null = null
  posted: { id: number; method: string; args: string[] }[] = []
  terminated = false

  constructor() {
    FakeWorker.instances.push(this)
  }

  postMessage(msg: unknown) {
    this.posted.push(msg as (typeof this.posted)[number])
  }

  terminate() {
    this.terminated = true
  }
}

beforeEach(() => {
  FakeWorker.instances = []
  vi.stubGlobal('Worker', FakeWorker)
  // The singleton lives at module scope; a fresh module per test.
  vi.resetModules()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

const client = () => import('./engineClient')

describe('engineClient worker lifecycle', () => {
  it('spawns exactly one worker across many calls and correlates by id', async () => {
    const { wasmVersion, wasmCheck } = await client()
    const p1 = wasmVersion()
    const p2 = wasmCheck('x = 2', '{}')
    expect(FakeWorker.instances.length).toBe(1)
    const w = FakeWorker.instances[0]
    expect(w.posted.map((m) => m.method)).toEqual(['version', 'check'])
    // Answer out of order — correlation is by id, not arrival.
    w.onmessage?.({ data: { id: w.posted[1].id, ok: true, result: '{"solvable":true}' } })
    w.onmessage?.({ data: { id: w.posted[0].id, ok: true, result: '0.1.0' } })
    await expect(p2).resolves.toEqual({ solvable: true })
    await expect(p1).resolves.toBe('0.1.0')
  })

  it('a dead worker rejects everything in flight and the next call respawns', async () => {
    const { wasmVersion, wasmSolve } = await client()
    const p1 = wasmVersion()
    const p2 = wasmSolve('x = 2', '{}')
    const first = FakeWorker.instances[0]

    // The worker script dies (what a wasm abort looks like from outside).
    first.onerror?.({ message: 'RuntimeError: unreachable' })

    await expect(p1).rejects.toThrow('RuntimeError: unreachable')
    await expect(p2).rejects.toThrow('RuntimeError: unreachable')
    expect(first.terminated).toBe(true)

    // The next call must not hang on the corpse: a fresh worker spawns.
    const p3 = wasmVersion()
    expect(FakeWorker.instances.length).toBe(2)
    const second = FakeWorker.instances[1]
    second.onmessage?.({ data: { id: second.posted[0].id, ok: true, result: '0.1.0' } })
    await expect(p3).resolves.toBe('0.1.0')
  })

  it('an unreadable message from the worker also fails over', async () => {
    const { wasmVersion } = await client()
    const p = wasmVersion()
    FakeWorker.instances[0].onmessageerror?.()
    await expect(p).rejects.toThrow('unreadable')
    expect(FakeWorker.instances[0].terminated).toBe(true)
  })

  it('a response for an unknown id is ignored, not a crash', async () => {
    const { wasmVersion } = await client()
    const p = wasmVersion()
    const w = FakeWorker.instances[0]
    w.onmessage?.({ data: { id: 999, ok: true, result: 'stray' } })
    w.onmessage?.({ data: { id: w.posted[0].id, ok: true, result: '0.1.0' } })
    await expect(p).resolves.toBe('0.1.0')
  })

  it('an {ok:false} message rejects that one call and keeps the worker', async () => {
    const { wasmCheck, wasmVersion } = await client()
    const bad = wasmCheck('nonsense', '{}')
    const w = FakeWorker.instances[0]
    w.onmessage?.({ data: { id: w.posted[0].id, ok: false, error: 'parse failed' } })
    await expect(bad).rejects.toThrow('parse failed')
    expect(w.terminated).toBe(false)

    const good = wasmVersion()
    expect(FakeWorker.instances.length).toBe(1) // same worker, no respawn
    w.onmessage?.({ data: { id: w.posted[1].id, ok: true, result: '0.1.0' } })
    await expect(good).resolves.toBe('0.1.0')
  })
})
