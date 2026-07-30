export function formatValue(value: number | null | undefined, decimals?: number): string {
  if (value === null || value === undefined || Number.isNaN(value)) return '—'
  if (!Number.isFinite(value)) return String(value)
  // An explicit decimal-place count (e.g. a Diagram output element's setting)
  // overrides the adaptive formatter below.
  if (decimals !== undefined && decimals >= 0) return value.toFixed(decimals)
  if (value === 0) return '0'
  const abs = Math.abs(value)
  if (abs >= 1e7 || abs < 1e-4) return value.toExponential(6)
  return Number(value.toPrecision(8)).toString()
}

/**
 * Stable React keys for lists of possibly-duplicated strings: the value
 * itself plus its occurrence number.
 */
export function withStableKeys(
  items: string[],
): { key: string; value: string }[] {
  const counts = new Map<string, number>()
  return items.map((value) => {
    const occurrence = (counts.get(value) ?? 0) + 1
    counts.set(value, occurrence)
    return { key: `${value}#${occurrence}`, value }
  })
}
