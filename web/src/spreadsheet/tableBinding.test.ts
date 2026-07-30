import { describe, expect, it } from 'vitest'
import { FunctionTableSpec, ParamTableSpec, toFunctionTableDtos } from '../tables'
import {
  boundColumnCount,
  isErrorValue,
  isHostedTable,
  paramComputedValue,
  protectedRangesFor,
  RegionRead,
  sheetEditsToSpec,
  specToSheetData,
  TABLE_MAX_ROWS,
} from './tableBinding'

function spec1D(over: Partial<FunctionTableSpec> = {}): FunctionTableSpec {
  return {
    id: 't1',
    kind: 'function',
    name: 'eff',
    argName: 'Re',
    paramName: '',
    xLog: false,
    yLog: false,
    columns: [''],
    rows: [
      { x: '1', ys: ['10'] },
      { x: '2', ys: ['20'] },
    ],
    is1D: true,
    ...over,
  }
}

function spec2D(over: Partial<FunctionTableSpec> = {}): FunctionTableSpec {
  return {
    id: 't2',
    kind: 'function',
    name: 'fanCurve',
    argName: 'V',
    paramName: 'rpm',
    xLog: false,
    yLog: false,
    columns: ['1000', '2000'],
    rows: [
      { x: '0', ys: ['100', '200'] },
      { x: '1', ys: ['80', '160'] },
    ],
    is1D: false,
    ...over,
  }
}

/** Region matrices as the facade would return them for a given sheet. */
function readOf(sheet: { celldata: { r: number; c: number; v: { v: unknown; f?: string } }[] }): RegionRead {
  let maxR = 0
  let maxC = 0
  for (const cd of sheet.celldata) {
    if (cd.r > maxR) maxR = cd.r
    if (cd.c > maxC) maxC = cd.c
  }
  const values: (string | number | null)[][] = Array.from({ length: maxR + 1 }, () =>
    Array(maxC + 1).fill(null),
  )
  const formulas: string[][] = Array.from({ length: maxR + 1 }, () => Array(maxC + 1).fill(''))
  for (const cd of sheet.celldata) {
    values[cd.r][cd.c] = cd.v.v as string | number
    if (cd.v.f) formulas[cd.r][cd.c] = cd.v.f
  }
  return { values, formulas }
}

describe('specToSheetData', () => {
  it('lays out header + data anchored at A1', () => {
    const sheet = specToSheetData(spec2D())
    const at = (r: number, c: number) => sheet.celldata.find((cd) => cd.r === r && cd.c === c)?.v
    expect(at(0, 0)?.v).toBe('V')
    expect(at(0, 1)?.v).toBe(1000) // param header, numeric
    expect(at(0, 2)?.v).toBe(2000)
    expect(at(1, 0)?.v).toBe(0)
    expect(at(2, 2)?.v).toBe(160)
    expect(sheet.styles?.A1).toContain('bold')
    expect(sheet.id).toBe('t2')
    expect(sheet.name).toBe('fanCurve')
  })

  it('uses a fixed y label for 1-D tables', () => {
    const sheet = specToSheetData(spec1D())
    const b1 = sheet.celldata.find((cd) => cd.r === 0 && cd.c === 1)?.v
    expect(b1?.v).toBe('y')
  })

  it('re-applies the formula overlay as f cells (contract f)', () => {
    const sheet = specToSheetData(spec1D({ formulas: { B2: '=A2*10' } }))
    const b2 = sheet.celldata.find((cd) => cd.r === 1 && cd.c === 1)?.v
    expect(b2?.f).toBe('=A2*10')
  })

  it('emits overlay cells even when the cached value is blank', () => {
    const sheet = specToSheetData(
      spec1D({ rows: [{ x: '1', ys: [''] }], formulas: { B2: '=1/0' } }),
    )
    const b2 = sheet.celldata.find((cd) => cd.r === 1 && cd.c === 1)?.v
    expect(b2?.f).toBe('=1/0')
  })
})

