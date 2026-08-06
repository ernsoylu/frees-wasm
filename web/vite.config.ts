import { readdirSync } from 'node:fs'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'
import { VitePWA } from 'vite-plugin-pwa'
import pkg from './package.json'

// Inject the runtime build-info.js script into the production HTML.
// This classic (non-module) script sets window.__BUILD_COMMIT__ and is written
// at container start by the nginx entrypoint (see docker-entrypoint.d/).
// Injecting it via transformIndexHtml avoids Vite's "can't be bundled without
// type=module" warning that occurs when the tag is in the static index.html.
function buildInfoPlugin() {
  return {
    name: 'inject-build-info',
    transformIndexHtml(html: string) {
      return html.replace(
        '</head>',
        '  <script src="/build-info.js"></script>\n  </head>',
      )
    },
  }
}

// KaTeX's CSS lists every font as woff2 → woff → ttf; browsers that support
// woff2 (all of them, since ~2016) never request the fallbacks, so the woff
// and ttf copies are ~900 KB of dead weight in dist. Drop them at emit time —
// the CSS keeps its (now dangling) fallback URLs, which no supported browser
// resolves.
function stripLegacyKatexFontsPlugin() {
  return {
    name: 'strip-legacy-katex-fonts',
    generateBundle(_opts: unknown, bundle: Record<string, unknown>) {
      for (const name of Object.keys(bundle)) {
        if (/KaTeX_[^/]*\.(ttf|woff)$/.test(name)) delete bundle[name]
      }
    },
  }
}

// Univer's render engine lazy-loads hyphenation dictionaries (a docs-engine
// text-layout feature) as one chunk per language — ~80 chunks, ~4.4 MB. A
// dictionary is only ever *consulted* when `hyphenConfig()` is satisfied, which
// needs `autoHyphenation` set on the section break config; the sheets preset
// never sets it, so no cell layout can reach `hyphenate()`. Derive the exact
// module filenames from the package's own dist listing so this tracks Univer
// upgrades instead of hardcoding locale names.
const UNIVER_HYPHENATION_ES_DIR = 'node_modules/@univerjs/engine-render/lib/es'

function univerHyphenationModules(): string[] {
  const patternNames = new Set(
    readdirSync(
      'node_modules/@univerjs/engine-render/lib/types/components/docs/layout/hyphenation/patterns',
    ).map((f) => f.replace(/\.d\.ts$/, '')),
  )
  return readdirSync(UNIVER_HYPHENATION_ES_DIR).filter((f) => {
    const m = f.match(/^(.*)-[A-Za-z0-9_-]{8}\.js$/)
    return m !== null && patternNames.has(m[1])
  })
}

// Vite names each emitted chunk "<original stem>-<hash>.js".
//
// With stripUniverHyphenationPlugin below working, this list matches nothing —
// no dictionary chunk is emitted to ignore. It is kept as a backstop, and that
// is not theoretical: an intermediate version of that plugin silently stopped
// firing (missing `enforce: 'pre'`), the build stayed green, all ~4.4 MB of
// dictionaries came back into dist — and this list is what kept them out of the
// precache, holding it at 15020 KiB while dist ballooned to 19.06 MB. Cheap
// insurance against exactly the failure mode that already happened once.
function univerHyphenationGlobIgnores(): string[] {
  return univerHyphenationModules().map((f) => `**/${f.replace(/\.js$/, '')}-*.js`)
}

