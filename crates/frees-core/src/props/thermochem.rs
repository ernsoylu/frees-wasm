//! Ideal-gas mixtures and frozen-product flame temperature.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/Thermochemistry.java`
//! (218 LOC).
//!
//! Everything here sits on one **unified molar accessor**: NASA-7 polynomials
//! ([`crate::props::nasa`]) where the combustion mechanism has the species,
//! falling back to the cubic JANAF fits in [`crate::props::idealgas`] for the
//! fuels the mechanism lacks (octane, dodecane, the alcohols). Both bases are
//! absolute and formation-referenced, so they compose inside a single energy
//! balance without a reference-state correction.
//!
//! Exposed to the language as `AdiabaticFlameTemp(fuel$, phi, T_react)` and the
//! mixture functions `mix_mw` / `mix_cp` / `mix_enthalpy` / `mix_entropy`.
//! Mixture outputs are SI mass basis.
//!
//! # Parity notes
//!
//! * The two bases disagree slightly on molar mass — NASA-7 says N2 is 28.014
//!   g/mol, `IdealGas` says 28.013 — and which one answers depends on which
//!   table has the species. That is the Java's behaviour and the oracle
//!   records it (`mix_mw('N2:0.79, O2:0.21')` = 0.028850640000000004 uses the
//!   NASA-7 masses; `MolarMass(N2)` = 0.028013 uses the `IdealGas` one).
//! * [`composition`] returns an **ordered** list, not a map: the mixture sums
//!   are floating-point accumulations whose result depends on the order the
//!   Java `LinkedHashMap` iterates, which is first-seen order.
//! * `!(phi > 0.0)` in [`adiabatic_flame_temp`] stays written as a negation so
//!   that a NaN equivalence ratio takes the reject branch. `phi <= 0.0` would
//!   let NaN through.

// The equivalence-ratio guard is written `!(phi > 0.0)` on purpose: the
// negation makes NaN take the reject branch, which `phi <= 0.0` would not.
// Clippy's `neg_cmp_op_on_partial_ord` exists to catch the *accidental* form;
// here the NaN behaviour is the point, and it is the Java guard being ported.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::diag::{FreesError, Result};
use crate::props::formula;
use crate::props::idealgas;
use crate::props::nasa;

/// Species → mole fraction, in first-seen order (Java `LinkedHashMap`).
pub type Composition = Vec<(String, f64)>;

fn unknown(species: &str) -> FreesError {
    FreesError::property(format!(
        "Thermochemistry: no ideal-gas thermo data for species '{species}'. \
         Known: the standard combustion-mechanism species (N2, O2, CO2, H2O, CH4, C3H8, ...) \
         and the IdealGas fuels (C8H18, CH3OH, ...)."
    ))
}

// ----- unified molar accessor (NASA-7 preferred, IdealGas fallback) ---------

/// Absolute molar enthalpy [J/mol], or an error if no thermo data is known.
pub fn h_mol(species: &str, t: f64) -> Result<f64> {
    if nasa::has(species) {
        return nasa::molar_enthalpy(species, t);
    }
    let h = idealgas::molar_enthalpy(&species.to_ascii_lowercase(), t);
    if h.is_nan() {
        return Err(unknown(species));
    }
    Ok(h)
}

/// Molar heat capacity [J/mol-K].
pub fn cp_mol(species: &str, t: f64) -> Result<f64> {
    if nasa::has(species) {
        return nasa::molar_cp(species, t);
    }
    let cp = idealgas::molar_cp(&species.to_ascii_lowercase(), t);
    if cp.is_nan() {
        return Err(unknown(species));
    }
    Ok(cp)
}

/// Absolute molar entropy at (T, partial pressure p) [J/mol-K].
pub fn s_mol(species: &str, t: f64, p: f64) -> Result<f64> {
    if nasa::has(species) {
        return nasa::molar_entropy(species, t, p);
    }
    let s = idealgas::molar_entropy(&species.to_ascii_lowercase(), t, p);
    if s.is_nan() {
        return Err(unknown(species));
    }
    Ok(s)
}

