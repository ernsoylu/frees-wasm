// curveFitShared.test.ts — the pure pieces shared by CurveFitModal and the
// digitizer's fit dialog (Wave H): template-variable substitution and the
// fitted-model editor text.

import { describe, expect, it } from 'vitest'
import { FIT_TEMPLATES, fittedModelInsertText, templateModelFor } from './curveFitShared'

describe('templateModelFor', () => {
  it('rewrites whole-word x and y to the caller names', () => {
    const linear = FIT_TEMPLATES[0]
    expect(templateModelFor(linear, 'Re', 'f_D')).toBe('f_D = a * Re + b')
  })

  it('does not touch the x inside exp()', () => {
    const exp = FIT_TEMPLATES.find((t) => t.name.startsWith('Exponential ('))
    expect(exp).toBeDefined()
    expect(templateModelFor(exp!, 'T', 'k')).toBe('k = a * exp(b * T)')
  })

  it('survives an x/y swap without cascading (single-pass substitution)', () => {
    const linear = FIT_TEMPLATES[0]
    expect(templateModelFor(linear, 'y', 'x')).toBe('x = a * y + b')
  })
})

describe('fittedModelInsertText', () => {
  it('emits the template comment, parameter lines (with units) and the model', () => {
    const text = fittedModelInsertText(
      'Linear (y = a * x + b)',
      'y = a * x + b',
      ['a', 'b'],
      [2, 3],
      { a: 'kPa/m', b: ' ' },
    )
    const lines = text.split('\n')
    expect(lines[0]).toBe('{ Fitted Model: Linear (y = a * x + b) }')
    expect(lines[1]).toBe('a = 2 [kPa/m]')
    // A blank unit adds no bracket.
    expect(lines[2]).toBe('b = 3')
    expect(lines[3]).toBe('y = a * x + b')
  })

  it('labels a custom model as Custom and defaults to no units', () => {
    const text = fittedModelInsertText('custom', ' q = a * t ', ['a'], [1.5])
    expect(text).toBe('{ Fitted Model: Custom }\na = 1.5\nq = a * t')
  })
})