describe('sheetEditsToSpec round-trip', () => {
  it('spec -> sheet -> spec is identity for values and overlay', () => {
    const spec = spec2D({ formulas: { B2: '=C2/2' } })
    const back = sheetEditsToSpec(spec, readOf(specToSheetData(spec)))
    const f = back.spec as FunctionTableSpec
    expect(f.rows).toEqual(spec.rows)
    expect(f.columns).toEqual(spec.columns)
    expect(f.formulas).toEqual(spec.formulas)
    expect(back.truncated).toBe(false)
    expect(back.errorCells).toEqual([])
    expect(back.outOfRegion).toBe(false)
  })

  it('maps 2-D header edits back to spec.columns but never changes the column count', () => {
    const spec = spec2D()
    const read = readOf(specToSheetData(spec))
    read.values[0][2] = 3000
    const back = sheetEditsToSpec(spec, read)
    const f = back.spec as FunctionTableSpec
    expect(f.columns).toEqual(['1000', '3000'])
    expect(f.columns.length).toBe(spec.columns.length)
  })

  it('keeps blank/invalid-cell omission in step with toFunctionTableDtos', () => {
    const spec = spec1D()
    const read = readOf(specToSheetData(spec))
    read.values[2][1] = 'garbage' // non-numeric y
    const back = sheetEditsToSpec(spec, read)
    const dtos = toFunctionTableDtos([back.spec])
    expect(dtos[0].curves[0].points).toEqual([[1, 10]]) // row 2 omitted
  })

  it('ignores content outside the bound columns (spatial filter) and flags it', () => {
    const spec = spec1D()
    const read = readOf(specToSheetData(spec))
    read.values[1].push('stray') // column C on a 2-column table
    read.formulas[1].push('')
    const back = sheetEditsToSpec(spec, read)
    expect(back.spec.rows).toEqual(spec.rows)
    expect(back.outOfRegion).toBe(true)
  })

  it('truncates at the row cap (contract a)', () => {
    const spec = spec1D()
    const values: (string | number | null)[][] = [['Re', 'y']]
    for (let i = 0; i < TABLE_MAX_ROWS + 100; i++) values.push([i, i * 2])
    const formulas = values.map((row) => row.map(() => ''))
    const back = sheetEditsToSpec(spec, { values, formulas })
    expect(back.truncated).toBe(true)
    expect(back.spec.rows.length).toBe(TABLE_MAX_ROWS)
  })

  it('sanitizes formula error strings to omitted values, keeping the overlay (contract f)', () => {
    const spec = spec1D()
    const read = readOf(specToSheetData(spec))
    read.values[1][1] = '#REF!'
    read.formulas[1][1] = '=Gone!A1'
    const back = sheetEditsToSpec(spec, read)
    const f = back.spec as FunctionTableSpec
    expect(back.errorCells).toEqual(['B2'])
    expect(f.rows[0].ys[0]).toBe('')
    expect(f.formulas?.B2).toBe('=Gone!A1')
    const dtos = toFunctionTableDtos([back.spec])
    expect(dtos[0].curves[0].points).toEqual([[2, 20]])
  })

  it('drops the overlay entry when a formula is replaced by a literal', () => {
    const spec = spec1D({ formulas: { B2: '=A2*10' } })
    const read = readOf(specToSheetData(spec))
    read.formulas[1][1] = ''
    read.values[1][1] = 42
    const back = sheetEditsToSpec(spec, read)
    const f = back.spec as FunctionTableSpec
    expect(f.formulas).toBeUndefined()
    expect(f.rows[0].ys[0]).toBe('42')
  })

  it('never mutates code-sourced specs (mapper defense in depth)', () => {
    const spec = spec1D({ source: 'code' })
    const read = readOf(specToSheetData(spec))
    read.values[1][1] = 999999
    const back = sheetEditsToSpec(spec, read)
    expect(back.spec).toBe(spec)
  })

  it('captures rows typed below the current data (region growth)', () => {
    const spec = spec1D()
    const read = readOf(specToSheetData(spec))
    read.values.push([3, 30])
    read.formulas.push(['', ''])
    const back = sheetEditsToSpec(spec, read)
    expect(back.spec.rows.length).toBe(3)
    expect(back.spec.rows[2]).toEqual({ x: '3', ys: ['30'] })
  })
})

describe('protection map', () => {
  it('protects only schema header cells on GUI tables', () => {
    expect(protectedRangesFor(spec1D())).toEqual(['A1:B1'])
    expect(protectedRangesFor(spec2D())).toEqual(['A1:A1'])
  })
  it('protects the whole region on code tables', () => {
    const ranges = protectedRangesFor(spec2D({ source: 'code' }))
    expect(ranges).toHaveLength(1)
    expect(ranges[0].startsWith('A1:C')).toBe(true)
  })
})

