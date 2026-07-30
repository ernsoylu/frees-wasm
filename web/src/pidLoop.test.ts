import { describe, expect, it } from 'vitest'
import { analyzePidLoop } from './pidLoop'

describe('analyzePidLoop', () => {
  it('reads a standard loop (reference on sp)', () => {
    const doc = `SigConstant SP(k=5)
SigPID PID(Kp=2, Ki=1, Kd=0)
SigFirstOrder PLANT(tau=2, y0=0)
connect(SP.out, PID.sp)
connect(PID.out, PLANT.in)
connect(PLANT.out, PID.pv)
DYNAMIC loop(method = ode23s, time = 0 .. 40, points = 400)
END`
    expect(analyzePidLoop(doc, 'PID')).toEqual({
      dynamic: 'loop',
      reference: 'SP',
      output: 'plant.out.sig',
      referenceOnSp: true,
    })
  })

  it('reads a reverse-acting loop (reference on pv, measurement on sp)', () => {
    const doc = `SigConstant SP(k=303)
SigThermalProbe TB()
SigPID PID(model$=clamped, Kp=0.05, Ki=0.005)
connect(TB.out, PID.sp)
connect(SP.out, PID.pv)
connect(PID.out, EXV.u)
DYNAMIC cool(method = ode23s, time = 0 .. 4000, points = 400)
END`
    expect(analyzePidLoop(doc, 'PID')).toEqual({
      dynamic: 'cool',
      reference: 'SP',
      output: 'tb.out.sig',
      referenceOnSp: false,
    })
  })

  it('returns null when there is no DYNAMIC block', () => {
    expect(analyzePidLoop('SigConstant SP(k=1)\nconnect(SP.out, PID.sp)', 'PID')).toBeNull()
  })

  it('returns null when neither PID input is fed by a SigConstant', () => {
    const doc = `SigRamp R()
SigProbe P()
connect(R.out, PID.sp)
connect(P.out, PID.pv)
DYNAMIC loop(time = 0 .. 1)
END`
    expect(analyzePidLoop(doc, 'PID')).toBeNull()
  })
})
