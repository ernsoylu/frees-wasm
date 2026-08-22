// Interactive PID Tuner (industry-standard interactive-tuner style). Given a SISO plant transfer
// function it auto-tunes P/PI/PID gains via loop-shaping and previews the
// closed-loop step response live as the two sliders move:
//   • Response time  → target crossover wc (bandwidth)
//   • Robustness     → target phase margin (transient behaviour)
// Backend math lives in PidTuner.java (/api/control/pidtune); this component is
// the UI, the debounced fetch, and the Plotly preview. It is shared by both
// entry points — the Tools-menu tuner (manual plant) and the Inspector "Tune…"
// button on a selected SigPID (plant supplied by the caller).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Alert,
  Box,
  Button,
  Group,
  Loader,
  Modal,
  SegmentedControl,
  Slider,
  Stack,
  Table,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { IconAlertTriangle } from '@tabler/icons-react'
import PlotlyChart from './plots/PlotlyChart'
import { pidTune, type PidTuneResponse } from './api'
import { formatValue } from './format'

export interface PidPlant {
  num: number[]
  den: number[]
}

export type PidType = 'p' | 'pi' | 'pid'

export interface TunedGains {
  type: PidType
  kp: number
  ki: number
  kd: number
}

interface Props {
  opened: boolean
  onClose: () => void
  /** Plant to tune against; when absent, the user enters num/den by hand. */
  plant?: PidPlant
  /** Controller type + current gains to seed the UI (e.g. from a SigPID). */
  initial?: Partial<TunedGains>
  /** A label for what is being tuned (e.g. the component name). */
  subject?: string
  /** Apply the tuned gains (write back to the SigPID / insert a snippet). */
  onApply?: (gains: TunedGains) => void
  /** The plant is being auto-extracted (linearizing the loop). */
  plantLoading?: boolean
  /** Auto-extraction failed / was skipped; shown above the manual entry. */
  plantError?: string
  dark?: boolean
}

const num = (s: string): number[] =>
  s
    .split(/[,\s]+/)
    .map((x) => x.trim())
    .filter((x) => x !== '')
    .map(Number)

const isPlant = (p: { num: number[]; den: number[] }): boolean =>
  p.num.length > 0 && p.den.length > 0 && p.num.every(Number.isFinite) && p.den.every(Number.isFinite)

