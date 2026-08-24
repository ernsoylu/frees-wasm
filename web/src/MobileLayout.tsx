import { helpUrl } from './helpUrl'
import { useState, useEffect, ReactNode } from 'react'
import { Flex, Group, Title, UnstyledButton, Text, Box, Paper, ActionIcon, Menu } from '@mantine/core'
import {
  IconMathFunction,
  IconVariable,
  IconChartLine,
  IconTable,
  IconTerminal2,
  IconSettings,
  IconDatabase,
  IconDeviceFloppy,
  IconChecks,
  IconFolderOpen,
  IconFilePlus,
  IconHelp,
  IconBook,
  IconTargetArrow
} from '@tabler/icons-react'
import { TableSpec } from './tables'
import { TABLES_WORKBOOK_WINDOW_ID } from './tablesGrid/tablesWorkbookBridge'

/** One selectable entry in the mobile Tables tab. */
export interface MobileTableEntry {
  id: string
  name: string
  /** True for a read-only STATE TABLE panel (`state:<name>`). */
  state: boolean
}

/** The panelContent key the mobile Tables tab renders for an entry, or null
 * when nothing is available. Pure and exported so the test can pin the key
 * contract against App's panelContent ids — the 2026-08-24 audit found the
 * old inline lookup silently ignoring the tapped table (workbook selection is
 * App-level) and STATE TABLE panels missing from mobile entirely. */
export function resolveTablePanelKey(
  entry: MobileTableEntry,
  has: (key: string) => boolean,
): string | null {
  if (entry.state) return has(entry.id) ? entry.id : null
  if (has(`table:${entry.id}`)) return `table:${entry.id}`
  return has(TABLES_WORKBOOK_WINDOW_ID) ? TABLES_WORKBOOK_WINDOW_ID : null
}

/**
 * The smallest layout-vs-visual viewport gap that means "an on-screen keyboard
 * is up" rather than "browser chrome is mid-animation". A software keyboard is
 * never under ~200 px tall; a collapsing URL bar is ~50–60.
 */
const KEYBOARD_MIN_PX = 64

/**
 * The mobile shell's root height. Pure and exported so the contract is pinned
 * by a test rather than by a device nobody has to hand.
 *
 * `100dvh` follows the *layout* viewport, and iOS Safari does not shrink that
 * for the on-screen keyboard — so a bottom-anchored control (the REPL prompt,
 * the tab bar) ends up underneath it, which is exactly the failure the fifth
 * Terminal tab would otherwise ship with. `visualViewport.height` is the one
 * signal that does shrink. Switch to it only once the gap is larger than any
 * browser-chrome animation, so the URL-bar collapse is not fought with JS.
 */
export function resolveShellHeight(visual: number | null, layout: number): string {
  if (visual == null || !Number.isFinite(visual) || visual <= 0) return '100dvh'
  if (!Number.isFinite(layout) || layout <= 0) return '100dvh'
  return layout - visual >= KEYBOARD_MIN_PX ? `${Math.round(visual)}px` : '100dvh'
}

/** Track `resolveShellHeight` against the live visual viewport. */
function useShellHeight(): string {
  const [height, setHeight] = useState('100dvh')
  useEffect(() => {
    const viewport = globalThis.visualViewport
    if (!viewport) return
    const update = () => setHeight(resolveShellHeight(viewport.height, globalThis.innerHeight))
    update()
    viewport.addEventListener('resize', update)
    viewport.addEventListener('scroll', update)
    return () => {
      viewport.removeEventListener('resize', update)
      viewport.removeEventListener('scroll', update)
    }
  }, [])
  return height
}

