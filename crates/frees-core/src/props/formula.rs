//! Chemical-formula parsing and molar mass.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/ChemicalFormula.java`
//! (100 LOC). Parses `C8H18`, `Ca(OH)2`, `Al2(SO4)3`, `KNO3`, `FeSO4(H2O)7` —
//! nested parentheses with integer multipliers — into element counts, and sums
//! [`crate::props::periodic`] weights over them.
//!
//! # Why the counts are an ordered `Vec`, not a `HashMap`
//!
//! The Java accumulates into a `LinkedHashMap` and then sums the molar mass by
//! iterating it. Floating-point addition is not associative, so **the iteration
//! order is part of the answer**: `Ca(OH)2` sums `40.078 + 31.998 + 2.016` and
//! lands on `74.09200000000001`, where a different order gives `74.092`. The
//! oracle records the former (fixture `chem_molar_mass`, `m_caoh2 =
//! 0.07409200000000002` after the `/1000`), so first-seen insertion order is
//! reproduced exactly, including `LinkedHashMap::merge`'s rule that updating an
//! existing key leaves it in its original position.
//!
//! # Error mapping
//!
//! The Java `FormulaException` is a `RuntimeException` with no counterpart in
//! [`FreesError`], so it surfaces as [`FreesError::Property`]. The parity gate
//! maps unrecognised Java exception types to "any Rust error", and a bad
//! formula is a property-evaluation failure in every user-visible sense.

use crate::diag::{FreesError, Result};
use crate::props::periodic;

/// Element symbol → atom count, in **first-seen order** (Java `LinkedHashMap`).
pub type ElementCounts = Vec<(String, i32)>;

fn formula_error(message: impl Into<String>) -> FreesError {
    FreesError::property(message)
}

