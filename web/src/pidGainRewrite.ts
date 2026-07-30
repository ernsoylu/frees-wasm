// Write tuned PID gains back into a SigPID component instantiation in the
// editor text. Targets the declaration `SigPID <name>( ... )` (not the
// connect() references to the same name) and updates the Kp/Ki/Kd assignments
// inside its argument list, inserting a gain that was previously absent.

import { formatValue } from './format'

export interface PidGains {
  type: 'p' | 'pi' | 'pid'
  kp: number
  ki: number
  kd: number
}

/** Locate the balanced `( … )` argument span of `SigPID <name>(`; null if absent. */
function findInstanceArgs(text: string, name: string): { open: number; close: number } | null {
  const decl = new RegExp(String.raw`\bSigPID\s+${escapeRegex(name)}\s*\(`, 'i')
  const m = decl.exec(text)
  if (m === null) return null
  const open = m.index + m[0].length - 1 // index of '('
  let depth = 0
  for (let i = open; i < text.length; i++) {
    if (text[i] === '(') depth++
    else if (text[i] === ')') {
      depth--
      if (depth === 0) return { open, close: i }
    }
  }
  return null
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`)
}

/** Set one `Key=value` inside an argument list, replacing or appending it. */
function setArg(args: string, key: string, value: string): string {
  const re = new RegExp(String.raw`(\b${key}\s*=\s*)(-?[\d.eE+]+)`, 'i')
  if (re.test(args)) return args.replace(re, `$1${value}`)
  const prefix = args.trim().length === 0 ? '' : `${args.trimEnd()}, `
  return `${prefix}${key}=${value}`
}

/**
 * Return `text` with the named SigPID's gains updated. Kp is always written;
 * Ki only for PI/PID; Kd only for PID (so a P/PI retune doesn't leave a stale
 * derivative term). Returns the original text unchanged if the instance is not
 * found.
 */
export function rewritePidGains(text: string, name: string, gains: PidGains): string {
  const span = findInstanceArgs(text, name)
  if (span === null) return text
  let args = text.slice(span.open + 1, span.close)
  args = setArg(args, 'Kp', formatValue(gains.kp))
  if (gains.type !== 'p') args = setArg(args, 'Ki', formatValue(gains.ki))
  if (gains.type === 'pid') args = setArg(args, 'Kd', formatValue(gains.kd))
  return text.slice(0, span.open + 1) + args + text.slice(span.close)
}
