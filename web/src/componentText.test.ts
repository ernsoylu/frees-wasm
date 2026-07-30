import { describe, it, expect } from 'vitest'
import type { ComponentSpec } from './componentCatalog'
import {
  generateComponentText,
  isValidInstanceName,
  suggestInstanceName,
  missingRequiredParams,
  activeParams,
  selectedVariant,
  assembleBlock,
} from './componentText'

const param = (over: Partial<ComponentSpec['params'][number]> & { name: string }) => ({
  isString: over.name.endsWith('$'),
  isSelector: false,
  isMap: false,
  unit: '',
  description: '',
  required: true,
  values: [],
  variants: [],
  ...over,
})

const chiller: ComponentSpec = {
  type: 'Chiller',
  library: 'ac',
  summary: '',
  tags: [],
  ports: ['ref_in', 'ref_out'],
  params: [
    param({ name: 'ref$' }),
    param({ name: 'cool$' }),
    param({ name: 'U_tp', isString: false, unit: 'W/m^2-K' }),
    param({ name: 'eps_zone', isString: false, unit: '' }),
    param({ name: 'model$', isSelector: true, required: false, values: ['isentropic', 'volumetric'] }),
  ],
  variants: [],
}

// A component with a real variant: volumetric requires eta_v/disp/rpm.
const compressor: ComponentSpec = {
  type: 'Compressor',
  library: 'fluid',
  summary: '',
  tags: [],
  ports: ['in', 'out'],
  params: [
    param({ name: 'eta', isString: false }),
    param({ name: 'fluid$' }),
    param({ name: 'model$', isSelector: true, required: false, values: ['isentropic', 'volumetric'] }),
    param({ name: 'eta_v', isString: false, variants: ['volumetric'] }),
    param({ name: 'disp', isString: false, variants: ['volumetric'] }),
    param({ name: 'rpm', isString: false, variants: ['volumetric'] }),
  ],
  variants: [
    { name: 'isentropic', requires: [] },
    { name: 'volumetric', requires: ['eta_v', 'disp', 'rpm'] },
  ],
}

describe('generateComponentText', () => {
  it('writes string params unquoted with their $ and units on numerics', () => {
    const text = generateComponentText(chiller, 'CHLR1', {
      ref$: 'R1234yf',
      cool$: 'EG50',
      U_tp: '3000',
      eps_zone: '0.01',
    })
    expect(text).toBe('Chiller CHLR1(ref$=R1234yf, cool$=EG50, U_tp=3000 [W/m^2-K], eps_zone=0.01)')
  })

  it('omits empty params (optional selector left unset)', () => {
    const text = generateComponentText(chiller, 'C1', { ref$: 'Water', cool$: 'Water', U_tp: '1', eps_zone: '0' })
    expect(text).not.toContain('model$')
  })

  it('includes the selector when a variant is chosen', () => {
    const text = generateComponentText(chiller, 'C1', { ref$: 'Water', cool$: 'Water', U_tp: '1', eps_zone: '0', model$: 'volumetric' })
    expect(text).toContain('model$=volumetric')
  })

  it('falls back to a suggested name when none is given', () => {
    const text = generateComponentText(chiller, '   ', { ref$: 'Water', cool$: 'Water', U_tp: '1', eps_zone: '0' })
    expect(text.startsWith('Chiller CHIL(')).toBe(true)
  })
})

describe('instance name helpers', () => {
  it('accepts legal identifiers and rejects illegal ones', () => {
    expect(isValidInstanceName('CHLR1')).toBe(true)
    expect(isValidInstanceName('_x')).toBe(true)
    expect(isValidInstanceName('1bad')).toBe(false)
    expect(isValidInstanceName('has space')).toBe(false)
    expect(isValidInstanceName('')).toBe(false)
  })

  it('suggests capitals first, else the first four chars', () => {
    expect(suggestInstanceName('MovingBoundaryEvaporator')).toBe('MBE')
    expect(suggestInstanceName('Chiller')).toBe('CHIL')
  })
})

describe('missingRequiredParams', () => {
  it('lists only empty required params, ignoring optional selectors', () => {
    expect(missingRequiredParams(chiller, { ref$: 'Water', U_tp: '1' })).toEqual(['cool$', 'eps_zone'])
    expect(missingRequiredParams(chiller, { ref$: 'W', cool$: 'W', U_tp: '1', eps_zone: '0' })).toEqual([])
  })
})

describe('variant gating', () => {
  it('defaults the variant to the first when model$ is unset', () => {
    expect(selectedVariant(compressor, {})).toBe('isentropic')
    expect(selectedVariant(compressor, { model$: 'volumetric' })).toBe('volumetric')
  })

  it('hides variant params unless their variant is active', () => {
    const isentropic = activeParams(compressor, {}).map((p) => p.name)
    expect(isentropic).not.toContain('eta_v')
    const volumetric = activeParams(compressor, { model$: 'volumetric' }).map((p) => p.name)
    expect(volumetric).toEqual(expect.arrayContaining(['eta_v', 'disp', 'rpm']))
  })

  it('requires variant params only when that variant is selected', () => {
    expect(missingRequiredParams(compressor, { eta: '0.7', fluid$: 'R134a' })).toEqual([])
    expect(missingRequiredParams(compressor, { eta: '0.7', fluid$: 'R134a', model$: 'volumetric' }))
      .toEqual(['eta_v', 'disp', 'rpm'])
  })

  it('drops stale inactive-variant values from the generated text', () => {
    const text = generateComponentText(compressor, 'C1', { eta: '0.7', fluid$: 'R134a', eta_v: '0.9' })
    expect(text).not.toContain('eta_v') // isentropic active → eta_v inactive
  })
})

describe('assembleBlock', () => {
  it('joins preamble lines above the component line, dropping blanks', () => {
    expect(assembleBlock(['UA_x = ua_hx(1,2,3,4,5)', ''], 'Chiller C1(UA=UA_x)'))
      .toBe('UA_x = ua_hx(1,2,3,4,5)\nChiller C1(UA=UA_x)')
  })
})
