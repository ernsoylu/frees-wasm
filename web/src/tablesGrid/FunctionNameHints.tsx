// tablesGrid/FunctionNameHints.tsx
//
// The two name-collision notices every "create a function table" dialog must
// show (Wave H), rendered from a composeTables.checkFunctionName result:
//  - replacing a same-named GUI function table is called out (the Create
//    button should also ask, via its label);
//  - a same-named document TABLE block is surfaced with the D10 merge rule:
//    the DOCUMENT definition wins on the solve path — the UI must never
//    promise that a GUI table overrides it.

import { Text } from '@mantine/core'
import type { NameCheck } from './composeTables'

export function FunctionNameHints({
  name,
  check,
}: Readonly<{ name: string; check: NameCheck }>) {
  if (!check.ok) return null
  return (
    <>
      {check.replacesGui && (
        <Text size="xs" c="yellow.5">
          A function table named “{name.trim()}” already exists in the Tables window — creating
          will replace it.
        </Text>
      )}
      {check.shadowedByCode && (
        <Text size="xs" c="orange.4">
          A TABLE block named “{name.trim()}” is defined in the document. The document’s table
          takes precedence when equations are solved — this GUI table will not override it.
        </Text>
      )}
    </>
  )
}

/** The always-visible footnote (shown even without a collision, so the rule
 * is never a surprise). */
export function FunctionPrecedenceNote() {
  return (
    <Text size="xs" c="dimmed">
      Note: a TABLE block with the same name in the document always takes precedence on the
      solve path — a GUI function table never overrides it.
    </Text>
  )
}