/// Java `String.isBlank()`: empty, or every character is whitespace.
fn is_blank(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// Java `String.trim()`: strips leading/trailing characters `<= ' '`.
///
/// Deliberately *not* `str::trim`, which also strips Unicode spaces such as
/// U+00A0 that Java's `trim` keeps.
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// `LinkedHashMap::merge(key, n, Integer::sum)`.
///
/// Present keys are summed **in place** (position preserved); absent keys are
/// appended. `wrapping_add` mirrors Java `int` overflow rather than panicking
/// in a debug build.
fn merge(counts: &mut ElementCounts, element: &str, n: i32) {
    if let Some(slot) = counts.iter_mut().find(|(key, _)| key == element) {
        slot.1 = slot.1.wrapping_add(n);
    } else {
        counts.push((element.to_string(), n));
    }
}

/// `Map::getOrDefault(element, 0)` over [`ElementCounts`].
pub fn count_of(counts: &ElementCounts, element: &str) -> i32 {
    counts
        .iter()
        .find(|(key, _)| key == element)
        .map_or(0, |(_, n)| *n)
}

/// Element → atom count for `formula`, in first-seen order.
///
/// Element symbols are **case-sensitive**: one uppercase letter optionally
/// followed by one lowercase letter, exactly as the Java reads them.
pub fn parse(formula: &str) -> Result<ElementCounts> {
    if is_blank(formula) {
        return Err(formula_error("Empty chemical formula."));
    }
    let mut parser = Parser {
        chars: java_trim(formula).chars().collect(),
        pos: 0,
    };
    let counts = parser.parse_group(formula)?;
    if parser.pos != parser.chars.len() {
        return Err(formula_error(format!(
            "Unexpected character at position {} in formula '{formula}'.",
            parser.pos
        )));
    }
    Ok(counts)
}

/// Molar mass of `formula` in g/mol (== kg/kmol).
pub fn molar_mass_grams_per_mole(formula: &str) -> Result<f64> {
    let mut total = 0.0;
    for (element, count) in parse(formula)? {
        let weight = periodic::atomic_weight(&element);
        if weight.is_nan() {
            return Err(formula_error(format!(
                "Unknown element '{element}' in formula '{formula}'."
            )));
        }
        total += weight * f64::from(count);
    }
    Ok(total)
}

/// The recursive-descent cursor. `chars` holds the *trimmed* formula; `pos` is
/// a character index into it, mirroring the Java's index into its `String`.
struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    /// One parenthesis level. Stops at `)` (the caller consumes it) or at the
    /// end of input.
    fn parse_group(&mut self, original: &str) -> Result<ElementCounts> {
        let mut counts: ElementCounts = Vec::new();
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c == '(' {
                self.pos += 1;
                let inner = self.parse_group(original)?;
                if self.pos >= self.chars.len() || self.chars[self.pos] != ')' {
                    return Err(formula_error(format!(
                        "Unbalanced parentheses in '{}'.",
                        self.formula()
                    )));
                }
                self.pos += 1;
                let mult = self.read_number(original)?;
                for (element, count) in inner {
                    merge(&mut counts, &element, count.wrapping_mul(mult));
                }
            } else if c == ')' {
                break;
            } else if c.is_uppercase() {
                let element = self.read_element();
                let n = self.read_number(original)?;
                merge(&mut counts, &element, n);
            } else {
                return Err(formula_error(format!(
                    "Unexpected character '{c}' in '{}'.",
                    self.formula()
                )));
            }
        }
        Ok(counts)
    }

    /// The trimmed formula, for the messages that quote `this.formula`.
    fn formula(&self) -> String {
        self.chars.iter().collect()
    }

    /// One uppercase letter, optionally followed by one lowercase letter.
    fn read_element(&mut self) -> String {
        let start = self.pos;
        self.pos += 1; // uppercase letter
        if self.pos < self.chars.len() && self.chars[self.pos].is_lowercase() {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    /// An optional trailing integer multiplier; absent means 1.
    ///
    /// Java's `Character.isDigit` accepts non-ASCII decimal digits (and
    /// `Integer.parseInt` then parses them); this accepts ASCII digits only.
    /// A chemical formula written with Arabic-Indic digits is the only input
    /// that can tell the two apart.
    fn read_number(&mut self, original: &str) -> Result<i32> {
        let start = self.pos;
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == start {
            return Ok(1);
        }
        let digits: String = self.chars[start..self.pos].iter().collect();
        digits.parse::<i32>().map_err(|_| {
            // Java raises NumberFormatException here; both engines refuse the
            // document, which is the parity that matters.
            formula_error(format!(
                "Atom count '{digits}' in formula '{original}' does not fit in a 32-bit integer."
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(formula: &str) -> ElementCounts {
        parse(formula).expect("formula parses")
    }

    fn pairs(entries: &[(&str, i32)]) -> ElementCounts {
        entries
            .iter()
            .map(|(k, n)| ((*k).to_string(), *n))
            .collect()
    }

    #[test]
    fn simple_formula_keeps_first_seen_order() {
        assert_eq!(counts("C8H18"), pairs(&[("C", 8), ("H", 18)]));
        assert_eq!(counts("KNO3"), pairs(&[("K", 1), ("N", 1), ("O", 3)]));
    }

    #[test]
    fn parenthesised_groups_multiply_and_merge_in_place() {
        // Ca first, then the group's O then H — the order the molar-mass sum
        // depends on.
        assert_eq!(counts("Ca(OH)2"), pairs(&[("Ca", 1), ("O", 2), ("H", 2)]));
        assert_eq!(
            counts("Al2(SO4)3"),
            pairs(&[("Al", 2), ("S", 3), ("O", 12)])
        );
        // O is already present when (H2O)7 merges, so it stays in slot 3 and
        // only H is appended.
        assert_eq!(
            counts("FeSO4(H2O)7"),
            pairs(&[("Fe", 1), ("S", 1), ("O", 11), ("H", 14)])
        );
    }

    #[test]
    fn nested_parentheses() {
        assert_eq!(
            counts("Ca(N(CH3)2)2"),
            pairs(&[("Ca", 1), ("N", 2), ("C", 4), ("H", 12)])
        );
    }

    #[test]
    fn two_letter_symbols_win_over_one() {
        assert_eq!(counts("Co"), pairs(&[("Co", 1)]));
        assert_eq!(counts("CO"), pairs(&[("C", 1), ("O", 1)]));
    }

    #[test]
    fn count_of_defaults_to_zero() {
        let c = counts("C2H5OH");
        assert_eq!(count_of(&c, "C"), 2);
        assert_eq!(count_of(&c, "H"), 6);
        assert_eq!(count_of(&c, "O"), 1);
        assert_eq!(count_of(&c, "N"), 0);
    }

    #[test]
    fn rejects_empty_unbalanced_and_stray_characters() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
        assert!(parse("Ca(OH2").is_err());
        assert!(parse("2H").is_err());
        assert!(parse("c8h18").is_err(), "lowercase start is not an element");
        // A trailing ')' leaves pos short of the end -> "unexpected character".
        let err = parse("H2O)").unwrap_err().to_string();
        assert!(err.contains("Unexpected character at position 3"), "{err}");
    }

    #[test]
    fn leading_and_trailing_space_is_trimmed() {
        assert_eq!(counts("  H2O  "), counts("H2O"));
    }

    #[test]
    fn unknown_element_is_a_molar_mass_error_not_a_parse_error() {
        assert!(parse("Xx2").is_ok());
        let err = molar_mass_grams_per_mole("Xx2").unwrap_err().to_string();
        assert!(err.contains("Unknown element 'Xx'"), "{err}");
    }

    // ---- oracle ground truth -------------------------------------------
    // Values from the Java engine via tools/golden-dumper (fixture
    // `chem_molar_mass`), which reports MolarMass in kg/mol; these are the
    // g/mol figures the fixture's values correspond to, `* 1000`. Compared
    // with `assert_eq!` on purpose: this is pure addition and multiplication
    // of table literals, so it must be bit-exact, not merely close.
    #[test]
    fn molar_mass_matches_the_oracle_bit_for_bit() {
        assert_eq!(
            molar_mass_grams_per_mole("Ca(OH)2").unwrap(),
            74.09200000000001
        );
        assert_eq!(molar_mass_grams_per_mole("Al2(SO4)3").unwrap(), 342.131076);
        assert_eq!(molar_mass_grams_per_mole("KNO3").unwrap(), 101.1023);
        assert_eq!(molar_mass_grams_per_mole("C6H12O6").unwrap(), 180.156);
        assert_eq!(
            molar_mass_grams_per_mole("FeSO4(H2O)7").unwrap(),
            278.00600000000003
        );
        assert_eq!(molar_mass_grams_per_mole("U").unwrap(), 238.02891);
        assert_eq!(molar_mass_grams_per_mole("HgCl2").unwrap(), 271.492);
    }

    /// The formula parser and the `IdealGas` table disagree about octane:
    /// summing the periodic table gives 114.232 g/mol, the tabulated species
    /// says 114.231. `MolarMass(C8H18)` reports the *tabulated* value
    /// (oracle: `m_c8h18 = 0.114231`) because `Combustion::molar_mass`
    /// consults the species table first — see `props::combustion`.
    #[test]
    fn octane_from_the_periodic_table_is_not_the_tabulated_species_mass() {
        assert_eq!(molar_mass_grams_per_mole("C8H18").unwrap(), 114.232);
    }
}