describe('error literal detection', () => {
  it.each(['#REF!', '#DIV/0!', '#VALUE!', '#NAME?', '#N/A', '#NUM!'])('flags %s', (s) => {
    expect(isErrorValue(s)).toBe(true)
  })
  it('does not flag ordinary strings or numbers', () => {
    expect(isErrorValue('#hashtag')).toBe(false)
    expect(isErrorValue('REF')).toBe(false)
    expect(isErrorValue(42)).toBe(false)
  })
})

describe('boundColumnCount', () => {
  it('is x plus one per curve', () => {
    expect(boundColumnCount(spec1D())).toBe(2)
    expect(boundColumnCount(spec2D())).toBe(3)
  })
})

// ---------------------------------------------------------------------------
// Parametric tables (Phase 2)

function paramSpec(over: Partial<ParamTableSpec> = {}): ParamTableSpec {
  return {
    id: 'p1',
    kind: 'parametric',
    name: 'Sweep',
    vars: ['T', 'P'],
    rows: [
      { id: 'r1', values: { T: '300', P: '' } },
      { id: 'r2', values: { T: '350', P: '' } },
    ],
    results: [],
    stats: null,
    checkResult: null,
    checkMessage: '',
    ...over,
  }
}

function solvedParam(): ParamTableSpec {
  return paramSpec({
    results: [
      { success: true, values: { T: 300, P: 101.3 } },
      { success: false, error: 'diverged', values: {} },
    ] as ParamTableSpec['results'],
  })
}

describe('parametric specToSheetData', () => {
  it('lays out Run column + var headers + inputs', () => {
    const sheet = specToSheetData(paramSpec())
    const at = (r: number, c: number) => sheet.celldata.find((cd) => cd.r === r && cd.c === c)?.v
    expect(at(0, 0)?.v).toBe('Run')
    expect(at(0, 1)?.v).toBe('T')
    expect(at(0, 2)?.v).toBe('P')
    expect(at(1, 0)?.v).toBe(1)
    expect(at(1, 1)?.v).toBe(300)
    // Blank input cells are materialized as empty bordered cells — the
    // table must read as a bounded grid ("these cells should exist").
    expect(at(1, 2)?.v).toBe('')
    expect(sheet.styles?.C2).toContain('border')
    expect(sheet.rowCount ?? 0).toBeGreaterThanOrEqual(1000)
  })

  it('materializes computed cells (blank input + successful run) with the green style', () => {
    const sheet = specToSheetData(solvedParam())
    const at = (r: number, c: number) => sheet.celldata.find((cd) => cd.r === r && cd.c === c)?.v
    expect(at(1, 2)?.v).toBe(101.3) // P computed for run 1
    expect(sheet.styles?.C2).toContain('color')
    expect(at(2, 0)?.v).toBe('2 ✗') // failed run marker
  })
})