interface MobileLayoutProps {
  panelContent: Record<string, ReactNode>
  tables: TableSpec[]
  /** Declared STATE TABLE blocks — read-only fluid-state panels
   * (`state:<name>` in panelContent). The desktop rail lists these beside the
   * tables; the mobile Tables tab now does too (2026-08-24 audit: they were
   * unreachable on mobile while the solve produced them). */
  stateTables: { id: string; name: string }[]
  /** The App-level active table (the Tables workbook follows THIS — the
   * workbook is one panel hosting many editable tables, so a local-only
   * selection could never switch it; 2026-08-24 audit fix). */
  activeTableId: string | null
  onActiveTableId: (id: string) => void
  /** Plot windows, shown in the Plots tab (each renders panelContent['plot:<id>']). */
  plots: { id: string; name: string }[]
  projectName: string
  checking: boolean
  solving: boolean
  onCheck: () => void
  onSolve: () => Promise<'workspace' | 'table' | void> | void
  checkingTableId: string | null
  solvingTableId: string | null
  onCheckTable: (id: string) => void
  onSolveTable: (id: string) => Promise<boolean> | void
  onSaveProject: () => void
  onSaveProjectAs: () => void
  onNewProject: () => void
  onOpenProject: () => void
  /** The browser-resident project library (Phase 11, D4). */
  onOpenLibrary: () => void
  onPreferences: () => void
  onRenameProject: () => void
  onOpenExamples: () => void
}

