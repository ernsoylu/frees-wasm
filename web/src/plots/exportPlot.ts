import type { PlotlyFigure } from 'plotly.js/lib/core'

/**
 * Plot file export. SVG/PNG/JPG come straight from Plotly (raster at 4x
 * scale for print resolution). PDF/EPS are clipped (decision D5): they were
 * transcoded server-side by Apache FOP, which has no browser-engine
 * equivalent, and the api.ts stub could only reject. SVG covers the
 * fully-vector need client-side.
 */

export type ExportFormat = 'svg' | 'png' | 'jpg'

export const EXPORT_FORMATS: { value: ExportFormat; label: string }[] = [
  { value: 'svg', label: 'SVG (vector)' },
  { value: 'png', label: 'PNG (high resolution)' },
  { value: 'jpg', label: 'JPG (high resolution)' },
]

const EXPORT_WIDTH = 1200
const EXPORT_HEIGHT = 800
const RASTER_SCALE = 4

async function figureToSvg(figure: PlotlyFigure): Promise<string> {
  const { default: Plotly } = await import('./plotlyBundle')
  const url = await Plotly.toImage(figure, {
    format: 'svg',
    width: EXPORT_WIDTH,
    height: EXPORT_HEIGHT,
  })
  // Plotly returns a data URL: data:image/svg+xml,<percent-encoded svg>
  return decodeURIComponent(url.substring(url.indexOf(',') + 1))
}

function download(href: string, filename: string) {
  const link = document.createElement('a')
  link.href = href
  link.download = filename
  document.body.appendChild(link)
  link.click()
  link.remove()
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  download(url, filename)
  URL.revokeObjectURL(url)
}

export async function exportPlot(
  figure: PlotlyFigure,
  format: ExportFormat,
  baseName: string,
): Promise<void> {
  const filename = `${baseName || 'plot'}.${format}`
  if (format === 'svg') {
    const svg = await figureToSvg(figure)
    downloadBlob(new Blob([svg], { type: 'image/svg+xml' }), filename)
    return
  }
  const { default: Plotly } = await import('./plotlyBundle')
  const url = await Plotly.toImage(figure, {
    format: format === 'jpg' ? 'jpeg' : 'png',
    width: EXPORT_WIDTH,
    height: EXPORT_HEIGHT,
    scale: RASTER_SCALE,
  })
  download(url, filename)
}
