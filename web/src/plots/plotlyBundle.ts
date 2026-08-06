// Custom Plotly partial bundle: core (scatter) plus only the trace modules the
// app actually renders (figure.ts / the analyzer instruments). Replaces the
// full plotly.js-dist-min bundle, which shipped maps, finance, polar, WebGL-2D
// and every other trace family (~4.6 MB → ~2.6 MB; mesh3d drags the gl3d
// stack and is the largest remaining piece — it backs the surface3d chart).
// Always load this via `import('./plotlyBundle')` so Plotly stays code-split.
import Plotly from 'plotly.js/lib/core'
import bar from 'plotly.js/lib/bar'
import pie from 'plotly.js/lib/pie'
import histogram from 'plotly.js/lib/histogram'
import mesh3d from 'plotly.js/lib/mesh3d'

Plotly.register([bar, pie, histogram, mesh3d])

export default Plotly
