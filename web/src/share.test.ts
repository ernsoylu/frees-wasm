import { describe, expect, it } from 'vitest'
import { buildShareUrl, extractSharedText, MAX_SHARE_URL_CHARS } from './share'

const BASE = 'https://frees.example/'

describe('share-by-URL', () => {
  it('round-trips a document through the fragment', () => {
    const doc = 'x^2 + y^3 = 77\nx / y = 1.23456\n{ a comment with ünïcödé and [units] }'
    const url = buildShareUrl(doc, BASE)
    expect(url).not.toBeNull()
    expect(url).toMatch(/^https:\/\/frees\.example\/#share=/)
    const hash = new URL(url!).hash
    expect(extractSharedText(hash)).toBe(doc)
  })

  it('round-trips a realistic multi-hundred-line document', () => {
    const doc = Array.from({ length: 400 }, (_, i) => `T[${i + 1}] = ${i} + x_${i} * exp(${i / 7})`).join('\n')
    const url = buildShareUrl(doc, BASE)
    expect(url).not.toBeNull()
    expect(extractSharedText(new URL(url!).hash)).toBe(doc)
  })

  it('refuses documents whose link would exceed the ceiling', () => {
    // Incompressible content (random-ish digits) to defeat lz-string.
    let seed = 1
    const junk = Array.from({ length: 80000 }, () => {
      seed = (seed * 48271) % 2147483647
      return String.fromCharCode(33 + (seed % 90))
    }).join('')
    expect(buildShareUrl(junk, BASE)).toBeNull()
    expect(MAX_SHARE_URL_CHARS).toBeGreaterThan(1000)
  })

  it('returns null for non-share and empty hashes', () => {
    expect(extractSharedText('')).toBeNull()
    expect(extractSharedText('#')).toBeNull()
    expect(extractSharedText('#refpage:lqr')).toBeNull()
    expect(extractSharedText('#share=')).toBeNull()
  })

  it('returns null for a mangled payload instead of throwing', () => {
    expect(extractSharedText('#share=!!!not-lz-data!!!')).toBeNull()
    const url = buildShareUrl('x = 1', BASE)!
    const truncated = new URL(url).hash.slice(0, 12)
    expect(extractSharedText(truncated)).toBeNull()
  })
})
