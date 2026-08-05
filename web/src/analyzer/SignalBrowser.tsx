// The analyzer's variable/signal browser (the "Variables" window): file
// import (CSV + solved ODE/Parametric tables), template-mode relocation
// (§2.5b), per-file time offsets, and the searchable channel list that
// assigns signals to the selected strip.
//
// Extracted from DataAnalyzerTab so the SAME browser renders in two hosts:
// embedded on the left of each analyzer window, and in the Inspector edge
// panel whenever an analyzer window is focused (the measurement-tool-style dockable
// variable-selection window). Both hosts share the AnalyzerSpec, so the
// selected strip and file list stay in sync.

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react'
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  FileButton,
  Group,
  Menu,
  Modal,
  NumberInput,
  Radio,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import {
  IconAlertTriangle,
  IconFileImport,
  IconFileSearch,
  IconPlus,
  IconSearch,
  IconTable,
  IconTrash,
} from '@tabler/icons-react'
import { channelStore } from './channelStore'
import {
  importCsvFile,
  type ImportedMeasurement,
  type TimeCandidate,
  type TimeChoice,
  type TimeKind,
} from './csvImport'
import { checkRelocatedFile } from './relocate'
import { signalColor } from './palette'
import { importableTables, tableToMeasurement } from './tableImport'
import { newStrip, type AnalyzerSpec, type ChannelMeta } from './types'
import type { TableSpec } from '../tables'

// ---------------------------------------------------------------------------
// Time-base modal (§2.5c: ambiguous/absent time column → ask, never guess)
// ---------------------------------------------------------------------------

const KIND_LABELS: Record<TimeKind, string> = {
  iso: 'ISO-8601 timestamps',
  'epoch-s': 'epoch seconds',
  'epoch-ms': 'epoch milliseconds',
  relative: 'relative seconds',
  index: 'sample index',
}

function TimeColumnModal({
  fileName,
  candidates,
  onConfirm,
  onCancel,
}: Readonly<{
  fileName: string
  candidates: TimeCandidate[]
  onConfirm: (choice: TimeChoice) => void
  onCancel: () => void
}>) {
  const [selection, setSelection] = useState<string>(
    candidates.length > 0 ? String(candidates[0].column) : 'dt',
  )
  const [dt, setDt] = useState<number | string>(0.01)

  const confirm = () => {
    if (selection === 'dt') {
      const v = typeof dt === 'number' ? dt : Number(dt)
      if (!(v > 0)) return
      onConfirm({ mode: 'dt', dt: v })
    } else {
      onConfirm({ mode: 'column', column: Number(selection) })
    }
  }

  return (
    <Modal opened onClose={onCancel} title="Select the time base" centered>
      <Stack gap="sm">
        <Text size="sm">
          The time column of “{fileName}” could not be identified unambiguously. Pick the column
          that holds time, or give a fixed sample interval for index-based data.
        </Text>
        <Radio.Group value={selection} onChange={setSelection}>
          <Stack gap={6}>
            {candidates.map((c) => (
              <Radio
                key={c.column}
                value={String(c.column)}
                label={`${c.name} — ${KIND_LABELS[c.kind]}`}
              />
            ))}
            <Radio value="dt" label="No time column — use a fixed sample interval" />
          </Stack>
        </Radio.Group>
        {selection === 'dt' && (
          <NumberInput
            label="Sample interval dt"
            suffix=" s"
            value={dt}
            onChange={setDt}
            min={1e-9}
            step={0.001}
            decimalScale={9}
          />
        )}
        <Group justify="flex-end" gap="xs">
          <Button variant="default" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={confirm}>Import</Button>
        </Group>
      </Stack>
    </Modal>
  )
}

// ---------------------------------------------------------------------------
// The browser
// ---------------------------------------------------------------------------

interface Props {
  spec: AnalyzerSpec
  /** setState-compatible mutate against the LATEST spec (never a stale copy). */
  updateSpec: (mutate: (current: AnalyzerSpec) => AnalyzerSpec) => void
  /** Solved document tables offered under “Import table” (ODE + parametric). */
  tables?: TableSpec[]
  /** Fired after a successful import/rebind (the tab resets its zoom). */
  onAfterImport?: () => void
}

