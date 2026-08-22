import { useMemo, useState } from 'react'
import {
  Alert,
  Button,
  Group,
  Modal,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
} from '@mantine/core'
import { parameterFit, type ParameterFitResult, type StopCriteria, type VariableInfo, type FunctionTableDto } from './api'
import { channelStore } from './analyzer/channelStore'
import { offsetRawRange, offsetsOf } from './analyzer/offsets'
import type { AnalyzerSpec } from './analyzer/types'
import type { TableSpec } from './tables'
import { formatValue } from './format'

interface Bounds {
  initial: string
  lower: string
  upper: string
}

/**
 * Replaces the document's assignment of each fitted parameter with its fitted
 * value — the frontend twin of the terminal-override replace rule. A
 * parameter with no assignment line gets one appended.
 */
export function applyFittedParameters(text: string, names: string[], values: number[]): string {
  let out = text
  names.forEach((name, i) => {
    const value = formatValue(values[i])
    const pattern = new RegExp(`^(\\s*)${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*=[^;\\n]*`, 'im')
    if (pattern.test(out)) {
      out = out.replace(pattern, `$1${name} = ${value}`)
    } else {
      out = `${out.trim()}\n${name} = ${value}`
    }
  })
  return out
}

/**
 * Parameter estimation: fit chosen document parameters so a DYNAMIC column
 * matches a measured analyzer channel. Calibration closes the loop the
 * Compare instrument opens.
 */
