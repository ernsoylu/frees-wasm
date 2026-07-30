import { useState, useMemo } from 'react'
import {
  Box, Button, Code, Divider, Grid, Group, Modal, SegmentedControl, Stack, Text, TextInput,
} from '@mantine/core'
import {
  buildUaCorrelation, PHASE_LABELS, Phase, UaSide, UaResult,
} from './uaCorrelation'

interface Props {
  opened: boolean
  onClose: () => void
  instanceName: string
  onConfirm: (result: UaResult) => void
}

const emptySide = (phase: Phase): UaSide => ({
  phase, fluid: '', P: '', state: '', mdot: '', Dh: '', Aflow: '', area: '', L: '',
})

// For two-phase sides the thermal state is a vapour quality x; otherwise a temperature.
const stateLabel = (phase: Phase) =>
  phase === 'boiling' || phase === 'condensing' ? 'x [-] (quality)' : 'T [K]'
const dhLabel = (phase: Phase) => (phase === 'extair' ? 'D [m] (tube)' : 'Dh [m]')

function SideForm({
  title, side, onChange,
}: Readonly<{ title: string; side: UaSide; onChange: (s: UaSide) => void }>) {
  const set = (k: keyof UaSide, v: string) => onChange({ ...side, [k]: v })
  const field = (label: string, k: keyof UaSide, ph = 'value or variable') => (
    <Grid.Col span={6}>
      <TextInput size="xs" label={label} placeholder={ph}
        value={side[k] ?? ''} onChange={(e) => set(k, e.currentTarget.value)} />
    </Grid.Col>
  )
  return (
    <Box>
      <Text size="sm" fw={600} mb={4}>{title}</Text>
      <SegmentedControl size="xs" fullWidth value={side.phase}
        onChange={(v) => set('phase', v)}
        data={(Object.keys(PHASE_LABELS) as Phase[]).map((p) => ({ value: p, label: PHASE_LABELS[p] }))} />
      <Grid gap="xs" mt="xs">
        {field('fluid', 'fluid', 'R1234yf')}
        {field('P [Pa]', 'P')}
        {field(stateLabel(side.phase), 'state')}
        {field('mdot [kg/s]', 'mdot')}
        {field(dhLabel(side.phase), 'Dh')}
        {field('Aflow [m^2]', 'Aflow')}
        {field('area [m^2] or passage count', 'area')}
        {field('L [m] (set ⇒ area=count)', 'L')}
      </Grid>
    </Box>
  )
}

// Guided builder: picks each side's film correlation from its phase state and
// emits the htc_* → area → ua_hx equations that compute UA [W/K].
export default function UaBuilderModal({ opened, onClose, instanceName, onConfirm }: Readonly<Props>) {
  const [sideA, setSideA] = useState<UaSide>(emptySide('boiling'))
  const [sideB, setSideB] = useState<UaSide>(emptySide('single'))
  const [rwall, setRwall] = useState('1e-4')

  const result = useMemo(
    () => buildUaCorrelation(instanceName, sideA, sideB, rwall),
    [instanceName, sideA, sideB, rwall],
  )

  return (
    <Modal opened={opened} onClose={onClose} title="Compute UA from correlations" size="lg" centered>
      <Stack gap="sm">
        <Text size="xs" c="dimmed">
          Pick each side's phase state; the matching film correlation is selected
          automatically (boiling/condensing use quality x, single-phase/air use temperature).
        </Text>
        <SideForm title="Side A" side={sideA} onChange={setSideA} />
        <SideForm title="Side B" side={sideB} onChange={setSideB} />
        <TextInput size="xs" label="Wall resistance Rwall [K-m^2/W]" w={240}
          value={rwall} onChange={(e) => setRwall(e.currentTarget.value)} />
        <Divider />
        <Box>
          <Text size="xs" fw={600} c="dimmed" mb={4}>Preview — {result.uaVar} fed into UA</Text>
          <Code block>{result.lines.join('\n')}</Code>
        </Box>
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>Cancel</Button>
          <Button onClick={() => { onConfirm(result); onClose() }}>Use this UA</Button>
        </Group>
      </Stack>
    </Modal>
  )
}
