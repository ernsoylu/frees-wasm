// Dockview-based workspace manager.
//
// Replaces the old single-`activeTab` content area: every window kind
// (Editor, Tables, Plots, Thermo, Digitizer, Diagram, Solution) is a dockview
// panel that can be tabbed, split/docked, floated, maximized, or closed.
//
// Design: the panel *content* lives in App (closing over all its state) and is
// handed in as a `content` map keyed by window kind. A single dockview panel
// component looks its content up by kind through React context, so there is no
// prop-threading and closed panels never mount their (heavy) subtrees — only
// the element is created, not rendered, until dockview opens it.

import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewPanelProps,
  type IDockviewPanelHeaderProps,
} from 'dockview-react'
import 'dockview-react/dist/styles/dockview.css'
import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useSyncExternalStore,
  type FC,
  type ReactNode,
} from 'react'
import { useComputedColorScheme } from '@mantine/core'
import {
  IconChartGridDots,
  IconSitemap,
  IconSettings,
  IconChartLine,
  IconChecks,
  IconCode,
  IconTable,
  IconTemperature,
  IconTerminal2,
  IconVariable,
  IconWaveSine,
  IconX,
  IconGrid4x4,
  type IconProps,
} from '@tabler/icons-react'

const LAYOUT_KEY = 'frees-dock-layout-v3'

/** Latest panel content, keyed by window kind, read by every dockview panel. */
const PanelContentContext = createContext<Record<string, ReactNode>>({})

function WorkspacePanel(props: IDockviewPanelProps) {
  const content = useContext(PanelContentContext)
  // Content is keyed by panel id so multi-instance kinds (e.g. several diagram
  // windows: "diagram:<id>") each show their own content.
  return (
    <div style={{ height: '100%', width: '100%', overflow: 'auto', backgroundColor: 'var(--mantine-color-body)' }}>
      {content[props.api.id] ?? null}
    </div>
  )
}

const components = { panel: WorkspacePanel }

// Per-kind tab icon so each window's "application" is recognisable at a glance
// even when the tab is narrow and its title is clipped.
const KIND_ICONS: Record<string, FC<IconProps>> = {
  equations: IconCode,
  table: IconTable,
  plots: IconChartLine,
  plot: IconChartLine,
  digitizer: IconChartGridDots,
  schematic: IconSitemap,
  workspace: IconVariable,
  terminal: IconTerminal2,
  spreadsheet: IconGrid4x4,
  analyzer: IconWaveSine,
  states: IconTemperature,
  solution: IconChecks,
  inspector: IconSettings,
}

/** Tab renderer: a per-kind icon, the live title, and a close button.
 *  Mirrors dockview's default tab markup (so the bundled CSS applies) but
 *  prefixes the icon — the default tab ignores children, hence the rebuild. */
function WorkspaceTab(props: IDockviewPanelHeaderProps) {
  const { api } = props
  const kind = (props.params?.kind as string) ?? api.id
  const Icon = KIND_ICONS[kind]
  const title = useSyncExternalStore(
    (cb) => {
      const sub = api.onDidTitleChange(cb)
      return () => sub.dispose()
    },
    () => api.title,
    () => api.title,
  )
  return (
    <div className="dv-default-tab" data-testid="dockview-dv-default-tab">
      {Icon && <Icon size={13} style={{ flexShrink: 0, marginRight: 4 }} aria-hidden />}
      <span className="dv-default-tab-content">{title}</span>
      <button
        type="button"
        aria-label="Close"
        className="dv-default-tab-action"
        style={{ background: 'none', border: 'none', padding: 0, cursor: 'pointer', display: 'flex', alignItems: 'center', color: 'inherit' }}
        onPointerDown={(e) => e.preventDefault()}
        onClick={(e) => {
          e.preventDefault()
          api.close()
        }}
      >
        <IconX size={13} />
      </button>
    </div>
  )
}

/** A window currently open in the dock. */
export interface OpenWindow {
  id: string
  kind: string
  title: string
}

/** Named layout presets ("perspectives") a user can jump between without
 *  hand-rearranging panels after an accidental close/drag. */
export type LayoutPerspective = 'default' | 'results' | 'split'

