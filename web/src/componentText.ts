// Converts a Component Browser/Wizard selection into the frees component
// instantiation line that gets injected into the equation editor, e.g.
//   Chiller CHLR1(ref$=R1234yf, cool$=EG50, U_tp=3000 [W/m^2-K], ...)
//
// Mirrors the surface syntax used throughout examples.ts: string params keep
// their `$` and are written unquoted (fluid$=R1234yf); numeric params with a
// known unit append it in brackets (P=200000 [Pa]); unitless numerics are bare.
import type { ComponentSpec, ComponentParam } from './componentCatalog'

export type ParamValues = Record<string, string>

/** The model$ variant currently in effect: the chosen value, else the default
 *  (the first variant — matching the backend's selectVariant default). */
export function selectedVariant(spec: ComponentSpec, values: ParamValues): string | null {
  if (spec.variants.length === 0) return null
  const chosen = (values['model$'] ?? '').trim()
  return chosen || spec.variants[0].name
}

/** Params relevant to the current selection: shared params (variants: []) plus
 *  the variant-specific params required by the active model$. Mirrors the backend
 *  rule (ComponentExpander.resolve): a param required only by other variants is
 *  inactive (optional / hidden). */
export function activeParams(spec: ComponentSpec, values: ParamValues): ComponentParam[] {
  const variant = selectedVariant(spec, values)
  return spec.params.filter((p) => p.variants.length === 0 || (variant !== null && p.variants.includes(variant)))
}

/** Build the single-line component instantiation text. Only active params are
 *  emitted (so a stale value from an unselected variant is dropped); empty values
 *  are skipped (an unset optional/selector param uses the std-library default). */
export function generateComponentText(
  spec: ComponentSpec,
  instanceName: string,
  values: ParamValues,
): string {
  const parts: string[] = []
  for (const p of activeParams(spec, values)) {
    const raw = (values[p.name] ?? '').trim()
    if (raw === '') continue
    if (p.isString) {
      // name already carries the trailing `$` (e.g. "fluid$"); value unquoted.
      parts.push(`${p.name}=${raw}`)
    } else if (p.unit) {
      parts.push(`${p.name}=${raw} [${p.unit}]`)
    } else {
      parts.push(`${p.name}=${raw}`)
    }
  }
  const name = instanceName.trim() || suggestInstanceName(spec.type)
  return `${spec.type} ${name}(${parts.join(', ')})`
}

/** Assemble the full editor block: any preamble lines (correlation helpers, a
 *  TABLE block) followed by the component line, ready for insertStatement. */
export function assembleBlock(preamble: string[], componentLine: string): string {
  return [...preamble, componentLine].filter((l) => l.trim() !== '').join('\n')
}

/** Legal frees identifier for an instance name (letters/digits/underscore,
 *  not starting with a digit). */
export function isValidInstanceName(name: string): boolean {
  return /^[A-Za-z_]\w*$/.test(name.trim())
}

/** A short, editable default instance name derived from the component type:
 *  prefer its capital letters (MovingBoundaryEvaporator → MBE), else the first
 *  four characters uppercased (Chiller → CHIL). */
export function suggestInstanceName(type: string): string {
  const caps = type.replace(/[^A-Z]/g, '')
  const base = caps.length >= 2 ? caps : type.slice(0, 4)
  return base.toUpperCase().replace(/[^A-Z0-9]/g, '') || 'C'
}

/** Which required params are still empty — used to gate the Add button. Only
 *  considers params active for the current variant. */
export function missingRequiredParams(spec: ComponentSpec, values: ParamValues): string[] {
  return activeParams(spec, values)
    .filter((p) => p.required && (values[p.name] ?? '').trim() === '')
    .map((p) => p.name)
}
