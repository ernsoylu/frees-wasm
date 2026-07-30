import { describe, expect, it } from 'vitest'
import { buildReportHtml } from './report'
import type { SolveResponse } from './api'

const RESPONSE: SolveResponse = {
  success: true,
  variables: [
    { name: 'T_out', value: 360.653066, units: 'K', uncertainty: 0.5 },
    { name: 'Q_dot', value: 1250, units: 'W', uncertainty: 0 },
    { name: 'note<name>', value: 1e-9, units: '', uncertainty: 0 },
  ],
  blocks: [],
  residuals: [],
  stats: { equations: 3, unknowns: 3, blocks: 2, iterations: 7, elapsedMillis: 4, maxResidual: 1e-12 },
  solutions: [],
  unitWarnings: ['T_out: mixing K with C in "T_out = T_in + dT"'],
  error: null,
}

describe('printable report', () => {
  it('renders the sections with escaped content', () => {
    const html = buildReportHtml('Heat Exchanger Sizing', 'Q = m*cp*dT { <sensible> }', RESPONSE,
      new Date('2026-07-26T10:00:00Z'))
    expect(html).toContain('<h1>Heat Exchanger Sizing</h1>')
    expect(html).toContain('3 equations · 3 unknowns · 2 blocks')
    expect(html).toContain('Q = m*cp*dT { &lt;sensible&gt; }')
    expect(html).toContain('note&lt;name&gt;')
    expect(html).toContain('± 0.5')
    expect(html).toContain('mixing K with C')
    expect(html).toContain('generated 2026-07-26 10:00 UTC')
    expect(html).not.toContain('<sensible>')
  })

  it('formats extremes in exponential and omits zero uncertainty', () => {
    const html = buildReportHtml('p', 'x = 1', RESPONSE)
    expect(html).toContain('1.000000e-9')
    const qRow = html.split('\n').find((l) => l.includes('Q_dot'))
    expect(qRow).toBeDefined()
    expect(qRow).not.toContain('±')
  })

  it('omits the warnings section when there are none', () => {
    const html = buildReportHtml('p', 'x = 1', { ...RESPONSE, unitWarnings: [] })
    expect(html).not.toContain('Unit warnings')
  })
})
