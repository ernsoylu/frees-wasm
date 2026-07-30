// Calculated-signal modal (todo.md Phase 4): a frees formula evaluated per
// sample over a merged raster — full function library incl. units-aware
// CoolProp properties on measured data. Over-cap is a GUIDED path: the typed
// RASTER_CAP_EXCEEDED error becomes a one-click "switch to fixed dt" (never a
// generic failure toast).

import { useMemo, useState } from 'react'
import {
  ActionIcon,
  Alert,
  Button,
  Group,
  Modal,
  NumberInput,
  Radio,
  Select,
  Stack,
  Text,
  TextInput,
  Tooltip,
  useComputedColorScheme,
} from '@mantine/core'
import { IconAlertTriangle, IconPlus, IconTrash, IconWand } from '@tabler/icons-react'
import CodeMirror from '@uiw/react-codemirror'
import { syntaxHighlighting } from '@codemirror/language'
import { freesHighlightDark, freesHighlightLight, freesLanguage } from '../EquationEditor'
import { applyOverCap, buildCalcRequest, parseOverCap, type CalcBinding, type CalcRaster } from './calc'
import { calcSignal, type CalcResultDto } from './measurementApi'
import { channelStore } from './channelStore'
import type { AnalyzerSpec } from './types'

interface ChannelOption {
  value: string
  label: string
  kind: 'analog' | 'boolean' | 'string'
}

interface BindingRow {
  var: string
  selection: string | null
  interp: 'step' | 'linear'
}

interface Props {
  spec: AnalyzerSpec
  onResult: (result: CalcResultDto) => void
  onClose: () => void
}

