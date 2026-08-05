// Imperative uPlot wrapper, modeled on plots/PlotlyChart.tsx in LIFECYCLE only
// (create/destroy on mount, ResizeObserver, no bespoke CSS) — uPlot's
// aligned-arrays data shape and cursor.sync API differ materially from Plotly.
//
// uPlot and its CSS are statically imported here: this module is only reached
// through the code-split DataAnalyzerTab chunk, so the main bundle never
// carries the charting engine (the same goal PlotlyChart achieves with a
// dynamic import).
//
// The ResizeObserver callback is rAF-throttled and hands uPlot explicit pixel
// dimensions (as PlotlyChart already does): uPlot in a flex/grid dock tile can
// otherwise trigger a "ResizeObserver loop limit exceeded" feedback loop.
//
// TIMING TRAP (found in live testing): uPlot flushes its hook queue via
// queueMicrotask, so a setScale caused by our own setData fires AFTER the
// synchronous call returns. A naive set-flag/call/clear-flag guard therefore
// misses it, every programmatic auto-fit gets reported as a user zoom, and —
// because the decimated envelope's bucket midpoints are inset from the
// requested window — the view ratchets inward forever. Two defenses:
//  1. the guard flag is cleared in a microtask queued after uPlot's flush;
//  2. the x scale is pinned to the REQUESTED window (xRange prop) through a
//     scale range() function, so auto-fit can never drift from what the
//     reducer asked for.

import { useEffect, useRef } from 'react'
import uPlot from 'uplot'
import 'uplot/dist/uPlot.min.css'
import { formatValue } from '../format'

/** Measurement cursors A + B (§2.5e), drawn as labeled vertical lines. */
export interface AbCursors {
  a: number | null
  b: number | null
}

/** What a mouse drag does on the plot area (toolbar-selected, oscilloscope-tool style). */
export type MouseMode = 'zoom' | 'pan'

const CURSOR_COLORS = { a: '#ffd43b', b: '#3bc9db' } as const

/** Min ms between live pan dispatches (windows are re-decimated per range). */
const PAN_DISPATCH_MS = 80

interface Props {
  /** Chart options sans width/height (owned by the wrapper's ResizeObserver). */
  options: Omit<uPlot.Options, 'width' | 'height'>
  data: uPlot.AlignedData
  /** Requested x window; null = fit the full data extents. */
  xRange: [number, number] | null
  /** A/B measurement cursor positions (time values) to draw. */
  cursors?: AbCursors
  /** Drag behavior: 'zoom' = box zoom (default), 'pan' = scroll the window. */
  mouseMode?: MouseMode
  /** User changed the x scale (drag-zoom or wheel); NOT fired for setData. */
  onUserZoom?: (min: number, max: number) => void
  /** Double-click — reset to the full recording. */
  onResetZoom?: () => void
  /** Plain click placed a cursor: A on click, B on Shift+click. */
  onCursorSet?: (t: number, which: 'a' | 'b') => void
  /** SHIFT-drag finished: shift the strip's file offset by this many seconds
   *  (Phase 5a; numeric entry remains the precise path). */
  onOffsetDrag?: (deltaSeconds: number) => void
  /** Hover cursor moved (display-time x), null on leave. Fires from the
   *  hovered chart only; synced strips follow via uPlot's cursor sync. */
  onHover?: (t: number | null) => void
}