// Each perspective is groups of tabbed center panels laid out left→right, plus
// whether the right edge group (Variable Explorer / Inspector) starts expanded.
// 'default' is not listed: it delegates to buildDefault so it always tracks the
// `defaultOpen` prop.
const PERSPECTIVES: Record<Exclude<LayoutPerspective, 'default'>, { center: string[][]; expandEdge: boolean }> = {
  // Editor beside the solved outputs (Plots tabbed with Fluid States), with the
  // Variable Explorer expanded — everything a re-solve changes, on one screen.
  results: { center: [['equations'], ['plots', 'states']], expandEdge: true },
  // A clean 50/50 Editor | Plots split; the edge group stays collapsed.
  split: { center: [['equations'], ['plots']], expandEdge: false },
}

export interface WorkspaceDockHandle {
  /** Open (or focus) a singleton window whose id equals its kind. */
  open: (kind: string) => void
  /** Open (or focus) a specific window instance (multi-instance kinds). */
  openInstance: (id: string, kind: string, title: string) => void
  /** Close a window by id. */
  close: (id: string) => void
  /** Update an open window's tab title (e.g. after a rename). */
  setTitle: (id: string, title: string) => void
  isOpen: (id: string) => boolean
  /** Discard the saved layout and reopen the default set. */
  reset: () => void
  /** Restore a previously-serialised dockview layout (from a project file). */
  restore: (layout: unknown) => void
  /** Rebuild the dock as one of the named layout presets. */
  applyPerspective: (perspective: LayoutPerspective) => void
}

interface Props {
  /** Rendered content for each window id (recomputed every App render). */
  content: Record<string, ReactNode>
  /** Tab title for each singleton kind / default window id. */
  titles: Record<string, string>
  /** Window ids to open on first run / reset, in order (first is the anchor). */
  defaultOpen: string[]
  /** Kinds rendered in the collapsible right edge group (Solution, Inspector)
   *  rather than as center tabs. */
  edgeKinds?: string[]
  onActiveChange?: (active: OpenWindow | null) => void
  onOpenChange?: (windows: OpenWindow[]) => void
  handleRef?: React.MutableRefObject<WorkspaceDockHandle | null>
}

const RIGHT_EDGE_ID = 'edge-right'

