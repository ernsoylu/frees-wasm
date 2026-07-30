import { useState } from 'react'
import {
  Button,
  Checkbox,
  Group,
  Modal,
  Stack,
  Text,
} from '@mantine/core'

interface Props {
  variables: string[]
  selected: string[]
  onSave: (selected: string[]) => void
  onClose: () => void
}

export default function ConfigureTableModal({
  variables,
  selected,
  onSave,
  onClose,
}: Readonly<Props>) {
  const [local, setLocal] = useState<string[]>(selected)

  function toggle(name: string) {
    setLocal((prev) =>
      prev.includes(name)
        ? prev.filter((v) => v !== name)
        : [...prev, name]
    )
  }

  function save() {
    onSave(local)
  }

  return (
    <Modal opened onClose={onClose} title="Configure Table Columns" centered size="lg">
      <Text size="sm" c="dimmed" mb="md">
        Select which variables you want to include as columns in the parametric table.
      </Text>

      {variables.length === 0 ? (
        <Text c="dimmed" size="sm" style={{ fontStyle: 'italic' }}>
          No variables detected yet — please check your equations first to find variables.
        </Text>
      ) : (
        <Stack gap="xs" style={{ maxHeight: 300, overflowY: 'auto' }}>
          {variables.map((name) => (
            <Checkbox
              key={name}
              label={name}
              checked={local.includes(name)}
              onChange={() => toggle(name)}
            />
          ))}
        </Stack>
      )}

      <Group justify="flex-end" mt="xl">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button onClick={save} disabled={variables.length === 0}>
          Save
        </Button>
      </Group>
    </Modal>
  )
}