export default function UPlotChart({
  options,
  data,
  xRange,
  cursors,
  mouseMode = 'zoom',
  onUserZoom,
  onResetZoom,
  onCursorSet,
  onOffsetDrag,
  onHover,
}: Readonly<Props>) {
  const containerRef = useRef<HTMLDivElement>(null)
  const chartRef = useRef<uPlot | null>(null)
  const dataRef = useRef(data)
  dataRef.current = data
  const xRangeRef = useRef(xRange)
  xRangeRef.current = xRange
  const cursorsRef = useRef(cursors)
  cursorsRef.current = cursors
  const modeRef = useRef(mouseMode)
  modeRef.current = mouseMode
  /** True while the pointer is over THIS chart (not a sync follower). */
  const hovered = useRef(false)
  // True while a programmatic update (create/setData/setSize) is in flight,
  // including the microtask in which uPlot flushes the resulting hooks.
  const internalUpdate = useRef(false)
  const cbRef = useRef({ onUserZoom, onResetZoom, onCursorSet, onOffsetDrag, onHover })
  cbRef.current = { onUserZoom, onResetZoom, onCursorSet, onOffsetDrag, onHover }

  /** Run a programmatic chart mutation without it registering as a user zoom. */
  const guarded = (fn: () => void) => {
    internalUpdate.current = true
    fn()
    // uPlot queues its hook flush as a microtask inside fn(); queueing ours
    // afterwards guarantees the flag outlives that flush.
    queueMicrotask(() => {
      internalUpdate.current = false
    })
  }

  // (Re)create the chart when the options change (series set, scales, sync).
  useEffect(() => {
    const el = containerRef.current
    if (el === null) return

    const opts: uPlot.Options = {
      ...options,
      width: Math.max(el.clientWidth, 100),
      height: Math.max(el.clientHeight, 80),
      cursor: {
        ...options.cursor,
        bind: {
          // SHIFT is reserved for cursor B and offset-drag, and pan mode owns
          // the plain drag: keep uPlot's drag-select (zoom-box) off for both.
          mousedown: (_u, _targ, handler) => (e) => {
            if (!(e as MouseEvent).shiftKey && modeRef.current !== 'pan') handler(e)
            return null
          },
        },
      },
      scales: {
        ...options.scales,
        x: {
          ...options.scales?.x,
          // Pin the view to the requested window; with no request, fit data.
          range: (_u, dataMin, dataMax) =>
            xRangeRef.current ?? ([dataMin, dataMax] as [number, number]),
        },
      },
      hooks: {
        ...options.hooks,
        setScale: [
          ...(options.hooks?.setScale ?? []),
          (u: uPlot, key: string) => {
            if (key !== 'x' || internalUpdate.current) return
            const { min, max } = u.scales.x
            if (min != null && max != null) cbRef.current.onUserZoom?.(min, max)
          },
        ],
        // Live hover readout: report the hovered time (only from the chart the
        // mouse is actually over — synced followers fire setCursor too).
        setCursor: [
          ...(options.hooks?.setCursor ?? []),
          (u: uPlot) => {
            if (!hovered.current) return
            const left = u.cursor.left
            cbRef.current.onHover?.(
              left == null || left < 0 ? null : u.posToVal(left, 'x'),
            )
          },
        ],
        // Measurement cursors A/B: labeled vertical lines over the plot area,
        // read from a ref so cursor moves only need a redraw, not a rebuild.
        draw: [
          ...(options.hooks?.draw ?? []),
          (u: uPlot) => {
            const cur = cursorsRef.current
            if (!cur) return
            const ctx = u.ctx
            const dpr = window.devicePixelRatio || 1
            for (const which of ['a', 'b'] as const) {
              const t = cur[which]
              if (t == null) continue
              const { min, max } = u.scales.x
              if (min == null || max == null || t < min || t > max) continue
              const x = u.valToPos(t, 'x', true)
              ctx.save()
              ctx.strokeStyle = CURSOR_COLORS[which]
              ctx.lineWidth = dpr
              ctx.setLineDash([4 * dpr, 4 * dpr])
              ctx.beginPath()
              ctx.moveTo(x, u.bbox.top)
              ctx.lineTo(x, u.bbox.top + u.bbox.height)
              ctx.stroke()
              ctx.setLineDash([])
              ctx.fillStyle = CURSOR_COLORS[which]
              ctx.font = `${11 * dpr}px sans-serif`
              // Label + time value attached to the cursor line (cursor tooltip).
              ctx.fillText(
                `${which.toUpperCase()} ${formatValue(t)}s`,
                x + 3 * dpr,
                u.bbox.top + 11 * dpr,
              )
              ctx.restore()
            }
          },
        ],
      },
    }
    let chart: uPlot
    guarded(() => {
      chart = new uPlot(opts, dataRef.current, el)
    })
    chartRef.current = chart!

    // Double-click resets to the full recording. uPlot's own dblclick only
    // refits the (already windowed) current data, so intercept in capture
    // phase and stop it reaching uPlot's bubble listener.
    const onDblClick = (e: MouseEvent) => {
      e.stopPropagation()
      cbRef.current.onResetZoom?.()
    }
    chart!.over.addEventListener('dblclick', onDblClick, { capture: true })

    // Plain click (no drag) places measurement cursor A; Shift+click places B;
    // SHIFT-drag (travel > 3px) shifts the strip's file offset (Phase 5a).
    // A plain drag is uPlot's zoom-box in 'zoom' mode, a live window pan in
    // 'pan' mode (window listeners so the drag survives leaving the strip).
    let downPos: { x: number; y: number; shift: boolean } | null = null
    let panStart: { px: number; min: number; max: number; width: number } | null = null
    let lastPanDispatch = 0
    const onMouseDown = (e: MouseEvent) => {
      downPos = { x: e.clientX, y: e.clientY, shift: e.shiftKey }
      if (modeRef.current === 'pan' && !e.shiftKey && e.button === 0) {
        const c = chartRef.current
        const { min, max } = c?.scales.x ?? {}
        if (c && min != null && max != null) {
          panStart = { px: e.clientX, min, max, width: c.over.getBoundingClientRect().width }
        }
      }
    }
    const onWindowMove = (e: MouseEvent) => {
      if (!panStart || panStart.width <= 0) return
      const now = performance.now()
      if (now - lastPanDispatch < PAN_DISPATCH_MS) return
      lastPanDispatch = now
      const d = ((panStart.px - e.clientX) / panStart.width) * (panStart.max - panStart.min)
      cbRef.current.onUserZoom?.(panStart.min + d, panStart.max + d)
    }
    const onWindowUp = (e: MouseEvent) => {
      if (!panStart) return
      const start = panStart
      panStart = null
      if (start.width <= 0 || Math.abs(e.clientX - start.px) <= 3) return
      const d = ((start.px - e.clientX) / start.width) * (start.max - start.min)
      cbRef.current.onUserZoom?.(start.min + d, start.max + d)
    }
    const onMouseUp = (e: MouseEvent) => {
      const down = downPos
      downPos = null
      if (!down) return
      const c = chartRef.current
      if (!c) return
      const rect = c.over.getBoundingClientRect()
      const dragged = Math.abs(e.clientX - down.x) > 3 || Math.abs(e.clientY - down.y) > 3
      if (dragged) {
        if (down.shift) {
          const t0 = c.posToVal(down.x - rect.left, 'x')
          const t1 = c.posToVal(e.clientX - rect.left, 'x')
          if (Number.isFinite(t0) && Number.isFinite(t1) && t1 !== t0) {
            cbRef.current.onOffsetDrag?.(t1 - t0)
          }
        }
        return // plain drag = zoom-box ('zoom') or pan (handled above)
      }
      const t = c.posToVal(e.clientX - rect.left, 'x')
      if (Number.isFinite(t)) cbRef.current.onCursorSet?.(t, down.shift ? 'b' : 'a')
    }
    const onEnter = () => {
      hovered.current = true
    }
    const onLeave = () => {
      hovered.current = false
      cbRef.current.onHover?.(null)
    }
    chart!.over.addEventListener('mousedown', onMouseDown)
    chart!.over.addEventListener('mouseup', onMouseUp)
    chart!.over.addEventListener('mouseenter', onEnter)
    chart!.over.addEventListener('mouseleave', onLeave)
    window.addEventListener('mousemove', onWindowMove)
    window.addEventListener('mouseup', onWindowUp)

    // Wheel = x-zoom centered on the cursor.
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const c = chartRef.current
      if (!c) return
      const { min, max } = c.scales.x
      if (min == null || max == null) return
      const rect = c.over.getBoundingClientRect()
      const xVal = c.posToVal(e.clientX - rect.left, 'x')
      const factor = e.deltaY < 0 ? 0.8 : 1.25
      cbRef.current.onUserZoom?.(xVal - (xVal - min) * factor, xVal + (max - xVal) * factor)
    }
    chart!.over.addEventListener('wheel', onWheel, { passive: false })

    let frame = 0
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const c = chartRef.current
        const host = containerRef.current
        if (!c || !host) return
        guarded(() =>
          c.setSize({ width: Math.max(host.clientWidth, 100), height: Math.max(host.clientHeight, 80) }),
        )
      })
    })
    observer.observe(el)

    return () => {
      observer.disconnect()
      cancelAnimationFrame(frame)
      chart.over.removeEventListener('dblclick', onDblClick, { capture: true })
      chart.over.removeEventListener('wheel', onWheel)
      chart.over.removeEventListener('mousedown', onMouseDown)
      chart.over.removeEventListener('mouseup', onMouseUp)
      chart.over.removeEventListener('mouseenter', onEnter)
      chart.over.removeEventListener('mouseleave', onLeave)
      window.removeEventListener('mousemove', onWindowMove)
      window.removeEventListener('mouseup', onWindowUp)
      chartRef.current = null
      // Destroy can fire hooks too — keep it guarded.
      guarded(() => chart.destroy())
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options])

  // Cursor moves only repaint (draw hooks re-run); no data or scale change.
  useEffect(() => {
    chartRef.current?.redraw(false)
  }, [cursors])


  // Data-only updates keep the chart instance (cursor state survives). The
  // scale range() function re-applies xRangeRef during the reset.
  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return
    guarded(() => chart.setData(data, true))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data])

  // Pan mode advertises itself with a grab cursor (inherited by the overlay).
  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        height: '100%',
        minHeight: 0,
        cursor: mouseMode === 'pan' ? 'grab' : undefined,
      }}
    />
  )
}
