// Fixed 10-color categorical palette for analyzer signals (design contract
// §2.5e). Colors are auto-assigned by assignment slot and persisted per-signal
// in AnalyzerSpec so sessions stay color-stable. This is deliberately a new
// palette: plots/figure.ts's colors are property-semantic (isobars/isotherms),
// not categorical. Hues are tuned to stay readable on the dark theme.

export const SIGNAL_PALETTE: readonly string[] = [
  '#4dabf7', // blue
  '#ffa94d', // orange
  '#69db7c', // green
  '#ff6b6b', // red
  '#b197fc', // violet
  '#f783ac', // pink
  '#3bc9db', // cyan
  '#ffd43b', // yellow
  '#a9e34b', // lime
  '#e599f7', // grape
]

/** Color for the n-th signal assignment (wraps past 10). */
export function signalColor(slot: number): string {
  return SIGNAL_PALETTE[((slot % SIGNAL_PALETTE.length) + SIGNAL_PALETTE.length) % SIGNAL_PALETTE.length]
}
