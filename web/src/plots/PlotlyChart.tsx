import { useEffect, useRef } from 'react'
import type { PlotlyFigure } from 'plotly.js-dist-min'

/**
 * Renders a pre-built Plotly figure. Plotly is loaded on demand so the
 * main bundle does not carry the charting library.
 */
export default function PlotlyChart({
  figure,
  minHeight = 380,
}: Readonly<{ figure: PlotlyFigure; minHeight?: number }>) {
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let cancelled = false
    async function render() {
      const { default: Plotly } = await import('plotly.js-dist-min')
      const el = containerRef.current
      if (cancelled || el === null) return
      await Plotly.react(el, figure.data, figure.layout, {
        responsive: true,
        displaylogo: false,
      })
    }
    void render()
    return () => {
      cancelled = true
    }
  }, [figure])

  // Plotly's `responsive` config only tracks the *window*; it does not react to
  // the panel/tile being resized within the dockview workspace. Observe the
  // container directly and resize the plot so it always fills its tile (no
  // scrollbars when shrunk, no empty margins when grown).
  useEffect(() => {
    const el = containerRef.current
    if (el === null) return
    let plotly: typeof import('plotly.js-dist-min').default | null = null
    let frame = 0
    void import('plotly.js-dist-min').then(({ default: P }) => {
      plotly = P
    })
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        if (plotly && containerRef.current) plotly.Plots.resize(containerRef.current)
      })
    })
    observer.observe(el)
    return () => {
      observer.disconnect()
      cancelAnimationFrame(frame)
    }
  }, [])

  useEffect(() => {
    const el = containerRef.current
    return () => {
      if (el) {
        void import('plotly.js-dist-min').then(({ default: Plotly }) =>
          Plotly.purge(el),
        )
      }
    }
  }, [])

  return (
    <div
      ref={containerRef}
      style={{ width: '100%', height: '100%', minHeight }}
    />
  )
}
