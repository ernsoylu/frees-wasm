// REST client for the backend measurement service (Data Analyzer Phase 3).
// .mf4 files are uploaded once, indexed server-side, and read back as
// windowed decimated envelopes — the browser never holds the raw file.

const API_BASE = import.meta.env.VITE_API_BASE || ''

export interface RemoteChannelInfo {
  name: string
  unit?: string | null
  timeMaster: boolean
  kind: 'analog' | 'boolean' | 'string'
}

export interface RemoteGroupInfo {
  index: number
  name: string
  records: number
  channels: RemoteChannelInfo[]
}

export interface RemoteMeasurement {
  measurementId: string
  name: string
  size: number
  metadata: { groups: RemoteGroupInfo[] }
}

export interface RemoteWindow {
  t: number[]
  v: number[] | null
  min: number[] | null
  max: number[] | null
  decimated: boolean
  totalSamples: number
  unit?: string | null
  kind: string
}

/** Error carrying the HTTP status + typed payload (e.g. RASTER_CAP_EXCEEDED). */
export class MeasurementApiError extends Error {
  readonly status: number
  readonly payload: Record<string, unknown> | null
  constructor(status: number, message: string, payload: Record<string, unknown> | null = null) {
    super(message)
    this.name = 'MeasurementApiError'
    this.status = status
    this.payload = payload
  }
}

async function fail(response: Response): Promise<never> {
  let message = `Measurement request failed (${response.status}).`
  let payload: Record<string, unknown> | null = null
  try {
    const body = await response.json()
    if (body && typeof body === 'object') payload = body as Record<string, unknown>
    if (typeof payload?.error === 'string') message = payload.error as string
  } catch {
    /* non-JSON error body */
  }
  throw new MeasurementApiError(response.status, message, payload)
}

export async function uploadMeasurement(file: File): Promise<RemoteMeasurement> {
  const form = new FormData()
  form.append('file', file)
  const response = await fetch(`${API_BASE}/api/measurements`, { method: 'POST', body: form })
  if (!response.ok) await fail(response)
  return (await response.json()) as RemoteMeasurement
}

export async function fetchChannelWindow(
  id: string,
  group: number,
  channel: string,
  from: number | null,
  to: number | null,
  maxPoints: number,
): Promise<RemoteWindow> {
  const params = new URLSearchParams({ group: String(group), maxPoints: String(maxPoints) })
  if (from !== null) params.set('from', String(from))
  if (to !== null) params.set('to', String(to))
  const response = await fetch(
    `${API_BASE}/api/measurements/${id}/channels/${encodeURIComponent(channel)}?${params}`,
  )
  if (!response.ok) await fail(response)
  return (await response.json()) as RemoteWindow
}

export interface CalcRequestDto {
  name: string
  formula: string
  inputs: {
    var: string
    interp: 'step' | 'linear'
    measurementId?: string
    channel?: string
    group?: number
    inline?: { t: number[]; v: number[] }
  }[]
  raster: { mode: 'merge' } | { mode: 'fixed'; dt: number } | { mode: 'sameAs'; sameAs: string }
}

export interface CalcResultDto {
  name: string
  t: number[]
  v: number[]
}

/**
 * Evaluate a calculated signal server-side (Phase 4). Small/call-free
 * formulas answer synchronously; property-function formulas over large
 * rasters come back as 202 + jobId and are polled here (decision 3 — no
 * mid-job cancel in v1, abandoning the poll just leaves the job to its TTL).
 */
export async function calcSignal(request: CalcRequestDto): Promise<CalcResultDto> {
  const response = await fetch(`${API_BASE}/api/measurements/calc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (response.status === 202) {
    const ticket = (await response.json()) as { jobId?: string }
    if (!ticket.jobId) throw new MeasurementApiError(202, 'Job submission did not return a jobId.')
    return pollCalcJob(ticket.jobId)
  }
  if (!response.ok) await fail(response)
  return (await response.json()) as CalcResultDto
}

async function pollCalcJob(jobId: string): Promise<CalcResultDto> {
  const deadline = Date.now() + 5 * 60_000
  while (Date.now() < deadline) {
    const response = await fetch(`${API_BASE}/api/jobs/${jobId}`)
    if (response.status === 404) throw new MeasurementApiError(404, 'Calc job not found (API node restarted?).')
    if (!response.ok) await fail(response)
    const state = (await response.json()) as { status: string; result?: CalcResultDto; error?: string }
    if (state.status === 'COMPLETED' && state.result) return state.result
    if (state.status === 'FAILED') {
      throw new MeasurementApiError(422, state.error ?? 'Calculated-signal job failed.')
    }
    await new Promise((resolve) => setTimeout(resolve, 400))
  }
  throw new MeasurementApiError(408, 'Calculated-signal job timed out after 5 minutes.')
}

export function deleteMeasurement(id: string): void {
  // Fire-and-forget; the backend TTL sweep is the safety net.
  void fetch(`${API_BASE}/api/measurements/${id}`, { method: 'DELETE' }).catch(() => undefined)
}
