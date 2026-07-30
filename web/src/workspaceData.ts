import { VariableResult } from './api'

const ARRAY_ELEMENT_REGEX = /^([^[]+)\[([\d,\s-]+)\]$/

export interface ArrayGroup {
  name: string
  is2D: boolean
  rows: number[]
  cols: number[]
  cells: Map<string, VariableResult>
  units: string
}

export interface Grouped {
  scalars: VariableResult[]
  groups: ArrayGroup[]
}

export function group(vars: VariableResult[]): Grouped {
  const scalars: VariableResult[] = []
  const groups = new Map<string, ArrayGroup>()

  for (const v of vars) {
    const match = ARRAY_ELEMENT_REGEX.exec(v.name)
    const indices = match
      ? match[2].split(',').map((s) => Number.parseInt(s.trim(), 10))
      : []
    if (!match || indices.length > 2 || indices.some(Number.isNaN)) {
      scalars.push(v)
      continue
    }
    const base = match[1]
    let g = groups.get(base)
    if (!g) {
      g = { name: base, is2D: indices.length === 2, rows: [], cols: [], cells: new Map(), units: v.units }
      groups.set(base, g)
    }
    if (indices.length === 2) {
      g.is2D = true
      const [r, c] = indices
      if (!g.rows.includes(r)) g.rows.push(r)
      if (!g.cols.includes(c)) g.cols.push(c)
      g.cells.set(`${r},${c}`, v)
    } else {
      const [r] = indices
      if (!g.rows.includes(r)) g.rows.push(r)
      g.cells.set(`${r}`, v)
    }
  }

  for (const g of groups.values()) {
    g.rows.sort((a, b) => a - b)
    g.cols.sort((a, b) => a - b)
  }

  return {
    scalars: scalars.sort((a, b) => a.name.localeCompare(b.name)),
    groups: Array.from(groups.values()).sort((a, b) => a.name.localeCompare(b.name)),
  }
}
