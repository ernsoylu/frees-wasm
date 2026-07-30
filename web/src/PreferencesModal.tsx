import { useState } from 'react'
import { Button, Checkbox, Group, Modal, Select, Stack, Text, TextInput } from '@mantine/core'
import {
  DEFAULT_STOP_CRITERIA,
  StopCriteria,
  UNIT_SYSTEM_OPTIONS,
  UnitSystem,
} from './api'

interface Props {
  criteria: StopCriteria
  unitSystem: UnitSystem
  fillMissing: boolean
  onSave: (criteria: StopCriteria, unitSystem: UnitSystem, fillMissing: boolean) => void
  onClose: () => void
}

type StopCriteriaField = Exclude<keyof StopCriteria, 'complexMode'>

interface Field {
  key: StopCriteriaField
  label: string
  hint: string
}

// Mirrors the Options > Preferences > Stop Crit tab.
const FIELDS: Field[] = [
  { key: 'maxIterations', label: 'No. iterations', hint: 'Maximum Newton iterations per block' },
  { key: 'relativeResiduals', label: 'Relative residuals', hint: '|lhs − rhs| / |lhs| convergence tolerance' },
  { key: 'changeInVariables', label: 'Change in variables', hint: 'Stop when the largest variable change is below this' },
  { key: 'elapsedTimeSeconds', label: 'Elapsed time (sec)', hint: 'Abort the solve after this many seconds' },
]

export default function PreferencesModal({ criteria, unitSystem, fillMissing, onSave, onClose }: Readonly<Props>) {
  const [draft, setDraft] = useState<Record<StopCriteriaField, string>>({
    maxIterations: String(criteria.maxIterations),
    relativeResiduals: String(criteria.relativeResiduals),
    changeInVariables: String(criteria.changeInVariables),
    elapsedTimeSeconds: String(criteria.elapsedTimeSeconds),
  })
  const [system, setSystem] = useState<UnitSystem>(unitSystem)
  const [fillMissingState, setFillMissingState] = useState<boolean>(fillMissing)
  const [error, setError] = useState<string | null>(null)

  function setField(key: StopCriteriaField, value: string) {
    setDraft((d) => ({ ...d, [key]: value }))
    setError(null)
  }

  function restoreDefaults() {
    setDraft({
      maxIterations: String(DEFAULT_STOP_CRITERIA.maxIterations),
      relativeResiduals: String(DEFAULT_STOP_CRITERIA.relativeResiduals),
      changeInVariables: String(DEFAULT_STOP_CRITERIA.changeInVariables),
      elapsedTimeSeconds: String(DEFAULT_STOP_CRITERIA.elapsedTimeSeconds),
    })
    setFillMissingState(false)
    setError(null)
  }

  function save() {
    const parsed: Partial<Record<StopCriteriaField, number>> = {}
    for (const field of FIELDS) {
      const value = Number(draft[field.key])
      if (!Number.isFinite(value) || value <= 0) {
        setError(`${field.label} must be a positive number.`)
        return
      }
      parsed[field.key] = value
    }
    if (!Number.isInteger(parsed.maxIterations!)) {
      setError('No. iterations must be a whole number.')
      return
    }
    onSave({ ...criteria, ...parsed }, system, fillMissingState)
  }

  return (
    <Modal opened onClose={onClose} title="Preferences — Stop Criteria" centered size="lg">
      <Text size="sm" c="dimmed" mb="md">
        Calculation stops when any criterion is satisfied. Restore Defaults
        applies the frees defaults (tight tolerances for higher precision).
      </Text>

      <Stack gap="sm">
        <Select
          label="Display unit system"
          description="Calculations always run in SI; results are converted for display"
          data={UNIT_SYSTEM_OPTIONS}
          value={system}
          onChange={(v) => v && setSystem(v as UnitSystem)}
          allowDeselect={false}
        />
        <Checkbox
          label="Fill all missing state variables in background"
          description="Runs thermodynamic queries to compute other properties (like specific volume, quality, enthalpy) for detected state points"
          checked={fillMissingState}
          onChange={(e) => setFillMissingState(e.currentTarget.checked)}
        />
        {FIELDS.map((field) => (
          <TextInput
            key={field.key}
            label={field.label}
            description={field.hint}
            value={draft[field.key]}
            onChange={(e) => setField(field.key, e.currentTarget.value)}
            spellCheck={false}
            styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
          />
        ))}
      </Stack>

      {error && (
        <Text c="red" size="sm" mt="sm">
          {error}
        </Text>
      )}

      <Group justify="space-between" mt="lg">
        <Button variant="subtle" onClick={restoreDefaults}>
          Restore Defaults
        </Button>
        <Group gap="xs">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={save}>OK</Button>
        </Group>
      </Group>
    </Modal>
  )
}