/// Molar mass [kg/kmol].
///
/// Resolution order is NASA-7, then the `IdealGas` species table, then the
/// formula parser — so `mw_of("N2")` is the mechanism's 28.014, while
/// `MolarMass(N2)` (which starts from `IdealGas`) is 28.013.
pub fn mw_of(species: &str) -> Result<f64> {
    if nasa::has(species) {
        return nasa::molar_mass(species);
    }
    let m = idealgas::molar_mass_of(&species.to_ascii_lowercase());
    if m.is_nan() {
        return formula::molar_mass_grams_per_mole(species);
    }
    Ok(m)
}

// ----- adiabatic flame temperature ------------------------------------------

/// Constant-pressure adiabatic flame temperature [K] for **complete**
/// combustion of a hydrocarbon/alcohol fuel CxHyOz in air (3.76 N2 : 1 O2) at
/// fuel/air equivalence ratio `phi` (≤ 1), reactants entering at `t_react`.
///
/// Products are CO2, H2O, excess O2 and N2 with no dissociation, so the result
/// is an upper bound; it overpredicts real flames most at stoichiometric
/// conditions. [`crate::props::equilibrium::adiabatic_flame_temp`] is the
/// version that dissociates (and admits `phi > 1`).
pub fn adiabatic_flame_temp(fuel: &str, phi: f64, t_react: f64) -> Result<f64> {
    let counts = formula::parse(fuel)?;
    let x = f64::from(formula::count_of(&counts, "C"));
    let y = f64::from(formula::count_of(&counts, "H"));
    let z = f64::from(formula::count_of(&counts, "O"));
    let a_st = x + y / 4.0 - z / 2.0; // stoichiometric O2 per mol fuel
    if a_st <= 0.0 {
        return Err(FreesError::property(format!(
            "AdiabaticFlameTemp: '{fuel}' has no oxygen demand (non-combustible)."
        )));
    }
    // Negated on purpose: a NaN phi must be rejected, and `phi <= 0.0` is
    // false for NaN.
    if !(phi > 0.0) {
        return Err(FreesError::property(format!(
            "AdiabaticFlameTemp: equivalence ratio phi must be > 0, got {phi}."
        )));
    }
    if phi > 1.0 {
        return Err(FreesError::property(
            "AdiabaticFlameTemp: rich combustion (phi > 1) needs a CO/H2 dissociation model \
             that frees does not have yet; use phi <= 1 (stoichiometric or excess air).",
        ));
    }
    // Java reaches `Math.clamp(value, tReact, 6000.0)`, which throws when the
    // lower bound exceeds the upper one or is NaN.
    if t_react.is_nan() || t_react > 6000.0 {
        return Err(FreesError::evaluation(format!(
            "AdiabaticFlameTemp: reactant temperature must be a number at most 6000 K, got {t_react}."
        )));
    }

    let o2sup = a_st / phi; // O2 supplied per mol fuel
    let n2 = 3.76 * o2sup;
    let o2ex = o2sup - a_st; // unburned excess O2

    let h_react =
        h_mol(fuel, t_react)? + o2sup * h_mol("O2", t_react)? + n2 * h_mol("N2", t_react)?;

    let h_prod = |t: f64| -> Result<f64> {
        Ok(x * h_mol("CO2", t)?
            + (y / 2.0) * h_mol("H2O", t)?
            + o2ex * h_mol("O2", t)?
            + n2 * h_mol("N2", t)?)
    };
    let cp_prod = |t: f64| -> Result<f64> {
        Ok(x * cp_mol("CO2", t)?
            + (y / 2.0) * cp_mol("H2O", t)?
            + o2ex * cp_mol("O2", t)?
            + n2 * cp_mol("N2", t)?)
    };

    let mut t = 2000.0;
    for _ in 0..100 {
        let f = h_prod(t)? - h_react;
        let slope = cp_prod(t)?;
        let step = f / slope;
        let t_next = java_clamp(t - step, t_react, 6000.0);
        if (t_next - t).abs() < 1e-6 {
            return Ok(t_next);
        }
        t = t_next;
    }
    Err(FreesError::property(format!(
        "AdiabaticFlameTemp: energy balance did not converge for fuel '{fuel}'."
    )))
}

