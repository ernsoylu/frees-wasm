//! Stoichiometry helpers: molar mass, heating value, air-fuel ratio.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/Combustion.java`
//! (93 LOC). These are the three chemistry calls the language exposes with a
//! *token* argument rather than a variable, so the fuel spelling survives
//! parsing intact:
//!
//! ```text
//!   MolarMass(token)          -> kg/mol   (fluid, ideal-gas species, or formula)
//!   HeatingValue(fuel, mode)  -> J/kg     (mode = LHV or HHV)
//!   StoichAFR(fuel)           -> kg air / kg fuel
//! ```
//!
//! # The real-fluid step, and why it is a parameter
//!
//! `Combustion.molarMass` resolves in three stages: the tabulated ideal-gas
//! species, then **CoolProp** for a real fluid (`Water`, `Air`, `R134a`), then
//! the formula parser. The middle stage is the only part of this file that
//! needs the property backend, and that backend is not this module's to own —
//! so it is injected. [`molar_mass_with`] takes the resolver;
//! [`molar_mass`] is the no-resolver form, which behaves exactly as the Java
//! does on a machine where `CoolProp.isAvailable()` is false: `MolarMass(CH4)`
//! and `MolarMass('Ca(OH)2')` work, `MolarMass(Water)` falls through to the
//! formula parser and fails on the element `Wa`.
//!
//! When the property layer lands, pass a closure that performs the Java's
//! `isKnownFluid(lower) && CoolProp.isAvailable()` check and returns
//! `Props1SI(resolveFluid(lower), "molar_mass")` in kg/mol — returning `None`
//! for every failure mode (unknown fluid, backend missing, non-finite or
//! non-positive result) reproduces the Java's fall-through exactly.
//!
//! # Parity note: two different octanes
//!
//! `MolarMass(C8H18)` reports 0.114231 kg/mol — the *tabulated species* mass —
//! while `HeatingValue`/`StoichAFR` divide by the *formula* mass, 114.232
//! g/mol, because they call `ChemicalFormula` directly. The Java does exactly
//! this and the oracle records both; the discrepancy is upstream data, not a
//! porting slip.

use crate::diag::{FreesError, Result};
use crate::props::formula;
use crate::props::idealgas;

const M_O2: f64 = 31.999; // g/mol
const M_N2: f64 = 28.013; // g/mol
/// Air per mole of O2 on a 1 O2 : 3.76 N2 basis [g].
const AIR_PER_MOL_O2: f64 = M_O2 + 3.76 * M_N2;

/// Molar mass [kg/mol] of an ideal-gas species or chemical formula.
///
/// Equivalent to [`molar_mass_with`] with no real-fluid resolver — see the
/// module documentation.
pub fn molar_mass(token: &str) -> Result<f64> {
    molar_mass_with(token, |_| None)
}

/// Molar mass [kg/mol] of a fluid name, ideal-gas species, or chemical formula.
///
/// Resolution order: tabulated ideal-gas species (CO2, CH4, …), then a real
/// fluid via `fluid_molar_mass` (which receives the **lowercased** token and
/// answers in kg/mol), then the formula parser (C8H18, Ca(OH)2). Formulas are
/// case-sensitive.
pub fn molar_mass_with(token: &str, fluid_molar_mass: impl Fn(&str) -> Option<f64>) -> Result<f64> {
    let lower = token.to_ascii_lowercase();
    let ig = idealgas::molar_mass_of(&lower);
    if !ig.is_nan() {
        return Ok(ig / 1000.0);
    }
    if let Some(m) = fluid_molar_mass(&lower) {
        if m.is_finite() && m > 0.0 {
            return Ok(m);
        }
    }
    Ok(formula::molar_mass_grams_per_mole(token)? / 1000.0)
}

/// Heating value [J/kg of fuel] for a hydrocarbon/alcohol CxHyOz burned to CO2
/// and H2O.
///
/// `mode` is `"LHV"` (water leaves as vapour) or `"HHV"` (water condenses);
/// anything that is not `hhv`, case-insensitively, is treated as LHV — the
/// Java's `"hhv".equalsIgnoreCase(mode)` rule.
pub fn heating_value(fuel: &str, mode: &str) -> Result<f64> {
    let counts = formula::parse(fuel)?;
    let x = f64::from(formula::count_of(&counts, "C"));
    let y = f64::from(formula::count_of(&counts, "H"));
    let hf_fuel = idealgas::formation_enthalpy_of(&fuel.to_ascii_lowercase());
    if hf_fuel.is_nan() {
        return Err(FreesError::property(format!(
            "No formation enthalpy tabulated for fuel '{fuel}'. \
             Add it to IdealGas or supply the heating value directly."
        )));
    }
    if x == 0.0 && y == 0.0 {
        return Err(FreesError::property(format!(
            "'{fuel}' has no C or H to burn."
        )));
    }
    let hhv = mode.eq_ignore_ascii_case("hhv");
    let hf_water = if hhv {
        idealgas::HF_H2O_LIQUID
    } else {
        idealgas::formation_enthalpy_of("h2o")
    };
    let hf_co2 = idealgas::formation_enthalpy_of("co2");
    // Reaction enthalpy per kmol fuel [kJ/kmol]; O2 formation enthalpy is 0.
    let d_h = x * hf_co2 + (y / 2.0) * hf_water - hf_fuel;
    let m_fuel = formula::molar_mass_grams_per_mole(fuel)?; // g/mol == kg/kmol
                                                            // -dH [kJ/kmol] / M [kg/kmol] = kJ/kg -> *1000 = J/kg.
    Ok(-d_h / m_fuel * 1000.0)
}

