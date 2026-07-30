import { useEffect, useRef } from 'react'
import katex from 'katex'

interface Props {
  math: string
  block?: boolean
}

export default function Latex({ math, block = false }: Readonly<Props>) {
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (containerRef.current) {
      try {
        katex.render(math, containerRef.current, {
          displayMode: block,
          throwOnError: false,
          // Emit MathML alongside the visual HTML so screen readers can read the
          // equation (KaTeX marks the HTML aria-hidden and exposes the MathML).
          output: 'htmlAndMathml',
        })
      } catch (err) {
        // KaTeX can still throw on malformed input despite throwOnError:
        // false; degrade to plain text so the equation stays visible.
        console.error('KaTeX rendering failed, showing plain text:', err)
        containerRef.current.textContent = math
      }
    }
  }, [math, block])

  return <div ref={containerRef} style={{ display: block ? 'block' : 'inline-block' }} />
}