export default function PidTunerModal({
  opened,
  onClose,
  plant,
  initial,
  subject,
  onApply,
  plantLoading = false,
  plantError,
  dark = true,
}: Readonly<Props>) {
  const [type, setType] = useState<PidType>(initial?.type ?? 'pi')
  // Manual plant entry (Tools-menu path) — ignored when `plant` is supplied.
  const [numText, setNumText] = useState('1')
  const [denText, setDenText] = useState('5, 1')
  // Slider positions are log10(wc) and phase-margin degrees.
  const [logWc, setLogWc] = useState(-0.3) // ~0.5 rad/s
  const [pm, setPm] = useState(60)
  const [result, setResult] = useState<PidTuneResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const seededWc = useRef(false)

  const effectivePlant = useMemo<PidPlant | null>(() => {
    if (plant && isPlant(plant)) return plant
    const p = { num: num(numText), den: num(denText) }
    return isPlant(p) ? p : null
  }, [plant, numText, denText])

  // Debounced tune whenever the plant, type, or slider targets change.
  const runId = useRef(0)
  const doTune = useCallback(
    (p: PidPlant, t: PidType, wc: number, phaseMargin: number) => {
      const id = ++runId.current
      setLoading(true)
      pidTune({ num: p.num, den: p.den, type: t, wc, pm: phaseMargin })
        .then((r) => {
          if (id !== runId.current) return
          setResult(r)
          setError(null)
        })
        .catch((e: unknown) => {
          if (id !== runId.current) return
          setError(e instanceof Error ? e.message : String(e))
          setResult(null)
        })
        .finally(() => {
          if (id === runId.current) setLoading(false)
        })
    },
    [],
  )

  // Seed the response-time slider from the plant the first time it appears.
  useEffect(() => {
    if (!opened) {
      seededWc.current = false
      return
    }
    if (seededWc.current || !effectivePlant) return
    seededWc.current = true
    // One tune with wc omitted lets the backend suggest a crossover; adopt it.
    setLoading(true)
    pidTune({ num: effectivePlant.num, den: effectivePlant.den, type, pm })
      .then((r) => {
        setResult(r)
        setError(null)
        setLogWc(Math.log10(r.wc))
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, effectivePlant])

  // Re-tune on control changes once seeded (debounced for slider drags).
  useEffect(() => {
    if (!opened || !seededWc.current || !effectivePlant) return
    const h = setTimeout(() => doTune(effectivePlant, type, 10 ** logWc, pm), 120)
    return () => clearTimeout(h)
  }, [opened, effectivePlant, type, logWc, pm, doTune])

  const figure = useMemo(() => {
    if (!result) return null
    const axis = dark ? '#909296' : '#495057'
    const grid = dark ? 'rgba(134,142,150,0.15)' : 'rgba(134,142,150,0.25)'
    return {
      data: [
        {
          x: result.t,
          y: result.y,
          type: 'scatter' as const,
          mode: 'lines' as const,
          line: { color: '#4dabf7', width: 2 },
          name: 'closed loop',
        },
        {
          x: [result.t[0], result.t[result.t.length - 1]],
          y: [1, 1],
          type: 'scatter' as const,
          mode: 'lines' as const,
          line: { color: axis, width: 1, dash: 'dot' as const },
          name: 'setpoint',
          hoverinfo: 'skip' as const,
        },
      ],
      layout: {
        margin: { l: 44, r: 12, t: 10, b: 34 },
        paper_bgcolor: 'rgba(0,0,0,0)',
        plot_bgcolor: 'rgba(0,0,0,0)',
        font: { color: axis, size: 11 },
        xaxis: { title: { text: 'time (s)' }, gridcolor: grid, zeroline: false },
        yaxis: { title: { text: 'normalized output' }, gridcolor: grid, zeroline: false },
        showlegend: false,
      },
    }
  }, [result, dark])

  const apply = () => {
    if (!result || !onApply) return
    onApply({ type, kp: result.kp, ki: result.ki, kd: result.kd })
    onClose()
  }

  const metric = (label: string, value: string) => (
    <Table.Tr>
      <Table.Td>
        <Text size="xs" c="dimmed">
          {label}
        </Text>
      </Table.Td>
      <Table.Td ta="right">
        <Text size="xs" ff="monospace">
          {value}
        </Text>
      </Table.Td>
    </Table.Tr>
  )

  const title = subject ? `PID Tuner — ${subject}` : 'PID Tuner'

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={title}
      size="xl"
      centered
    >
      <Group align="stretch" gap="md" wrap="nowrap">
        {/* Controls */}
        <Stack gap="sm" w={300} style={{ flexShrink: 0 }}>
          <SegmentedControl
            size="xs"
            value={type}
            onChange={(v) => setType(v as PidType)}
            data={[
              { label: 'P', value: 'p' },
              { label: 'PI', value: 'pi' },
              { label: 'PID', value: 'pid' },
            ]}
            fullWidth
          />

          {plantLoading && (
            <Group gap="xs" wrap="nowrap">
              <Loader size="xs" />
              <Text size="xs" c="dimmed">
                Linearizing the loop to identify the plant…
              </Text>
            </Group>
          )}

          {plant && !plantLoading && (
            <Text size="xs" c="dimmed">
              Plant auto-identified from the loop by linearization.
            </Text>
          )}

          {!plant && !plantLoading && (
            <>
              {plantError !== undefined && (
                <Alert color="yellow" p="xs" icon={<IconAlertTriangle size={14} />}>
                  <Text size="xs">{plantError}</Text>
                </Alert>
              )}
              <TextInput
                size="xs"
                label="Plant numerator (descending powers)"
                value={numText}
                onChange={(e) => setNumText(e.currentTarget.value)}
                placeholder="e.g. 1"
              />
              <TextInput
                size="xs"
                label="Plant denominator"
                value={denText}
                onChange={(e) => setDenText(e.currentTarget.value)}
                placeholder="e.g. 5, 1"
              />
            </>
          )}

          <Box>
            <Group justify="space-between" gap={4}>
              <Text size="xs" fw={600}>
                Response time
              </Text>
              <Text size="xs" c="dimmed" ff="monospace">
                ωc = {formatValue(10 ** logWc)} rad/s
              </Text>
            </Group>
            <Tooltip label="Faster response ↔ slower, more robust">
              <Slider
                size="sm"
                min={-3}
                max={2}
                step={0.02}
                value={logWc}
                onChange={setLogWc}
                label={null}
                marks={[
                  { value: -3, label: 'slow' },
                  { value: 2, label: 'fast' },
                ]}
              />
            </Tooltip>
          </Box>

          <Box mt="xs">
            <Group justify="space-between" gap={4}>
              <Text size="xs" fw={600}>
                Transient behaviour
              </Text>
              <Text size="xs" c="dimmed" ff="monospace">
                PM = {pm}°
              </Text>
            </Group>
            <Tooltip label="Aggressive (low margin) ↔ robust (high margin)">
              <Slider
                size="sm"
                min={20}
                max={85}
                step={1}
                value={pm}
                onChange={setPm}
                label={null}
                marks={[
                  { value: 20, label: 'aggressive' },
                  { value: 85, label: 'robust' },
                ]}
              />
            </Tooltip>
          </Box>

          <Table withRowBorders={false} verticalSpacing={2} mt="xs">
            <Table.Tbody>
              {metric('Kp', result ? formatValue(result.kp) : '—')}
              {type !== 'p' && metric('Ki', result ? formatValue(result.ki) : '—')}
              {type === 'pid' && metric('Kd', result ? formatValue(result.kd) : '—')}
            </Table.Tbody>
          </Table>
        </Stack>

        {/* Preview + metrics */}
        <Stack gap="xs" style={{ flex: 1, minWidth: 0 }}>
          {error !== null && (
            <Alert color="red" p="xs" icon={<IconAlertTriangle size={14} />}>
              <Text size="xs">{error}</Text>
            </Alert>
          )}
          <Box style={{ position: 'relative', height: 260 }}>
            {figure ? (
              <PlotlyChart figure={figure} minHeight={0} />
            ) : (
              <Group justify="center" h="100%">
                <Text size="sm" c="dimmed">
                  Enter a plant transfer function to tune.
                </Text>
              </Group>
            )}
            {loading && (
              <Loader size="xs" style={{ position: 'absolute', top: 6, right: 6 }} />
            )}
          </Box>
          {result && (
            <Group gap="lg" wrap="wrap">
              <Text size="xs" c="dimmed">
                Rise <b>{formatValue(result.riseTime)} s</b>
              </Text>
              <Text size="xs" c="dimmed">
                Settling <b>{formatValue(result.settlingTime)} s</b>
              </Text>
              <Text size="xs" c="dimmed">
                Overshoot <b>{formatValue(result.overshoot)} %</b>
              </Text>
              <Text size="xs" c="dimmed">
                Phase margin <b>{formatValue(result.phaseMargin)}°</b>
              </Text>
              <Text size="xs" c="dimmed">
                Gain margin <b>{result.gainMargin >= 1e8 ? '∞' : `${formatValue(result.gainMargin)} dB`}</b>
              </Text>
            </Group>
          )}
        </Stack>
      </Group>

      <Group justify="flex-end" gap="xs" mt="md">
        <Button variant="default" onClick={onClose}>
          Close
        </Button>
        {onApply && (
          <Button onClick={apply} disabled={!result}>
            Apply gains
          </Button>
        )}
      </Group>
    </Modal>
  )
}