/// Stoichiometric air-fuel ratio (mass basis) for CxHyOz.
pub fn stoich_afr(fuel: &str) -> Result<f64> {
    let counts = formula::parse(fuel)?;
    let x = f64::from(formula::count_of(&counts, "C"));
    let y = f64::from(formula::count_of(&counts, "H"));
    let z = f64::from(formula::count_of(&counts, "O"));
    let o2 = x + y / 4.0 - z / 2.0;
    if o2 <= 0.0 {
        return Err(FreesError::property(format!(
            "'{fuel}' requires no oxidizer (non-combustible)."
        )));
    }
    let mass_air = o2 * AIR_PER_MOL_O2; // g air / mol fuel
    let m_fuel = formula::molar_mass_grams_per_mole(fuel)?;
    Ok(mass_air / m_fuel)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle values from `tools/golden-dumper` (fixtures `chem_molar_mass`,
    /// `chem_heating_value`). Relative tolerance 1e-9 per `fixtures/README.md`.
    fn close(actual: f64, expected: f64) {
        let tol = 1e-9 * expected.abs().max(1e-3);
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual} (|Δ| = {})",
            (actual - expected).abs()
        );
    }

    #[test]
    fn air_per_mole_of_oxygen_is_the_java_constant() {
        assert_eq!(AIR_PER_MOL_O2, 31.999 + 3.76 * 28.013);
    }

    // ---- MolarMass -------------------------------------------------------

    #[test]
    fn molar_mass_prefers_the_species_table() {
        close(molar_mass("CH4").unwrap(), 0.016042999999999998);
        close(molar_mass("CO2").unwrap(), 0.04401);
        close(molar_mass("N2").unwrap(), 0.028013000000000003);
        close(molar_mass("O2").unwrap(), 0.031999);
        close(molar_mass("H2O").unwrap(), 0.018015);
        // Tabulated 114.231, *not* the formula parser's 114.232.
        close(molar_mass("C8H18").unwrap(), 0.114231);
    }

    #[test]
    fn molar_mass_falls_through_to_the_formula_parser() {
        close(molar_mass("Ca(OH)2").unwrap(), 0.07409200000000002);
        close(molar_mass("Al2(SO4)3").unwrap(), 0.34213107600000003);
        close(molar_mass("KNO3").unwrap(), 0.1011023);
        close(molar_mass("C6H12O6").unwrap(), 0.180156);
        close(molar_mass("FeSO4(H2O)7").unwrap(), 0.27800600000000003);
        close(molar_mass("U").unwrap(), 0.23802890999999998);
        close(molar_mass("HgCl2").unwrap(), 0.271492);
    }

    #[test]
    fn species_lookup_is_case_insensitive_but_formulas_are_not() {
        close(molar_mass("ch4").unwrap(), 0.016042999999999998);
        close(molar_mass("Ch4").unwrap(), 0.016042999999999998);
        // 'Kno3' parses as K + No (nobelium, untabulated) + 3.
        assert!(molar_mass("Kno3").is_err());
    }

    #[test]
    fn a_real_fluid_resolver_takes_the_middle_slot() {
        // Without a resolver, "Water" reaches the formula parser and dies on
        // the element "Wa" — the Java's behaviour with CoolProp unavailable.
        assert!(molar_mass("Water").is_err());

        // With one, it answers in kg/mol. The resolver sees the lowercased
        // token, exactly as `PropertyFunctions.isKnownFluid(lower)` does.
        let resolver = |fluid: &str| match fluid {
            "water" => Some(0.018015268),
            _ => None,
        };
        close(molar_mass_with("Water", resolver).unwrap(), 0.018015268);
        // The species table still wins ahead of it.
        close(molar_mass_with("CO2", |_| Some(999.0)).unwrap(), 0.04401);
        // Non-finite / non-positive answers fall through, as in the Java.
        assert!(molar_mass_with("Water", |_| Some(f64::NAN)).is_err());
        assert!(molar_mass_with("Water", |_| Some(0.0)).is_err());
        assert!(molar_mass_with("Water", |_| Some(-1.0)).is_err());
    }

    // ---- HeatingValue ----------------------------------------------------

    #[test]
    fn heating_value_matches_the_oracle() {
        close(heating_value("CH4", "LHV").unwrap(), 50_009_973.19703297);
        close(heating_value("CH4", "HHV").unwrap(), 55_496_478.21479773);
        close(heating_value("C8H18", "LHV").unwrap(), 44_786_837.31353736);
        close(heating_value("C8H18", "HHV").unwrap(), 48_254_254.49961482);
        close(
            heating_value("C2H5OH", "LHV").unwrap(),
            27_723_414.877683472,
        );
        close(heating_value("C2H5OH", "HHV").unwrap(), 30_589_333.39121752);
        close(heating_value("H2", "LHV").unwrap(), 119_950_396.82539682);
        close(heating_value("H2", "HHV").unwrap(), 141_780_753.96825397);
        close(heating_value("C3H8", "LHV").unwrap(), 46_352_132.79814953);
        close(heating_value("CH3OH", "HHV").unwrap(), 23_839_960.05243118);
    }

    #[test]
    fn hhv_exceeds_lhv_by_the_latent_heat_of_the_product_water() {
        for fuel in ["CH4", "C8H18", "C2H5OH", "H2", "C3H8"] {
            let lhv = heating_value(fuel, "LHV").unwrap();
            let hhv = heating_value(fuel, "HHV").unwrap();
            assert!(hhv > lhv, "{fuel}: HHV {hhv} should exceed LHV {lhv}");
        }
    }

    #[test]
    fn mode_matching_is_case_insensitive_and_defaults_to_lhv() {
        let lhv = heating_value("CH4", "LHV").unwrap();
        assert_eq!(heating_value("CH4", "lhv").unwrap(), lhv);
        assert_eq!(heating_value("CH4", "").unwrap(), lhv);
        assert_eq!(heating_value("CH4", "nonsense").unwrap(), lhv);
        let hhv = heating_value("CH4", "HHV").unwrap();
        assert_eq!(heating_value("CH4", "hhv").unwrap(), hhv);
        assert_eq!(heating_value("CH4", "hHv").unwrap(), hhv);
    }

    #[test]
    fn heating_value_refuses_fuels_it_has_no_data_for() {
        // Parses, but no tabulated formation enthalpy.
        let err = heating_value("C6H12O6", "LHV").unwrap_err().to_string();
        assert!(err.contains("No formation enthalpy tabulated"), "{err}");
        // Tabulated, but nothing to burn.
        let err = heating_value("N2", "LHV").unwrap_err().to_string();
        assert!(err.contains("no C or H to burn"), "{err}");
        assert!(heating_value("not a formula", "LHV").is_err());
    }

    /// CO2 has carbon, so it clears the "no C or H" guard, but it is already
    /// fully oxidised: `dH` cancels to zero and the heating value is zero.
    /// The bare arithmetic lands on **-0.0** (`-(+0.0)`), in Java as in Rust;
    /// the solved document reports `+0.0` because the value reaches the
    /// fixture through a Newton root-find, which does not carry the sign of
    /// zero. Oracle: `chem_heating_value`, `lhv_co2 = 0.0`.
    #[test]
    fn a_fully_oxidised_fuel_releases_nothing() {
        let v = heating_value("CO2", "LHV").unwrap();
        assert_eq!(v, 0.0);
        assert!(v.is_sign_negative(), "expected -0.0, got {v:?}");
    }

    // ---- StoichAFR -------------------------------------------------------

    #[test]
    fn stoich_afr_matches_the_oracle() {
        close(stoich_afr("CH4").unwrap(), 17.119975067007417);
        close(stoich_afr("C8H18").unwrap(), 15.027299705861754);
        close(stoich_afr("C2H5OH").unwrap(), 8.942751959017993);
        close(stoich_afr("H2").unwrap(), 34.05949404761905);
        close(stoich_afr("C12H26").unwrap(), 14.914675237759774);
    }

    #[test]
    fn oxygen_in_the_fuel_lowers_the_air_demand() {
        // Ethanol (C2H6O) needs less air than ethane (C2H6) per unit mass.
        assert!(stoich_afr("C2H5OH").unwrap() < stoich_afr("C2H6").unwrap());
    }

    #[test]
    fn stoich_afr_refuses_non_combustibles() {
        let err = stoich_afr("CO2").unwrap_err().to_string();
        assert!(err.contains("requires no oxidizer"), "{err}");
        assert!(stoich_afr("H2O").is_err());
        assert!(stoich_afr("N2").is_err());
        assert!(stoich_afr("not a formula").is_err());
    }
}
