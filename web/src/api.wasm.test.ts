// DTO-parity assertions for the wasm-engine seam: what the Rust boundary
// actually emits (fixtures below are captured verbatim from
// crates/frees-wasm — see tests/dto_parity.rs on that side) versus what
// solve()/check() promise their call sites. The two contracts under test:
//
//  1. every SolveResponse/CheckResponse field App.tsx dereferences is either
//     supplied by the wasm payload with the right type and casing, or
//     defaulted by the shim (mapSolveData / check()'s literal), and
//  2. an engine-infrastructure failure RESOLVES to a failure envelope —
//     solve()/check() call sites have no catch, so a rejection would take
//     down the handler.
import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('./wasm/engineClient', () => ({
  wasmSolve: vi.fn(),
  wasmCheck: vi.fn(),
}))

import { check, solve, DEFAULT_STOP_CRITERIA } from './api'
import { wasmCheck, wasmSolve } from './wasm/engineClient'

const solveMock = vi.mocked(wasmSolve)
const checkMock = vi.mocked(wasmCheck)

// ── Fixtures: verbatim boundary output (crates/frees-wasm) ────────────────

const SOLVE_OK =
  '{"blocks":[{"equations":["P = 140 [kPa]"],"index":0,"variables":["P"]},{"equations":["Q = P * 2"],"index":1,"variables":["Q"]}],"error":null,"errorLine":null,"failedBlockIndex":null,"residuals":[{"equation":"P = 140 [kPa]","value":0.0},{"equation":"Q = P * 2","value":0.0}],"solutions":[{"maxResidual":0.0,"variables":[{"name":"P","units":"Pa","value":140000.0},{"name":"Q","units":"Pa","value":280000.0}]}],"stats":{"blocks":2,"elapsedMillis":1,"equations":2,"iterations":4,"maxResidual":0.0,"unknowns":2},"success":true,"unitWarnings":[],"variables":[{"name":"P","units":"Pa","value":140000.0},{"name":"Q","units":"Pa","value":280000.0}]}'

const SOLVE_PARSE_ERROR =
  '{"blocks":[],"error":"Syntax error: expected an expression, found `=`","errorLine":2,"failedBlockIndex":null,"residuals":[],"solutions":[],"stats":null,"success":false,"unitWarnings":[],"variables":[]}'

const SOLVE_FAILED_BLOCK =
  '{"blocks":[],"error":"Block 2 (1 equation) failed: Newton iteration stalled.","errorLine":null,"failedBlockIndex":1,"residuals":[],"solutions":[],"stats":null,"success":false,"unitWarnings":[],"variables":[]}'

const CHECK_OK =
  '{"equations":2,"errorLine":null,"errors":[],"inferredUnits":{"T_out":"K","Tin":"K"},"message":"No syntax errors were detected. There are 2 equations and 2 variables.","solvable":true,"unitWarnings":[],"unknowns":2,"variables":["T_out","Tin"]}'

const CHECK_PARSE_ERROR =
  '{"equations":0,"errorLine":3,"errors":[{"column":5,"line":3,"message":"expected an expression, found `=`"}],"inferredUnits":{},"message":"Syntax error: expected an expression, found `=`","solvable":false,"unitWarnings":[],"unknowns":0,"variables":[]}'

/** Resolves like the real engineClient: JSON.parse of the worker's string. */
function engineReturns(
  mock: { mockResolvedValueOnce(value: never): unknown },
  payload: string,
) {
  mock.mockResolvedValueOnce(JSON.parse(payload) as never)
}

function runSolve(text: string) {
  return solve(text, DEFAULT_STOP_CRITERIA, [], false, 'SI', false)
}

beforeEach(() => {
  solveMock.mockReset()
  checkMock.mockReset()
})

