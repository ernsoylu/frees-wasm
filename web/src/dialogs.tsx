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
