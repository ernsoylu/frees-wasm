// Rasterize the checked-in icon SVGs into the PNGs the manifest and index.html
// reference, and verify that what the manifest asks for is what exists.
//
//   node scripts/gen-icons.mjs          rasterize, then verify
//   node scripts/gen-icons.mjs --check  verify only (no writes)
//
// This is not part of `npm run build` — the PNGs are checked in, and a build
// that quietly re-rasterized them would make the artifacts depend on whichever
// Chromium happened to be installed. Run it after editing an SVG, and commit
// the PNGs it writes.
//
// WHY CHROMIUM. The rule was "add no new dependency", and there is no
// rasterizer in the dependency tree: no sharp, no resvg, no canvas. Playwright
// already IS a devDependency (it runs the offline PWA gate), and its Chromium
// is the same renderer that will display these SVGs, so a raster from it is by
// definition what a browser would draw. The alternative — hand-writing PNG
// encoders or shelling out to whatever ImageMagick build a machine happens to
// have, with its own SVG delegate and its own gradient handling — is worse on
// both reproducibility and honesty.

import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const iconsDir = resolve(here, '..', 'public', 'icons')
const viteConfig = resolve(here, '..', 'vite.config.ts')
const indexHtml = resolve(here, '..', 'index.html')

/**
 * source → output, at size. `icon.svg` is the rounded tile (favicon and the
 * manifest's "any" icons); `icon-maskable.svg` is full-bleed and backs both the
 * Android maskable icon and apple-touch-icon (iOS masks and de-alphas it
 * itself, so it wants full bleed too).
 */
const TARGETS = [
  { from: 'icon.svg', to: 'icon-192.png', size: 192 },
  { from: 'icon.svg', to: 'icon-512.png', size: 512 },
  { from: 'icon-maskable.svg', to: 'icon-maskable-512.png', size: 512 },
  { from: 'icon-maskable.svg', to: 'apple-touch-icon.png', size: 180 },
]

async function rasterize() {
  const { chromium } = await import('@playwright/test')
  const browser = await chromium.launch()
  try {
    for (const target of TARGETS) {
      const svg = await readFile(join(iconsDir, target.from), 'utf8')
      const uri = `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`
      const page = await browser.newPage({
        viewport: { width: target.size, height: target.size },
        deviceScaleFactor: 1,
      })
      // An <img> at exactly the viewport size: the SVG's own 512 px intrinsic
      // size is scaled by the renderer, which is the point — one source,
      // every size, no per-size hand tuning.
      await page.setContent(
        `<style>html,body{margin:0;padding:0;background:transparent}` +
          `img{display:block;width:${target.size}px;height:${target.size}px}</style>` +
          `<img src="${uri}">`,
      )
      await page.screenshot({ path: join(iconsDir, target.to), omitBackground: true })
      await page.close()
      console.log(`  wrote icons/${target.to}  ${target.size}x${target.size}  (from ${target.from})`)
    }
  } finally {
    await browser.close()
  }
}

/** PNG width/height straight out of the IHDR chunk — no decoder needed. */
function pngSize(buffer) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
  if (buffer.length < 24 || !buffer.subarray(0, 8).equals(signature)) return null
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) }
}

/**
 * Every `icons/…` path named by the manifest, includeAssets and index.html has
 * to exist, be a PNG of the size it claims, and be one of the files this script
 * writes. A manifest entry pointing at a file nobody generates is exactly the
 * rot this check exists to catch.
 */
async function verify() {
  const referenced = new Map()
  const note = (path, expected, where) => {
    const entry = referenced.get(path) ?? { sizes: new Set(), where: new Set() }
    if (expected) entry.sizes.add(expected)
    entry.where.add(where)
    referenced.set(path, entry)
  }

  const config = await readFile(viteConfig, 'utf8')
  for (const match of config.matchAll(/src:\s*'(icons\/[^']+)'[^}]*?sizes:\s*'(any|\d+x\d+)'/g)) {
    const declared = match[2] === 'any' ? null : Number(match[2].split('x')[0])
    note(match[1], declared, 'manifest')
  }
  for (const match of config.matchAll(/includeAssets:\s*\[([^\]]*)\]/g)) {
    for (const asset of match[1].matchAll(/'(icons\/[^']+)'/g)) note(asset[1], null, 'includeAssets')
  }
  const html = await readFile(indexHtml, 'utf8')
  for (const match of html.matchAll(/href="\/(icons\/[^"]+)"/g)) note(match[1], null, 'index.html')

  const generated = new Set(TARGETS.map((t) => `icons/${t.to}`))
  const sources = new Set(TARGETS.map((t) => `icons/${t.from}`))
  const problems = []

  for (const [path, entry] of [...referenced].sort()) {
    const where = [...entry.where].join(', ')
    let file
    try {
      file = await readFile(resolve(iconsDir, '..', path))
    } catch {
      problems.push(`${path} is referenced by ${where} but does not exist`)
      continue
    }
    if (path.endsWith('.svg')) {
      if (!sources.has(path)) problems.push(`${path} (${where}) is not one of this script's sources`)
      console.log(`  ok  ${path}  ${file.length} B  (${where})`)
      continue
    }
    if (!generated.has(path)) {
      problems.push(`${path} (${where}) is referenced but not generated by this script`)
      continue
    }
    const size = pngSize(file)
    if (!size) {
      problems.push(`${path} is not a PNG`)
      continue
    }
    for (const expected of entry.sizes) {
      if (size.width !== expected || size.height !== expected) {
        problems.push(`${path} is ${size.width}x${size.height}, ${where} declares ${expected}x${expected}`)
      }
    }
    const digest = createHash('sha256').update(file).digest('hex').slice(0, 12)
    console.log(`  ok  ${path}  ${size.width}x${size.height}  ${file.length} B  sha256:${digest}  (${where})`)
  }

  for (const path of generated) {
    if (!referenced.has(path)) problems.push(`${path} is generated but nothing references it`)
  }

  if (problems.length > 0) {
    for (const problem of problems) console.error(`  FAIL ${problem}`)
    process.exitCode = 1
    return false
  }
  return true
}

const checkOnly = process.argv.includes('--check')
if (!checkOnly) {
  console.log('Rasterizing icons from public/icons/*.svg')
  await rasterize()
}
console.log('Verifying every referenced icon exists at its declared size')
const ok = await verify()
console.log(ok ? 'Icons OK.' : 'Icons FAILED.')
