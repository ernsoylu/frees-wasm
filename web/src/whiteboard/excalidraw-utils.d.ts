// Ambient type declaration for `@excalidraw/utils/export`.
//
// Excalidraw's main package bundles the export helpers (exportToBlob /
// exportToSvg / exportToCanvas / exportToClipboard) inline at runtime and
// ships their type definitions under dist/types/utils/export.d.ts, but the
// main entry's .d.ts re-exports them from the EXTERNAL package path
// "@excalidraw/utils/export" — which is not a published stable release and
// is not installed. This declaration makes that external path resolve so
// `import { exportToBlob } from "@excalidraw/excalidraw"` type-checks.
// Signatures mirror Excalidraw's bundled dist/types/utils/export.d.ts.
import type { AppState, BinaryFiles } from '@excalidraw/excalidraw/types'
import type {
  ExcalidrawElement,
  ExcalidrawFrameLikeElement,
  NonDeleted,
} from '@excalidraw/excalidraw/element/types'

type ExportOpts = {
  elements: readonly NonDeleted<ExcalidrawElement>[]
  appState?: Partial<Omit<AppState, 'offsetTop' | 'offsetLeft'>>
  files: BinaryFiles | null
  maxWidthOrHeight?: number
  exportingFrame?: ExcalidrawFrameLikeElement | null
  getDimensions?: (width: number, height: number) => {
    width: number
    height: number
    scale?: number
  }
}

declare module '@excalidraw/utils/export' {
  export const exportToCanvas: (opts: ExportOpts & {
    exportPadding?: number
  }) => Promise<HTMLCanvasElement>
  export const exportToBlob: (opts: ExportOpts & {
    mimeType?: string
    quality?: number
    exportPadding?: number
  }) => Promise<Blob>
  export const exportToSvg: (opts: Omit<ExportOpts, 'getDimensions'> & {
    exportPadding?: number
    renderEmbeddables?: boolean
    skipInliningFonts?: true
    reuseImages?: boolean
  }) => Promise<SVGSVGElement>
  export const exportToClipboard: (opts: ExportOpts & {
    mimeType?: string
    quality?: number
    type: 'png' | 'svg' | 'json'
  }) => Promise<void>
}
