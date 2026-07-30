import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { visualizer } from 'rollup-plugin-visualizer'
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

export default defineConfig({
  plugins: [react(), buildInfoPlugin(), visualizer({ open: false, filename: 'stats.html' })],
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
          if (id.includes('docsCatalog.ts') || id.includes('referenceCatalog.ts') || id.includes('examples.ts') || id.includes('searchIndex.ts')) {
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
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  // `vite preview` serves the built bundle (the dev server can hit watcher
  // ENOSPC limits here); give it the same /api proxy so host verification
  // against the Docker backend works.
  preview: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
