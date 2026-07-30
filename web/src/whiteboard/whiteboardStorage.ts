import { emptyWhiteboardScene, type WhiteboardSpec } from './types'

// Whiteboard persistence helpers, mirroring diagramStorage.ts: the editor
// shell can load/save whiteboards eagerly while the heavy WhiteboardTab UI
// is code-split and only fetched when its tab is opened.

const WHITEBOARDS_STORAGE_KEY = 'frees-whiteboards'

/** Broadcast name for save failures so the whiteboard window can warn. */
export const WHITEBOARD_SAVE_ERROR_EVENT = 'frees-whiteboard-save-error'

export function loadWhiteboards(): WhiteboardSpec[] {
  try {
    const raw = localStorage.getItem(WHITEBOARDS_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed)) return parsed
    }
  } catch {}
  return []
}

/**
 * Persist all whiteboards. Returns false on failure (most commonly a
 * localStorage quota overflow from large embedded images) and broadcasts an
 * event so the UI can surface it — mirroring saveDiagrams.
 */
export function saveWhiteboards(whiteboards: WhiteboardSpec[]): boolean {
  try {
    localStorage.setItem(WHITEBOARDS_STORAGE_KEY, JSON.stringify(whiteboards))
    return true
  } catch (err) {
    const message =
      err instanceof DOMException && /quota/i.test(err.name + err.message)
        ? 'Storage is full — the whiteboard could not be saved. Large embedded images are the usual cause; export the whiteboard as PNG/SVG to keep a copy.'
        : 'The whiteboard could not be saved to local storage.'
    if (typeof window !== 'undefined') {
      window.dispatchEvent(
        new CustomEvent(WHITEBOARD_SAVE_ERROR_EVENT, { detail: message }),
      )
    }
    return false
  }
}

/** Construct a new named whiteboard with an empty Excalidraw scene. */
export function newWhiteboard(count: number): WhiteboardSpec {
  return {
    id: crypto.randomUUID(),
    name: `Whiteboard ${count + 1}`,
    ...emptyWhiteboardScene(),
  }
}
