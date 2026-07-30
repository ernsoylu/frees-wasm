// Excalidraw whiteboard editor — a freehand sketching surface that
// complements the native Diagram window (Epic 6/10). Unlike the Diagram's
// solver-bound schematic editor, the whiteboard is a pure canvas: hand-drawn
// shapes, text, and imported images for quick problem-explanation sketches.
//
// Mirrors DiagramTab's single-instance dock-window pattern: App opens one
// dock window per whiteboard ("whiteboard:<id>"), each mounting this
// component with its own `singleWhiteboardId`. The scene (elements, appState,
// files) is persisted as opaque JSON through App-owned state → .frees file,
// so whiteboards round-trip with the rest of the project.
//
// Excalidraw is a large dependency, so this module is code-split: App lazy-
// loads it only when a whiteboard window is first opened (see App.tsx).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ActionIcon, Alert, Group, Text, Tooltip } from '@mantine/core'
import { useComputedColorScheme } from '@mantine/core'
import { IconPhoto, IconFileCode } from '@tabler/icons-react'
import {
  Excalidraw,
  restore,
  serializeAsJSON,
  exportToBlob,
  exportToSvg,
} from '@excalidraw/excalidraw'
import '@excalidraw/excalidraw/index.css'
import type {
  ExcalidrawImperativeAPI,
  AppState,
  BinaryFiles,
} from '@excalidraw/excalidraw/types'
import type { ExcalidrawElement } from '@excalidraw/excalidraw/element/types'
import { WHITEBOARD_SAVE_ERROR_EVENT } from './whiteboardStorage'
import type { WhiteboardSpec } from './types'

interface Props {
  /** The full whiteboard collection (mirrors DiagramTab's diagrams prop). */
  whiteboards: WhiteboardSpec[]
  /** When set, render only this one whiteboard and skip any list chrome. */
  singleWhiteboardId: string
  onWhiteboardsChange: (
    update: WhiteboardSpec[] | ((prev: WhiteboardSpec[]) => WhiteboardSpec[]),
  ) => void
}

/** Persisted scene slice written back on change. */
interface SceneSlice {
  elements: unknown[]
  appState: Record<string, unknown>
  files: Record<string, unknown>
}

