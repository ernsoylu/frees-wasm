// CSV ingestion contract tests (§2.5c): delimiters, unit rows, NaN gaps, and
// the full time-column detection matrix — ISO / epoch s / epoch ms / relative /
// index+dt / ambiguous→ask / non-monotonic→hard error with row numbers.
// Fixtures are generated inline; large ones are built programmatically.

import { describe, expect, it } from 'vitest'
import {
  CsvImportError,
  detectHeader,
  detectTimeColumn,
  GrowableFloat64,
  headerHash,
  ingestCsvText,
  type IngestResult,
} from './csvImport'

function okResult(text: string, choice?: Parameters<typeof ingestCsvText>[1]): IngestResult {
  const outcome = ingestCsvText(text, choice)
  if (outcome.status !== 'ok') throw new Error(`expected ok, got ask: ${JSON.stringify(outcome)}`)
  return outcome.result
}

describe('GrowableFloat64', () => {
  it('grows past the initial capacity and trims on finish', () => {
    const g = new GrowableFloat64()
    for (let i = 0; i < 5000; i++) g.push(i)
    const out = g.finish()
    expect(out.length).toBe(5000)
    expect(out[4999]).toBe(4999)
  })
})

describe('detectHeader', () => {
  it('detects a header row and a bracketed unit row on line 2', () => {
    const h = detectHeader([
      ['time', 'speed', 'active'],
      ['[s]', '[m/s]', '[-]'],
      ['0', '1.5', '1'],
    ])
    expect(h.names).toEqual(['time', 'speed', 'active'])
    expect(h.units[1]).toBe('m/s')
    expect(h.dataStart).toBe(2)
  })

  it('handles headerless all-numeric files with generated names', () => {
    const h = detectHeader([
      ['0', '1.5'],
      ['1', '2.5'],
    ])
    expect(h.names).toEqual(['col1', 'col2'])
    expect(h.dataStart).toBe(0)
  })

  it('does not consume a numeric data row as units', () => {
    const h = detectHeader([
      ['time', 'speed'],
      ['0', '1.5'],
    ])
    expect(h.units).toEqual([])
    expect(h.dataStart).toBe(1)
  })
})

describe('delimiters', () => {
  const expectParsed = (text: string) => {
    const r = okResult(text)
    expect(r.channels.map((c) => c.name)).toEqual(['speed'])
    expect(Array.from(r.time)).toEqual([0, 1, 2])
    expect(Array.from(r.channels[0].values ?? [])).toEqual([10, 20, 30])
  }

  it('parses comma-separated data', () => {
    expectParsed('time,speed\n0,10\n1,20\n2,30\n')
  })

  it('parses tab-separated data', () => {
    expectParsed('time\tspeed\n0\t10\n1\t20\n2\t30\n')
  })

  it('parses semicolon-separated data', () => {
    expectParsed('time;speed\n0;10\n1;20\n2;30\n')
  })
})

describe('gaps and channel kinds', () => {
  it('turns empty cells into NaN gaps', () => {
    const r = okResult('time,a\n0,1\n1,\n2,3\n')
    const v = r.channels[0].values ?? new Float64Array(0)
    expect(v[0]).toBe(1)
    expect(Number.isNaN(v[1])).toBe(true)
    expect(v[2]).toBe(3)
    expect(r.channels[0].min).toBe(1)
    expect(r.channels[0].max).toBe(3)
  })

  it('classifies 0/1 and true/false columns as boolean', () => {
    const r = okResult('time,flag,txt\n0,true,a\n1,false,b\n2,1,c\n3,0,d\n')
    const flag = r.channels.find((c) => c.name === 'flag')
    expect(flag?.kind).toBe('boolean')
    expect(Array.from(flag?.values ?? [])).toEqual([1, 0, 1, 0])
  })

  it('keeps string columns listed but unplottable (values null, §2.5d)', () => {
    const r = okResult('time,state\n0,IDLE\n1,RUN\n2,RUN\n')
    expect(r.channels[0].kind).toBe('string')
    expect(r.channels[0].values).toBeNull()
  })
})

