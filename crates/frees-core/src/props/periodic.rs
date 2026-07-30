//! Standard atomic weights of the elements.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/PeriodicTable.java`
//! (55 LOC). The values are IUPAC conventional atomic weights in g/mol, and the
//! table is **data**: it is transcribed entry for entry from the Java, in the
//! Java's own order, with the literals byte-identical. A shortened or "rounded"
//! table would silently change every molar mass the engine reports, so the unit
//! test at the bottom pins the full length and several spot values.
//!
//! The Java stores the table in a `Map.ofEntries(...)` — an unordered hash map
//! used only for lookups — so a sorted-by-nothing array with a linear scan is
//! behaviourally identical here. Fifty-eight entries is small enough that the
//! scan never shows up next to a `ChemicalFormula` parse.

/// Standard atomic weights [g/mol], transcribed from `PeriodicTable.WEIGHTS`.
///
/// The order is the Java's declaration order. It carries no meaning (the Java
/// map is unordered) but keeps the two files diffable side by side.
const WEIGHTS: [(&str, f64); 58] = [
    ("H", 1.008),
    ("He", 4.002602),
    ("Li", 6.94),
    ("Be", 9.0121831),
    ("B", 10.81),
    ("C", 12.011),
    ("N", 14.007),
    ("O", 15.999),
    ("F", 18.998403),
    ("Ne", 20.1797),
    ("Na", 22.989769),
    ("Mg", 24.305),
    ("Al", 26.981538),
    ("Si", 28.085),
    ("P", 30.973762),
    ("S", 32.06),
    ("Cl", 35.45),
    ("Ar", 39.948),
    ("K", 39.0983),
    ("Ca", 40.078),
    ("Sc", 44.955908),
    ("Ti", 47.867),
    ("V", 50.9415),
    ("Cr", 51.9961),
    ("Mn", 54.938044),
    ("Fe", 55.845),
    ("Co", 58.933194),
    ("Ni", 58.6934),
    ("Cu", 63.546),
    ("Zn", 65.38),
    ("Ga", 69.723),
    ("Ge", 72.630),
    ("As", 74.921595),
    ("Se", 78.971),
    ("Br", 79.904),
    ("Kr", 83.798),
    ("Rb", 85.4678),
    ("Sr", 87.62),
    ("Y", 88.90584),
    ("Zr", 91.224),
    ("Nb", 92.90637),
    ("Mo", 95.95),
    ("Ag", 107.8682),
    ("Cd", 112.414),
    ("Sn", 118.710),
    ("Sb", 121.760),
    ("I", 126.90447),
    ("Xe", 131.293),
    ("Cs", 132.905452),
    ("Ba", 137.327),
    ("Pt", 195.084),
    ("Au", 196.966569),
    ("Hg", 200.592),
    ("Pb", 207.2),
    ("Bi", 208.98040),
    ("W", 183.84),
    ("U", 238.02891),
    ("Th", 232.0377),
];

/// Standard atomic weight [g/mol] of an element symbol, or `NaN` if unknown.
///
/// Symbols are **case-sensitive**, exactly as in the Java: `"Co"` is cobalt,
/// `"CO"` is not an element. The `NaN` sentinel (rather than an `Option`) is
/// the Java contract, and [`crate::props::formula`] depends on it — it turns
/// the `NaN` into the "unknown element" formula error at the call site.
pub fn atomic_weight(symbol: &str) -> f64 {
    for (name, weight) in WEIGHTS {
        if name == symbol {
            return weight;
        }
    }
    f64::NAN
}

/// Whether `symbol` names a tabulated element (case-sensitive).
pub fn is_element(symbol: &str) -> bool {
    WEIGHTS.iter().any(|(name, _)| *name == symbol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_complete() {
        // PeriodicTable.java declares exactly 58 entries. A short table is a
        // silent wrong-answer generator, so the count is pinned.
        assert_eq!(WEIGHTS.len(), 58);
        let mut seen: Vec<&str> = WEIGHTS.iter().map(|(n, _)| *n).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before, "duplicate element symbol in the table");
    }

    #[test]
    fn spot_values_match_the_java_literals() {
        assert_eq!(atomic_weight("H"), 1.008);
        assert_eq!(atomic_weight("C"), 12.011);
        assert_eq!(atomic_weight("N"), 14.007);
        assert_eq!(atomic_weight("O"), 15.999);
        assert_eq!(atomic_weight("S"), 32.06);
        assert_eq!(atomic_weight("Ca"), 40.078);
        assert_eq!(atomic_weight("Fe"), 55.845);
        assert_eq!(atomic_weight("Hg"), 200.592);
        // W is declared out of atomic-number order in the Java, between Bi and
        // U; transcribing the order blindly must not drop it.
        assert_eq!(atomic_weight("W"), 183.84);
        assert_eq!(atomic_weight("U"), 238.02891);
        assert_eq!(atomic_weight("Th"), 232.0377);
    }

    #[test]
    fn unknown_symbol_is_nan_and_lookup_is_case_sensitive() {
        assert!(atomic_weight("Xx").is_nan());
        assert!(atomic_weight("co").is_nan(), "lowercase is not a symbol");
        assert!(
            atomic_weight("CO").is_nan(),
            "CO is a molecule, not element"
        );
        assert!(atomic_weight("").is_nan());
        assert!(!is_element("Xx"));
        assert!(is_element("Co"));
        assert!(is_element("C"));
    }
}