describe('solve() over the wasm boundary payloads', () => {
  it('maps a success payload and defaults every not-yet-ported field', async () => {
    engineReturns(solveMock, SOLVE_OK)
    const r = await runSolve('P = 140 [kPa]\nQ = P * 2\n')

    expect(r.success).toBe(true)
    expect(r.error).toBeNull()

    // Supplied by the payload, exact casing and types.
    expect(r.variables).toEqual([
      { name: 'P', units: 'Pa', value: 140000 },
      { name: 'Q', units: 'Pa', value: 280000 },
    ])
    expect(r.blocks[0]).toEqual({ index: 0, equations: ['P = 140 [kPa]'], variables: ['P'] })
    expect(r.residuals[1]).toEqual({ equation: 'Q = P * 2', value: 0 })
    expect(r.stats).toEqual({
      equations: 2, unknowns: 2, blocks: 2, iterations: 4, elapsedMillis: 1, maxResidual: 0,
    })
    expect(r.solutions).toHaveLength(1)
    expect(r.solutions[0].maxResidual).toBe(0)
    // App.tsx prefers solutions[0].variables over variables — they must agree.
    expect(r.solutions[0].variables).toEqual(r.variables)

    // Defaulted by mapSolveData — App.tsx iterates these without guards.
    expect(r.cyclePath).toEqual([])
    expect(r.codeTables).toEqual([])
    expect(r.parametricTables).toEqual([])
    expect(r.definedPlots).toEqual([])
    expect(r.stateTableDefs).toEqual([])
    expect(r.odeTables).toEqual([])
    expect(r.components).toEqual([])
    expect(r.uncertaintyBreakdown).toEqual([])
    expect(r.errorLine).toBeNull()
    expect(r.failedBlockIndex).toBeNull()
  })

  it('surfaces a parse failure with its 1-based errorLine', async () => {
    engineReturns(solveMock, SOLVE_PARSE_ERROR)
    const r = await runSolve('a = 1\nb = = 2\n')
    expect(r.success).toBe(false)
    expect(r.error).toMatch(/^Syntax error: /)
    expect(r.errorLine).toBe(2)
    expect(r.stats).toBeNull()
    expect(r.variables).toEqual([])
  })

  it('surfaces a solver failure with its 0-based failedBlockIndex', async () => {
    engineReturns(solveMock, SOLVE_FAILED_BLOCK)
    const r = await runSolve('k = 1\nexp(x) = -k\n')
    expect(r.success).toBe(false)
    expect(r.failedBlockIndex).toBe(1)
    expect(r.errorLine).toBeNull()
  })

  it('resolves (never rejects) when the engine infrastructure dies', async () => {
    solveMock.mockRejectedValueOnce(new Error('The engine worker failed'))
    const r = await runSolve('x = 1\n')
    expect(r.success).toBe(false)
    expect(r.error).toBe('Browser engine error: The engine worker failed')
    expect(r.variables).toEqual([])
    expect(r.stats).toBeNull()
  })

  it('sends the full former POST body so the boundary can grow into it', async () => {
    engineReturns(solveMock, SOLVE_OK)
    await runSolve('x = 1\n')
    const [source, requestJson] = solveMock.mock.calls[0]
    expect(source).toBe('x = 1\n')
    const request = JSON.parse(requestJson)
    expect(request.stopCriteria.maxIterations).toBe(250)
    expect(request.variableInfo).toEqual([])
    expect(request.displayUnitSystem).toBe('SI')
    expect(request.overrides).toEqual([])
  })
})

describe('check() over the wasm boundary payloads', () => {
  it('maps a success payload; inferredUnits are keyed by the display spellings in variables', async () => {
    engineReturns(checkMock, CHECK_OK)
    const r = await check('Tin = 300 [K]\nT_out = Tin * 2\n', [], false)

    expect(r.solvable).toBe(true)
    expect(r.equations).toBe(2)
    expect(r.unknowns).toBe(2)
    // Display spellings, not lowercase canonical names …
    expect(r.variables).toEqual(['T_out', 'Tin'])
    // … and the unit lookup App.tsx does with those names must connect.
    for (const name of r.variables) {
      expect(r.inferredUnits[name]).toBe('K')
    }
    expect(r.errorLine).toBeNull()
    expect(r.errors).toEqual([])
    // Defaulted by the shim — App.tsx merges these without guards.
    expect(r.codeTables).toEqual([])
    expect(r.parametricTables).toEqual([])
    expect(r.definedPlots).toEqual([])
    expect(r.stateTableDefs).toEqual([])
    expect(r.connections).toEqual([])
  })

  it('surfaces syntax errors as data with 1-based line/column for the editor marks', async () => {
    engineReturns(checkMock, CHECK_PARSE_ERROR)
    const r = await check('a = 1\nb = 2\nc = = 3\n', [], false)
    expect(r.solvable).toBe(false)
    expect(r.message).toMatch(/^Syntax error: /)
    expect(r.errorLine).toBe(3)
    // EquationEditor filters `line >= 1` and squiggles from `column - 1`.
    expect(r.errors).toEqual([{ line: 3, column: 5, message: 'expected an expression, found `=`' }])
  })

  it('resolves (never rejects) when the engine infrastructure dies', async () => {
    checkMock.mockRejectedValueOnce(new Error('wasm failed to load'))
    const r = await check('x = 1\n', [], false)
    expect(r.solvable).toBe(false)
    expect(r.message).toBe('Browser engine error: wasm failed to load')
    expect(r.variables).toEqual([])
    expect(r.inferredUnits).toEqual({})
  })

  it('forwards variableInfo and complexMode in the request body', async () => {
    engineReturns(checkMock, CHECK_OK)
    const row = { name: 'x', guess: 2, lower: null, upper: null, units: 'm', uncertainty: null }
    await check('x = 1\n', [row], true)
    const [, requestJson] = checkMock.mock.calls[0]
    const request = JSON.parse(requestJson)
    expect(request.variableInfo).toEqual([row])
    expect(request.stopCriteria.complexMode).toBe(true)
  })
})
