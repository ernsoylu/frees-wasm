import { useCallback, useEffect, useState } from 'react'
import { ActionIcon, Button, Group, Modal, Stack, Table, Text, Tooltip } from '@mantine/core'
import { IconDeviceFloppy, IconFolderOpen, IconPencil, IconTrash } from '@tabler/icons-react'
import { TextPromptModal } from './dialogs'
import type { StoredProjectMeta } from './projectStore'
import { deleteStoredProject, listStoredProjects, renameStoredProject, subscribeLibraryChanges } from './projectStore'

// Phase 11: the browser project library (decision D4). Projects saved here
// live in this browser's IndexedDB — no server, no files to juggle. The modal
// owns the list and its mutations (rename/delete); opening and saving go
// through App, which owns the workspace.

function formatWhen(iso: string): string {
  const t = Date.parse(iso)
  return Number.isFinite(t) ? new Date(t).toLocaleString() : '—'
}

function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'
  if (bytes < 1024) return `${bytes} B`
  const kb = bytes / 1024
  return kb < 1024 ? `${kb.toFixed(1)} KB` : `${(kb / 1024).toFixed(1)} MB`
}

export function ProjectLibraryModal({
  opened,
  currentName,
  onClose,
  onSaveCurrent,
  onOpenProject,
}: Readonly<{
  opened: boolean
  /** The workspace's project name — what "Save current" saves under. */
  currentName: string
  onClose: () => void
  /**
   * Save the live workspace under its current name. `'conflict'` means another
   * tab moved the row on and App has raised its own resolution dialog — the
   * one outcome this modal must NOT report as a storage failure.
   */
  onSaveCurrent: () => Promise<'saved' | 'conflict' | 'unavailable'>
  /** Load a stored project into the workspace (App applies its own dirty guard). */
  onOpenProject: (name: string) => void
}>) {
  const [projects, setProjects] = useState<StoredProjectMeta[]>([])
  const [renaming, setRenaming] = useState<string | null>(null)
  /** Name whose delete button is armed; a second click within the timeout deletes. */
  const [armedDelete, setArmedDelete] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(() => {
    void listStoredProjects().then(setProjects)
  }, [])

  // Reset the transient state each time the modal opens (state-adjustment
  // during render, not an effect — see react.dev "you might not need an
  // effect"); the list refresh is an external read and stays effect-shaped.
  const [prevOpened, setPrevOpened] = useState(opened)
  if (opened !== prevOpened) {
    setPrevOpened(opened)
    if (opened) {
      setError(null)
      setArmedDelete(null)
    }
  }
  useEffect(() => {
    if (opened) refresh()
  }, [opened, refresh])

  // Wave E: another tab saved/deleted/renamed — refresh the listing live
  // while this modal is open (BroadcastChannel never echoes to the sender).
  useEffect(() => {
    if (!opened) return
    return subscribeLibraryChanges(() => refresh())
  }, [opened, refresh])

  // Disarm the delete confirmation after a beat — a destructive second click
  // must be deliberate, not a double-click landing on a moved button.
  useEffect(() => {
    if (armedDelete === null) return
    const id = setTimeout(() => setArmedDelete(null), 3000)
    return () => clearTimeout(id)
  }, [armedDelete])

  const handleSaveCurrent = useCallback(async () => {
    const outcome = await onSaveCurrent()
    if (outcome === 'unavailable') {
      setError('Could not save to browser storage — it may be unavailable in this browsing mode.')
      return
    }
    setError(null)
    // A conflict wrote nothing, but the listing is stale either way — the other
    // tab's save is exactly what the refresh should show.
    refresh()
  }, [onSaveCurrent, refresh])

  const handleDelete = useCallback(
    async (name: string) => {
      if (armedDelete !== name) {
        setArmedDelete(name)
        return
      }
      setArmedDelete(null)
      await deleteStoredProject(name)
      refresh()
    },
    [armedDelete, refresh],
  )

  const submitRename = useCallback(
    async (to: string) => {
      const from = renaming
      setRenaming(null)
      if (!from) return
      const clean = to.trim()
      if (!clean || clean === from) return
      const ok = await renameStoredProject(from, clean)
      if (!ok) setError(`Could not rename — “${clean}” may already exist.`)
      refresh()
    },
    [renaming, refresh],
  )

  return (
    <>
      <Modal opened={opened} onClose={onClose} title="Browser Projects" size="lg" centered>
        <Stack gap="md">
          <Group justify="space-between" align="center">
            <Text size="sm" c="dimmed">
              Saved in this browser — no server involved. Use Save Project for a .frees file you can move
              between machines.
            </Text>
            <Button
              size="xs"
              leftSection={<IconDeviceFloppy size={14} />}
              onClick={() => void handleSaveCurrent()}
            >
              Save “{currentName}”
            </Button>
          </Group>
          {error && (
            <Text size="sm" c="yellow.5">
              {error}
            </Text>
          )}
          {projects.length === 0 ? (
            <Text size="sm" c="dimmed" ta="center" py="lg">
              No projects saved in this browser yet.
            </Text>
          ) : (
            <Table verticalSpacing="xs" highlightOnHover>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Name</Table.Th>
                  <Table.Th>Saved</Table.Th>
                  <Table.Th>Size</Table.Th>
                  <Table.Th aria-label="actions" />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {projects.map((p) => (
                  <Table.Tr key={p.name}>
                    <Table.Td>
                      <Text size="sm" fw={500}>
                        {p.name}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm" c="dimmed">
                        {formatWhen(p.savedAt)}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm" c="dimmed">
                        {formatSize(p.size)}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Group gap="xs" justify="flex-end" wrap="nowrap">
                        <Button
                          size="compact-xs"
                          variant="light"
                          leftSection={<IconFolderOpen size={14} />}
                          onClick={() => onOpenProject(p.name)}
                        >
                          Open
                        </Button>
                        <Tooltip label="Rename">
                          <ActionIcon variant="subtle" color="gray" onClick={() => setRenaming(p.name)}>
                            <IconPencil size={14} />
                          </ActionIcon>
                        </Tooltip>
                        <Tooltip label={armedDelete === p.name ? 'Click again to delete' : 'Delete'}>
                          <ActionIcon
                            variant={armedDelete === p.name ? 'filled' : 'subtle'}
                            color="red"
                            onClick={() => void handleDelete(p.name)}
                          >
                            <IconTrash size={14} />
                          </ActionIcon>
                        </Tooltip>
                      </Group>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Stack>
      </Modal>
      <TextPromptModal
        opened={renaming !== null}
        title="Rename Browser Project"
        label="New name"
        defaultValue={renaming ?? ''}
        confirmLabel="Rename"
        onSubmit={(v) => void submitRename(v)}
        onClose={() => setRenaming(null)}
      />
    </>
  )
}