// Excluding the dictionaries from the precache stopped them being *downloaded*,
// but Rollup still emitted all ~80 of them — 4.4 MB of dist that no client can
// reach and every deploy has to carry. Resolve each one to an empty module so
// they are never emitted at all.
//
// This is safe because of how Univer consumes them: `Hyphen.loadPattern(lang)`
// awaits the dynamic import and does `loaded?.[snackToPascal(lang)]`, then
// `if (pattern == null) return` — an empty namespace is the already-handled
// "no such dictionary" path, not an error.
//
// It also fixes a real bug rather than only saving bytes. `Hyphen`'s
// constructor eagerly calls `loadPattern('en-gb')`, and `DocumentSkeleton`
// constructs `Hyphen` unconditionally — so every spreadsheet render fetched the
// 102 kB en-gb chunk, which the precache exclusion above had already told the
// service worker to ignore. Online that was a wasted download; offline it was a
// failed one. Now it resolves to the no-op the surrounding code already knows
// how to handle. `en-us` is unaffected either way: Univer inlines it into its
// own entry and preloads it synchronously.
// All ~80 imports are redirected to ONE shared virtual module rather than each
// being stubbed in place, so Rollup emits a single tiny chunk instead of ~80
// near-identical ones. (Stubbing in place still cost 77 files — small, but they
// are exactly the kind of unreachable output this plugin exists to remove.)
const HYPHENATION_STUB_ID = '\0univer-hyphenation-stub'

function stripUniverHyphenationPlugin() {
  const stubbed = new Set(univerHyphenationModules())
  return {
    name: 'strip-univer-hyphenation',
    // `pre` is load-bearing: a normal-order plugin's resolveId runs *after*
    // vite:resolve has already resolved the relative specifier to the real
    // dictionary file, so the redirect never fires and all ~4.4 MB come back —
    // silently, with a green build. If this plugin ever stops saving bytes,
    // check this line first.
    enforce: 'pre' as const,
    resolveId(source: string, importer: string | undefined) {
      if (importer === undefined || !importer.includes('@univerjs/engine-render')) return null
      const file = source.split('?')[0].split('/').pop()
      return file !== undefined && stubbed.has(file) ? HYPHENATION_STUB_ID : null
    },
    load(id: string) {
      return id === HYPHENATION_STUB_ID ? 'export default undefined\n' : null
    },
  }
}

