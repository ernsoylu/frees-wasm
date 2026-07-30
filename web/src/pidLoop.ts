// Read a SigPID's loop wiring out of the document text so the PID Tuner can
// auto-extract the plant: which DYNAMIC block holds the loop, which SigConstant
// is the reference, which signal the PID measures, and whether the reference
// drives the sp or pv input (frees' PID error is sp − pv, so the wiring sets
// the plant sign). Pure string analysis — no parser needed.

export interface PidLoopInfo {
  /** DYNAMIC block name holding the loop. */
  dynamic: string
  /** Reference SigConstant instance (its constant is perturbed to probe G). */
  reference: string
  /** Measured signal variable the PID reads, e.g. "tb.out.sig". */
  output: string
  /** True when the reference feeds the PID's sp input (the common wiring). */
  referenceOnSp: boolean
}

/** The source instance whose `.out` feeds `<pid>.<port>`, or null. */
function feederOf(text: string, pid: string, port: string): string | null {
  const re = /connect\s*\(([^)]*)\)/gi
  const portRe = new RegExp(`^${pid}\\.${port}$`, 'i')
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    const parts = m[1].split(',').map((s) => s.trim())
    if (parts.some((p) => portRe.test(p))) {
      const src = parts.find((p) => /\.out$/i.test(p))
      if (src) return src.replace(/\.out$/i, '')
    }
  }
  return null
}

/**
 * Analyze the loop around SigPID `pid`. Returns null when the wiring can't be
 * identified (no DYNAMIC block, a PID input not fed by a source, or neither
 * feeder is a SigConstant reference) — the caller then falls back to manual
 * plant entry.
 */
export function analyzePidLoop(text: string, pid: string): PidLoopInfo | null {
  const dynamic = /\bDYNAMIC\s+(\w+)/i.exec(text)?.[1]
  if (dynamic === undefined) return null

  const spSrc = feederOf(text, pid, 'sp')
  const pvSrc = feederOf(text, pid, 'pv')
  if (spSrc === null || pvSrc === null) return null

  const constants = new Set(
    [...text.matchAll(/\bSigConstant\s+(\w+)/gi)].map((m) => m[1].toLowerCase()),
  )
  let reference: string
  let measurement: string
  let referenceOnSp: boolean
  if (constants.has(spSrc.toLowerCase())) {
    reference = spSrc
    measurement = pvSrc
    referenceOnSp = true
  } else if (constants.has(pvSrc.toLowerCase())) {
    reference = pvSrc
    measurement = spSrc
    referenceOnSp = false
  } else {
    return null
  }
  return { dynamic, reference, output: `${measurement.toLowerCase()}.out.sig`, referenceOnSp }
}