/// Java `Math.clamp(value, lo, hi)`: `min(hi, max(value, lo))`, `NaN`
/// propagating. Callers guarantee `lo <= hi` (Java throws otherwise).
fn java_clamp(value: f64, lo: f64, hi: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

// ----- ideal-gas mixtures ---------------------------------------------------

/// Parses `'N2:3.76, O2:1, CO2:0.5'` into normalized mole fractions, in
/// first-seen order. Repeated species are summed before normalising.
pub fn composition(spec: &str) -> Result<Composition> {
    let mut moles: Composition = Vec::new();
    let mut total = 0.0;
    for part in spec.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let Some(colon) = token.find(':') else {
            return Err(FreesError::property(format!(
                "Mixture: each component must be 'species:amount', got '{token}'. \
                 Example: 'N2:0.79, O2:0.21'."
            )));
        };
        let sp = token[..colon].trim();
        let text = token[colon + 1..].trim();
        let amount = text.parse::<f64>().map_err(|_| {
            // Java raises NumberFormatException from Double.parseDouble here.
            FreesError::property(format!(
                "Mixture: amount for '{sp}' must be a number, got '{text}'."
            ))
        })?;
        if amount < 0.0 {
            return Err(FreesError::property(format!(
                "Mixture: amount for '{sp}' must be >= 0, got {amount}."
            )));
        }
        match moles.iter_mut().find(|(key, _)| key == sp) {
            Some(slot) => slot.1 += amount,
            None => moles.push((sp.to_string(), amount)),
        }
        total += amount;
    }
    if total <= 0.0 {
        return Err(FreesError::property(format!(
            "Mixture: composition '{spec}' has no positive amounts."
        )));
    }
    for entry in &mut moles {
        entry.1 /= total;
    }
    Ok(moles)
}

/// Mixture molar mass [kg/mol] (matches `MolarMass`'s SI convention).
pub fn mixture_molar_mass(comp: &str) -> Result<f64> {
    let mut mw = 0.0;
    for (species, x) in composition(comp)? {
        mw += x * mw_of(&species)?;
    }
    Ok(mw / 1000.0)
}

/// Mixture specific heat at constant pressure [J/kg-K].
pub fn mixture_cp(comp: &str, t: f64) -> Result<f64> {
    let xs = composition(comp)?;
    let mut cp_molar = 0.0;
    let mut mw = 0.0;
    for (species, x) in xs {
        cp_molar += x * cp_mol(&species, t)?;
        mw += x * mw_of(&species)?;
    }
    Ok(cp_molar / (mw / 1000.0))
}

/// Mixture specific enthalpy [J/kg] (absolute, formation-referenced).
pub fn mixture_enthalpy(comp: &str, t: f64) -> Result<f64> {
    let xs = composition(comp)?;
    let mut h_molar = 0.0;
    let mut mw = 0.0;
    for (species, x) in xs {
        h_molar += x * h_mol(&species, t)?;
        mw += x * mw_of(&species)?;
    }
    Ok(h_molar / (mw / 1000.0))
}

