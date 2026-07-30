// Excalidraw whiteboard model.
//
// Excalidraw scenes serialize to a plain-JSON object `{ elements, appState,
// files }`. We persist that blob verbatim as opaque JSON (`unknown`) rather
// than coupling to Excalidraw's evolving element type union, so a project
// file stays valid even if the Excalidraw schema gains fields across
// releases. The strict Excalidraw types are only referenced inside
// WhiteboardTab.tsx where the component actually consumes the scene.

/**
 * One persisted whiteboard document. Mirrors `DiagramSpec` so whiteboards
 * live alongside diagrams as managed, multi-instance dock windows.
 */
export interface WhiteboardSpec {
  id: string
  name: string
  /** Excalidraw `ExcalidrawElement[]` — opaque JSON for persistence. */
  elements: unknown[]
  /**
   * Excalidraw `AppState` subset worth round-tripping (view settings, grid,
   * current tool, …). Opaque JSON.
   */
  appState: Record<string, unknown>
  /** Excalidraw `BinaryFiles` map (embedded image data) — opaque JSON. */
  files: Record<string, unknown>
}

/** A fresh, empty whiteboard scene. */
export function emptyWhiteboardScene(): Pick<
  WhiteboardSpec,
  'elements' | 'appState' | 'files'
> {
  return { elements: [], appState: {}, files: {} }
}
