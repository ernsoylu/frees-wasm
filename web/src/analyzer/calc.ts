// Pure helpers for the calculated-signal modal (todo.md Phase 4): request
// assembly (local signals ride inline, server-side .mf4 channels go by
// address), the over-cap recovery transform, and result → measurement
// conversion so calc channels land in the ChannelStore as first-class
// channels.

import { channelStore } from './channelStore'
import type { ImportedMeasurement } from './csvImport'
import { MeasurementApiError, type CalcRequestDto } from './measurementApi'
import type { SignalRef } from './types'

export interface CalcBinding {
  var: string
  signal: SignalRef
  interp: 'step' | 'linear'
}

export type CalcRaster = CalcRequestDto['raster']

/**
 * Assemble the calc request. Throws with a user-facing message when a bound
 * signal's data is not available (evicted local measurement).
 */
export function buildCalcRequest(
  name: string,
  formula: string,
  bindings: CalcBinding[],
  raster: CalcRaster,
): CalcRequestDto {
  const inputs: CalcRequestDto['inputs'] = bindings.map((b) => {
    const remote = channelStore.remoteAddress(b.signal)
    if (remote) {
      return {
        var: b.var,
        interp: b.interp,
        measurementId: remote.serverId,
        channel: remote.name,
        group: remote.group,
      }
    }
    const raw = channelStore.getRawRange(b.signal, null, null)
    if (!raw) {
      throw new Error(
        `The signal bound to "${b.var}" (${b.signal.channel}) is not in memory — re-import its file first.`,
      )
    }
    return { var: b.var, interp: b.interp, inline: { t: Array.from(raw.t), v: Array.from(raw.v) } }
  })
  return { name, formula, inputs, raster }
}

export interface OverCapInfo {
  actualPoints: number
  suggestedDt: number
}

/** Extract the typed RASTER_CAP_EXCEEDED payload, if that is what failed. */
export function parseOverCap(err: unknown): OverCapInfo | null {
  if (!(err instanceof MeasurementApiError) || err.payload?.code !== 'RASTER_CAP_EXCEEDED') {
    return null
  }
  const actualPoints = Number(err.payload.actualPoints)
  const suggestedDt = Number(err.payload.suggestedDt)
  if (!Number.isFinite(actualPoints) || !(suggestedDt > 0)) return null
  return { actualPoints, suggestedDt }
}

/** The one-click recovery: switch the raster to the suggested fixed dt. */
export function applyOverCap(info: OverCapInfo): CalcRaster {
  return { mode: 'fixed', dt: info.suggestedDt }
}

/** Boolean sniff, mirroring the CSV import path: all finite values ∈ {0,1}. */
export function sniffKind(v: Float64Array): 'analog' | 'boolean' {
  let sawFinite = false
  for (const x of v) {
    if (Number.isNaN(x)) continue
    sawFinite = true
    if (x !== 0 && x !== 1) return 'analog'
  }
  return sawFinite ? 'boolean' : 'analog'
}

/** Wrap a calc result as an in-memory measurement for ChannelStore.register. */
export function calcResultToMeasurement(name: string, t: number[], v: number[]): ImportedMeasurement {
  const values = Float64Array.from(v)
  let min = Number.POSITIVE_INFINITY
  let max = Number.NEGATIVE_INFINITY
  for (const x of values) {
    if (Number.isNaN(x)) continue
    if (x < min) min = x
    if (x > max) max = x
  }
  return {
    signatureName: `ƒ ${name}`,
    size: 0,
    headerHash: 'calc',
    time: Float64Array.from(t),
    rowCount: t.length,
    channels: [
      {
        name,
        unit: undefined,
        kind: sniffKind(values),
        values,
        min: min === Number.POSITIVE_INFINITY ? Number.NaN : min,
        max: max === Number.NEGATIVE_INFINITY ? Number.NaN : max,
      },
    ],
  }
}
