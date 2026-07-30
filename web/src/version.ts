declare global {
  interface Window {
    __BUILD_COMMIT__?: string
  }
}

// The app version from package.json, baked in at build time by Vite's `define`.
// Falling back to '0.0.0' covers non-Vite contexts (tests) where the define
// isn't applied.
export const APP_VERSION: string =
  (typeof __APP_VERSION__ !== 'undefined' && __APP_VERSION__) || '0.0.0'

// The git commit this build is running. Prefer the runtime stamp written by
// the container entrypoint (window.__BUILD_COMMIT__, from RENDER_GIT_COMMIT on
// Render), then the build-time stamp baked by Vite (frees.sh locally sets
// VITE_COMMIT_HASH). 'dev' when neither is present. See CLAUDE.md
// "Build stamping".
export const COMMIT_HASH: string =
  (typeof window !== 'undefined' && window.__BUILD_COMMIT__) ||
  import.meta.env.VITE_COMMIT_HASH ||
  'dev'

export const COMMIT_SHORT: string = COMMIT_HASH.slice(0, 7)
export const COMMIT_IS_REAL: boolean = COMMIT_HASH !== 'dev'

// "v0.1.0" — the version label used by the About dialog and the REPL banner.
export const VERSION_LABEL: string = `v${APP_VERSION}`

// REPL banner identity: "frees v0.1.0 (abcdefg)" for a real build, or
// "frees v0.1.0 (dev)" when no commit stamp is present.
export const REPL_BANNER: string =
  `frees ${VERSION_LABEL} (${COMMIT_IS_REAL ? COMMIT_SHORT : 'dev'})`

export {}
