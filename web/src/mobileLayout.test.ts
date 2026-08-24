// The mobile Tables tab's panel-key contract (2026-08-24 mobile audit).
//
// The audit found two rots this file pins against recurrence: the workbook
// window id hardcoded out of sync with the bridge constant, and STATE TABLE
// panels absent from mobile while the desktop rail listed them. The resolver
// is pure; App's panelContent publishes `table:<id>` per read-only table,
// `state:<name>` per STATE TABLE block, and the single workbook window under
// TABLES_WORKBOOK_WINDOW_ID.
import { describe, expect, it } from 'vitest'

import { resolveTablePanelKey } from './MobileLayout'
import { TABLES_WORKBOOK_WINDOW_ID } from './tablesGrid/tablesWorkbookBridge'

const hasOf = (keys: string[]) => (k: string) => keys.includes(k)

describe('resolveTablePanelKey', () => {
  it('routes a read-only table to its per-table panel', () => {
    const key = resolveTablePanelKey(
      { id: 'code-param-drive', name: 'drive', state: false },
      hasOf(['table:code-param-drive', TABLES_WORKBOOK_WINDOW_ID]),
    )
    expect(key).toBe('table:code-param-drive')
  })

  it('falls back to the shared workbook for hosted (editable) tables', () => {
    const key = resolveTablePanelKey(
      { id: 'gui-1', name: 'Parametric 1', state: false },
      hasOf([TABLES_WORKBOOK_WINDOW_ID]),
    )
    expect(key).toBe(TABLES_WORKBOOK_WINDOW_ID)
  })

  it('routes a STATE TABLE entry to its own panel and never the workbook', () => {
    const has = hasOf(['state:circuit', TABLES_WORKBOOK_WINDOW_ID])
    expect(resolveTablePanelKey({ id: 'state:circuit', name: 'circuit', state: true }, has)).toBe(
      'state:circuit',
    )
    expect(
      resolveTablePanelKey({ id: 'state:gone', name: 'gone', state: true }, has),
    ).toBeNull()
  })

  it('answers null when nothing is renderable', () => {
    expect(
      resolveTablePanelKey({ id: 'gui-1', name: 'Parametric 1', state: false }, hasOf([])),
    ).toBeNull()
  })

  it('pins the workbook id to the bridge constant (persisted in saved layouts)', () => {
    // A renamed id silently drops the Tables window from saved dock layouts
    // (D10 compatibility policy) — this test makes the rename loud instead.
    expect(TABLES_WORKBOOK_WINDOW_ID).toBe('table:univer-workbook')
  })
})
