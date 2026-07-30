import { describe, it, expect } from 'vitest'
import { groupComponents } from './Workspace'
import { ComponentResult, VariableResult } from './api'

const v = (name: string, value = 0): VariableResult => ({ name, value, units: '' })

const inst = (
  name: string,
  type: string,
  params: ComponentResult['params'] = [],
): ComponentResult => ({ name, type, params })

describe('groupComponents', () => {
  it('keeps dotless names as plain scalars', () => {
    const { plain, components } = groupComponents([v('k_fin'), v('t_fin')], [])
    expect(plain.map((p) => p.name)).toEqual(['k_fin', 't_fin'])
    expect(components).toHaveLength(0)
  })

  it('attaches dotted members to their seeded instance and strips the prefix', () => {
    const scalars = [
      v('chlr.out.h'),
      v('chlr.wall.qdot'),
      v('chlr.tevap'),
      v('cmp.in.h'),
      v('k_fin'),
    ]
    const instances = [
      inst('CHLR', 'TwoPhaseEvaporatorUA', [
        { name: 'UA', ref: 'UA_chl_r', value: 575.46, units: 'W/K' },
        { name: 'SH', ref: '5', value: 5, units: null },
      ]),
      inst('CMP', 'TwoPhaseCompressor'),
    ]
    const { plain, components } = groupComponents(scalars, instances)

    expect(plain.map((p) => p.name)).toEqual(['k_fin'])
    // sorted by name; instance display case preserved from the metadata
    expect(components.map((c) => c.name)).toEqual(['CHLR', 'CMP'])

    const chlr = components[0]
    expect(chlr.type).toBe('TwoPhaseEvaporatorUA')
    expect(chlr.params.map((p) => p.name)).toEqual(['UA', 'SH'])
    // members sorted by label, instance prefix removed (case-insensitive match)
    expect(chlr.members.map((m) => m.label)).toEqual(['out.h', 'tevap', 'wall.qdot'])
  })

  it('keeps a param-only instance (no member variables) so its inputs still show', () => {
    const instances = [inst('OBAT', 'LiquidOrifice', [
      { name: 'CdA', ref: '1.6e-5', value: 1.6e-5, units: null },
    ])]
    const { components } = groupComponents([v('k_fin')], instances)
    expect(components.map((c) => c.name)).toEqual(['OBAT'])
    expect(components[0].members).toHaveLength(0)
    expect(components[0].params).toHaveLength(1)
  })

  it('drops a fully empty instance (no params, no members)', () => {
    const { components } = groupComponents([v('k_fin')], [inst('MIX', 'LiquidMixer')])
    expect(components).toHaveLength(0)
  })

  it('forms a group for a dotted name with no matching instance (unknown type)', () => {
    const { components } = groupComponents([v('ghost.x')], [])
    expect(components.map((c) => c.name)).toEqual(['ghost'])
    expect(components[0].type).toBeUndefined()
  })
})
