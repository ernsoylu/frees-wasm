import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { runCompute } from './api'

/** Builds a minimal Response-like object for the fetch mock. */
function mockResponse(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
    text: async () => (typeof body === 'string' ? body : JSON.stringify(body)),
  } as unknown as Response
}

describe('runCompute (async submit + poll)', () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
    vi.useRealTimers()
  })

  it('polls a 202 PENDING job until COMPLETED and returns the result DTO', async () => {
    const calls: string[] = []
    globalThis.fetch = vi.fn(async (url: string | URL) => {
      const path = String(url)
      calls.push(path)
      if (path.endsWith('/api/solve')) {
        return mockResponse(202, { jobId: 'job-1', status: 'PENDING' })
      }
      if (path.endsWith('/api/jobs/job-1')) {
        return mockResponse(200, {
          jobId: 'job-1',
          status: 'PENDING',
          error: null,
          result: null,
        })
      }
      throw new Error(`unexpected fetch ${path}`)
    }) as unknown as typeof fetch

    const pending = runCompute('/api/solve', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    })

    // First poll returns PENDING. Advance the 150ms poll interval so a second
    // poll fires and returns COMPLETED.
    await vi.advanceTimersByTimeAsync(200)
    // Swap the job response to COMPLETED before the next poll resolves.
    globalThis.fetch = vi.fn(async (url: string | URL) => {
      if (String(url).endsWith('/api/jobs/job-1')) {
        return mockResponse(200, {
          jobId: 'job-1',
          status: 'COMPLETED',
          error: null,
          result: { success: true, variables: [{ name: 'x', value: 2 }] },
        })
      }
      throw new Error('unexpected')
    }) as unknown as typeof fetch

    await vi.advanceTimersByTimeAsync(200)
    const outcome = await pending

    expect(outcome.kind).toBe('completed')
    if (outcome.kind === 'completed') {
      expect(outcome.result.success).toBe(true)
      expect(outcome.result.variables[0].value).toBe(2)
    }
    // The submit POST plus at least one poll happened.
    expect(calls.some(c => c.endsWith('/api/solve'))).toBe(true)
  })

  it('maps a FAILED job to a failure outcome with the error message', async () => {
    globalThis.fetch = vi.fn(async (url: string | URL) => {
      const path = String(url)
      if (path.endsWith('/api/solve')) {
        return mockResponse(202, { jobId: 'job-2', status: 'PENDING' })
      }
      return mockResponse(200, {
        jobId: 'job-2',
        status: 'FAILED',
        error: 'Singular Jacobian',
        result: null,
      })
    }) as unknown as typeof fetch

    const pending = runCompute('/api/solve', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    })
    await vi.advanceTimersByTimeAsync(200)
    const outcome = await pending

    expect(outcome.kind).toBe('failed')
    if (outcome.kind === 'failed') {
      expect(outcome.error).toBe('Singular Jacobian')
    }
  })

  it('maps a synchronous 4xx validation rejection to a failure outcome', async () => {
    globalThis.fetch = vi.fn(async () =>
      mockResponse(400, { success: false, error: 'Syntax error: unexpected =' }),
    ) as unknown as typeof fetch

    const outcome = await runCompute('/api/solve', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    })

    expect(outcome.kind).toBe('failed')
    if (outcome.kind === 'failed') {
      expect(outcome.error).toContain('Syntax error')
    }
  })

  it('maps a network failure to a failure outcome', async () => {
    globalThis.fetch = vi.fn(async () => {
      throw new Error('connection refused')
    }) as unknown as typeof fetch

    const outcome = await runCompute('/api/solve', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    })

    expect(outcome.kind).toBe('failed')
    if (outcome.kind === 'failed') {
      expect(outcome.error).toContain('connection refused')
    }
  })
})