export default function SignalBrowser({
  spec,
  updateSpec,
  tables,
  onAfterImport,
}: Readonly<Props>) {
  useSyncExternalStore(channelStore.subscribe, channelStore.version)
  const [search, setSearch] = useState('')
  const [importing, setImporting] = useState(false)
  const [importError, setImportError] = useState<string | null>(null)
  const [pendingTime, setPendingTime] = useState<{
    file: File
    candidates: TimeCandidate[]
    relocateId?: string
  } | null>(null)
  const [pendingAdvisory, setPendingAdvisory] = useState<{
    measurement: ImportedMeasurement
    relocateId: string
    mismatches: string[]
  } | null>(null)
  const [memWarning, setMemWarning] = useState(false)

  useEffect(
    () =>
      channelStore.subscribe((ev) => {
        if (ev === 'warn') setMemWarning(true)
      }),
    [],
  )

  /** Template-mode re-pick (§2.5b): rebind an existing measurementId to the
   *  re-imported data and refresh the stored signature. */
  const applyRelocation = useCallback(
    (measurement: ImportedMeasurement, relocateId: string) => {
      const meta = channelStore.register(measurement, spec.id, relocateId)
      updateSpec((cur) => ({
        ...cur,
        files: cur.files.map((f) =>
          f.measurementId === relocateId ? { ...f, signature: meta.signature } : f,
        ),
      }))
      onAfterImport?.()
    },
    [spec.id, updateSpec, onAfterImport],
  )

  const handleImport = useCallback(
    async (file: File | null, choice?: TimeChoice, relocateId?: string) => {
      if (file === null) return
      setImporting(true)
      setImportError(null)
      try {
        const outcome = await importCsvFile(file, choice)
        if (outcome.status === 'needs-time') {
          setPendingTime({ file, candidates: outcome.candidates, relocateId })
        } else if (relocateId !== undefined) {
          // §2.5b verification: referenced channels are mandatory; size/hash
          // differences are advisory with an explicit override.
          const required = spec.strips
            .flatMap((s) => s.signals)
            .filter((sig) => sig.measurementId === relocateId)
            .map((sig) => sig.channel)
          const stored = spec.files.find((f) => f.measurementId === relocateId)?.signature
          const check = checkRelocatedFile(
            [...new Set(required)],
            outcome.measurement.channels.map((c) => c.name),
            stored ?? { name: file.name, size: -1, headerHash: '' },
            { size: outcome.measurement.size, headerHash: outcome.measurement.headerHash },
          )
          if (check.status === 'rejected') {
            setImportError(
              `Wrong file: it is missing the channel(s) ${check.missingChannels.join(', ')} that this analyzer uses.`,
            )
          } else if (check.status === 'advisory') {
            setPendingAdvisory({ measurement: outcome.measurement, relocateId, mismatches: check.mismatches })
          } else {
            applyRelocation(outcome.measurement, relocateId)
          }
        } else {
          const meta = channelStore.register(outcome.measurement, spec.id)
          updateSpec((cur) => ({
            ...cur,
            files: [
              ...cur.files,
              { measurementId: meta.measurementId, signature: meta.signature },
            ],
          }))
          onAfterImport?.()
        }
      } catch (err) {
        setImportError(err instanceof Error ? err.message : String(err))
      } finally {
        setImporting(false)
      }
    },
    [spec, updateSpec, applyRelocation, onAfterImport],
  )

  /** Import a solved ODE/Parametric table as an in-memory measurement. */
  const importTable = (tableId: string) => {
    const table = importableTables(tables ?? []).find((t) => t.id === tableId)
    if (!table) return
    const measurement = tableToMeasurement(table)
    if (measurement === null) {
      setImportError(`Table “${table.name}” has no numeric columns to import.`)
      return
    }
    const meta = channelStore.register(measurement, spec.id)
    updateSpec((cur) => ({
      ...cur,
      files: [...cur.files, { measurementId: meta.measurementId, signature: meta.signature }],
    }))
    onAfterImport?.()
  }

  const removeFile = (measurementId: string) => {
    channelStore.release(measurementId, spec.id)
    updateSpec((cur) => ({
      ...cur,
      files: cur.files.filter((f) => f.measurementId !== measurementId),
      strips: cur.strips.map((s) => ({
        ...s,
        signals: s.signals.filter((sig) => sig.measurementId !== measurementId),
      })),
    }))
  }

  const addSignal = (measurementId: string, channel: ChannelMeta) => {
    updateSpec((cur) => {
      // Color by assignment slot, persisted per-signal (§2.5e). Target = the
      // selected strip (shared through the spec, so the Inspector-hosted
      // browser adds to the same strip), else the last one.
      const slot = cur.strips.reduce((acc, s) => acc + s.signals.length, 0)
      let strips = cur.strips
      let target =
        strips.find((s) => s.id === cur.selectedStripId) ?? strips[strips.length - 1]
      if (target === undefined) {
        target = newStrip()
        strips = [target]
      }
      if (target.signals.some((s) => s.measurementId === measurementId && s.channel === channel.name)) {
        return cur
      }
      const signal = { measurementId, channel: channel.name, color: signalColor(slot) }
      const targetId = target.id
      return {
        ...cur,
        strips: strips.map((s) => (s.id === targetId ? { ...s, signals: [...s.signals, signal] } : s)),
      }
    })
  }

  /** Per-file time offset (Phase 5a): numeric entry is the precise path. */
  const setFileOffset = (measurementId: string, offset: number) => {
    updateSpec((cur) => ({
      ...cur,
      files: cur.files.map((f) => (f.measurementId === measurementId ? { ...f, offset } : f)),
    }))
  }

  const docTables = importableTables(tables ?? [])
  const searchLower = search.trim().toLowerCase()

  return (
    <Stack gap="xs" h="100%" style={{ minHeight: 0 }}>
      <Group gap="xs" wrap="nowrap">
        <FileButton onChange={(f) => void handleImport(f)} accept=".csv,.tsv,.txt,text/csv">
          {(props) => (
            <Button
              {...props}
              size="xs"
              variant="light"
              leftSection={<IconFileImport size={14} />}
              loading={importing}
            >
              Import CSV
            </Button>
          )}
        </FileButton>
        <Menu withinPortal position="bottom-start" shadow="md" width={240}>
          <Menu.Target>
            <Button
              size="xs"
              variant="default"
              leftSection={<IconTable size={14} />}
              disabled={docTables.length === 0}
            >
              Table
            </Button>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>Import a solved table</Menu.Label>
            {docTables.map((t) => (
              <Menu.Item key={t.id} onClick={() => importTable(t.id)}>
                <Group gap={6} wrap="nowrap">
                  <Text size="xs" truncate style={{ flex: 1 }}>
                    {t.name}
                  </Text>
                  <Badge size="xs" variant="light" color={t.origin === 'ode' ? 'teal' : 'gray'}>
                    {t.origin === 'ode' ? 'ODE' : 'param'}
                  </Badge>
                </Group>
              </Menu.Item>
            ))}
          </Menu.Dropdown>
        </Menu>
      </Group>
      <TextInput
        size="xs"
        placeholder="Search signals…"
        leftSection={<IconSearch size={13} />}
        value={search}
        onChange={(e) => setSearch(e.currentTarget.value)}
      />
      {importError !== null && (
        <Alert
          color="red"
          icon={<IconAlertTriangle size={14} />}
          withCloseButton
          onClose={() => setImportError(null)}
          p="xs"
        >
          <Text size="xs">{importError}</Text>
        </Alert>
      )}
      {memWarning && (
        <Alert
          color="yellow"
          icon={<IconAlertTriangle size={14} />}
          withCloseButton
          onClose={() => setMemWarning(false)}
          p="xs"
        >
          <Text size="xs">
            Measurement cache is large (&gt;50M cells). Least-recently-used files not shown in an
            open analyzer may be evicted.
          </Text>
        </Alert>
      )}
      <ScrollArea style={{ flex: 1 }} type="auto">
        <Stack gap="sm">
          {spec.files.length === 0 && (
            <Text size="xs" c="dimmed" ta="center" mt="lg">
              Import a CSV/TSV measurement file — or a solved ODE/Parametric table — to browse
              its signals.
            </Text>
          )}
          {spec.files.map((f) => {
            const meta = channelStore.getMeta(f.measurementId)
            const loaded = channelStore.isLoaded(f.measurementId)
            return (
              <Box key={f.measurementId}>
                <Group justify="space-between" gap={4} wrap="nowrap">
                  <Text size="xs" fw={600} truncate title={f.signature.name}>
                    {f.signature.name}
                  </Text>
                  <Tooltip label="Remove file from this analyzer">
                    <ActionIcon
                      size="xs"
                      variant="subtle"
                      color="gray"
                      onClick={() => removeFile(f.measurementId)}
                    >
                      <IconTrash size={12} />
                    </ActionIcon>
                  </Tooltip>
                </Group>
                {meta !== null && loaded && (
                  <Group gap={6} wrap="nowrap" justify="space-between">
                    <Text size="xs" c="dimmed">
                      {meta.totalSamples.toLocaleString()} samples · {meta.channels.length} channels
                    </Text>
                    {/* Per-file time offset (Phase 5a): numeric entry is the
                        precise path; SHIFT-drag on a strip adjusts it too. */}
                    <NumberInput
                      size="xs"
                      w={100}
                      hideControls
                      value={f.offset ?? 0}
                      step={0.1}
                      decimalScale={9}
                      prefix="Δt "
                      suffix=" s"
                      aria-label={`Time offset for ${f.signature.name}`}
                      onChange={(v) =>
                        setFileOffset(f.measurementId, typeof v === 'number' ? v : Number(v) || 0)
                      }
                    />
                  </Group>
                )}
                {!loaded && (
                  // Template mode (§2.5b): the layout survived the project
                  // round-trip; the samples did not. One re-pick repopulates
                  // every strip bound to this file.
                  <Alert color="orange" p={6} mt={4} icon={<IconAlertTriangle size={14} />}>
                    <Stack gap={6}>
                      <Text size="xs">
                        Measurement data is not loaded ({(f.signature.size / 1e6).toFixed(1)} MB
                        file).
                      </Text>
                      <FileButton
                        onChange={(nf) => void handleImport(nf, undefined, f.measurementId)}
                        accept=".csv,.tsv,.txt,text/csv"
                      >
                        {(props) => (
                          <Button
                            {...props}
                            size="compact-xs"
                            variant="light"
                            color="orange"
                            leftSection={<IconFileSearch size={13} />}
                          >
                            Locate file…
                          </Button>
                        )}
                      </FileButton>
                    </Stack>
                  </Alert>
                )}
                {loaded &&
                  meta?.channels
                    .filter((ch) => searchLower === '' || ch.name.toLowerCase().includes(searchLower))
                    .map((ch) => (
                      <Group key={ch.name} justify="space-between" gap={4} wrap="nowrap" py={1}>
                        <Group gap={4} wrap="nowrap" style={{ overflow: 'hidden' }}>
                          <Text size="xs" truncate title={ch.name}>
                            {ch.name}
                          </Text>
                          {ch.unit !== undefined && (
                            <Text size="xs" c="dimmed">
                              [{ch.unit}]
                            </Text>
                          )}
                          {ch.kind !== 'analog' && (
                            <Badge size="xs" variant="light" color={ch.kind === 'boolean' ? 'teal' : 'gray'}>
                              {ch.kind === 'boolean' ? 'bool' : 'str'}
                            </Badge>
                          )}
                        </Group>
                        <Tooltip
                          label={
                            ch.kind === 'string'
                              ? 'String channels cannot be plotted (known limitation)'
                              : 'Add to the selected strip'
                          }
                        >
                          <ActionIcon
                            size="xs"
                            variant="subtle"
                            disabled={ch.kind === 'string'}
                            onClick={() => addSignal(f.measurementId, ch)}
                          >
                            <IconPlus size={12} />
                          </ActionIcon>
                        </Tooltip>
                      </Group>
                    ))}
              </Box>
            )
          })}
        </Stack>
      </ScrollArea>

      {pendingTime !== null && (
        <TimeColumnModal
          fileName={pendingTime.file.name}
          candidates={pendingTime.candidates}
          onConfirm={(choice) => {
            const { file, relocateId } = pendingTime
            setPendingTime(null)
            void handleImport(file, choice, relocateId)
          }}
          onCancel={() => setPendingTime(null)}
        />
      )}

      {pendingAdvisory !== null && (
        <Modal opened onClose={() => setPendingAdvisory(null)} title="File differs from the saved reference" centered>
          <Stack gap="sm">
            <Text size="sm">
              The picked file has all the channels this analyzer uses, but its{' '}
              {pendingAdvisory.mismatches.join(' and ')} differ
              {pendingAdvisory.mismatches.length === 1 ? 's' : ''} from the file the project was
              saved with. It may be a different recording.
            </Text>
            <Group justify="flex-end" gap="xs">
              <Button variant="default" onClick={() => setPendingAdvisory(null)}>
                Cancel
              </Button>
              <Button
                color="orange"
                onClick={() => {
                  const { measurement, relocateId } = pendingAdvisory
                  setPendingAdvisory(null)
                  applyRelocation(measurement, relocateId)
                }}
              >
                Use anyway
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}
    </Stack>
  )
}
