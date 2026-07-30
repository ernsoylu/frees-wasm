import { Kbd, Modal, Stack, Table, Text } from '@mantine/core'

interface Props {
  opened: boolean
  onClose: () => void
}

// Detect macOS so the palette shortcut shows ⌘ instead of Ctrl.
const isMac =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)
const mod = isMac ? '⌘' : 'Ctrl'

interface Shortcut {
  keys: string[]
  action: string
}

const SHORTCUTS: Shortcut[] = [
  { keys: ['F2'], action: 'Check & Solve the system (or every table run)' },
  { keys: ['F4'], action: 'Check — validate syntax and solvability' },
  { keys: [mod, 'K'], action: 'Open the command palette' },
  { keys: [mod, 'Space'], action: 'Autocomplete functions & variables (in the editor)' },
  { keys: ['?'], action: 'Show this shortcuts list' },
]

// A read-only reference of the global keyboard shortcuts, opened with "?" or
// from the command palette — so hotkeys aren't only discoverable via the
// welcome banner and Help portal.
export default function ShortcutsModal({ opened, onClose }: Readonly<Props>) {
  return (
    <Modal opened={opened} onClose={onClose} title="Keyboard Shortcuts" size="md" centered>
      <Stack gap="sm">
        <Table verticalSpacing="xs">
          <Table.Tbody>
            {SHORTCUTS.map((s) => (
              <Table.Tr key={s.action}>
                <Table.Td style={{ width: 140, whiteSpace: 'nowrap' }}>
                  {s.keys.map((k, i) => (
                    <span key={k}>
                      {i > 0 && <Text component="span" c="dimmed" mx={4}>+</Text>}
                      <Kbd>{k}</Kbd>
                    </span>
                  ))}
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{s.action}</Text>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
        <Text size="xs" c="dimmed">
          Equations can be entered in any order and variable names are
          case-insensitive. See Help for the full language reference.
        </Text>
      </Stack>
    </Modal>
  )
}
