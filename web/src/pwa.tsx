import { Button, Stack, Text } from '@mantine/core'
import { notifications } from '@mantine/notifications'
import { registerSW } from 'virtual:pwa-register'
import { decidePrecache, dropPrecache, readDataSaver } from './dataSaver'

// Phase 11: service-worker registration and the update flow.
//
// The worker is built with `registerType: 'prompt'` (vite.config.ts): a new
// deploy installs in the background but does NOT activate until the user opts
// in. Activating under a running tab would purge the old hashed chunks the tab
// is still lazy-loading — the exact stale-chunk failure main.tsx's
// vite:preloadError handler exists to paper over — so instead the new worker
// waits, and this notification hands the choice to the user. `updateSW(true)`
// tells the waiting worker to skipWaiting and reloads the page onto the new
// precache in one motion.
//
// Data saver (see dataSaver.ts for why this is the only honest lever) gates
// the whole thing: on, and nothing is registered — the app is downloaded as it
// is used and there is no offline guarantee. Off is the default and is
// byte-for-byte the behaviour that shipped before the switch existed, which is
// what keeps the offline e2e gate meaningful.

export function setupPwa() {
  const decision = decidePrecache(readDataSaver(), 'serviceWorker' in navigator)

  if (decision === 'unsupported') return

  if (decision === 'drop') {
    // Fire-and-forget: reclaims the storage this browser already spent and,
    // more to the point, stops the next deploy precaching in the background.
    void dropPrecache({ serviceWorker: navigator.serviceWorker, caches: globalThis.caches })
    return
  }

  const updateSW = registerSW({
    onNeedRefresh() {
      notifications.show({
        id: 'pwa-update-ready',
        color: 'teal',
        autoClose: false,
        title: 'Update ready',
        message: (
          <Stack gap="xs">
            <Text size="sm">A new version of frees has been downloaded.</Text>
            <Button size="xs" onClick={() => void updateSW(true)}>
              Reload to update
            </Button>
          </Stack>
        ),
      })
    },
    onOfflineReady() {
      notifications.show({
        color: 'teal',
        title: 'Ready to work offline',
        message: 'frees is fully cached — the whole app now works without a network connection.',
      })
    },
    onRegisterError() {
      // Registration failing (e.g. an insecure context) leaves a plain web
      // app, which is exactly what shipped before this phase. Nothing to say.
    },
  })
}
