import { useState } from 'react'
import {
  Badge,
  Button,
  Code,
  Group,
  Modal,
  Table,
  Text,
  TextInput,
  Tooltip,
} from '@mantine/core'
import { readGuessDirectives, writeGuessDirectives } from './guessDirectives'

export interface VariableDraft {
  guess: string
  lower: string
  upper: string
  units: string
  isUnitsUserSet?: boolean
  uncertainty: string
  relativeUncertainty: string
  uncertaintyType: 'absolute' | 'relative'
}

export const DEFAULT_DRAFT: VariableDraft = {
  // Empty guess means automatic: the solver picks per-variable defaults
  // (1.0, and 0.0 for imaginary components in complex mode) and explores
  // alternative starts itself when a block fails to converge.
  guess: '',
  lower: '-infinity',
  upper: 'infinity',
  units: '',
  isUnitsUserSet: false,
  uncertainty: '',
  relativeUncertainty: '',
  uncertaintyType: 'absolute',
}

export function parseBound(raw: string): number | null | undefined {
  const s = raw.trim().toLowerCase()
  if (s === '' || s === 'infinity' || s === '-infinity' || s === 'inf' || s === '-inf') {
    return null
  }
  const value = Number(s)
  return Number.isFinite(value) ? value : undefined
}

interface Props {
  variables: string[]
  drafts: Record<string, VariableDraft>
  solvedValues: Record<string, number>
  onSave: (drafts: Record<string, VariableDraft>) => void
  onClose: () => void
  /** The document, so the window can show what the text already declares. */
  documentText?: string
  /** Write the entered guesses/bounds back into the document as GUESS lines
   *  (absent = the affordance is hidden). */
  onWriteToDocument?: (nextText: string) => void
}

/** Mirrors the Options > Variable Information window. */
/** Relative (%) uncertainty derived from an absolute value, or '' when not derivable. */
function relUncFromAbs(value: string, val: number | undefined): string {
  if (val === undefined || val === 0 || value.trim() === '') return ''
  const numVal = Number(value)
  return Number.isFinite(numVal) ? String(Number(((numVal / Math.abs(val)) * 100).toPrecision(6))) : ''
}

/** Absolute uncertainty derived from a relative (%) value, or '' when not derivable. */
function absUncFromRel(value: string, val: number | undefined): string {
  if (val === undefined || value.trim() === '') return ''
  const numVal = Number(value)
  return Number.isFinite(numVal) ? String(Number(((numVal / 100) * Math.abs(val)).toPrecision(6))) : ''
}

/** Applies one edited field to a draft, keeping the absolute/relative uncertainty pair in sync. */
function applyFieldUpdate(draft: VariableDraft, field: keyof VariableDraft, value: string, val: number | undefined): VariableDraft {
  const updated = { ...draft, [field]: value }
  if (field === 'uncertainty') {
    updated.uncertaintyType = 'absolute'
    updated.relativeUncertainty = relUncFromAbs(value, val)
  } else if (field === 'relativeUncertainty') {
    updated.uncertaintyType = 'relative'
    updated.uncertainty = absUncFromRel(value, val)
  }
  return updated
}

/** Validates and normalizes one variable draft; returns the saved draft or an error message. */
function processDraft(name: string, draft: VariableDraft): { error: string } | { saved: VariableDraft } {
  const guessText = draft.guess.trim()
  const guess = guessText === '' ? null : Number(draft.guess)
  if (guess !== null && !Number.isFinite(guess)) {
    return { error: `Guess value for ${name} must be a number (or empty for automatic).` }
  }
  const lower = parseBound(draft.lower)
  const upper = parseBound(draft.upper)
  if (lower === undefined) return { error: `Lower bound for ${name} must be a number or -infinity.` }
  if (upper === undefined) return { error: `Upper bound for ${name} must be a number or infinity.` }
  const lo = lower ?? Number.NEGATIVE_INFINITY
  const hi = upper ?? Number.POSITIVE_INFINITY
  if (lo > hi) return { error: `Lower bound exceeds upper bound for ${name}.` }
  if (guess !== null && (guess < lo || guess > hi)) {
    return { error: `Guess value for ${name} is outside its bounds.` }
  }
  const uncertaintyText = draft.uncertainty?.trim() ?? ''
  const uncertainty = uncertaintyText === '' ? 0 : Number(uncertaintyText)
  if (uncertaintyText !== '' && (!Number.isFinite(uncertainty) || uncertainty < 0)) {
    return { error: `Uncertainty for ${name} must be a non-negative number.` }
  }
  return {
    saved: {
      ...draft,
      uncertainty: uncertaintyText,
      relativeUncertainty: draft.relativeUncertainty?.trim() ?? '',
      uncertaintyType: draft.uncertaintyType ?? 'absolute',
      isUnitsUserSet: draft.units.trim() !== '',
    },
  }
}