// Phase 11: installable PWA + full offline session. The service worker
// precaches every built asset — including the ~3 MB wasm engine and the large
// lazy chunks (Plotly, the spreadsheet stack) — because "offline" for an
// engineering tool means the whole tool, not just the shell that was visited
// while online. `registerType: 'prompt'` keeps the old precache serving the
// running tab and surfaces an in-app "update ready" notification (src/pwa.tsx)
// instead of letting a background activation yank hashed chunks out from under
// a live session (which is exactly the stale-chunk condition main.tsx's
// vite:preloadError reload guards against).
function pwaPlugin() {
  return VitePWA({
    registerType: 'prompt',
    includeAssets: ['icons/icon.svg', 'icons/apple-touch-icon.png'],
    manifest: {
      name: 'frees Equation Solver',
      short_name: 'frees',
      description:
        'Declarative equation solving, acausal system modeling and measurement analysis — entirely in the browser.',
      start_url: '.',
      display: 'standalone',
      background_color: '#1a1b1e',
      theme_color: '#1a1b1e',
      icons: [
        { src: 'icons/icon-192.png', sizes: '192x192', type: 'image/png' },
        { src: 'icons/icon-512.png', sizes: '512x512', type: 'image/png' },
        { src: 'icons/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        { src: 'icons/icon.svg', sizes: 'any', type: 'image/svg+xml' },
      ],
    },
    workbox: {
      // woff2 only: KaTeX's woff/ttf fallbacks are stripped from the bundle
      // (stripLegacyKatexFontsPlugin) and no other dependency ships pre-woff2
      // fonts worth caching.
      globPatterns: ['**/*.{js,css,html,wasm,svg,png,woff2,json}'],
      // Unreachable-by-design chunks stay out of the precache; everything the
      // app can actually load is still cached up front, so offline is intact.
      globIgnores: univerHyphenationGlobIgnores(),
      // The Univer spreadsheet chunk (~5.5 MB) and Plotly are above workbox's
      // 2 MiB default; the point of this phase is that they work offline too.
      maximumFileSizeToCacheInBytes: 8 * 1024 * 1024,
      navigateFallback: 'index.html',
      // /api/ is the (optional, unwired) remote-fallback adapter's namespace —
      // a navigation there must fail honestly, not serve the app shell.
      navigateFallbackDenylist: [/^\/api\//],
      cleanupOutdatedCaches: true,
    },
  })
}

export default defineConfig({
  plugins: [
    react(),
    buildInfoPlugin(),
    stripLegacyKatexFontsPlugin(),
    stripUniverHyphenationPlugin(),
    pwaPlugin(),
    visualizer({ open: false, filename: 'stats.html' }),
  ],
  define: {
    // The app version from package.json, baked in at build time so the REPL
    // banner and About dialog show "v0.1.0" without a runtime lookup. Paired
    // with the commit hash (VITE_COMMIT_HASH / window.__BUILD_COMMIT__) it
    // gives a full "frees v0.1.0 (abcdefg)" identity.
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  build: {
    // The editor core ("App" chunk) sits around ~260 kB gzipped after the
    // feature tabs (Diagram, Digitizer, Plots, formatted report), modals, the
    // Help page and Plotly are all code-split into their own lazily-loaded
    // chunks. Bump the warning limit above the core so the build only flags a
    // genuine regression there; the one chunk that still exceeds it is Plotly
    // (~4.8 MB), which is intentionally isolated and dynamically imported in
    // PlotlyChart, so it never blocks first paint.
    chunkSizeWarningLimit: 1000,
    // Split the big shared libraries into their own cached vendor chunks so the
    // editor and Help-page chunks stay small.
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          // Rollup's virtual CommonJS-interop helpers are shared by every
          // CJS-converted vendor module. Left to auto-placement they can land
          // in a vendor chunk that itself imports react (e.g. mantine),
          // creating a mantine <-> react chunk cycle whose init order breaks
          // at runtime (React undefined inside Mantine). Pin them to the
          // react chunk, which everything loads first and imports nothing back.
          if (id.includes('commonjsHelpers')) return 'react'
          // The boot path needs exactly one string out of this family — App.tsx
          // imports DEFAULT_EXAMPLE_TEXT from defaultExample.ts (9 kB). Left to
          // Rollup's auto-placement that module landed in `docs-data` (because
          // examples.ts also imports it), which made the App chunk statically
          // import 1068 kB of reference documentation on every cold start.
          // Pinning it to its own chunk cuts that to 9 kB.
          if (id.includes('defaultExample.ts')) return 'default-example'
          // Likewise, ExamplesModal (3 kB, lazy) needs only examples.ts (51 kB);
          // sharing a chunk with referenceCatalog.ts made opening the Examples
          // dialog a 1 MB download. Keep the two apart — the whole reference
          // corpus is reachable only from the (lazy) Help page.
          if (id.includes('examples.ts')) return 'examples-data'
          if (id.includes('docsCatalog.ts') || id.includes('referenceCatalog.ts') || id.includes('searchIndex.ts')) {
            return 'docs-data'
          }
          if (!id.includes('node_modules')) return undefined
          if (id.includes('@mantine')) return 'mantine'
          if (id.includes('katex')) return 'katex'
          // CodeMirror (+ its @lezer runtime) changes far less often than app
          // code: isolating it keeps ~300 kB of the entry cacheable across
          // deploys and shrinks the frequently-invalidated App chunk.
          if (id.includes('@codemirror') || id.includes('@lezer') || id.includes('@uiw/react-codemirror')) {
            return 'codemirror'
          }
          // Match only the real react/react-dom/scheduler packages. A loose
          // `/react/` also caught e.g. @floating-ui/react (a Mantine dep),
          // dragging it into this chunk and creating a mantine <-> react
          // chunk cycle (tslib landed in mantine, @floating-ui/react needs it,
          // mantine needs @floating-ui/react) that broke init order at runtime.
          if (/node_modules\/(react|react-dom|scheduler)\//.test(id)) {
            return 'react'
          }
          return undefined
        },
      },
    },
  },
  // No /api dev proxy: nothing in src/ issues an /api request — the engine is
  // in-bundle wasm (the former proxy served the retired Spring backend). The
  // optional remote-fallback adapter is opt-in via VITE_API_BASE, which names
  // an absolute origin and needs no proxy.
  server: {
    port: 5173,
  },
})
