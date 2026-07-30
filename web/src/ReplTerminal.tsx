import { useEffect, useRef, useState, useCallback } from 'react'
import { ActionIcon, Group, Tooltip } from '@mantine/core'
import { IconTrash } from '@tabler/icons-react'
import { replEvaluate, VariableResult, type UnitSystem } from './api'
import { REPL_BANNER } from './version'

/**
 * Integrated REPL terminal. Evaluates a single line against the session's
 * workspace via `/api/repl/evaluate` — any solved or REPL-defined variable is in
 * scope. Supports command history (Up/Down) and Tab-completion over the current
 * variable names.
 *
 * Rendered as an ordinary dock window (like the Editor / Variable Explorer), so
 * it can be moved and docked anywhere; it simply fills its panel.
 *
 * Intentionally a lightweight React component rather than a full terminal
 * emulator (xterm.js): this is a line-oriented math REPL, not a PTY, so plain
 * React gives clean Mantine theming plus trivial history/completion handling.
 */

const BANNER = REPL_BANNER

interface Line {
  kind: 'input' | 'result' | 'error' | 'info'
  text: string
}

interface Props {
  sessionId: string
  /** Workspace variables with values, units, and uncertainties. */
  variables: VariableResult[]
  /** Set of lowercased names that are defined or overridden in the REPL. */
  replNames?: Set<string>
  /** All callable function names (built-ins + property functions) for Tab-completion. */
  functions?: string[]
  /** Current preferred display unit system — labels REPL results accordingly. */
  unitSystem?: UnitSystem
  /** Called when a line defines/changes a variable, so the workspace can reflect it. */
  onAssign?: (v: VariableResult) => void
  /** Run Check (same as the toolbar Check button) when the user types `check`. */
  onCheck?: () => void
  /** Run Check+Solve (same as the toolbar Solve button) when the user types `solve`. */
  onSolve?: () => void
  /** Clear all REPL-defined/overridden variables (workspace + memory) on `clear`. */
  onClear?: () => void
  /** Clear a specific REPL variable overlay (workspace + memory) on `clear <var>`. */
  onClearVar?: (name: string) => void
}

/** Built-in REPL commands that drive the app rather than evaluate an expression. */
const COMMANDS = ['check', 'solve', 'clear', 'clc', 'help', 'vars', 'who', 'whos']

/** Symbolic CAS functions (Symja-backed) offered in Tab-completion. */
const CAS_FUNCTIONS = [
  'Factor', 'Expand', 'Simplify', 'Together', 'Cancel', 'Numerator', 'Denominator',
  'Collect', 'Diff', 'Integrate', 'Apart', 'Laplace', 'InverseLaplace',
]

function longestCommonPrefix(words: string[]): string {
  if (words.length === 0) return ''
  let prefix = words[0]
  for (const w of words.slice(1)) {
    while (!w.toLowerCase().startsWith(prefix.toLowerCase())) {
      prefix = prefix.slice(0, -1)
      if (!prefix) return ''
    }
  }
  return prefix
}