/// Mixture specific entropy at (T, P) [J/kg-K], using **partial** pressures.
pub fn mixture_entropy(comp: &str, t: f64, p: f64) -> Result<f64> {
    let xs = composition(comp)?;
    let mut s_molar = 0.0;
    let mut mw = 0.0;
    for (species, xi) in xs {
        s_molar += xi * s_mol(&species, t, xi * p)?; // partial pressure xi*P
        mw += xi * mw_of(&species)?;
    }
    Ok(s_molar / (mw / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle values from `tools/golden-dumper` (fixtures `chem_mixture` and
    /// `chem_flame_temp`). Relative tolerance 1e-9 per `fixtures/README.md`.
    fn close(actual: f64, expected: f64) {
        let tol = 1e-9 * expected.abs().max(1e-3);
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual} (|Δ| = {})",
            (actual - expected).abs()
        );
    }

    fn comp(spec: &str) -> Composition {
        composition(spec).expect("composition parses")
    }

    #[test]
    fn nasa7_is_preferred_over_the_janaf_fits() {
        // N2 is in both tables and they disagree in the 5th digit.
        assert_eq!(mw_of("N2").unwrap(), 28.014);
        assert_eq!(idealgas::molar_mass_of("n2"), 28.013);
        // Octane is only in IdealGas.
        assert!(!nasa::has("C8H18"));
        assert_eq!(mw_of("C8H18").unwrap(), 114.231);
        // Calcium hydroxide is in neither, so the formula parser answers.
        assert_eq!(mw_of("Ca(OH)2").unwrap(), 74.09200000000001);
    }

    #[test]
    fn unknown_species_is_reported_by_the_thermo_accessors() {
        // The formula parses but has no thermo data.
        let err = h_mol("KNO3", 300.0).unwrap_err().to_string();
        assert!(err.contains("no ideal-gas thermo data"), "{err}");
        assert!(cp_mol("KNO3", 300.0).is_err());
        assert!(s_mol("KNO3", 300.0, 101_325.0).is_err());
        // mw_of, though, falls all the way through to the periodic table.
        assert_eq!(mw_of("KNO3").unwrap(), 101.1023);
        // Nothing anywhere.
        assert!(mw_of("Unobtainium").is_err());
    }

    // ---- composition parsing --------------------------------------------

    #[test]
    fn composition_normalises_and_keeps_first_seen_order() {
        assert_eq!(
            comp("N2:0.79, O2:0.21"),
            vec![("N2".to_string(), 0.79), ("O2".to_string(), 0.21)]
        );
        let air = comp("N2:3.76, O2:1");
        assert_eq!(air[0].0, "N2");
        assert_eq!(air[1].0, "O2");
        close(air[0].1, 3.76 / 4.76);
        close(air[1].1, 1.0 / 4.76);
        close(air.iter().map(|(_, x)| x).sum::<f64>(), 1.0);
    }

    #[test]
    fn repeated_species_are_summed_in_place() {
        let mixed = comp("N2:1, O2:1, N2:2");
        assert_eq!(mixed.len(), 2);
        assert_eq!(mixed[0].0, "N2");
        close(mixed[0].1, 0.75);
        assert_eq!(mixed[1].0, "O2");
        close(mixed[1].1, 0.25);
    }

    #[test]
    fn composition_tolerates_blank_components_and_rejects_bad_ones() {
        assert_eq!(comp("N2:1, , O2:1,").len(), 2);
        assert!(composition("N2 0.79").is_err(), "missing colon");
        assert!(composition("N2:-1").is_err(), "negative amount");
        assert!(composition("N2:0").is_err(), "no positive amount");
        assert!(composition("").is_err());
        assert!(composition("N2:abc").is_err());
    }

    // ---- mixture properties, oracle ground truth -------------------------

    #[test]
    fn mixture_molar_mass_matches_the_oracle() {
        close(
            mixture_molar_mass("N2:0.79, O2:0.21").unwrap(),
            0.028850640000000004,
        );
        close(
            mixture_molar_mass("N2:3.76, O2:1").unwrap(),
            0.028850974789915967,
        );
        close(
            mixture_molar_mass("CO2:1, H2O:2, N2:7.52").unwrap(),
            0.027633486692015208,
        );
        close(mixture_molar_mass("C8H18:1, N2:1").unwrap(), 0.0711225);
    }

    #[test]
    fn mixture_cp_matches_the_oracle() {
        close(
            mixture_cp("N2:0.79, O2:0.21", 300.0).unwrap(),
            1010.0686132802725,
        );
        close(
            mixture_cp("N2:0.79, O2:0.21", 1500.0).unwrap(),
            1219.28063110914,
        );
        close(
            mixture_cp("CO2:1, H2O:2, N2:7.52", 1200.0).unwrap(),
            1367.5099199898423,
        );
        // C8H18 comes from IdealGas, O2 from NASA-7, in one mixture.
        close(
            mixture_cp("C8H18:1, O2:12.5", 400.0).unwrap(),
            1197.0404663188153,
        );
    }

    #[test]
    fn mixture_enthalpy_matches_the_oracle() {
        close(
            mixture_enthalpy("N2:0.79, O2:0.21", 300.0).unwrap(),
            1907.6015934784157,
        );
        close(
            mixture_enthalpy("CO2:1, H2O:2, N2:7.52", 1200.0).unwrap(),
            -1_899_450.837209106,
        );
        close(
            mixture_enthalpy("CH3OH:1", 400.0).unwrap(),
            -6_126_305.066901012,
        );
    }

    #[test]
    fn mixture_entropy_matches_the_oracle() {
        close(
            mixture_entropy("N2:0.79, O2:0.21", 300.0, 101_325.0).unwrap(),
            6891.678273429128,
        );
        close(
            mixture_entropy("CO2:1, H2O:2, N2:7.52", 1200.0, 500_000.0).unwrap(),
            8426.775924376185,
        );
        close(
            mixture_entropy("N2:1, O2:1, N2:2", 400.0, 101_325.0).unwrap(),
            7177.239268819747,
        );
    }

    /// Mixture entropy uses partial pressures, so it exceeds the mole-weighted
    /// pure-component entropy at the same total pressure by the entropy of
    /// mixing. This is the property that makes `mix_entropy` more than a sum.
    #[test]
    fn mixture_entropy_includes_the_entropy_of_mixing() {
        let t = 300.0;
        let p = 101_325.0;
        let mixed = mixture_entropy("N2:0.79, O2:0.21", t, p).unwrap();
        let mw = mixture_molar_mass("N2:0.79, O2:0.21").unwrap();
        let unmixed = (0.79 * s_mol("N2", t, p).unwrap() + 0.21 * s_mol("O2", t, p).unwrap()) / mw;
        assert!(mixed > unmixed, "{mixed} should exceed {unmixed}");
    }

    // ---- adiabatic flame temperature -------------------------------------

    #[test]
    fn adiabatic_flame_temp_matches_the_oracle() {
        close(
            adiabatic_flame_temp("CH4", 1.0, 298.15).unwrap(),
            2325.598129753964,
        );
        close(
            adiabatic_flame_temp("CH4", 0.6, 298.15).unwrap(),
            1669.3305543435877,
        );
        close(
            adiabatic_flame_temp("CH4", 1.0, 600.0).unwrap(),
            2547.0731806785834,
        );
        close(
            adiabatic_flame_temp("C3H8", 1.0, 298.15).unwrap(),
            2392.097596054567,
        );
        close(
            adiabatic_flame_temp("C8H18", 1.0, 298.15).unwrap(),
            2407.7915277881666,
        );
        close(
            adiabatic_flame_temp("H2", 1.0, 298.15).unwrap(),
            2519.402202013419,
        );
        close(
            adiabatic_flame_temp("C2H5OH", 1.0, 298.15).unwrap(),
            2351.090768119678,
        );
        close(
            adiabatic_flame_temp("CH3OH", 0.8, 298.15).unwrap(),
            2047.2051537558593,
        );
        close(
            adiabatic_flame_temp("C2H4", 1.0, 298.15).unwrap(),
            2564.5336892884156,
        );
    }

    #[test]
    fn adiabatic_flame_temp_refuses_the_cases_the_model_cannot_cover() {
        // No oxygen demand.
        let err = adiabatic_flame_temp("CO2", 1.0, 298.15)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no oxygen demand"), "{err}");
        // phi outside (0, 1].
        assert!(adiabatic_flame_temp("CH4", 0.0, 298.15).is_err());
        assert!(adiabatic_flame_temp("CH4", -1.0, 298.15).is_err());
        assert!(adiabatic_flame_temp("CH4", 1.2, 298.15).is_err());
        // The negated guard is what rejects NaN.
        assert!(adiabatic_flame_temp("CH4", f64::NAN, 298.15).is_err());
        // Unparseable fuel.
        assert!(adiabatic_flame_temp("not a formula", 1.0, 298.15).is_err());
    }

    /// Preheating the reactants raises the flame temperature, and leaning the
    /// mixture out lowers it — the monotonicity that makes the Newton loop's
    /// single start point at 2000 K safe.
    #[test]
    fn flame_temperature_is_monotone_in_preheat_and_equivalence_ratio() {
        let cold = adiabatic_flame_temp("CH4", 1.0, 298.15).unwrap();
        let warm = adiabatic_flame_temp("CH4", 1.0, 500.0).unwrap();
        assert!(warm > cold);
        let mut previous = f64::INFINITY;
        for phi in [1.0, 0.9, 0.8, 0.7, 0.6, 0.5] {
            let t = adiabatic_flame_temp("CH4", phi, 298.15).unwrap();
            assert!(t < previous, "phi={phi} gave {t}, not below {previous}");
            previous = t;
        }
    }
}
