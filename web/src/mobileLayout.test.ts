// The mobile Tables tab's panel-key contract (2026-08-24 mobile audit).
//
// The audit found two rots this file pins against recurrence: the workbook
// window id hardcoded out of sync with the bridge constant, and STATE TABLE
// panels absent from mobile while the desktop rail listed them. The resolver
// is pure; App's panelContent publishes `table:<id>` per read-only table,
// `state:<name>` per STATE TABLE block, and the single workbook window under
// TABLES_WORKBOOK_WINDOW_ID.
import { describe, expect, it } from 'vitest'

import { resolveShellHeight, resolveTablePanelKey } from './MobileLayout'
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

// The mobile Terminal tab (the REPL as a fifth tab) is the first surface with a
// bottom-anchored text input, so it is the first that the on-screen keyboard
// can cover. iOS Safari does not shrink the layout viewport for the keyboard,
// which means `100dvh` is unchanged while the bottom ~300 px are hidden — this
// is the rule that swaps in the visual viewport instead, without reacting to an
// ordinary URL-bar collapse.
describe('resolveShellHeight', () => {
  it('stays on 100dvh when there is no visual viewport to consult', () => {
    expect(resolveShellHeight(null, 812)).toBe('100dvh')
  })

  it('stays on 100dvh when the viewports agree', () => {
    expect(resolveShellHeight(812, 812)).toBe('100dvh')
  })

  it('stays on 100dvh through a URL-bar collapse (a ~60 px gap)', () => {
    expect(resolveShellHeight(752, 812)).toBe('100dvh')
    // The boundary itself is keyboard-sized and does switch.
    expect(resolveShellHeight(748, 812)).toBe('748px')
  })

  it('follows the visual viewport once a keyboard is up', () => {
    // iPhone 13 (812 tall) with the software keyboard: ~336 px of viewport.
    expect(resolveShellHeight(476, 812)).toBe('476px')
  })

  it('rounds a fractional viewport height to a whole pixel', () => {
    expect(resolveShellHeight(475.6, 812)).toBe('476px')
  })

  it('refuses nonsense rather than collapsing the shell to nothing', () => {
    expect(resolveShellHeight(0, 812)).toBe('100dvh')
    expect(resolveShellHeight(-10, 812)).toBe('100dvh')
    expect(resolveShellHeight(Number.NaN, 812)).toBe('100dvh')
    expect(resolveShellHeight(476, 0)).toBe('100dvh')
    expect(resolveShellHeight(476, Number.NaN)).toBe('100dvh')
  })
})