export default function MobileLayout({
  panelContent,
  tables,
  stateTables,
  activeTableId,
  onActiveTableId,
  plots,
  projectName,
  checking,
  solving,
  onCheck,
  onSolve,
  checkingTableId,
  solvingTableId,
  onCheckTable,
  onSolveTable,
  onSaveProject,
  onSaveProjectAs,
  onNewProject,
  onOpenProject,
  onOpenLibrary,
  onPreferences,
  onRenameProject,
  onOpenExamples,
}: MobileLayoutProps) {
  const [activeTab, setActiveTab] = useState<'equations' | 'workspace' | 'plots' | 'table' | 'terminal'>(
    'equations',
  )
  const shellHeight = useShellHeight()
  // One entry per selectable panel in the Tables tab: editable/code tables
  // first (TableSpec ids), then the read-only STATE TABLE panels.
  const tableEntries = [
    ...tables.map((t) => ({ id: t.id, name: t.name, state: false })),
    ...stateTables.map((st) => ({ id: st.id, name: st.name, state: true })),
  ]
  const [selectedEntryId, setSelectedEntryId] = useState<string | null>(
    activeTableId ?? (tableEntries.length > 0 ? tableEntries[0].id : null),
  )
  const [activePlotId, setActivePlotId] = useState<string | null>(plots.length > 0 ? plots[0].id : null)

  useEffect(() => {
    if (tableEntries.length > 0 && (!selectedEntryId || !tableEntries.find((e) => e.id === selectedEntryId))) {
      setSelectedEntryId(tableEntries[tableEntries.length - 1].id)
    } else if (tableEntries.length === 0 && selectedEntryId !== null) {
      setSelectedEntryId(null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tables, stateTables, selectedEntryId])

  useEffect(() => {
    if (plots.length > 0 && (!activePlotId || !plots.find((p) => p.id === activePlotId))) {
      setActivePlotId(plots[plots.length - 1].id)
    } else if (plots.length === 0 && activePlotId !== null) {
      setActivePlotId(null)
    }
  }, [plots, activePlotId])

  const TABS = [
    { id: 'equations', label: 'Equations', icon: IconMathFunction },
    { id: 'workspace', label: 'Variables', icon: IconVariable },
    { id: 'plots', label: 'Plots', icon: IconChartLine },
    { id: 'table', label: 'Tables', icon: IconTable },
    // The one desktop-only surface worth having on a phone: the REPL is the
    // fastest way to interrogate a solved workspace, and it costs no new
    // plumbing — App already publishes panelContent['terminal'].
    { id: 'terminal', label: 'Terminal', icon: IconTerminal2 },
  ] as const

  let content: ReactNode = null
  if (activeTab === 'equations') content = panelContent['equations']
  if (activeTab === 'workspace') content = panelContent['workspace']
  if (activeTab === 'terminal') content = panelContent['terminal']
  if (activeTab === 'plots') {
    if (plots.length === 0) {
      content = <Box p="md"><Text c="dimmed">No plots yet. Add one from a table (select columns → Plot curve) or with a PLOT block in code.</Text></Box>
    } else {
      const pId = activePlotId ?? plots[0].id
      content = panelContent[`plot:${pId}`] || <Box p="md"><Text c="dimmed">Plot not found.</Text></Box>
    }
  }
  if (activeTab === 'table') {
    if (tableEntries.length === 0) {
      content = <Box p="md"><Text c="dimmed">No tables available.</Text></Box>
    } else {
      const entry = tableEntries.find((e) => e.id === selectedEntryId) ?? tableEntries[0]
      // STATE TABLE panels are keyed directly; editable tables live in the
      // single Tables workbook window (which follows the App-level
      // activeTableId); read-only code/ODE tables have per-table panels.
      const key = resolveTablePanelKey(entry, (k) => panelContent[k] != null)
      content = (key && panelContent[key]) || <Box p="md"><Text c="dimmed">Table not found.</Text></Box>
    }
  }

  const selectedTableId =
    selectedEntryId && tables.some((t) => t.id === selectedEntryId) ? selectedEntryId : null
  const isTableActive = activeTab === 'table' && selectedTableId !== null
  const isChecking = isTableActive && selectedTableId ? checkingTableId === selectedTableId : checking
  const isSolving = isTableActive && selectedTableId ? solvingTableId === selectedTableId : solving

  return (
    <Flex direction="column" h={shellHeight} style={{ overflow: 'hidden' }}>
      {/* Top Bar */}
      <Paper
        shadow="xs"
        radius={0}
        style={{
          borderBottom: '1px solid var(--mantine-color-default-border)',
          backgroundColor: 'var(--mantine-color-body)',
          zIndex: 10,
          paddingTop: 'calc(env(safe-area-inset-top, 0px) + 16px)',
          paddingBottom: '16px',
          paddingLeft: '16px',
          paddingRight: '16px'
        }}
      >
        <Group justify="space-between" align="center">
          <div>
            <UnstyledButton onClick={onRenameProject}>
              <Title order={5} c="teal" lineClamp={1}>
                {projectName}
              </Title>
            </UnstyledButton>
            <Text size="xs" c="dimmed">
              {activeTab === 'table' ? 'Tables' : TABS.find((t) => t.id === activeTab)?.label}
            </Text>
          </div>
          <Group gap="xs">
            <ActionIcon
              variant="light"
              color="teal"
              loading={isChecking}
              onClick={() => {
                if (isTableActive && selectedTableId) {
                  onCheckTable(selectedTableId)
                } else {
                  onCheck()
                }
              }}
              title="Check"
            >
              <IconChecks size={18} />
            </ActionIcon>
            <ActionIcon
              variant="filled"
              color="teal"
              loading={isSolving}
              onClick={async () => {
                let res: 'workspace' | 'table' | void | boolean
                if (isTableActive && selectedTableId) {
                  res = await onSolveTable(selectedTableId)
                } else {
                  res = await onSolve()
                }
                if (res === 'workspace' || res === 'table') {
                  setActiveTab(res)
                }
              }}
              title="Solve"
            >
              <IconTargetArrow size={18} />
            </ActionIcon>
            <Menu position="bottom-end">
              <Menu.Target>
                <ActionIcon variant="subtle" color="gray" aria-label="Menu" title="Menu">
                  <IconSettings size={18} />
                </ActionIcon>
              </Menu.Target>
              <Menu.Dropdown>
                <Menu.Item
                  leftSection={<IconFilePlus size={14} />}
                  onClick={onNewProject}
                >
                  New Project
                </Menu.Item>
                <Menu.Item
                  leftSection={<IconFolderOpen size={14} />}
                  onClick={onOpenProject}
                >
                  Open Project
                </Menu.Item>
                <Menu.Item
                  leftSection={<IconDatabase size={14} />}
                  onClick={onOpenLibrary}
                >
                  Browser Projects
                </Menu.Item>
                <Menu.Item
                  leftSection={<IconDeviceFloppy size={14} />}
                  onClick={onSaveProject}
                >
                  Save Project
                </Menu.Item>
                <Menu.Item
                  leftSection={<IconDeviceFloppy size={14} />}
                  onClick={onSaveProjectAs}
                >
                  Save Project As...
                </Menu.Item>
                <Menu.Divider />
                <Menu.Item
                  leftSection={<IconSettings size={14} />}
                  onClick={onPreferences}
                >
                  Preferences
                </Menu.Item>
                <Menu.Divider />
                <Menu.Item
                  leftSection={<IconBook size={14} />}
                  onClick={onOpenExamples}
                >
                  Examples
                </Menu.Item>
                <Menu.Item
                  component="a"
                  href={helpUrl()}
                  target="_blank"
                  leftSection={<IconHelp size={14} />}
                >
                  Help
                </Menu.Item>
              </Menu.Dropdown>
            </Menu>
          </Group>
        </Group>
      </Paper>

      {/* Main Content Area — the editor and the terminal fill edge-to-edge
          (each carries its own padding and its own internal scroller); other
          tabs keep a comfortable inset and scroll here. The terminal must NOT
          scroll here: its prompt is bottom-anchored inside a height:100% flex
          column, so an outer scroller would push the prompt off-screen instead
          of scrolling only the output. */}
      <Box
        style={{ flex: 1, minHeight: 0, overflowY: activeTab === 'terminal' ? 'hidden' : 'auto' }}
        p={activeTab === 'equations' || activeTab === 'terminal' ? 0 : 'xs'}
      >
        {activeTab === 'table' && tableEntries.length > 1 && (
          <Group gap="xs" mb="sm" style={{ overflowX: 'auto', flexWrap: 'nowrap' }}>
            {tableEntries.map((e) => (
              <ActionIcon
                key={e.id}
                variant={(selectedEntryId ?? tableEntries[0].id) === e.id ? 'filled' : 'light'}
                color={e.state ? 'blue' : 'teal'}
                onClick={() => {
                  setSelectedEntryId(e.id)
                  // The workbook panel shows the App-level active table, so a
                  // hosted (editable) table must be selected THERE too — a
                  // local-only selection left the workbook on the old table.
                  if (!e.state && tables.some((t) => t.id === e.id)) onActiveTableId(e.id)
                }}
                size="lg"
                title={e.name}
              >
                <Text size="xs" fw={700}>
                  {e.name.slice(0, 2).toUpperCase()}
                </Text>
              </ActionIcon>
            ))}
          </Group>
        )}
        {activeTab === 'plots' && plots.length > 1 && (
          <Group gap="xs" mb="sm" style={{ overflowX: 'auto', flexWrap: 'nowrap' }}>
            {plots.map((p) => (
              <ActionIcon
                key={p.id}
                variant={activePlotId === p.id ? 'filled' : 'light'}
                color="teal"
                onClick={() => setActivePlotId(p.id)}
                size="lg"
                title={p.name}
              >
                <Text size="xs" fw={700}>
                  {p.name.slice(0, 2).toUpperCase()}
                </Text>
              </ActionIcon>
            ))}
          </Group>
        )}
        {content}
      </Box>

      {/* Bottom Navigation */}
      <Paper
        shadow="lg"
        radius={0}
        style={{
          borderTop: '1px solid var(--mantine-color-default-border)',
          backgroundColor: 'var(--mantine-color-body)',
          paddingBottom: 'calc(env(safe-area-inset-bottom, 0px) + 8px)',
          paddingTop: '8px'
        }}
      >
        <Group grow gap={0} align="center">
          {TABS.map((tab) => {
            const Icon = tab.icon
            const isActive = activeTab === tab.id
            return (
              <UnstyledButton
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                aria-label={tab.label}
                aria-current={isActive ? 'page' : undefined}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  // Five tabs at 375 px leave ~75 px each; without minWidth the
                  // longest label ("Equations") would widen its own column and
                  // starve the rest.
                  minWidth: 0,
                  color: isActive ? 'var(--mantine-color-teal-filled)' : 'var(--mantine-color-text)',
                  opacity: isActive ? 1 : 0.6,
                  transition: 'opacity 0.2s, color 0.2s',
                }}
              >
                <Box
                  p={5}
                  style={{
                    backgroundColor: isActive ? 'var(--mantine-color-teal-light)' : 'transparent',
                    borderRadius: '12px',
                    marginBottom: '4px',
                  }}
                >
                  <Icon size={22} stroke={isActive ? 2.5 : 1.5} />
                </Box>
                <Text size="0.62rem" fw={isActive ? 600 : 400} style={{ whiteSpace: 'nowrap' }}>
                  {tab.label}
                </Text>
              </UnstyledButton>
            )
          })}
        </Group>
      </Paper>
    </Flex>
  )
}
