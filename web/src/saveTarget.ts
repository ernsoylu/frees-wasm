// Wave I (closing Phase 11's gap 2, the FileSystemFileHandle half): where
// does plain Save write? Wave E gave Save provenance — a library-opened
// project re-saves to the library — but a file project still always opened
// the picker. When the project was opened or saved through the File System
// Access API the handle survives (it is structured-cloneable, so it also
// survives a reload via IndexedDB), and Save can write back to the same file.
//
// The decision is pure and lives here so vitest can grade the whole matrix
// without a browser; the two async helpers below wrap the Chromium-only
// permission API with the same "absent API degrades to the picker" rule the
// rest of the app follows (Firefox and Safari have no File System Access
// pickers, so no handle can exist there and Save keeps meaning the picker).

export type SaveProvenance = 'file' | 'browser' | null

/**
 * `queryPermission`'s answer, extended with 'unsupported' for handles (or
 * browsers) that do not implement the permission API at all.
 */
export type HandlePermission = PermissionState | 'unsupported'

export type SaveDestination = 'library' | 'handle' | 'picker'

/**
 * The Save decision:
 * - a browser-library project re-saves to the library (Wave E);
 * - a file project with a live handle writes back to that file — 'prompt' is
 *   still a handle save, because `requestPermission` runs inside the Save
 *   click (a user gesture) and the caller falls back to the picker if the
 *   user refuses;
 * - everything else (no provenance, no handle, permission already denied,
 *   permission API unsupported) means the picker, exactly as before Wave I.
 */
export function saveTarget(
  provenance: SaveProvenance,
  hasHandle: boolean,
  permission: HandlePermission,
): SaveDestination {
  if (provenance === 'browser') return 'library'
  if (provenance !== 'file' || !hasHandle) return 'picker'
  return permission === 'granted' || permission === 'prompt' ? 'handle' : 'picker'
}

/**
 * The Chromium extension of FileSystemHandle behind the FS Access API.
 * `queryPermission`/`requestPermission` are not in lib.dom (WICG-only), so
 * they are typed optional here and feature-detected at every use.
 */
interface PermissionedHandle extends FileSystemFileHandle {
  queryPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
  requestPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
}

/** The handle's current write-permission state; 'unsupported' when the API is absent or throws. */
export async function queryWritePermission(handle: FileSystemFileHandle): Promise<HandlePermission> {
  const h = handle as PermissionedHandle
  if (typeof h.queryPermission !== 'function') return 'unsupported'
  try {
    return await h.queryPermission({ mode: 'readwrite' })
  } catch {
    return 'unsupported'
  }
}

/**
 * Write `content` back to the handle, requesting permission first when the
 * state is 'prompt' (legal here: this runs inside the Save click's user
 * gesture). 'saved' on success; 'denied' when permission was refused or the
 * permission API is unusable; 'failed' when the write itself threw (file
 * moved or deleted, disk error). Both failure shapes send the caller to the
 * picker — a failed Save must never be silent, and never lose the document.
 */
export async function writeToHandle(
  handle: FileSystemFileHandle,
  content: string,
): Promise<'saved' | 'denied' | 'failed'> {
  const h = handle as PermissionedHandle
  let state = await queryWritePermission(handle)
  if (state === 'prompt' && typeof h.requestPermission === 'function') {
    try {
      state = await h.requestPermission({ mode: 'readwrite' })
    } catch {
      return 'denied'
    }
  }
  if (state !== 'granted') return 'denied'
  try {
    const writable = await handle.createWritable()
    await writable.write(content)
    await writable.close()
    return 'saved'
  } catch {
    return 'failed'
  }
}
