/// <reference types="vite/client" />

// Baked in at build time by Vite's `define` from package.json (see vite.config.ts).
declare const __APP_VERSION__: string

interface ImportMetaEnv {
  /** Base URL for API requests (empty for same-origin). */
  readonly VITE_API_BASE?: string
  /** When truthy, the client submits solve/optimize/curve-fit jobs
   *  asynchronously (202 + polling /api/jobs/{id}) instead of expecting a
   *  synchronous 200 response. Matches the backend `api` Spring profile. */
  readonly VITE_ASYNC_API?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
