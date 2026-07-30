// Component-expanded variables are stored flat with '$' separators (e.g.
// `brg$port$t` for the port member `BRG.port.t`). The backend renders them
// dotted for display (see Blocker.java), and the UI should do the same anywhere
// a flat solver name is shown to the user. The raw '$' name stays the data key
// (table column id, plot variable value, color map key) — only the visible
// label is demangled, so lookups are unaffected.

/** Demangle a flat solver variable name for display: `brg$port$t` → `brg.port.t`. */
export function displayVar(name: string): string {
  return name.replaceAll('$', '.')
}

/** Build Mantine Select/MultiSelect options that show the demangled label but
 *  keep the raw name as the value (so the selection still keys into the data). */
export function varOptions(names: string[]): { value: string; label: string }[] {
  return names.map((value) => ({ value, label: displayVar(value) }))
}