export default function WhiteboardTab(props: Readonly<Props>) {
  const { whiteboards, singleWhiteboardId, onWhiteboardsChange } = props
  const scheme = useComputedColorScheme('dark')
  const excalTheme: 'light' | 'dark' = scheme === 'light' ? 'light' : 'dark'

  const wb = useMemo(
    () => whiteboards.find((w) => w.id === singleWhiteboardId) ?? null,
    [whiteboards, singleWhiteboardId],
  )

  const apiRef = useRef<ExcalidrawImperativeAPI | null>(null)
  // Tracks whether initialData has been consumed so theme syncs don't fight
  // the initial render.
  const [ready, setReady] = useState(false)
  const [exporting, setExporting] = useState<'png' | 'svg' | null>(null)
  // Debounce timer for scene persistence.
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  // localStorage save-failure notice (usually quota overflow from embedded
  // images) — surfaced so the user is warned before losing work.
  const [saveError, setSaveError] = useState<string | null>(null)

  // Normalize the persisted opaque scene once via Excalidraw's restore() so a
  // scene saved by an older/newer Excalidraw release is migrated to the
  // current schema before mount. Always returns an object (even for an empty
  // whiteboard) with the app's current color scheme baked in, so Excalidraw
  // mounts in the correct theme immediately — no light flash in dark mode.
  // The live-toggle effect below handles subsequent scheme flips.
  const initialData = useMemo(() => {
    if (!wb || (wb.elements.length === 0 && Object.keys(wb.files).length === 0)) {
      return { elements: [], appState: { theme: excalTheme }, files: {} }
    }
    const restored = restore(
      {
        elements: wb.elements as ExcalidrawElement[],
        appState: wb.appState as Partial<AppState>,
        files: (wb.files ?? undefined) as BinaryFiles,
      },
      null,
      null,
    )
    return {
      elements: restored.elements,
      appState: { ...restored.appState, theme: excalTheme },
      files: restored.files,
      scrollToContent: true,
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wb?.id])

  // Sync the app color scheme into Excalidraw whenever it flips (light/dark).
  // updateScene sets appState.theme, which Excalidraw applies to its canvas
  // AND UI chrome (toolbar, panels) via its internal theme class.
  useEffect(() => {
    if (!ready || !apiRef.current) return
    apiRef.current.updateScene({ appState: { theme: excalTheme } })
  }, [excalTheme, ready])

  // Debounced write-back of the live scene to App state. serializeAsJSON
  // produces Excalidraw's canonical, transient-state-free JSON (the same
  // format written to a .excalidraw file), so we store the parsed result —
  // this keeps the .frees project file clean and version-stable.
  const persistScene = useCallback(
    (elements: readonly ExcalidrawElement[], appState: AppState, files: BinaryFiles) => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
      saveTimerRef.current = setTimeout(() => {
        const json = serializeAsJSON(elements, appState, files, 'local')
        let parsed: SceneSlice = { elements: [], appState: {}, files: {} }
        try {
          const obj = JSON.parse(json) as {
            elements?: unknown[]
            appState?: Record<string, unknown>
            files?: Record<string, unknown>
          }
          parsed = {
            elements: Array.isArray(obj.elements) ? obj.elements : [],
            appState: obj.appState ?? {},
            files: obj.files ?? {},
          }
        } catch {
          // Malformed serialize output — fall back to the raw live values.
          parsed = {
            elements: [...elements],
            appState: appState as unknown as Record<string, unknown>,
            files: files as unknown as Record<string, unknown>,
          }
        }
        onWhiteboardsChange((prev) =>
          prev.map((w) =>
            w.id === singleWhiteboardId
              ? {
                  ...w,
                  elements: parsed.elements,
                  appState: parsed.appState,
                  files: parsed.files,
                }
              : w,
          ),
        )
      }, 500)
    },
    [onWhiteboardsChange, singleWhiteboardId],
  )

  useEffect(() => {
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current)
    }
  }, [])

  // Surface localStorage save failures (previously swallowed) so the user is
  // warned before losing work — usually quota overflow from embedded images.
  useEffect(() => {
    const onSaveError = (e: Event) => {
      const message = (e as CustomEvent<string>).detail
      setSaveError(message || 'The whiteboard could not be saved.')
    }
    window.addEventListener(WHITEBOARD_SAVE_ERROR_EVENT, onSaveError)
    return () => window.removeEventListener(WHITEBOARD_SAVE_ERROR_EVENT, onSaveError)
  }, [])

  const handleExcalidrawAPI = useCallback((api: ExcalidrawImperativeAPI) => {
    apiRef.current = api
    setReady(true)
  }, [])

  const download = (blob: Blob, filename: string) => {
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    document.body.appendChild(a)
    a.click()
    a.remove()
    URL.revokeObjectURL(url)
  }

  const exportPng = useCallback(async () => {
    const api = apiRef.current
    if (!api || exporting) return
    setExporting('png')
    try {
      const blob = await exportToBlob({
        elements: api.getSceneElements(),
        appState: { ...api.getAppState(), exportBackground: true },
        files: api.getFiles(),
        mimeType: 'image/png',
      })
      download(blob, `${(wb?.name ?? 'whiteboard').replace(/[^\w.-]+/g, '_')}.png`)
    } finally {
      setExporting(null)
    }
  }, [exporting, wb?.name])

  const exportSvg = useCallback(async () => {
    const api = apiRef.current
    if (!api || exporting) return
    setExporting('svg')
    try {
      const svg = await exportToSvg({
        elements: api.getSceneElements(),
        appState: api.getAppState(),
        files: api.getFiles(),
      })
      const xml = new XMLSerializer().serializeToString(svg)
      download(
        new Blob([xml], { type: 'image/svg+xml' }),
        `${(wb?.name ?? 'whiteboard').replace(/[^\w.-]+/g, '_')}.svg`,
      )
    } finally {
      setExporting(null)
    }
  }, [exporting, wb?.name])

  if (!wb) {
    return (
      <Group justify="center" align="center" h="100%">
        <ActionIcon variant="light" loading size="lg" color="teal" />
      </Group>
    )
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      {saveError && (
        <Alert
          color="red"
          variant="light"
          p="xs"
          mb={0}
          withCloseButton
          onClose={() => setSaveError(null)}
          title="Whiteboard save failed"
          style={{ flexShrink: 0, borderRadius: 0 }}
        >
          <Text size="xs">{saveError}</Text>
        </Alert>
      )}
      <Group
        justify="flex-end"
        gap="xs"
        px="sm"
        py={4}
        style={{ flexShrink: 0, borderBottom: '1px solid var(--mantine-color-default-border)' }}
      >
        <Tooltip label="Export as PNG image">
          <ActionIcon
            variant="subtle"
            color="gray"
            onClick={exportPng}
            loading={exporting === 'png'}
            aria-label="Export whiteboard as PNG"
          >
            <IconPhoto size={16} />
          </ActionIcon>
        </Tooltip>
        <Tooltip label="Export as SVG vector">
          <ActionIcon
            variant="subtle"
            color="gray"
            onClick={exportSvg}
            loading={exporting === 'svg'}
            aria-label="Export whiteboard as SVG"
          >
            <IconFileCode size={16} />
          </ActionIcon>
        </Tooltip>
      </Group>
      <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>
        <Excalidraw
          initialData={initialData}
          excalidrawAPI={handleExcalidrawAPI}
          onChange={persistScene}
          UIOptions={{
            canvasActions: {
              loadScene: true,
              saveToActiveFile: true,
              export: { saveFileToDisk: true },
              toggleTheme: false,
            },
          }}
          langCode="en"
        />
      </div>
      {/* Excalidraw ships its own fonts and CSS that don't track Mantine; pin
          the canvas wrapper background to the app surface so a light/dark
          flip doesn't leave a mismatched frame around the drawing area. */}
      <style>{`
        .excalidraw-wrapper {
          height: 100%;
          background-color: var(--mantine-color-body);
        }
        .excalidraw {
          --is-mobile: 0;
        }
      `}</style>
    </div>
  )
}
