import { useState, useMemo } from 'react'
import {
  Alert, Box, Button, Code, Divider, Grid, Group, Modal, Stack, Text, TextInput, Textarea, Badge,
} from '@mantine/core'
import { parseMapData, buildTableBlock, isValidTableName } from './mapTable'

interface Props {
  opened: boolean
  onClose: () => void
  defaultName: string
  onConfirm: (result: { tableName: string; block: string }) => void
}

const PLACEHOLDER = `Paste a performance map (whitespace or comma separated).

1-D (arg  value):
  0.00  250
  0.05  180
  0.10   60

2-D (header row = family values):
  rpm   0.25  0.5   1.0
  1000  320   300   290
  3000  280   260   250`

// Ingests pasted/typed map data, detects 1-D vs 2-D + family axis, lets the user
// confirm names/units, and emits a frees TABLE block.
export default function MapBuilderModal({ opened, onClose, defaultName, onConfirm }: Readonly<Props>) {
  const [raw, setRaw] = useState('')
  const [name, setName] = useState(defaultName)
  const [argName, setArgName] = useState('x')
  const [argUnit, setArgUnit] = useState('')
  const [outUnit, setOutUnit] = useState('')
  const [famName, setFamName] = useState('param')
  const [familyStr, setFamilyStr] = useState('')

  const parsed = useMemo(() => {
    if (raw.trim() === '') return { error: '' as string, data: null }
    try {
      return { error: '', data: parseMapData(raw) }
    } catch (e) {
      return { error: (e as Error).message, data: null }
    }
  }, [raw])

  // Prefill the editable family field from the parsed values the first time.
  const detectedFamily = parsed.data?.kind === '2d' ? parsed.data.family : []
  const effectiveFamilyStr = familyStr !== '' ? familyStr : detectedFamily.join(', ')

  const block = useMemo(() => {
    if (!parsed.data) return ''
    const family = parsed.data.kind === '2d'
      ? effectiveFamilyStr.split(/[\s,]+/).filter(Boolean).map(Number)
      : []
    return buildTableBlock({
      name, argName, argUnit, outUnit,
      famName, family, rows: parsed.data.rows,
    })
  }, [parsed.data, name, argName, argUnit, outUnit, famName, effectiveFamilyStr])

  const nameOk = isValidTableName(name)
  const canAdd = !!parsed.data && nameOk

  return (
    <Modal opened={opened} onClose={onClose} title="Build a performance map (TABLE)" size="xl" centered>
      <Stack gap="sm">
        <Textarea
          label="Map data"
          description="Paste rows of numbers. A non-numeric leading header row supplies 2-D family values."
          placeholder={PLACEHOLDER}
          value={raw}
          onChange={(e) => setRaw(e.currentTarget.value)}
          autosize minRows={6} maxRows={12}
          styles={{ input: { fontFamily: 'monospace' } }}
        />
        {parsed.error && <Alert color="red" variant="light">{parsed.error}</Alert>}
        {parsed.data && (
          <Group gap="xs">
            <Badge variant="light">{parsed.data.kind.toUpperCase()} map</Badge>
            <Text size="xs" c="dimmed">{parsed.data.rows.length} rows</Text>
          </Group>
        )}

        <Grid gap="xs">
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <TextInput size="xs" label="Table name" value={name}
              onChange={(e) => setName(e.currentTarget.value)}
              error={name && !nameOk ? 'Invalid identifier' : undefined} />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <TextInput size="xs" label="Argument name" value={argName}
              onChange={(e) => setArgName(e.currentTarget.value)} />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <TextInput size="xs" label="Argument unit" placeholder="m^3/s" value={argUnit}
              onChange={(e) => setArgUnit(e.currentTarget.value)} />
          </Grid.Col>
          <Grid.Col span={{ base: 12, sm: 4 }}>
            <TextInput size="xs" label="Output unit" placeholder="Pa" value={outUnit}
              onChange={(e) => setOutUnit(e.currentTarget.value)} />
          </Grid.Col>
          {parsed.data?.kind === '2d' && (
            <>
              <Grid.Col span={{ base: 12, sm: 4 }}>
                <TextInput size="xs" label="Family axis name" value={famName}
                  onChange={(e) => setFamName(e.currentTarget.value)} />
              </Grid.Col>
              <Grid.Col span={{ base: 12, sm: 4 }}>
                <TextInput size="xs" label="Family values" value={effectiveFamilyStr}
                  onChange={(e) => setFamilyStr(e.currentTarget.value)} />
              </Grid.Col>
            </>
          )}
        </Grid>

        <Divider />
        <Box>
          <Text size="xs" fw={600} c="dimmed" mb={4}>Preview</Text>
          <Code block>{block || '— paste map data above —'}</Code>
        </Box>
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>Cancel</Button>
          <Button disabled={!canAdd} onClick={() => { onConfirm({ tableName: name, block }); onClose() }}>
            Use this map
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
