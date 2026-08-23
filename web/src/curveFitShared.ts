// Shared pieces of the curve-fit UI, used by CurveFitModal (Calculate >
// Curve Fit) and the Digitizer's "Fit curve…" dialog (Wave H): the model
// template list and the fitted-model editor text. The shared result
// rendering lives beside this file in FitResultView.tsx (components only,
// for react-refresh). Extracted rather than duplicated so the two fit
// surfaces cannot drift.

import { formatValue } from './format'

export const MONO_INPUT = {
  input: { fontFamily: 'var(--mantine-font-family-monospace)' },
}

export interface FitTemplate {
  name: string
  equation: string
  yVariable: string
  xVariable: string
  parameters: string
}

export const FIT_TEMPLATES: FitTemplate[] = [
  {
    name: 'Linear (y = a * x + b)',
    equation: 'y = a * x + b',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b',
  },
  {
    name: 'Quadratic (y = a * x^2 + b * x + c)',
    equation: 'y = a * x^2 + b * x + c',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b, c',
  },
  {
    name: 'Cubic (y = a * x^3 + b * x^2 + c * x + d)',
    equation: 'y = a * x^3 + b * x^2 + c * x + d',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b, c, d',
  },
  {
    name: 'Exponential (y = a * exp(b * x))',
    equation: 'y = a * exp(b * x)',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b',
  },
  {
    name: 'Exponential with offset (y = a * exp(b * x) + c)',
    equation: 'y = a * exp(b * x) + c',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b, c',
  },
  {
    name: 'Logarithmic (y = a * ln(x) + b)',
    equation: 'y = a * ln(x) + b',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b',
  },
  {
    name: 'Power (y = a * x^b)',
    equation: 'y = a * x^b',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b',
  },
  {
    name: 'Power with offset (y = a * x^b + c)',
    equation: 'y = a * x^b + c',
    yVariable: 'y',
    xVariable: 'x',
    parameters: 'a, b, c',
  },
]

/** A template's model equation rewritten to the caller's variable names
 * (used by the digitizer fit, whose axes carry their own names). Single-pass
 * whole-word substitution so `y → x` / `x → y` swaps cannot cascade. */
export function templateModelFor(template: FitTemplate, xVar: string, yVar: string): string {
  return template.equation.replace(/\b[xy]\b/g, (m) => (m === 'x' ? xVar : yVar))
}

/** The editor text "Insert equation"/"Copy to Editor" produces: a comment
 * naming the template, one `param = value [unit]` line per fitted parameter,
 * then the model equation itself. */
export function fittedModelInsertText(
  templateKey: string,
  model: string,
  parameterNames: readonly string[],
  fittedParameters: readonly number[],
  parameterUnits: Record<string, string> = {},
): string {
  const paramEquations = parameterNames.map((name, i) => {
    const val = formatValue(fittedParameters[i])
    const unit = parameterUnits[name]?.trim()
    const unitStr = unit ? ` [${unit}]` : ''
    return `${name} = ${val}${unitStr}`
  })
  return [
    `{ Fitted Model: ${templateKey === 'custom' ? 'Custom' : templateKey} }`,
    ...paramEquations,
    model.trim(),
  ].join('\n')
}