describe('time-column detection matrix (§2.5c)', () => {
  it('name match: "time" column wins even when other columns are monotonic', () => {
    const r = okResult('rpm,time\n100,0\n200,0.1\n300,0.2\n')
    expect(r.timeSource).toEqual({ mode: 'column', column: 1, kind: 'relative' })
    expect(Array.from(r.time)).toEqual([0, 0.1, 0.2])
  })

  it('ISO-8601 timestamps convert to strictly increasing seconds', () => {
    const r = okResult(
      'timestamp,v\n2024-01-01T00:00:00Z,1\n2024-01-01T00:00:01Z,2\n2024-01-01T00:00:02.5Z,3\n',
    )
    expect(r.timeSource.mode === 'column' && r.timeSource.kind).toBe('iso')
    expect(r.time[1] - r.time[0]).toBeCloseTo(1, 9)
    expect(r.time[2] - r.time[1]).toBeCloseTo(1.5, 9)
  })

  it('epoch seconds are recognized by magnitude and kept as seconds', () => {
    const r = okResult('time,v\n1700000000,1\n1700000001,2\n1700000002,3\n')
    expect(r.timeSource.mode === 'column' && r.timeSource.kind).toBe('epoch-s')
    expect(r.time[0]).toBe(1700000000)
  })

  it('epoch milliseconds are recognized by magnitude and divided to seconds', () => {
    const r = okResult('time,v\n1700000000000,1\n1700000000100,2\n1700000000200,3\n')
    expect(r.timeSource.mode === 'column' && r.timeSource.kind).toBe('epoch-ms')
    // Float64 can't represent 1.7e9 + 0.1 s exactly — sub-µs tolerance is the
    // best epoch-ms data can carry.
    expect(r.time[1] - r.time[0]).toBeCloseTo(0.1, 6)
  })

  it('relative seconds pass through unchanged', () => {
    const r = okResult('t,v\n0,1\n0.5,2\n1,3\n')
    expect(r.timeSource.mode === 'column' && r.timeSource.kind).toBe('relative')
    expect(Array.from(r.time)).toEqual([0, 0.5, 1])
  })

  it('index-based data with a user-supplied dt builds t = i*dt', () => {
    const r = okResult('a,b\n5,1\n3,2\n8,3\n', { mode: 'dt', dt: 0.25 })
    expect(Array.from(r.time)).toEqual([0, 0.25, 0.5])
    // Both columns stay data channels in dt mode.
    expect(r.channels.map((c) => c.name)).toEqual(['a', 'b'])
  })

  it('ambiguous (two monotonic columns, no name match) → ask, never guess', () => {
    const outcome = ingestCsvText('a,b\n0,10\n1,20\n2,30\n')
    expect(outcome.status).toBe('ask')
    if (outcome.status === 'ask') {
      expect(outcome.candidates.map((c) => c.name)).toEqual(['a', 'b'])
    }
  })

  it('absent time column (nothing monotonic) → ask with no candidates', () => {
    const outcome = ingestCsvText('a,b\n5,10\n3,5\n8,30\n')
    expect(outcome.status).toBe('ask')
    if (outcome.status === 'ask') expect(outcome.candidates).toEqual([])
  })

  it('a user choice resolves an ambiguous file', () => {
    const r = okResult('a,b\n0,10\n1,20\n2,30\n', { mode: 'column', column: 0 })
    expect(Array.from(r.time)).toEqual([0, 1, 2])
    expect(r.channels.map((c) => c.name)).toEqual(['b'])
  })

  it('detectTimeColumn prefers the named column directly', () => {
    const det = detectTimeColumn(
      ['zeit', 'v'],
      [
        ['0', '5'],
        ['1', '4'],
        ['2', '9'],
      ],
    )
    expect(det.status).toBe('ok')
    if (det.status === 'ok') expect(det.candidate.column).toBe(0)
  })
})

describe('strict monotonicity (hard errors, strict-over-warn)', () => {
  it('duplicate timestamps → CsvImportError naming the offending row', () => {
    let err: unknown
    try {
      okResult('time,v\n0,1\n1,2\n1,3\n2,4\n', { mode: 'column', column: 0 })
    } catch (e) {
      err = e
    }
    expect(err).toBeInstanceOf(CsvImportError)
    const cie = err as CsvImportError
    expect(cie.code).toBe('NON_MONOTONIC')
    // header row 1, data rows 2..5 → the duplicate "1" is file row 4.
    expect(cie.rows).toEqual([4])
    expect(cie.message).toContain('4')
  })

  it('out-of-order rows detected beyond the detection sample window', () => {
    // 300 clean rows, then one that jumps backwards — past the 200-row sample.
    const lines = ['time,v']
    for (let i = 0; i < 300; i++) lines.push(`${i * 0.01},${i}`)
    lines.push('1.5,999') // 1.5 < 2.99 → non-monotonic at file row 302
    lines.push('3.0,1000')
    let err: unknown
    try {
      okResult(lines.join('\n'))
    } catch (e) {
      err = e
    }
    expect(err).toBeInstanceOf(CsvImportError)
    expect((err as CsvImportError).code).toBe('NON_MONOTONIC')
    expect((err as CsvImportError).rows).toEqual([302])
  })

  it('rejects empty files and time-only files', () => {
    expect(() => okResult('time\n0\n1\n', { mode: 'column', column: 0 })).toThrowError(
      CsvImportError,
    )
    expect(() => okResult('\n')).toThrowError(CsvImportError)
  })
})

describe('file signature', () => {
  it('headerHash is stable for identical input and sensitive to both parts', () => {
    const a = headerHash('head-bytes', ['t', 'v'])
    expect(headerHash('head-bytes', ['t', 'v'])).toBe(a)
    expect(headerHash('other-bytes', ['t', 'v'])).not.toBe(a)
    expect(headerHash('head-bytes', ['t', 'w'])).not.toBe(a)
    expect(a).toMatch(/^[0-9a-f]{8}$/)
  })
})