export default function ReplTerminal({
  sessionId, variables, replNames = new Set(), functions = [], unitSystem, onAssign, onCheck, onSolve, onClear, onClearVar,
}: Readonly<Props>) {
  const [lines, setLines] = useState<Line[]>([{ kind: 'info', text: BANNER }])
  const [input, setInput] = useState('')
  const [history, setHistory] = useState<string[]>(() => {
    try {
      const saved = localStorage.getItem('frees-repl-history')
      if (saved) {
        const parsed = JSON.parse(saved)
        if (Array.isArray(parsed)) {
          return parsed.filter((x) => typeof x === 'string')
        }
      }
    } catch {
      // ignore
    }
    return []
  })
  const [histIndex, setHistIndex] = useState<number>(-1) // -1 = current draft
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    try {
      localStorage.setItem('frees-repl-history', JSON.stringify(history))
    } catch {
      // ignore
    }
  }, [history])

  const logRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  // Keep the newest output in view as lines are appended.
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [lines])

  // Focus the prompt whenever the panel opens.
  useEffect(() => { inputRef.current?.focus() }, [])

  const append = useCallback((...newLines: Line[]) => {
    setLines((prev) => [...prev, ...newLines])
  }, [])

  const submit = useCallback(async () => {
    const expr = input.trim()
    if (!expr || busy) return
    setHistory((h) => {
      const next = h[h.length - 1] === expr ? h : [...h, expr]
      return next.slice(-200)
    })
    setHistIndex(-1)
    setInput('')

    const cmd = expr.toLowerCase()
    if (cmd === 'clc') {
      setLines([])
      return
    }

    if (cmd === 'clear') {
      // Wipe REPL-defined/overridden variables (workspace + backend memory) and
      // the log — subsequent solves use the editor only.
      onClear?.()
      setLines([{ kind: 'info', text: 'Cleared REPL variables — solves now use the editor only.' }])
      return
    }

    const clearMatch = expr.match(/^clear\s+([A-Za-z_][A-Za-z0-9_$]*)$/i)
    if (clearMatch) {
      const varName = clearMatch[1]
      onClearVar?.(varName)
      append({ kind: 'input', text: expr })
      append({ kind: 'info', text: `Cleared REPL variable overlay: ${varName}` })
      return
    }

    if (cmd === 'help') {
      append({ kind: 'input', text: expr })
      append({
        kind: 'info',
        text: `frees REPL Commands & Usage:
  help                    Show this help message
  clc                     Clear the terminal screen
  clear                   Clear all REPL-defined overrides
  clear <var>             Clear a specific REPL variable overlay (e.g. clear x)
  check                   Run equation syntax and solvability check
  solve                   Run solve on the document with active overrides
  vars / who / whos       List all active workspace variables and their values

Basic Evaluation:
  Type any math expression or function call, e.g.:
  › 2 * sqrt(9) + 4
  = 10

Variable Queries & Assignments:
  › T_1
  = 300 [K]
  › x = 42 [m/s]
  = x = 42 [m/s]

Implicit Single-Unknown Solver:
  Solve equations implicitly by specifying a single unknown variable, e.g.:
  › P = 50000 * volume
  = volume = 5 [m^3]

CALL Library (eigen, control systems, partial fractions, …):
  Run a CALL procedure against workspace arrays. Inputs and outputs may be
  bare names — output lengths are sized automatically from the inputs:
  › CALL Eigenvalues(A : lambda)
  › CALL Routh(den : nRHP, stable)
  › CALL Bode(num, den, w : mag, phase)
  (Finite-zero counts for Zero/tf2zp still take an explicit size, e.g. zr[1:2].)

Symbolic CAS (Symja) — returns a transformed expression as text:
  › Factor(x^2 - 1)              = (-1+x)*(1+x)
  › Expand((x+1)^3)              Simplify(...)
  › Together(...)  Cancel(...)   Numerator(...)  Denominator(...)
  › Collect(a*x^2+b*x^2+x, x)    Diff(x^3+x^2, x)    Integrate(x^2, x)
  › Apart((s+3)/(s^2+3*s+2), s)  partial fractions in s
  › Laplace(t, t, s)             InverseLaplace(1/(s+2), s, t)`
      })
      return
    }

    if (cmd === 'vars' || cmd === 'who' || cmd === 'whos') {
      append({ kind: 'input', text: expr })
      if (variables.length === 0) {
        append({ kind: 'info', text: 'Workspace is empty. Solve the document or define variables.' })
      } else {
        const list = variables.map((v) => {
          const valueStr = v.value != null ? String(v.value) : '—'
          const unitStr = v.units && v.units !== '-' ? ` [${v.units}]` : ''
          const uncStr = v.uncertainty != null && v.uncertainty !== 0 ? ` ± ${v.uncertainty}` : ''
          const replMarker = replNames.has(v.name.toLowerCase()) ? ' (repl)' : ''
          return `  ${v.name} = ${valueStr}${uncStr}${unitStr}${replMarker}`
        }).join('\n')
        append({ kind: 'info', text: `Workspace variables:\n${list}` })
      }
      return
    }

    append({ kind: 'input', text: expr })
    if (cmd === 'check') {
      onCheck?.()
      append({ kind: 'info', text: 'Running Check…' })
      return
    }
    if (cmd === 'solve') {
      onSolve?.()
      append({ kind: 'info', text: 'Running Solve…' })
      return
    }

    setBusy(true)
    try {
      const res = await replEvaluate(sessionId, expr, unitSystem)
      if (res.success) {
        append({ kind: 'result', text: res.text ?? String(res.value ?? '') })
        // An assignment (name set) updates the workspace so the Variable Explorer
        // and Solution reflect the new/changed value.
        if (res.assignedVariables && res.assignedVariables.length > 0) {
          for (const v of res.assignedVariables) {
            onAssign?.(v)
          }
        } else if (res.name && res.value != null) {
          onAssign?.({ name: res.name, value: res.value, units: res.units ?? '', uncertainty: res.uncertainty })
        }
      } else {
        append({ kind: 'error', text: res.error ?? 'Could not evaluate expression.' })
      }
    } finally {
      setBusy(false)
      // Refocus after the async round-trip.
      requestAnimationFrame(() => inputRef.current?.focus())
    }
  }, [input, busy, sessionId, unitSystem, append, variables, replNames, onAssign, onCheck, onSolve, onClear, onClearVar])

  const complete = useCallback(() => {
    // Complete the identifier token immediately left of the cursor against
    // variables, every callable function, and the built-in commands.
    const token = input.match(/[A-Za-z_](?=([A-Za-z0-9_$]*))\1$/)?.[0] ?? ''
    if (!token) return
    const candidates = [...variables.map((v) => v.name), ...functions, ...CAS_FUNCTIONS, ...COMMANDS]
    const fnSet = new Set([...functions, ...CAS_FUNCTIONS].map((f) => f.toLowerCase()))
    const seen = new Set<string>()
    const matches = candidates.filter((c) => {
      const k = c.toLowerCase()
      if (seen.has(k) || !k.startsWith(token.toLowerCase())) return false
      seen.add(k)
      return true
    })
    if (matches.length === 0) return
    const replaceToken = (completion: string, fn: boolean) =>
      setInput((cur) => cur.slice(0, cur.length - token.length) + completion + (fn ? '(' : ''))
    if (matches.length === 1) {
      replaceToken(matches[0], fnSet.has(matches[0].toLowerCase()))
      return
    }
    const common = longestCommonPrefix(matches)
    if (common.length > token.length) {
      setInput((cur) => cur.slice(0, cur.length - token.length) + common)
    }
    append({ kind: 'info', text: matches.slice(0, 40).join('    ') })
  }, [input, variables, functions, append])

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      void submit()
    } else if (e.key === 'Tab') {
      e.preventDefault()
      complete()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      if (history.length === 0) return
      const next = histIndex === -1 ? history.length - 1 : Math.max(0, histIndex - 1)
      setHistIndex(next)
      setInput(history[next])
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      if (histIndex === -1) return
      const next = histIndex + 1
      if (next >= history.length) {
        setHistIndex(-1)
        setInput('')
      } else {
        setHistIndex(next)
        setInput(history[next])
      }
    }
  }

  const colorFor = (kind: Line['kind']) =>
    kind === 'input' ? 'var(--mantine-color-teal-4)'
      : kind === 'error' ? 'var(--mantine-color-red-4)'
        : kind === 'info' ? 'var(--mantine-color-dimmed)'
          : 'var(--mantine-color-text)'

  return (
    <div
      style={{
        height: '100%',
        minHeight: 0,
        display: 'flex',
        flexDirection: 'column',
        backgroundColor: 'var(--mantine-color-dark-8)',
      }}
    >
      <Group justify="flex-end" px="xs" py={2} style={{ flex: '0 0 auto' }}>
        <Tooltip label="Clear">
          <ActionIcon variant="subtle" color="gray" size="sm" onClick={() => setLines([])} aria-label="Clear terminal">
            <IconTrash size={14} />
          </ActionIcon>
        </Tooltip>
      </Group>

      <div
        ref={logRef}
        role="log"
        aria-label="Terminal output"
        aria-live="polite"
        style={{
          flex: 1,
          minHeight: 0,
          overflowY: 'auto',
          padding: '4px 10px',
          fontFamily: 'var(--mantine-font-family-monospace)',
          fontSize: 12.5,
          lineHeight: 1.5,
        }}
      >
        {lines.map((line, i) => (
          <div key={i} style={{ color: colorFor(line.kind), whiteSpace: 'pre-wrap' }}>
            {line.kind === 'input' ? '› ' : line.kind === 'error' ? '✗ ' : line.kind === 'result' ? '= ' : ''}
            {line.text}
          </div>
        ))}
      </div>

      <Group gap={6} px={10} py={6} wrap="nowrap" style={{ flex: '0 0 auto', borderTop: '1px solid var(--mantine-color-dark-5)' }}>
        <span style={{ flex: '0 0 auto', color: 'var(--mantine-color-teal-4)', fontFamily: 'var(--mantine-font-family-monospace)' }}>›</span>
        <input
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoComplete="off"
          placeholder="Evaluate an expression… (Up/Down history, Tab completes)"
          style={{
            flex: 1,
            background: 'transparent',
            border: 'none',
            outline: 'none',
            color: 'var(--mantine-color-text)',
            fontFamily: 'var(--mantine-font-family-monospace)',
            fontSize: 13,
          }}
          aria-label="REPL expression input"
        />
      </Group>
    </div>
  )
}