export function WorkspaceDock({
  content,
  titles,
  defaultOpen,
  edgeKinds = [],
  onActiveChange,
  onOpenChange,
  handleRef,
}: Readonly<Props>) {
  const apiRef = useRef<DockviewApi | null>(null)
  const scheme = useComputedColorScheme('dark')

  // Stable refs so the imperative handle and dockview callbacks always see the
  // latest props without being recreated.
  const titlesRef = useRef(titles)
  const defaultsRef = useRef(defaultOpen)
  const edgeKindsRef = useRef(edgeKinds)
  const cbRef = useRef({ onActiveChange, onOpenChange })

  useLayoutEffect(() => {
    titlesRef.current = titles
    defaultsRef.current = defaultOpen
    edgeKindsRef.current = edgeKinds
    cbRef.current = { onActiveChange, onOpenChange }
  })

  const kindOf = (panel: { id: string; params?: Record<string, unknown> }): string =>
    (panel.params?.kind as string) ?? panel.id

  // The collapsible right edge group hosts "chrome" panels (Solution, the
  // diagram Inspector) — created on demand and reused thereafter.
  const ensureRightEdge = (api: DockviewApi) => {
    const existing = api.getEdgeGroup('right')
    if (existing) return existing
    return api.addEdgeGroup('right', { id: RIGHT_EDGE_ID, initialSize: 400, minimumSize: 260 })
  }

  const emitOpen = (api: DockviewApi) => {
    cbRef.current.onOpenChange?.(
      api.panels.map((p) => ({ id: p.id, kind: kindOf(p), title: p.title ?? p.id })),
    )
  }

  const openInstance = (api: DockviewApi, id: string, kind: string, title: string) => {
    const existing = api.getPanel(id)
    if (existing) {
      existing.api.setActive()
      if (edgeKindsRef.current.includes(kind)) api.getEdgeGroup('right')?.expand()
      return
    }
    if (edgeKindsRef.current.includes(kind)) {
      ensureRightEdge(api)
      api.addPanel({
        id,
        component: 'panel',
        title,
        params: { kind },
        position: { referenceGroup: RIGHT_EDGE_ID },
      })
      api.getEdgeGroup('right')?.expand()
      return
    }
    // Open center windows as a tab in the main (center) group, anchored to an
    // existing center panel — never the active edge group — so plots/tables/
    // diagrams land next to the Editor instead of beside the edge panels.
    const centerRef = api.panels.find((p) => !edgeKindsRef.current.includes(kindOf(p)))
    let position: Parameters<DockviewApi['addPanel']>[0]['position']
    if (centerRef) {
      // Tab it next to an existing center panel (e.g. the Editor).
      position = { referencePanel: centerRef.id }
    } else if (api.getEdgeGroup('right')) {
      // No center panels left (e.g. the Editor was closed): a bare `undefined`
      // position would drop the panel into the active group, which is now the
      // right edge group. Anchor a fresh center group to the LEFT of the edge
      // group so the window lands in the main area, not beside the edge panels.
      position = { direction: 'left' }
    } else {
      // Empty dock: this becomes the first/root group.
      position = undefined
    }
    api.addPanel({
      id,
      component: 'panel',
      title,
      params: { kind },
      position,
    })
  }

  const buildDefault = (api: DockviewApi) => {
    api.clear()
    const ids = defaultsRef.current
    const centerIds = ids.filter((id) => !edgeKindsRef.current.includes(id))
    const edgeIds = ids.filter((id) => edgeKindsRef.current.includes(id))
    centerIds.forEach((id, i) => {
      api.addPanel({
        id,
        component: 'panel',
        title: titlesRef.current[id] ?? id,
        params: { kind: id },
        // First panel anchors; the rest dock to the right so the default
        // layout shows them side by side rather than stacked as tabs.
        position: i === 0 ? undefined : { direction: 'right' },
      })
    })
    if (edgeIds.length > 0) {
      ensureRightEdge(api)
      edgeIds.forEach((id) =>
        api.addPanel({
          id,
          component: 'panel',
          title: titlesRef.current[id] ?? id,
          params: { kind: id },
          position: { referenceGroup: RIGHT_EDGE_ID },
        }),
      )
      // Show the edge group (Variable Explorer) expanded by default rather than
      // as a collapsed rotated tab, so solved variables are visible immediately.
      api.getEdgeGroup('right')?.expand()
    }
    api.panels.find((p) => !edgeKindsRef.current.includes(kindOf(p)))?.api.setActive()
  }

  const buildPerspective = (api: DockviewApi, perspective: LayoutPerspective) => {
    if (perspective === 'default') {
      buildDefault(api)
      return
    }
    const spec = PERSPECTIVES[perspective]
    api.clear()
    spec.center.forEach((group, gi) => {
      group.forEach((id, pi) => {
        api.addPanel({
          id,
          component: 'panel',
          title: titlesRef.current[id] ?? id,
          params: { kind: id },
          // First panel of the first group anchors; later groups split to the
          // right; later panels within a group tab onto the group's first.
          position: pi > 0 ? { referencePanel: group[0] } : gi === 0 ? undefined : { direction: 'right' },
        })
      })
    })
    // The edge chrome (Variable Explorer / Inspector) is part of every
    // perspective; only whether it starts expanded varies.
    ensureRightEdge(api)
    for (const id of edgeKindsRef.current) {
      api.addPanel({
        id,
        component: 'panel',
        title: titlesRef.current[id] ?? id,
        params: { kind: id },
        position: { referenceGroup: RIGHT_EDGE_ID },
      })
    }
    if (spec.expandEdge) {
      api.getEdgeGroup('right')?.expand()
      // The last-added edge panel (Inspector) is active by default; the
      // Variable Explorer is the edge tab a results layout is about.
      api.getPanel(edgeKindsRef.current[0])?.api.setActive()
    }
    // Show each group's first tab, ending on the editor group.
    for (const group of [...spec.center].reverse()) {
      api.getPanel(group[0])?.api.setActive()
    }
  }

  // Expose the imperative handle.
  useEffect(() => {
    if (!handleRef) return
    handleRef.current = {
      open: (kind) =>
        apiRef.current &&
        openInstance(apiRef.current, kind, kind, titlesRef.current[kind] ?? kind),
      openInstance: (id, kind, title) =>
        apiRef.current && openInstance(apiRef.current, id, kind, title),
      close: (id) => {
        try {
          const p = apiRef.current?.getPanel(id)
          if (p) apiRef.current?.removePanel(p)
        } catch {
          /* ignore */
        }
      },
      setTitle: (id, title) => {
        try {
          const p = apiRef.current?.getPanel(id)
          if (p && p.title !== title) p.api.setTitle(title)
        } catch {
          /* never let a tab-title update crash the app */
        }
      },
      isOpen: (id) => !!apiRef.current?.getPanel(id),
      reset: () => {
        if (!apiRef.current) return
        localStorage.removeItem(LAYOUT_KEY)
        buildDefault(apiRef.current)
      },
      restore: (layout) => {
        if (!apiRef.current || layout == null || typeof layout !== 'object') return
        try {
          apiRef.current.fromJSON(layout as Parameters<DockviewApi['fromJSON']>[0])
        } catch {
          // Corrupt or version-mismatched layout — fall back to default.
          buildDefault(apiRef.current)
        }
      },
      applyPerspective: (perspective) => {
        if (apiRef.current) buildPerspective(apiRef.current, perspective)
      },
    }
    return () => {
      if (handleRef) handleRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api
    apiRef.current = api
    // Expose the dockview API for Playwright e2e tests
    ;(window as unknown as Record<string, unknown>).__freesTest = { dockviewApi: api }

    let restored = false
    const saved = localStorage.getItem(LAYOUT_KEY)
    if (saved) {
      try {
        api.fromJSON(JSON.parse(saved))
        restored = api.panels.length > 0
      } catch {
        restored = false
      }
    }
    if (!restored) buildDefault(api)

    api.onDidLayoutChange(() => {
      try {
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(api.toJSON()))
      } catch {
        /* quota — non-fatal; layout just won't persist */
      }
    })
    const reportActive = (panel: { id: string; title?: string; params?: Record<string, unknown> } | undefined) => {
      cbRef.current.onActiveChange?.(
        panel ? { id: panel.id, kind: kindOf(panel), title: panel.title ?? panel.id } : null,
      )
    }
    // dockview 7 wraps the panel in a { panel, origin } event object.
    api.onDidActivePanelChange(({ panel }) => reportActive(panel ?? undefined))
    const sync = () => emitOpen(api)
    api.onDidAddPanel(sync)
    api.onDidRemovePanel(sync)
    sync()
    reportActive(api.activePanel ?? undefined)
  }

  return (
    <PanelContentContext.Provider value={content}>
      {/* Align dockview's surfaces with the app's Mantine color scale (these
          vars are theme-aware, so one block serves both light and dark). */}
      <style>{`
        .dockview-theme-dark, .dockview-theme-light, .dockview-theme-abyss {
          /* One single surface colour everywhere — center groups, tab strips
             and the right edge group (Solution/Inspector) — so the whole
             workspace matches the app background instead of the edge bars
             reading as a lighter blue-grey. Active vs inactive tabs are
             distinguished by text colour and the teal active outline. */
          --dv-group-view-background-color: var(--mantine-color-body);
          --dv-tabs-and-actions-container-background-color: var(--mantine-color-body);
          --dv-activegroup-visiblepanel-tab-background-color: var(--mantine-color-body);
          --dv-activegroup-visiblepanel-tab-color: var(--mantine-color-text);
          --dv-inactivegroup-visiblepanel-tab-background-color: var(--mantine-color-body);
          --dv-inactivegroup-visiblepanel-tab-color: var(--mantine-color-dimmed);
          --dv-tabs-container-scrollbar-color: var(--mantine-color-default-border);
          --dv-tab-divider-color: var(--mantine-color-default-border);
          --dv-separator-border: var(--mantine-color-default-border);
          --dv-paneview-active-outline-color: var(--mantine-color-teal-6);
          --dv-active-sash-color: var(--mantine-color-teal-6);
          --dv-icon-hover-background-color: var(--mantine-color-default-hover);
          /* The decorative per-group accent line/chip defaults to blue
             (#1a73e8). Retint the palette to the app's teal and hide the line
             so the right edge group (Solution/Inspector) is not blue. */
          --dv-tab-group-color-blue: var(--mantine-color-teal-6);
          --dv-tab-group-color-cyan: var(--mantine-color-teal-6);
          --dv-tab-group-line-opacity: 0;
          --dv-tab-divider-color: var(--mantine-color-default-border);
        }
        /* The collapsed right edge group (rotated Solution/Inspector tabs) and
           its expanded surface render in a separate container — pin them to the
           app background too. */
        .dockview-theme-dark .dv-groupview-edge,
        .dockview-theme-light .dv-groupview-edge,
        .dockview-theme-abyss .dv-groupview-edge,
        .dockview-theme-dark .dv-edge-collapsed,
        .dockview-theme-light .dv-edge-collapsed,
        .dockview-theme-abyss .dv-edge-collapsed {
          background-color: var(--mantine-color-body);
        }
        /* When the edge group is EXPANDED, dockview rips it into a floating
           panel (.dv-resize-container) inside the overlay host. Pin that panel
           and its inner tree to the app background. Do NOT colour the
           .dv-floating-overlay-host itself: it is an absolutely-positioned
           sibling sized to the FULL grid rect (it mirrors the gridview so saved
           positions stay valid) and stacks above the panel-content render
           container — an opaque background on it paints over the entire grid and
           the whole workspace goes blank. Note dockview's hyphenated
           .dv-split-view-container (not .dv-splitview-container). */
        .dockview-theme-dark .dv-floating-overlay-host .dv-resize-container,
        .dockview-theme-light .dv-floating-overlay-host .dv-resize-container,
        .dockview-theme-abyss .dv-floating-overlay-host .dv-resize-container,
        .dockview-theme-dark .dv-floating-overlay-host .dv-split-view-container,
        .dockview-theme-light .dv-floating-overlay-host .dv-split-view-container,
        .dockview-theme-abyss .dv-floating-overlay-host .dv-split-view-container,
        .dockview-theme-dark .dv-floating-overlay-host .dv-view,
        .dockview-theme-light .dv-floating-overlay-host .dv-view,
        .dockview-theme-abyss .dv-floating-overlay-host .dv-view,
        .dockview-theme-dark .dv-groupview.dv-groupview-edge,
        .dockview-theme-light .dv-groupview.dv-groupview-edge,
        .dockview-theme-abyss .dv-groupview.dv-groupview-edge,
        .dockview-theme-dark .dv-groupview-edge > .dv-content-container,
        .dockview-theme-light .dv-groupview-edge > .dv-content-container,
        .dockview-theme-abyss .dv-groupview-edge > .dv-content-container,
        .dockview-theme-dark .dv-groupview-edge > .dv-tabs-and-actions-container,
        .dockview-theme-light .dv-groupview-edge > .dv-tabs-and-actions-container,
        .dockview-theme-abyss .dv-groupview-edge > .dv-tabs-and-actions-container {
          background-color: var(--mantine-color-body) !important;
        }
        /* Kill the floating panel's hardcoded border + shadow (the thin blue
           line on the left). Target the resize-container, not the full-grid
           overlay host (see note above). */
        .dockview-theme-dark .dv-floating-overlay-host .dv-resize-container,
        .dockview-theme-light .dv-floating-overlay-host .dv-resize-container,
        .dockview-theme-abyss .dv-floating-overlay-host .dv-resize-container {
          border-left: 1px solid var(--mantine-color-default-border) !important;
          box-shadow: none !important;
        }
        /* The resize sash (the other half of the "thin blue line") — transparent
           until hovered/active, then teal to match the app's accents. */
        .dockview-theme-dark .dv-sash,
        .dockview-theme-light .dv-sash,
        .dockview-theme-abyss .dv-sash {
          background-color: transparent !important;
        }
        .dockview-theme-dark .dv-sash:hover,
        .dockview-theme-light .dv-sash:hover,
        .dockview-theme-abyss .dv-sash:hover,
        .dockview-theme-dark .dv-sash.active,
        .dockview-theme-light .dv-sash.active,
        .dockview-theme-abyss .dv-sash.active {
          background-color: var(--mantine-color-teal-6) !important;
        }
        /* Hide the stubborn blue active-tab indicator inside the edge header. */
        .dockview-theme-dark .dv-groupview-edge .dv-tab-divider,
        .dockview-theme-light .dv-groupview-edge .dv-tab-divider,
        .dockview-theme-abyss .dv-groupview-edge .dv-tab-divider {
          background-color: transparent !important;
        }
        /* The focusable dock tabs/groups (tabindex=0) otherwise show the
           browser's default blue focus ring on the active edge group. Retint
           keyboard focus to teal and drop the ring for mouse focus. */
        .dockview-theme-dark .dv-tab:focus-visible,
        .dockview-theme-light .dv-tab:focus-visible,
        .dockview-theme-abyss .dv-tab:focus-visible,
        .dockview-theme-dark .dv-groupview:focus-visible,
        .dockview-theme-light .dv-groupview:focus-visible,
        .dockview-theme-abyss .dv-groupview:focus-visible {
          outline: 1px solid var(--mantine-color-teal-6);
          outline-offset: -1px;
        }
        .dockview-theme-dark .dv-tab:focus:not(:focus-visible),
        .dockview-theme-light .dv-tab:focus:not(:focus-visible),
        .dockview-theme-abyss .dv-tab:focus:not(:focus-visible),
        .dockview-theme-dark .dv-groupview:focus:not(:focus-visible),
        .dockview-theme-light .dv-groupview:focus:not(:focus-visible),
        .dockview-theme-abyss .dv-groupview:focus:not(:focus-visible),
        .dockview-theme-dark .dv-content-container:focus,
        .dockview-theme-light .dv-content-container:focus,
        .dockview-theme-abyss .dv-content-container:focus {
          outline: none;
        }
        /* The expanded edge group lays its content + rotated tab strip out as a
           row-reverse flex with overflow:hidden. Its content container defaults
           to min-width:auto, so a panel whose content is wider than the slot
           (e.g. the Variable Explorer's multi-column table) cannot shrink and
           overflows — clipping the LEFT side (names, title) since the strip is
           pinned right. min-width:0 lets the content shrink to the slot so inner
           scroll containers (the table) handle their own overflow instead. */
        .dockview-theme-dark .dv-groupview-edge .dv-content-container,
        .dockview-theme-light .dv-groupview-edge .dv-content-container,
        .dockview-theme-abyss .dv-groupview-edge .dv-content-container {
          min-width: 0;
        }
        /* Force the collapsed right/left edge tabs and containers to match
           the main app background and remove the default blue/grey color. */
        .dockview-theme-dark .dv-edge-collapsed,
        .dockview-theme-light .dv-edge-collapsed,
        .dockview-theme-abyss .dv-edge-collapsed,
        .dockview-theme-dark .dv-edge-collapsed .dv-tabs-and-actions-container,
        .dockview-theme-light .dv-edge-collapsed .dv-tabs-and-actions-container,
        .dockview-theme-abyss .dv-edge-collapsed .dv-tabs-and-actions-container,
        .dockview-theme-dark .dv-edge-collapsed .dv-tab,
        .dockview-theme-light .dv-edge-collapsed .dv-tab,
        .dockview-theme-abyss .dv-edge-collapsed .dv-tab,
        .dockview-theme-dark .dv-groupview-edge .dv-tab,
        .dockview-theme-light .dv-groupview-edge .dv-tab,
        .dockview-theme-abyss .dv-groupview-edge .dv-tab {
          background-color: var(--mantine-color-body) !important;
        }
      `}</style>
      <div style={{ width: '100%', height: '100%' }}>
        <DockviewReact
          className={scheme === 'light' ? 'dockview-theme-light' : 'dockview-theme-dark'}
          components={components}
          defaultTabComponent={WorkspaceTab}
          onReady={onReady}
        />
      </div>
    </PanelContentContext.Provider>
  )
}
