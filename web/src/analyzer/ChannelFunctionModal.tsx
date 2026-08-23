// analyzer/ChannelFunctionModal.tsx — CSV → Function (Wave H).
//
// Turns two channels of an imported measurement into a GUI Function Table
// callable from equations: pick the x channel (default: the recording's time
// base), the y channel, and a name. Series past the table row cap
// (TABLE_MAX_ROWS) are decimated uniformly, with the note shown up front.
// Conversion is composeTables.functionSpecFromXY; this file is only the
// dialog plus the ChannelStore reads.

import { useMemo, useState } from 'react'
import { Button, Group, Modal, Select, Stack, Text, TextInput } from '@mantine/core'
import { channelStore } from './channelStore'
import type { ChannelMeta } from './types'
import { checkFunctionName, functionSpecFromXY } from '../tablesGrid/composeTables'
import { FunctionNameHints, FunctionPrecedenceNote } from '../tablesGrid/FunctionNameHints'
import { TABLE_MAX_ROWS } from '../tablesGrid/tableGridModel'
import { FunctionTableSpec, identifier, TableSpec } from '../tables'

/** Sentinel for "use the recording's time base as x". */
const TIME = '__time__'

interface Props {
  fileName: string
  measurementId: string
  channels: ChannelMeta[]
  /** For function-name collision checks (may be empty). */
  tables: TableSpec[]
  onClose: () => void
  onCreate: (spec: FunctionTableSpec) => void
}

export default function ChannelFunctionModal({
  fileName,
  measurementId,
  channels,
  tables,
  onClose,
  onCreate,
}: Readonly<Props>) {
  // String channels have no numeric samples; boolean channels are stored
  // numerically and are legitimate function data.
  const numeric = channels.filter((ch) => ch.kind !== 'string')
  const [xSel, setXSel] = useState<string>(TIME)
  const [ySel, setYSel] = useState<string>(numeric[0]?.name ?? '')
  const [name, setName] = useState<string>(() =>
    numeric[0] ? identifier(numeric[0].name, 'f').toLowerCase() : '',
  )
  const [nameTouched, setNameTouched] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const nameCheck = checkFunctionName(tables, name)

  const totalSamples = useMemo(() => {
    const range = ySel
      ? channelStore.getRawRange({ measurementId, channel: ySel }, null, null)
      : null
    return range?.t.length ?? 0
  }, [measurementId, ySel])

  const argName = xSel === TIME ? 'time' : identifier(xSel, 'x')
  const willDecimate = totalSamples > TABLE_MAX_ROWS

  const pickY = (channel: string | null) => {
    if (!channel) return
    setYSel(channel)
    if (!nameTouched) setName(identifier(channel, 'f').toLowerCase())
  }

  const create = () => {
    if (!nameCheck.ok || ySel === '') return
    const yRange = channelStore.getRawRange({ measurementId, channel: ySel }, null, null)
    if (!yRange) {
      setError('The channel data is no longer loaded — re-import the file and try again.')
      return
    }
    const xs =
      xSel === TIME
        ? yRange.t
        : channelStore.getRawRange({ measurementId, channel: xSel }, null, null)?.v
    if (!xs) {
      setError('The x channel data is no longer loaded — re-import the file and try again.')
      return
    }
    const { spec, usedRows } = functionSpecFromXY({
      name: name.trim(),
      argName,
      xs,
      ys: yRange.v,
    })
    if (usedRows === 0) {
      setError('No numeric sample pairs in the selected channels.')
      return
    }
    onCreate(spec)
    onClose()
  }

  return (
    <Modal opened onClose={onClose} title="Channel as Function Table" centered size="lg">
      <Text size="sm" c="dimmed" mb="md">
        Turns two channels of “{fileName}” into a Function Table callable from equations
        (interpolated lookup). Non-numeric sample pairs are skipped; duplicate x values keep the
        first sample.
      </Text>

      <Stack gap="sm">
        <Group grow align="flex-start">
          <Select
            label="X channel (lookup argument)"
            data={[
              { value: TIME, label: 'Time (recording time base)' },
              ...numeric.filter((ch) => ch.name !== ySel).map((ch) => ({
                value: ch.name,
                label: ch.unit ? `${ch.name} [${ch.unit}]` : ch.name,
              })),
            ]}
            value={xSel}
            onChange={(v) => v && setXSel(v)}
            allowDeselect={false}
            searchable
          />
          <Select
            label="Y channel (function values)"
            data={numeric
              .filter((ch) => ch.name !== xSel)
              .map((ch) => ({
                value: ch.name,
                label: ch.unit ? `${ch.name} [${ch.unit}]` : ch.name,
              }))}
            value={ySel}
            onChange={pickY}
            allowDeselect={false}
            searchable
          />
        </Group>

        <TextInput
          label="Function name"
          value={name}
          onChange={(e) => {
            setName(e.currentTarget.value)
            setNameTouched(true)
          }}
          error={nameCheck.error}
          spellCheck={false}
          styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
        />

        <Text size="xs" c="dimmed">
          {totalSamples.toLocaleString()} samples
          {willDecimate
            ? ` — will be decimated uniformly to at most ${TABLE_MAX_ROWS.toLocaleString()} rows (the table row cap).`
            : '.'}
          {' '}Use in equations:{' '}
          <Text span size="xs" ff="monospace">
            U = {name.trim() || 'name'}({argName})
          </Text>
        </Text>

        <FunctionNameHints name={name} check={nameCheck} />
        <FunctionPrecedenceNote />

        {error && (
          <Text c="red" size="sm">
            {error}
          </Text>
        )}

        <Group justify="flex-end" mt="xs">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button
            onClick={create}
            disabled={!nameCheck.ok || ySel === '' || totalSamples === 0}
            color={nameCheck.replacesGui ? 'yellow' : undefined}
          >
            {nameCheck.replacesGui ? 'Create (replace existing)' : 'Create function table'}
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