describe('parametric sheetEditsToSpec', () => {
  it('round-trips inputs, preserving row identities, without invalidating', () => {
    const spec = solvedParam()
    const back = sheetEditsToSpec(spec, readOf(specToSheetData(spec)))
    const p = back.spec as ParamTableSpec
    expect(p.rows.map((r) => r.id)).toEqual(['r1', 'r2'])
    expect(p.rows[0].values).toEqual({ T: '300', P: '' }) // computed cell NOT an input
    expect(p.results.length).toBe(2) // untouched -> results kept
  })

  it('an input edit invalidates results/check', () => {
    const spec = solvedParam()
    const read = readOf(specToSheetData(spec))
    read.values[1][1] = 310 // edit T for run 1
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows[0].values.T).toBe('310')
    expect(p.results).toEqual([])
    expect(p.checkResult).toBeNull()
  })

  it('typing over a computed cell turns it into an input override and invalidates', () => {
    const spec = solvedParam()
    const read = readOf(specToSheetData(spec))
    read.values[1][2] = 999 // overwrite computed P
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows[0].values.P).toBe('999')
    expect(p.results).toEqual([])
  })

  it('typing below the last run adds rows; blank starter rows are never trimmed', () => {
    const spec = paramSpec()
    const read = readOf(specToSheetData(spec))
    read.values.push([null, 400, null])
    read.formulas.push(['', '', ''])
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows.length).toBe(3)
    expect(p.rows[2].values.T).toBe('400')

    const untouched = sheetEditsToSpec(spec, readOf(specToSheetData(spec))).spec as ParamTableSpec
    expect(untouched.rows.length).toBe(2) // floor = previous count
  })

  it('drops trailing runs the user deleted by clearing their cells', () => {
    // 4 filled runs; the user hand-clears the last two -> 2 runs, invalidated.
    const spec = paramSpec({
      rows: [
        { id: 'r1', values: { T: '300', P: '' } },
        { id: 'r2', values: { T: '350', P: '' } },
        { id: 'r3', values: { T: '400', P: '' } },
        { id: 'r4', values: { T: '450', P: '' } },
      ],
    })
    const read = readOf(specToSheetData(spec))
    read.values[3][1] = null
    read.values[4][1] = null
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows.length).toBe(2)
    expect(p.rows.map((r) => r.id)).toEqual(['r1', 'r2'])
  })

  it('keeps rows that were already blank (Add Row survives unrelated edits)', () => {
    // Third run added via Add Row (blank); editing run 1 must not drop it.
    const spec = paramSpec({
      rows: [
        { id: 'r1', values: { T: '300', P: '' } },
        { id: 'r2', values: { T: '350', P: '' } },
        { id: 'r3', values: {} },
      ],
    })
    const read = readOf(specToSheetData(spec))
    read.values[1][1] = 310
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows.length).toBe(3)
    expect(p.rows[0].values.T).toBe('310')
  })

  it('clearing a middle row keeps it as a blank (free-solve) run', () => {
    const spec = paramSpec({
      rows: [
        { id: 'r1', values: { T: '300', P: '' } },
        { id: 'r2', values: { T: '350', P: '' } },
        { id: 'r3', values: { T: '400', P: '' } },
      ],
    })
    const read = readOf(specToSheetData(spec))
    read.values[2][1] = null // clear run 2 only
    const p = sheetEditsToSpec(spec, read).spec as ParamTableSpec
    expect(p.rows.length).toBe(3)
    expect(p.rows[1].values.T).toBe('')
  })

  it('flags Run-column drift so the host repaints (fill-drag through protection)', () => {
    const spec = paramSpec()
    // Pristine materialized sheet: labels match -> no drift.
    const clean = sheetEditsToSpec(spec, readOf(specToSheetData(spec)))
    expect(clean.runColumnDrift).toBe(false)

    // Run numbers drag-filled below the table -> drift.
    const below = readOf(specToSheetData(spec))
    below.values.push([5, null, null])
    below.formulas.push(['', '', ''])
    expect(sheetEditsToSpec(spec, below).runColumnDrift).toBe(true)

    // Tampered in-range label -> drift.
    const tampered = readOf(specToSheetData(spec))
    tampered.values[1][0] = 99
    expect(sheetEditsToSpec(spec, tampered).runColumnDrift).toBe(true)

    // Failed-run marker is a legitimate label -> no drift.
    const solved = solvedParam()
    const withMarker = sheetEditsToSpec(solved, readOf(specToSheetData(solved)))
    expect(withMarker.runColumnDrift).toBe(false)
  })

  it('never mutates code/ODE specs and they are not hosted', () => {
    const code = paramSpec({ source: 'code' })
    const read = readOf(specToSheetData(code))
    read.values[1][1] = 999999
    expect(sheetEditsToSpec(code, read).spec).toBe(code)
    expect(isHostedTable(code)).toBe(false)
    expect(isHostedTable(paramSpec())).toBe(true)
    expect(isHostedTable(spec1D({ source: 'code' }))).toBe(true) // code FUNCTION tables are hosted (protected)
  })
})

describe('parametric protection + computed detection', () => {
  it('protects the header row and Run column on GUI tables', () => {
    expect(protectedRangesFor(paramSpec())).toEqual(['A1:C1', `A1:A${TABLE_MAX_ROWS + 1}`])
  })
  it('paramComputedValue only fires for blank input + successful run', () => {
    const spec = solvedParam()
    expect(paramComputedValue(spec, 0, 'P')).toBe(101.3)
    expect(paramComputedValue(spec, 0, 'T')).toBeUndefined() // has input draft
    expect(paramComputedValue(spec, 1, 'P')).toBeUndefined() // failed run
  })
})
