// Data saver — the opt-out from the full precache (Phase 11's offline
// guarantee), for people on a metered connection.
//
// WHAT IS ACTUALLY CONFIGURABLE, AND WHERE
//
// vite-plugin-pwa runs in `generateSW` mode (vite.config.ts). Workbox writes
// `sw.js` at BUILD time with the precache manifest — every hashed asset, ~9.3
// MiB across 97 entries — inlined as `self.__WB_MANIFEST` and installed by
// `precacheAndRoute`. Nothing in that file consults anything at RUNTIME: there
// is no flag, query parameter or message that makes the generated worker
// precache a subset. Splitting the manifest would mean switching to
// `injectManifest` and hand-writing the worker, which is a rewrite of the one
// artifact the offline e2e gate exists to protect.
//
// So the honest lever is the registration itself, which IS ours (src/pwa.tsx):
//
//   * data saver OFF (the default) — register exactly as before. Full
//     precache, full offline guarantee. Nothing about the shipped worker
//     changes, which is why the offline gate is untouched by this feature.
//   * data saver ON — never call `registerSW`, and tear down whatever is
//     already installed (unregister the worker, drop its caches) so no further
//     deploy re-precaches in the background.
//
// The consequence, stated plainly here and in the Preferences copy: the switch
// takes effect on the NEXT visit. The visit where it is flipped has already
// spent its bytes — turning it on reclaims the storage and stops the *next*
// download, it cannot un-download this one.

/** localStorage key. Read synchronously at boot, before registration. */
export const DATA_SAVER_KEY = 'frees.dataSaver'

/**
 * Is data saver on? Defaults to false — the offline guarantee is the headline
 * feature and stays the default. Any unavailable/garbled storage reads as off,
 * because the failure mode of a wrong "on" (silently no offline) is worse than
 * the failure mode of a wrong "off" (a download the user asked to skip once).
 */
export function readDataSaver(): boolean {
  try {
    return localStorage.getItem(DATA_SAVER_KEY) === '1'
  } catch {
    return false
  }
}

/** Persist the choice. Best-effort: a private mode that refuses storage just
 *  means the setting does not survive the tab, not that saving failed loudly. */
export function writeDataSaver(on: boolean): void {
  try {
    if (on) localStorage.setItem(DATA_SAVER_KEY, '1')
    else localStorage.removeItem(DATA_SAVER_KEY)
  } catch {
    // Best-effort.
  }
}

/**
 * What boot should do about the service worker.
 *
 * `'register'` — install/keep the precaching worker (the default path).
 * `'drop'` — do not register, and remove any worker + caches already here.
 * `'unsupported'` — no service worker in this context (insecure origin, some
 * embeds); there is nothing to register and nothing to drop.
 */
export type PrecacheDecision = 'register' | 'drop' | 'unsupported'

export function decidePrecache(dataSaver: boolean, serviceWorkerSupported: boolean): PrecacheDecision {
  if (!serviceWorkerSupported) return 'unsupported'
  return dataSaver ? 'drop' : 'register'
}

/** The two browser surfaces `dropPrecache` touches, narrowed so a test can
 *  supply doubles and so neither is assumed to exist. */
export interface PrecacheEnv {
  serviceWorker?: Pick<ServiceWorkerContainer, 'getRegistrations'>
  caches?: Pick<CacheStorage, 'keys' | 'delete'>
}

export interface DropResult {
  unregistered: number
  cachesDeleted: number
}

/**
 * Remove the installed worker and its caches.
 *
 * Every Cache Storage entry is dropped, not a name pattern: the service worker
 * is the only thing in this app that writes there (nothing under src/ touches
 * `caches`), and Workbox's cache names embed a build-time cacheId plus scope,
 * so matching them by name would rot silently the first time either changed.
 *
 * Best-effort throughout. A refused unregister leaves the worker serving the
 * old precache, which is a stale-but-working app, not a broken one.
 */
export async function dropPrecache(env: PrecacheEnv): Promise<DropResult> {
  const result: DropResult = { unregistered: 0, cachesDeleted: 0 }

  try {
    const registrations = (await env.serviceWorker?.getRegistrations()) ?? []
    for (const registration of registrations) {
      try {
        if (await registration.unregister()) result.unregistered += 1
      } catch {
        // One stubborn registration must not stop the others.
      }
    }
  } catch {
    // getRegistrations can reject in partitioned contexts.
  }

  try {
    const keys = (await env.caches?.keys()) ?? []
    for (const key of keys) {
      try {
        if (await env.caches!.delete(key)) result.cachesDeleted += 1
      } catch {
        // Same.
      }
    }
  } catch {
    // No Cache Storage here.
  }

  return result
}
