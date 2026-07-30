import { useState, useMemo } from 'react'
import { Badge, Card, Group, Modal, Select, SimpleGrid, Stack, Text } from '@mantine/core'
import { EXAMPLES, Example } from './examples'

interface Props {
  opened: boolean
  onClose: () => void
  /** Load the chosen example's document into the editor. */
  onSelect: (example: Example) => void
}

// A gallery of ready-to-solve examples. Picking one replaces the editor
// document; the user can then press Solve (F2) to see a full result.
export default function ExamplesModal({ opened, onClose, onSelect }: Readonly<Props>) {
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null)

  const categories = useMemo(() => {
    const cats = new Set(EXAMPLES.map((ex) => ex.category))
    return Array.from(cats).sort((a, b) => a.localeCompare(b))
  }, [])

  const filteredExamples = useMemo(() => {
    if (!selectedCategory) {
      return EXAMPLES
    }
    return EXAMPLES.filter((ex) => ex.category === selectedCategory)
  }, [selectedCategory])

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Open an Example"
      size="calc(100vw - 40px)"
      centered
    >
      <Stack gap="sm">
        <Group justify="space-between" align="flex-start">
          <Text size="sm" c="dimmed" style={{ flex: 1, maxWidth: '80%' }}>
            Pick a worked example to load into the editor, then press{' '}
            <strong>Solve</strong> (F2). Each one is ready to run — edit the inputs
            to explore.
          </Text>
          <Select
            placeholder="All Categories"
            data={['All Categories', ...categories]}
            value={selectedCategory || 'All Categories'}
            onChange={(val) => setSelectedCategory(val === 'All Categories' ? null : val)}
            style={{ width: 200 }}
          />
        </Group>
        <SimpleGrid cols={{ base: 1, sm: 2, md: 3, lg: 4, xl: 5 }} spacing="sm">
          {filteredExamples.map((ex) => (
            <Card
              key={ex.id}
              withBorder
              padding="sm"
              radius="md"
              role="button"
              tabIndex={0}
              style={{ cursor: 'pointer' }}
              onClick={() => onSelect(ex)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  onSelect(ex)
                }
              }}
            >
              <Group justify="space-between" wrap="nowrap" mb={4} align="flex-start">
                <Text fw={600} size="sm">
                  {ex.title}
                </Text>
                <Badge variant="light" size="xs" style={{ flexShrink: 0 }}>
                  {ex.category}
                </Badge>
              </Group>
              <Text size="xs" c="dimmed">
                {ex.description}
              </Text>
            </Card>
          ))}
        </SimpleGrid>
      </Stack>
    </Modal>
  )
}
