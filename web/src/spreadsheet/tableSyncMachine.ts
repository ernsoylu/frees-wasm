// spreadsheet/tableSyncMachine.ts
//
// Contract b of the table-spreadsheet unification plan (todo.md): spec→sheet
// materialization and user edits share one small state machine so solver /
// external writes can never clobber a cell mid-keystroke. Pure transition
// function — the host owns the side effects (running the materializer when a
// transition says so).
//
// Edit-settle rule (Rev 3): USER_EDITING exits on commit (debounced sync
// flushed) OR blur (focus left the workbook); either flushes a queued
// materialization, so an abandoned open cell cannot pin the queue forever.

export type SyncState = 'IDLE' | 'USER_EDITING' | 'MATERIALIZING'

export interface SyncMachine {
  state: SyncState
  /** A materialization arrived while busy; run it when we settle. */
  queued: boolean
}

export type SyncEvent =
  | 'EDIT_START' // cell editor opened / first mutation of a burst
  | 'EDIT_SETTLE' // commit (debounce flushed) or blur
  | 'MATERIALIZE_REQUEST' // external spec change wants to write the sheet
  | 'MATERIALIZE_DONE'

export interface Transition {
  machine: SyncMachine
  /** The host must run one materialization pass now. */
  runMaterialize: boolean
}

export const initialSyncMachine: SyncMachine = { state: 'IDLE', queued: false }

export function transition(m: SyncMachine, ev: SyncEvent): Transition {
  switch (ev) {
    case 'EDIT_START':
      // Materialization is synchronous and fast (ms); a user edit landing
      // during it keeps MATERIALIZING and is picked up by the next sync.
      if (m.state === 'MATERIALIZING') return { machine: m, runMaterialize: false }
      return { machine: { ...m, state: 'USER_EDITING' }, runMaterialize: false }
    case 'EDIT_SETTLE':
      if (m.state !== 'USER_EDITING') return { machine: m, runMaterialize: false }
      if (m.queued) return { machine: { state: 'MATERIALIZING', queued: false }, runMaterialize: true }
      return { machine: { state: 'IDLE', queued: false }, runMaterialize: false }
    case 'MATERIALIZE_REQUEST':
      if (m.state === 'IDLE') return { machine: { state: 'MATERIALIZING', queued: false }, runMaterialize: true }
      return { machine: { ...m, queued: true }, runMaterialize: false }
    case 'MATERIALIZE_DONE':
      if (m.queued) return { machine: { state: 'MATERIALIZING', queued: false }, runMaterialize: true }
      return { machine: { state: 'IDLE', queued: false }, runMaterialize: false }
  }
}
