import { useEffect, useState } from 'react'
import { Button, Group, Modal, Stack, Text, TextInput } from '@mantine/core'

// Mantine replacements for the native browser prompt()/confirm()/alert(),
// so project dialogs match the dark theme and stay keyboard/focus accessible.

export function TextPromptModal({
  opened,
  title,
  label,
  defaultValue,
  confirmLabel = 'OK',
  onSubmit,
  onClose,
}: Readonly<{
  opened: boolean
  title: string
  label: string
  defaultValue: string
  confirmLabel?: string
  onSubmit: (value: string) => void
  onClose: () => void
}>) {
  const [value, setValue] = useState(defaultValue)

  // Reseed the field each time the modal is (re)opened.
  useEffect(() => {
    if (opened) setValue(defaultValue)
  }, [opened, defaultValue])

  function submit() {
    onSubmit(value)
  }

  return (
    <Modal opened={opened} onClose={onClose} title={title} centered>
      <Stack gap="md">
        <TextInput
          label={label}
          value={value}
          data-autofocus
          onChange={(e) => setValue(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              submit()
            }
          }}
        />
        <Group justify="flex-end" gap="xs">
          <Button variant="default" size="xs" onClick={onClose}>
            Cancel
          </Button>
          <Button size="xs" onClick={submit}>
            {confirmLabel}
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}

/** "Save before closing?" — three-way choice: Save / Don't Save / Cancel. */
export function SaveCheckModal({
  opened,
  projectName,
  onSave,
  onDiscard,
  onCancel,
}: Readonly<{
  opened: boolean
  projectName: string
  onSave: () => void
  onDiscard: () => void
  onCancel: () => void
}>) {
  return (
    <Modal opened={opened} onClose={onCancel} title="Unsaved Changes" centered>
      <Stack gap="md">
        <Text size="sm">
          <strong>{projectName}</strong> has unsaved changes. Save before proceeding?
        </Text>
        <Group justify="flex-end" gap="xs">
          <Button variant="default" size="xs" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="default" size="xs" color="red" onClick={onDiscard}>
            Don&apos;t Save
          </Button>
          <Button size="xs" color="teal" onClick={onSave}>
            Save
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}

/** "Open the shared document?" — asked only when a `#share=` link would
 *  replace a *different* autosaved workspace. This used to be a bare
 *  `globalThis.confirm()`, which rendered as a browser-chrome dialog rather
 *  than the app's own UI, and blocked the boot render while it was open. */
export function SharedLinkModal({
  opened,
  onOpenShared,
  onCancel,
}: Readonly<{
  opened: boolean
  onOpenShared: () => void
  onCancel: () => void
}>) {
  return (
    <Modal opened={opened} onClose={onCancel} title="Open shared document" centered>
      <Stack gap="md">
        <Text size="sm">
          This link carries a complete document. Opening it replaces your current autosaved
          workspace — the same as loading an example. Nothing was sent to a server.
        </Text>
        <Group justify="flex-end" gap="xs">
          <Button variant="default" size="xs" onClick={onCancel}>
            Keep my workspace
          </Button>
          <Button size="xs" color="teal" onClick={onOpenShared} data-autofocus>
            Open shared document
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}

/**
 * "This browser project changed in another tab" — the resolution offered when
 * a library save is refused because the stored revision is no longer the one
 * this tab loaded. Nothing has been written when this opens, and nothing is
 * written unless one of the three actions is taken; Cancel leaves both copies
 * exactly as they are.
 */
export function ProjectConflictModal({
  opened,
  projectName,
  theirSavedAt,
  onOverwrite,
  onSaveCopy,
  onTakeTheirs,
  onCancel,
}: Readonly<{
  opened: boolean
  projectName: string
  /** When the other tab saved, already formatted for display. */
  theirSavedAt: string
  onOverwrite: () => void
  onSaveCopy: () => void
  onTakeTheirs: () => void
  onCancel: () => void
}>) {
  return (
    <Modal opened={opened} onClose={onCancel} title="Saved in another tab" centered size="lg">
      <Stack gap="md">
        <Text size="sm">
          <strong>{projectName}</strong> was saved in another tab ({theirSavedAt}) after this one
          opened it. Nothing has been written — choose what to keep.
        </Text>
        <Stack gap="xs">
          <Button variant="light" color="teal" onClick={onSaveCopy} data-autofocus>
            Save as a copy
          </Button>
          <Text size="xs" c="dimmed">
            Keeps both. This window&apos;s work is stored under a new name.
          </Text>
          <Button variant="light" color="red" onClick={onOverwrite}>
            Overwrite theirs
          </Button>
          <Text size="xs" c="dimmed">
            Replaces the other tab&apos;s version with this window&apos;s. Their changes are lost.
          </Text>
          <Button variant="light" color="yellow" onClick={onTakeTheirs}>
            Discard mine, load theirs
          </Button>
          <Text size="xs" c="dimmed">
            Loads the stored version into this window. This window&apos;s unsaved changes are lost.
          </Text>
        </Stack>
        <Group justify="flex-end">
          <Button variant="default" size="xs" onClick={onCancel}>
            Cancel
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}

export function MessageModal({
  opened,
  title,
  message,
  onClose,
}: Readonly<{
  opened: boolean
  title: string
  message: string
  onClose: () => void
}>) {
  return (
    <Modal opened={opened} onClose={onClose} title={title} centered>
      <Stack gap="md">
        <Text size="sm" style={{ whiteSpace: 'pre-wrap' }}>
          {message}
        </Text>
        <Group justify="flex-end">
          <Button size="xs" onClick={onClose} data-autofocus>
            OK
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