export default function VariableInfoModal({ variables, drafts, solvedValues, onSave, onClose, documentText, onWriteToDocument }: Readonly<Props>) {
  // Names the document itself declares with a GUESS line. The solver treats
  // the text as authoritative (it merges GUESS over these values, text
  // winning), so the window must say which rows the document already owns.
  const directives = readGuessDirectives(documentText ?? '')
  const inText = new Set(directives.map((d) => d.name.toLowerCase()))
  const [local, setLocal] = useState<Record<string, VariableDraft>>(() => {
    const byName = new Map(directives.map((d) => [d.name.toLowerCase(), d]))
    const initial: Record<string, VariableDraft> = {}
    for (const name of variables) {
      const draft = drafts[name] ?? { ...DEFAULT_DRAFT }
      // Show what the document declares, since that is what the solver will
      // actually use: the window is a view of the same state, not a rival
      // one. A value typed here still overrides the field until it is
      // written back (or the text wins again at solve time).
      const directive = byName.get(name.toLowerCase())
      initial[name] = directive
        ? {
            ...draft,
            guess: directive.guess !== null ? String(directive.guess) : draft.guess,
            lower: directive.lower !== null ? String(directive.lower) : draft.lower,
            upper: directive.upper !== null ? String(directive.upper) : draft.upper,
          }
        : draft
    }
    return initial
  })
  const [error, setError] = useState<string | null>(null)

  function setField(name: string, field: keyof VariableDraft, value: string) {
    setLocal((d) => {
      const draft = d[name] ?? { ...DEFAULT_DRAFT }
      const val = solvedValues[name.toLowerCase()]
      return { ...d, [name]: applyFieldUpdate(draft, field, value, val) }
    })
    setError(null)
  }

  function restoreDefaults() {
    const reset: Record<string, VariableDraft> = {}
    for (const name of variables) {
      reset[name] = { ...DEFAULT_DRAFT }
    }
    setLocal(reset)
    setError(null)
  }

  /**
   * Push the entered guesses and bounds into the document as GUESS lines.
   * Validation runs first, so the document never receives a value the window
   * itself would reject; a variable with neither a guess nor a complete pair
   * of finite bounds contributes nothing (and clears any line it had).
   */
  function writeToDocument() {
    if (!onWriteToDocument || documentText === undefined) {
      return
    }
    const entries: { name: string; guess: number | null; lower: number | null; upper: number | null }[] = []
    for (const name of variables) {
      const result = processDraft(name, local[name])
      if ('error' in result) {
        setError(result.error)
        return
      }
      const draft = result.saved
      const guess = draft.guess.trim() === '' ? null : Number(draft.guess)
      const lower = parseBound(draft.lower)
      const upper = parseBound(draft.upper)
      entries.push({
        name,
        guess: guess !== null && Number.isFinite(guess) ? guess : null,
        lower: typeof lower === 'number' ? lower : null,
        upper: typeof upper === 'number' ? upper : null,
      })
    }
    onWriteToDocument(writeGuessDirectives(documentText, entries))
    onClose()
  }

  function save() {
    const saved: Record<string, VariableDraft> = {}
    for (const name of variables) {
      const result = processDraft(name, local[name])
      if ('error' in result) {
        setError(result.error)
        return
      }
      saved[name] = result.saved
    }
    onSave(saved)
  }

  return (
    <Modal opened onClose={onClose} title="Variable Information" size="xl" centered>
      <Text size="sm" c="dimmed" mb="md">
        Guess values steer Newton&apos;s method toward a root; leave a guess
        empty to let the solver choose and explore starts automatically.
        Bounds constrain the search space (<Code>-infinity</Code> /{' '}
        <Code>infinity</Code> for unbounded). Units like <Code>kPa</Code> or{' '}
        <Code>kJ/kg-K</Code> enable dimensional checking; <Code>-</Code> means
        dimensionless.
      </Text>

      {variables.length === 0 ? (
        <Text c="dimmed" size="sm">
          No variables yet — run Check first to populate this table.
        </Text>
      ) : (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Variable</Table.Th>
              <Table.Th>Guess</Table.Th>
              <Table.Th>Lower</Table.Th>
              <Table.Th>Upper</Table.Th>
              <Table.Th>Units</Table.Th>
              <Table.Th>Uncertainty (Abs)</Table.Th>
              <Table.Th>Uncertainty (Rel %)</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {variables.map((name) => (
              <Table.Tr key={name}>
                <Table.Td ff="monospace" c="teal.4">
                  <Group gap={6} wrap="nowrap">
                    {name}
                    {inText.has(name.toLowerCase()) && (
                      <Tooltip label="Declared by a GUESS line in the document, which wins over this window">
                        <Badge size="xs" variant="light" color="blue">text</Badge>
                      </Tooltip>
                    )}
                  </Group>
                </Table.Td>
                {(['guess', 'lower', 'upper', 'units', 'uncertainty', 'relativeUncertainty'] as const).map((field) => (
                  <Table.Td key={field}>
                    <TextInput
                      size="xs"
                      value={local[name][field] ?? ''}
                      placeholder={field === 'guess' ? 'auto' : undefined}
                      onChange={(e) => setField(name, field, e.currentTarget.value)}
                      spellCheck={false}
                      styles={{
                        input: { fontFamily: 'var(--mantine-font-family-monospace)' },
                      }}
                    />
                  </Table.Td>
                ))}
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}

      {error && (
        <Text c="red" size="sm" mt="sm">
          {error}
        </Text>
      )}

      <Group justify="space-between" mt="lg">
        <Group gap="xs">
          <Button variant="subtle" onClick={restoreDefaults}>
            Restore Defaults
          </Button>
          {onWriteToDocument && documentText !== undefined && (
            <Tooltip label="Write these guesses and bounds into the document as GUESS lines, so they travel with the file">
              <Button variant="subtle" onClick={writeToDocument} disabled={variables.length === 0}>
                Write to document
              </Button>
            </Tooltip>
          )}
        </Group>
        <Group gap="xs">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={save} disabled={variables.length === 0}>
            OK
          </Button>
        </Group>
      </Group>
    </Modal>
  )
}
