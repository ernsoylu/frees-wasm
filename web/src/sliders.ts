// Workspace parameter sliders: pin a variable, drag it, watch the solution
// follow. Pure helpers — the range policy and the override wire format — kept
// out of the component so both are testable and the rules are stated once.

/** A variable pinned to the slider strip. Persisted in the project file. */
export interface PinnedSlider {
  /** Variable name in its display spelling (what the user typed). */
  name: string
  /** Current slider value, in the variable's display units. */
  value: number
  /** Display unit, carried into the override so the solver reads it right. */
  units: string
  /** Range ends; resolved once at pin time so dragging never moves the track. */
  min: number
  max: number
}

/**
 * The range a newly pinned variable gets. Declared bounds win when both ends
 * are finite and actually bracket the value — that is the user's own
 * statement of the physical range. Otherwise ±50 % of the current value, and
 * for a value of exactly zero (where a percentage collapses to nothing) a
 * unit interval, so the handle can still move.
 */
export function sliderRange(
  value: number,
  lower?: number | null,
  upper?: number | null,
): { min: number; max: number } {
  const lo = typeof lower === 'number' && Number.isFinite(lower) ? lower : null
  const hi = typeof upper === 'number' && Number.isFinite(upper) ? upper : null
  if (lo !== null && hi !== null && hi > lo && value >= lo && value <= hi) {
    return { min: lo, max: hi }
  }
  if (!Number.isFinite(value) || value === 0) {
    return { min: -1, max: 1 }
  }
  // Half-width off the magnitude, applied symmetrically: this stays ascending
  // for negative values too (-10 → [-15, -5]), so no sign branch is needed.
  const half = Math.abs(value) * 0.5
  return { min: value - half, max: value + half }
}

/** Slider granularity: 200 steps across the range, rounded to something a
 *  human would type rather than a float artefact. */
export function sliderStep(min: number, max: number): number {
  const span = Math.abs(max - min)
  if (!Number.isFinite(span) || span <= 0) {
    return 1
  }
  const raw = span / 200
  const magnitude = 10 ** Math.floor(Math.log10(raw))
  const normalized = raw / magnitude
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10
  return nice * magnitude
}

/**
 * One pinned slider as a solve override, in the same equation form the REPL
 * uses. The backend collapses overrides by variable name (last wins), so
 * appending these after the REPL's makes a dragged slider take priority over
 * a stale REPL assignment of the same variable.
 */
export function sliderOverrideEquation(pin: PinnedSlider): string {
  const unit = pin.units && pin.units !== '-' ? ` [${pin.units}]` : ''
  return `${pin.name} = ${pin.value}${unit}`
}

/** Case-insensitive lookup, matching how the solver treats names. */
export function findPin(pins: readonly PinnedSlider[], name: string): PinnedSlider | undefined {
  const lower = name.toLowerCase()
  return pins.find((p) => p.name.toLowerCase() === lower)
}

/**
 * Names the document assigns a literal — `eta = 0.7`, `P = 250 [kPa]` — which
 * are exactly the variables a slider may drive.
 *
 * This restriction is load-bearing, not cosmetic. An override replaces *any*
 * line that assigns the name, so pinning a computed variable (`c = a*b`)
 * would silently delete the equation that defines it and quietly change the
 * model's physics. A variable given a literal is a parameter; anything else
 * is a result, and results are not draggable.
 */
export function pinnableParameters(text: string): Set<string> {
  const names = new Set<string>()
  if (!text) {
    return names
  }
  for (const rawLine of text.split('\n')) {
    // Comments never declare anything; `{...}` free text is stripped the same
    // way the parser strips it.
    const line = rawLine.replace(/\{[^}]*\}/g, '').trim()
    if (line.startsWith('//')) {
      continue
    }
    for (const segment of line.split(';')) {
      const m = /^\s*([A-Za-z_]\w*)\s*=\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*(\[[^\]]*\])?\s*$/.exec(segment)
      if (m) {
        names.add(m[1].toLowerCase())
      }
    }
  }
  return names
}
