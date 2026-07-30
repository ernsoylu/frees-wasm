// Template-mode relocation checks (§2.5b): wrong file rejected hard, same
// channels with a different size/hash is advisory, identical file is ok.

import { describe, expect, it } from 'vitest'
import { checkRelocatedFile } from './relocate'

const stored = { name: 'run1.csv', size: 1000, headerHash: 'abcd1234' }

describe('checkRelocatedFile', () => {
  it('accepts an identical file', () => {
    const r = checkRelocatedFile(['speed', 'torque'], ['time', 'speed', 'torque'], stored, {
      size: 1000,
      headerHash: 'abcd1234',
    })
    expect(r.status).toBe('ok')
  })

  it('rejects a file missing referenced channels (mandatory, hard error)', () => {
    const r = checkRelocatedFile(['speed', 'torque'], ['time', 'speed'], stored, {
      size: 1000,
      headerHash: 'abcd1234',
    })
    expect(r.status).toBe('rejected')
    if (r.status === 'rejected') expect(r.missingChannels).toEqual(['torque'])
  })

  it('flags size/hash differences as advisory (explicit override)', () => {
    const r = checkRelocatedFile(['speed'], ['speed'], stored, {
      size: 2000,
      headerHash: 'ffff0000',
    })
    expect(r.status).toBe('advisory')
    if (r.status === 'advisory') expect(r.mismatches).toEqual(['file size', 'content hash'])
  })

  it('missing channels dominate advisory mismatches', () => {
    const r = checkRelocatedFile(['gone'], ['speed'], stored, { size: 2000, headerHash: 'x' })
    expect(r.status).toBe('rejected')
  })
})
