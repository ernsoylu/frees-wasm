import { describe, expect, it } from 'vitest'
import { initialSyncMachine, transition, type SyncMachine } from './tableSyncMachine'

function run(events: Parameters<typeof transition>[1][]): { m: SyncMachine; runs: number } {
  let m = initialSyncMachine
  let runs = 0
  for (const ev of events) {
    const t = transition(m, ev)
    m = t.machine
    if (t.runMaterialize) {
      runs++
      m = transition(m, 'MATERIALIZE_DONE').machine
    }
  }
  return { m, runs }
}

describe('tableSyncMachine (contract b)', () => {
  it('materializes immediately when idle', () => {
    const { m, runs } = run(['MATERIALIZE_REQUEST'])
    expect(runs).toBe(1)
    expect(m.state).toBe('IDLE')
  })

  it('queues a materialization during a user edit and flushes on settle', () => {
    const { m, runs } = run(['EDIT_START', 'MATERIALIZE_REQUEST', 'EDIT_SETTLE'])
    expect(runs).toBe(1)
    expect(m.state).toBe('IDLE')
  })

  it('never materializes mid-edit', () => {
    const { m, runs } = run(['EDIT_START', 'MATERIALIZE_REQUEST', 'MATERIALIZE_REQUEST'])
    expect(runs).toBe(0)
    expect(m.state).toBe('USER_EDITING')
    expect(m.queued).toBe(true)
  })

  it('coalesces multiple queued requests into one pass', () => {
    const { runs } = run(['EDIT_START', 'MATERIALIZE_REQUEST', 'MATERIALIZE_REQUEST', 'EDIT_SETTLE'])
    expect(runs).toBe(1)
  })

  it('blur behaves like commit (edit-settle rule, Rev 3)', () => {
    // The host maps both commit and blur to EDIT_SETTLE; the machine cannot
    // tell them apart — one queued flush either way.
    const { m, runs } = run(['EDIT_START', 'MATERIALIZE_REQUEST', 'EDIT_SETTLE', 'EDIT_SETTLE'])
    expect(runs).toBe(1)
    expect(m.state).toBe('IDLE')
  })

  it('settle without a queued request just returns to idle', () => {
    const { m, runs } = run(['EDIT_START', 'EDIT_SETTLE'])
    expect(runs).toBe(0)
    expect(m.state).toBe('IDLE')
  })
})
