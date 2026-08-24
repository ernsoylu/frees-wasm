// tablesGrid/ImportCsvModal.tsx — Import CSV… → Function Table (decision D11).
//
// The Data Analyzer's CSV import, relocated into the Tables workbook and
// pointed at the destination that made the Analyzer redundant: a measured
// series becomes a GUI Function Table, callable straight from the equations
// (`U = speed(time)`), instead of a strip chart you could only look at.
//
// The conversion is composeTables.functionSpecFromXY — the very function
// Wave H's ChannelFunctionModal called, so the row cap, the uniform
// decimation, the duplicate-x rule and the skipped-pair count are the same
// behaviour they always were. Reading is tablesGrid/csv.parseCsvTable.

import { useMemo, useState } from 'react'
import { Button, FileInput, Group, Modal, Select, Stack, Text, TextInput } from '@mantine/core'
import { IconFileTypeCsv } from '@tabler/icons-react'
import { FunctionTableSpec, identifier, TableSpec } from '../tables'
import { checkFunctionName, functionSpecFromXY } from './composeTables'
import { FunctionNameHints, FunctionPrecedenceNote } from './FunctionNameHints'
import { parseCsvTable, type CsvTable } from './csv'
import { TABLE_MAX_ROWS } from './tableGridModel'

/** Whole-file read cap. A function table holds 5 000 rows, so a recording
 *  bigger than this is one to trim before importing — and reading it as a
 *  single string would cost far more memory than the table it produces. */
const MAX_BYTES = 64 * 1024 * 1024

interface Props {
  /** For function-name collision checks (may be empty). */
  tables: TableSpec[]
  onClose: () => void
  onCreate: (spec: FunctionTableSpec) => void
}

interface Loaded {
  fileName: string
  table: CsvTable
}

export default function ImportCsvModal({ tables, onClose, onCreate }: Readonly<Props>) {
  const [loaded, setLoaded] = useState<Loaded | null>(null)
  const [reading, setReading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [xIndex, setXIndex] = useState<number | null>(null)
  const [yIndex, setYIndex] = useState<number | null>(null)
  const [name, setName] = useState('')
  const [nameTouched, setNameTouched] = useState(false)

  /** Columns with at least one number in them — a text column cannot be an
   *  axis, and listing it only invites a confusing empty result. */
  const numericColumns = loaded ? loaded.table.columns.filter((c) => c.numericCount > 0) : []
  const columnAt = (index: number | null) =>
    index === null ? null : (loaded?.table.columns[index] ?? null)
  const xColumn = columnAt(xIndex)
  const yColumn = columnAt(yIndex)

  const pickFile = (file: File | null) => {
    setError(null)
    setLoaded(null)
    setXIndex(null)
    setYIndex(null)
    if (!file) return
    if (file.size > MAX_BYTES) {
      setError(
        `“${file.name}” is ${(file.size / 1024 / 1024).toFixed(0)} MB — larger than the ` +
          `${MAX_BYTES / 1024 / 1024} MB import limit. Trim or downsample the recording first.`,
      )
      return
    }
    setReading(true)
    file
      .text()
      .then((text) => {
        const table = parseCsvTable(text)
        const numeric = table.columns.filter((c) => c.numericCount > 0)
        if (table.rowCount === 0) {
          setError(`“${file.name}” has no data rows.`)
          return
        }
        if (numeric.length < 2) {
          setError(
            `“${file.name}” has ${numeric.length} numeric column${numeric.length === 1 ? '' : 's'} — ` +
              'a function table needs two (the lookup argument and its values).',
          )
          return
        }
        setLoaded({ fileName: file.name, table })
        setXIndex(numeric[0].index)
        setYIndex(numeric[1].index)
        if (!nameTouched) setName(identifier(numeric[1].name, 'f').toLowerCase())
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setReading(false))
  }

  const pickY = (value: string | null) => {
    if (value === null) return
    const index = Number(value)
    setYIndex(index)
    const column = loaded?.table.columns[index]
    if (column && !nameTouched) setName(identifier(column.name, 'f').toLowerCase())
  }

  const argName = xColumn ? identifier(xColumn.name, 'x') : 'x'
  const nameCheck = checkFunctionName(tables, name)

  const preview = useMemo(() => {
    if (!xColumn || !yColumn) return null
    return functionSpecFromXY({
      name: name.trim(),
      argName,
      xs: xColumn.values,
      ys: yColumn.values,
    })
  }, [xColumn, yColumn, name, argName])

  const canCreate = nameCheck.ok && preview !== null && preview.usedRows > 0

  const create = () => {
    if (!canCreate || preview === null) return
    onCreate(preview.spec)
    onClose()
  }

  const options = numericColumns.map((c) => ({
    value: String(c.index),
    label: `${c.name} (${c.numericCount.toLocaleString()} numeric)`,
  }))

  return (
    <Modal opened onClose={onClose} title="Import CSV as Function Table" centered size="lg">
      <Text size="sm" c="dimmed" mb="md">
        Turns two columns of a .csv into a Function Table callable from equations (interpolated
        lookup). Blank and non-numeric cells are skipped; duplicate x values keep the first row.
        The file is read in this browser tab and never uploaded.
      </Text>

      <Stack gap="sm">
        <FileInput
          label="CSV file"
          placeholder="Choose a .csv file…"
          accept=".csv,.tsv,.txt,text/csv,text/tab-separated-values"
          leftSection={<IconFileTypeCsv size={16} />}
          onChange={pickFile}
          disabled={reading}
          clearable
        />

        {loaded && (
          <>
            <Text size="xs" c="dimmed">
              {loaded.table.rowCount.toLocaleString()} data row
              {loaded.table.rowCount === 1 ? '' : 's'} × {loaded.table.columns.length} column
              {loaded.table.columns.length === 1 ? '' : 's'}
              {loaded.table.headerless
                ? ' — no header row found, columns are named by position.'
                : '.'}
            </Text>

            <Group grow align="flex-start">
              <Select
                label="X column (lookup argument)"
                data={options.filter((o) => o.value !== String(yIndex))}
                value={xIndex === null ? null : String(xIndex)}
                onChange={(v) => v !== null && setXIndex(Number(v))}
                allowDeselect={false}
                searchable
              />
              <Select
                label="Y column (function values)"
                data={options.filter((o) => o.value !== String(xIndex))}
                value={yIndex === null ? null : String(yIndex)}
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

            {preview && (
              <Text size="xs" c={preview.usedRows === 0 ? 'red' : 'dimmed'}>
                {preview.usedRows === 0
                  ? 'No numeric pairs in the selected columns — pick different columns.'
                  : `${preview.usedRows.toLocaleString()} point${preview.usedRows === 1 ? '' : 's'} · ` +
                    `${preview.skippedRows.toLocaleString()} row${preview.skippedRows === 1 ? '' : 's'} skipped (blank or non-numeric)` +
                    (preview.decimated
                      ? ` · thinned uniformly to ${TABLE_MAX_ROWS.toLocaleString()} rows (the table row cap)`
                      : '')}
                {preview.usedRows > 0 && (
                  <>
                    {'. '}Use in equations:{' '}
                    <Text span size="xs" ff="monospace">
                      U = {name.trim() || 'name'}({argName})
                    </Text>
                  </>
                )}
              </Text>
            )}

            <FunctionNameHints name={name} check={nameCheck} />
            <FunctionPrecedenceNote />
          </>
        )}

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
            disabled={!canCreate}
            loading={reading}
            color={nameCheck.replacesGui ? 'yellow' : undefined}
          >
            {nameCheck.replacesGui ? 'Create (replace existing)' : 'Create function table'}
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