export default function ParameterFitModal({
  opened,
  onClose,
  text,
  stopCriteria,
  variableInfo,
  functionTables,
  analyzers,
  tables,
  onApply,
}: Readonly<{
  opened: boolean
  onClose: () => void
  text: string
  stopCriteria: StopCriteria
  variableInfo: VariableInfo[]
  functionTables: FunctionTableDto[]
  analyzers: AnalyzerSpec[]
  tables: TableSpec[]
  onApply: (nextText: string) => void
}>) {
  // Measured source: every non-table channel registered by any analyzer.
  const measuredOptions = useMemo(() => {
    const out: { value: string; label: string }[] = []
    for (const an of analyzers) {
      for (const file of an.files) {
        const meta = channelStore.getMeta(file.measurementId)
        if (!meta || meta.signature.headerHash.startsWith('table:')) continue
        for (const ch of meta.channels) {
          out.push({
            value: `${an.id}|${file.measurementId}|${ch.name}`,
            label: `${an.name}: ${ch.name} (${meta.signature.name})`,
          })
        }
      }
    }
    return out
  }, [analyzers])

  // Fit target: any solved DYNAMIC table column (time column excluded).
  const targetOptions = useMemo(() => {
    const out: { value: string; label: string }[] = []
    for (const t of tables) {
      if (t.kind !== 'parametric' || t.origin !== 'ode') continue
      const block = t.id.replace(/^code-ode-/, '')
      for (const v of t.vars.slice(1)) {
        out.push({ value: `${block}|${v}`, label: `${block}: ${v}` })
      }
    }
    return out
  }, [tables])

  const [measSel, setMeasSel] = useState<string | null>(null)
  const [targetSel, setTargetSel] = useState<string | null>(null)
  const [paramText, setParamText] = useState('')
  const [bounds, setBounds] = useState<Record<string, Bounds>>({})
  const [running, setRunning] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<ParameterFitResult | null>(null)

  const paramNames = paramText
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)

  const setBound = (name: string, key: keyof Bounds, value: string) =>
    setBounds((prev) => ({
      ...prev,
      [name]: { ...(prev[name] ?? { initial: '', lower: '', upper: '' }), [key]: value },
    }))

  const run = () => {
    setError(null)
    setResult(null)
    if (measSel === null || targetSel === null || paramNames.length === 0) {
      setError('Pick a measured channel, a fit target, and at least one parameter.')
      return
    }
    const initial: number[] = []
    const lower: number[] = []
    const upper: number[] = []
    for (const name of paramNames) {
      const b = bounds[name]
      const i = Number(b?.initial)
      const lo = Number(b?.lower)
      const hi = Number(b?.upper)
      if (!Number.isFinite(i) || !Number.isFinite(lo) || !Number.isFinite(hi) || lo >= hi) {
        setError(`Bounds for ${name} must be finite numbers with lower < upper.`)
        return
      }
      initial.push(i)
      lower.push(lo)
      upper.push(hi)
    }
    const [anId, measId, channel] = measSel.split('|')
    const an = analyzers.find((a) => a.id === anId)
    const offset = an ? (offsetsOf(an).get(measId) ?? 0) : 0
    const raw = offsetRawRange({ measurementId: measId, channel }, offset, null, null)
    if (!raw) {
      setError('The measured channel has no loaded samples — re-import the file first.')
      return
    }
    const [odeBlock, column] = targetSel.split('|')
    setRunning(true)
    parameterFit({
      text,
      stopCriteria,
      variableInfo,
      functionTables,
      parameters: paramNames,
      initial,
      lower,
      upper,
      odeBlock,
      column,
      measuredT: Array.from(raw.t),
      measuredV: Array.from(raw.v),
    })
      .then((r) => {
        if (r.success) {
          setResult(r)
        } else {
          setError(r.error ?? 'Parameter fit failed.')
        }
      })
      .catch((e) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setRunning(false))
  }

  return (
    <Modal opened={opened} onClose={onClose} title="Parameter Estimation" size="lg" centered>
      <Stack gap="sm">
        <Text size="sm" c="dimmed">
          Fits the chosen document parameters so a DYNAMIC column matches a measured channel —
          each trial re-solves the model, and the residuals are reduced on the measurement raster
          with the Compare instrument's rules.
        </Text>
        <Group grow>
          <Select
            label="Measured channel"
            searchable
            data={measuredOptions}
            value={measSel}
            onChange={setMeasSel}
            placeholder={measuredOptions.length === 0 ? 'Import a measurement first' : 'Pick a channel'}
          />
          <Select
            label="Fit target (DYNAMIC column)"
            searchable
            data={targetOptions}
            value={targetSel}
            onChange={setTargetSel}
            placeholder={targetOptions.length === 0 ? 'Solve a DYNAMIC document first' : 'Pick a column'}
          />
        </Group>
        <TextInput
          label="Parameters to fit (comma-separated document assignments)"
          placeholder="k, ua, x0"
          value={paramText}
          onChange={(e) => setParamText(e.currentTarget.value)}
        />
        {paramNames.length > 0 && (
          <Table withTableBorder fz="sm">
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Parameter</Table.Th>
                <Table.Th>Initial</Table.Th>
                <Table.Th>Lower</Table.Th>
                <Table.Th>Upper</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {paramNames.map((name) => (
                <Table.Tr key={name}>
                  <Table.Td ff="monospace">{name}</Table.Td>
                  {(['initial', 'lower', 'upper'] as const).map((key) => (
                    <Table.Td key={key}>
                      <TextInput
                        size="xs"
                        aria-label={`${key} of ${name}`}
                        value={bounds[name]?.[key] ?? ''}
                        onChange={(e) => setBound(name, key, e.currentTarget.value)}
                      />
                    </Table.Td>
                  ))}
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
        <Group>
          <Button onClick={run} loading={running}>
            Fit
          </Button>
        </Group>
        {error && (
          <Alert color="red" title="Parameter fit failed">
            {error}
          </Alert>
        )}
        {result && (
          <>
            <Text size="sm">
              RMSE {formatValue(result.initialRmse)} → <b>{formatValue(result.rmse)}</b> in{' '}
              {result.evaluations} evaluations
              {result.truncated ? ' (stopped at the time budget)' : ''}.
            </Text>
            <Table withTableBorder maw={420} fz="sm">
              <Table.Tbody>
                {result.parameterNames.map((name, i) => (
                  <Table.Tr key={name}>
                    <Table.Td ff="monospace">{name}</Table.Td>
                    <Table.Td ta="right" ff="monospace">
                      {formatValue(result.fittedValues[i])}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
            <Group>
              <Button
                variant="light"
                onClick={() => {
                  onApply(applyFittedParameters(text, result.parameterNames, result.fittedValues))
                  onClose()
                }}
              >
                Apply to editor
              </Button>
            </Group>
          </>
        )}
      </Stack>
    </Modal>
  )
}