export default function CalcSignalModal({ spec, onResult, onClose }: Readonly<Props>) {
  const dark = useComputedColorScheme('dark') === 'dark'
  const [name, setName] = useState('c1')
  const [formula, setFormula] = useState('')
  const [rows, setRows] = useState<BindingRow[]>([{ var: 'x', selection: null, interp: 'linear' }])
  const [rasterMode, setRasterMode] = useState<'merge' | 'fixed' | 'sameAs'>('merge')
  const [dt, setDt] = useState<number | string>(0.01)
  const [sameAs, setSameAs] = useState<string>('x')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [overCap, setOverCap] = useState<{ actualPoints: number; suggestedDt: number } | null>(null)

  // Every plottable channel of every attached file.
  const options = useMemo<ChannelOption[]>(() => {
    const out: ChannelOption[] = []
    for (const f of spec.files) {
      const meta = channelStore.getMeta(f.measurementId)
      if (!meta || !channelStore.isLoaded(f.measurementId)) continue
      for (const ch of meta.channels) {
        if (ch.kind === 'string') continue
        out.push({
          value: `${f.measurementId}|${ch.name}`,
          label: `${ch.name}  —  ${f.signature.name}`,
          kind: ch.kind,
        })
      }
    }
    return out
  }, [spec.files])

  const submit = async (raster: CalcRaster) => {
    setBusy(true)
    setError(null)
    setOverCap(null)
    try {
      const bindings: CalcBinding[] = rows
        .filter((r) => r.selection !== null && r.var.trim() !== '')
        .map((r) => {
          const [measurementId, channel] = (r.selection as string).split('|')
          return { var: r.var.trim(), signal: { measurementId, channel }, interp: r.interp }
        })
      if (bindings.length === 0) throw new Error('Bind at least one input signal.')
      if (formula.trim() === '') throw new Error('Enter a formula.')
      const request = buildCalcRequest(name.trim() || 'calc', formula.trim(), bindings, raster)
      onResult(await calcSignal(request))
    } catch (err) {
      const cap = parseOverCap(err)
      if (cap) setOverCap(cap)
      else setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const currentRaster = (): CalcRaster => {
    if (rasterMode === 'fixed') {
      const v = typeof dt === 'number' ? dt : Number(dt)
      return { mode: 'fixed', dt: v > 0 ? v : 0.01 }
    }
    if (rasterMode === 'sameAs') return { mode: 'sameAs', sameAs }
    return { mode: 'merge' }
  }

  return (
    <Modal opened onClose={onClose} title="Calculated signal" size="lg" centered>
      <Stack gap="sm">
        <Group grow>
          <TextInput label="Signal name" value={name} onChange={(e) => setName(e.currentTarget.value)} />
        </Group>

        <div>
          <Text size="sm" fw={500} mb={4}>
            Formula (frees expression — full function library, e.g.{' '}
            <Text span ff="monospace" size="xs">
              tq * w / 1000
            </Text>
            ,{' '}
            <Text span ff="monospace" size="xs">
              enthalpy('R134a', T=t1, P=p1)
            </Text>
            , <Text span ff="monospace" size="xs">delta(x)</Text>,{' '}
            <Text span ff="monospace" size="xs">integral(x)</Text>,{' '}
            <Text span ff="monospace" size="xs">movavg(x, 1)</Text>,{' '}
            <Text span ff="monospace" size="xs">delay(x, 0.5)</Text>)
          </Text>
          <CodeMirror
            value={formula}
            height="72px"
            theme={dark ? 'dark' : 'light'}
            extensions={[freesLanguage, syntaxHighlighting(dark ? freesHighlightDark : freesHighlightLight)]}
            onChange={setFormula}
            basicSetup={{ lineNumbers: false, foldGutter: false }}
          />
        </div>

        <div>
          <Group justify="space-between" mb={4}>
            <Text size="sm" fw={500}>
              Inputs (formula variable → signal)
            </Text>
            <Button
              size="compact-xs"
              variant="default"
              leftSection={<IconPlus size={12} />}
              onClick={() =>
                setRows((prev) => [...prev, { var: `x${prev.length + 1}`, selection: null, interp: 'linear' }])
              }
            >
              Add input
            </Button>
          </Group>
          <Stack gap={6}>
            {rows.map((row, i) => (
              <Group key={i} gap="xs" wrap="nowrap">
                <TextInput
                  w={90}
                  size="xs"
                  value={row.var}
                  aria-label="Formula variable"
                  onChange={(e) => {
                    const v = e.currentTarget.value
                    setRows((prev) => prev.map((r, k) => (k === i ? { ...r, var: v } : r)))
                  }}
                />
                <Select
                  flex={1}
                  size="xs"
                  placeholder="Bind a signal…"
                  data={options.map((o) => ({ value: o.value, label: o.label }))}
                  value={row.selection}
                  searchable
                  aria-label="Signal to bind"
                  onChange={(v) =>
                    setRows((prev) =>
                      prev.map((r, k) =>
                        k === i
                          ? {
                              ...r,
                              selection: v,
                              // Interp default by kind (§ Phase 4): step for
                              // boolean/enum, linear for continuous analog.
                              interp:
                                options.find((o) => o.value === v)?.kind === 'boolean' ? 'step' : 'linear',
                            }
                          : r,
                      ),
                    )
                  }
                />
                <Select
                  w={95}
                  size="xs"
                  data={[
                    { value: 'linear', label: 'linear' },
                    { value: 'step', label: 'step' },
                  ]}
                  value={row.interp}
                  aria-label="Interpolation"
                  onChange={(v) =>
                    setRows((prev) =>
                      prev.map((r, k) => (k === i ? { ...r, interp: (v ?? 'linear') as 'step' | 'linear' } : r)),
                    )
                  }
                />
                <Tooltip label="Remove input">
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="gray"
                    aria-label="Remove input"
                    onClick={() => setRows((prev) => prev.filter((_, k) => k !== i))}
                  >
                    <IconTrash size={13} />
                  </ActionIcon>
                </Tooltip>
              </Group>
            ))}
          </Stack>
        </div>

        <div>
          <Text size="sm" fw={500} mb={4}>
            Output raster
          </Text>
          <Radio.Group value={rasterMode} onChange={(v) => setRasterMode(v as typeof rasterMode)}>
            <Group gap="md">
              <Radio value="merge" label="Merge input rasters" />
              <Radio value="fixed" label="Fixed dt" />
              <Radio value="sameAs" label="Same as input" />
            </Group>
          </Radio.Group>
          <Group gap="sm" mt={6}>
            {rasterMode === 'fixed' && (
              <NumberInput
                size="xs"
                w={160}
                label="dt"
                suffix=" s"
                value={dt}
                onChange={setDt}
                min={1e-9}
                decimalScale={9}
              />
            )}
            {rasterMode === 'sameAs' && (
              <Select
                size="xs"
                w={160}
                label="Input variable"
                data={rows.map((r) => r.var).filter((v) => v.trim() !== '')}
                value={sameAs}
                onChange={(v) => setSameAs(v ?? 'x')}
              />
            )}
          </Group>
        </div>

        {overCap !== null && (
          <Alert color="yellow" icon={<IconAlertTriangle size={15} />}>
            <Stack gap={6}>
              <Text size="sm">
                The merged raster has {overCap.actualPoints.toLocaleString()} points — above the
                cap for this formula.
              </Text>
              <Button
                size="compact-sm"
                color="yellow"
                variant="light"
                leftSection={<IconWand size={14} />}
                onClick={() => {
                  setRasterMode('fixed')
                  setDt(overCap.suggestedDt)
                  void submit(applyOverCap(overCap))
                }}
              >
                Switch to fixed dt = {overCap.suggestedDt} s with linear interpolation
              </Button>
            </Stack>
          </Alert>
        )}
        {error !== null && (
          <Alert color="red" icon={<IconAlertTriangle size={15} />}>
            <Text size="sm">{error}</Text>
          </Alert>
        )}

        <Group justify="flex-end" gap="xs">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={busy} onClick={() => void submit(currentRaster())}>
            Compute signal
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
