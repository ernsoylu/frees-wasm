import { describe, expect, it } from 'vitest'
import { rewritePidGains } from './pidGainRewrite'

describe('rewritePidGains', () => {
  const doc = `SigConstant SP(k=303)
SigPID          PID(model$=clamped, Kp=0.05, Ki=0.005, Kd=0, tau=1, umax=1)
connect(PID.out, EXV.u)`

  it('replaces existing Kp/Ki/Kd of the named instance only', () => {
    const out = rewritePidGains(doc, 'PID', { type: 'pid', kp: 0.2, ki: 0.02, kd: 0.5 })
    expect(out).toContain('Kp=0.2')
    expect(out).toContain('Ki=0.02')
    expect(out).toContain('Kd=0.5')
    // untouched params + the connect reference survive
    expect(out).toContain('model$=clamped')
    expect(out).toContain('tau=1')
    expect(out).toContain('connect(PID.out, EXV.u)')
  })

  it('does not write Ki/Kd for a P controller, but updates Kp', () => {
    const out = rewritePidGains(doc, 'PID', { type: 'p', kp: 1.5, ki: 9, kd: 9 })
    expect(out).toContain('Kp=1.5')
    expect(out).toContain('Ki=0.005') // unchanged
    expect(out).toContain('Kd=0')
  })

  it('appends a gain that was absent', () => {
    const pi = `SigPID PID(Kp=0.1, Ki=0.01)`
    const out = rewritePidGains(pi, 'PID', { type: 'pid', kp: 0.1, ki: 0.01, kd: 0.3 })
    expect(out).toContain('Kd=0.3')
  })

  it('returns text unchanged when the instance is absent', () => {
    expect(rewritePidGains(doc, 'NOPE', { type: 'pi', kp: 1, ki: 1, kd: 0 })).toBe(doc)
  })
})
