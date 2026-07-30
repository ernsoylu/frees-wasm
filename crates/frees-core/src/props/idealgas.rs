//! Ideal-gas properties for spelled chemical formulas.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/IdealGas.java`
//! (274 LOC).
//!
//! Spelled formulas (`N2`, `CO2`, `C8H18`) are ideal gases whose enthalpy is
//! referenced to the **enthalpy of formation** at 298.15 K / 1 atm — the
//! convention that makes a combustion energy balance close without bookkeeping
//! (h of CO2 at 25 °C is −8941 kJ/kg, not 0). Full fluid names (`Nitrogen`,
//! `CarbonDioxide`) stay real fluids and are resolved elsewhere.
//!
//! Specific heats are the standard JANAF-style cubic fits
//! `cp = a + bT + cT² + dT³` [kJ/kmol-K], integrated in closed form for
//! enthalpy and entropy. Unlike a real-fluid equation of state (valid to
//! roughly 600–2000 K depending on the fluid) the polynomials extrapolate
//! smoothly through flame temperatures.
//!
//! All public outputs are SI mass basis (J/kg, J/kg-K, m³/kg); entropy is
//! absolute (third law), referenced to 1 atm.
//!
//! # Parity notes
//!
//! * `R_U` is the Java's **truncated** `8.31446`, not `std::f64::consts`-grade
//!   or the `R#` constant the language exposes (`8.314462618`). The two differ
//!   in the 7th digit and the difference is visible in `Cv` and `Volume`; the
//!   literal is transcribed, not "corrected".
//! * The species table is keyed by the **lowercased** spelling, exactly as the
//!   Java `Map.ofEntries` is. `is_ideal_gas` takes an already-lowercased key
//!   (its Java caller passes `parts[2]`, which the parser lowercases); the
//!   `*_of` accessors lowercase for themselves, as their Java counterparts do.
//! * `ch4o`/`ch3oh` and `c2h6o`/`c2h5oh` are duplicate spellings of the same
//!   species in the Java table and are kept as duplicates here.

use crate::diag::{FreesError, Result};

/// Universal gas constant [kJ/kmol-K] — the Java literal, deliberately
/// truncated relative to `R# = 8.314462618`.
const R_U: f64 = 8.31446;
const T_REF: f64 = 298.15;
const P_REF: f64 = 101_325.0;

/// Standard formation enthalpy of **liquid** water [kJ/kmol] (HHV reference).
pub const HF_H2O_LIQUID: f64 = -285_830.0;

/// Tabulated species: molar mass [kg/kmol], formation enthalpy `hf` [kJ/kmol]
/// and absolute entropy `s0` [kJ/kmol-K] at 298.15 K / 1 atm, plus the cp(T)
/// cubic coefficients [kJ/kmol-K].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Species {
    pub molar_mass: f64,
    pub hf: f64,
    pub s0: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
}

const fn sp(molar_mass: f64, hf: f64, s0: f64, a: f64, b: f64, c: f64, d: f64) -> Species {
    Species {
        molar_mass,
        hf,
        s0,
        a,
        b,
        c,
        d,
    }
}

/// `IdealGas.SPECIES`, transcribed entry for entry in declaration order.
const SPECIES: [(&str, Species); 26] = [
    (
        "n2",
        sp(28.013, 0.0, 191.61, 28.90, -0.1571e-2, 0.8081e-5, -2.873e-9),
    ),
    (
        "o2",
        sp(31.999, 0.0, 205.04, 25.48, 1.520e-2, -0.7155e-5, 1.312e-9),
    ),
    (
        "co2",
        sp(
            44.01, -393_520.0, 213.80, 22.26, 5.981e-2, -3.501e-5, 7.469e-9,
        ),
    ),
    (
        "co",
        sp(
            28.011, -110_530.0, 197.65, 28.16, 0.1675e-2, 0.5372e-5, -2.222e-9,
        ),
    ),
    (
        "h2o",
        sp(
            18.015, -241_820.0, 188.83, 32.24, 0.1923e-2, 1.055e-5, -3.595e-9,
        ),
    ),
    (
        "h2",
        sp(2.016, 0.0, 130.68, 29.11, -0.1916e-2, 0.4003e-5, -0.8704e-9),
    ),
    (
        "ch4",
        sp(
            16.043, -74_850.0, 186.16, 19.89, 5.024e-2, 1.269e-5, -11.01e-9,
        ),
    ),
    (
        "c2h6",
        sp(
            30.070, -84_680.0, 229.49, 6.900, 17.27e-2, -6.406e-5, 7.285e-9,
        ),
    ),
    (
        "c3h8",
        sp(
            44.097, -103_850.0, 269.91, -4.04, 30.48e-2, -15.72e-5, 31.74e-9,
        ),
    ),
    (
        "c4h10",
        sp(
            58.124, -126_150.0, 310.12, 3.96, 37.15e-2, -18.34e-5, 35.00e-9,
        ),
    ),
    (
        "c2h4",
        sp(
            28.054, 52_280.0, 219.83, 3.95, 15.64e-2, -8.344e-5, 17.67e-9,
        ),
    ),
    (
        "c2h2",
        sp(
            26.038, 226_730.0, 200.85, 21.80, 9.2143e-2, -6.527e-5, 18.21e-9,
        ),
    ),
    (
        "so2",
        sp(
            64.065, -296_830.0, 248.11, 25.78, 5.795e-2, -3.812e-5, 8.612e-9,
        ),
    ),
    (
        "no",
        sp(
            30.006,
            90_250.0,
            210.76,
            29.34,
            -0.09395e-2,
            0.9747e-5,
            -4.187e-9,
        ),
    ),
    (
        "no2",
        sp(46.006, 33_180.0, 240.06, 22.90, 5.715e-2, -3.52e-5, 7.87e-9),
    ),
    (
        "n2o",
        sp(
            44.013, 82_050.0, 219.96, 24.11, 5.8632e-2, -3.562e-5, 10.58e-9,
        ),
    ),
    // Radicals/atoms for flame chemistry (cp ~ constant; cubic fit flat).
    (
        "oh",
        sp(17.007, 38_987.0, 183.70, 29.10, -0.225e-2, 0.4e-5, -0.13e-9),
    ),
    ("h", sp(1.008, 218_000.0, 114.72, 20.786, 0.0, 0.0, 0.0)),
    ("o", sp(15.999, 249_190.0, 161.06, 20.786, 0.0, 0.0, 0.0)),
    ("n", sp(14.007, 472_680.0, 153.30, 20.786, 0.0, 0.0, 0.0)),
    // Liquid and vapour fuels common in combustion and rocketry.
    (
        "c8h18",
        sp(
            114.231, -208_450.0, 466.73, -6.96, 77.17e-2, -42.84e-5, 91.13e-9,
        ),
    ),
    (
        "c12h26",
        sp(
            170.34, -290_900.0, 622.83, -9.33, 113.7e-2, -64.0e-5, 137.0e-9,
        ),
    ),
    (
        "ch3oh",
        sp(
            32.042, -201_300.0, 239.88, 19.0, 9.152e-2, -1.22e-5, -8.039e-9,
        ),
    ),
    (
        "ch4o",
        sp(
            32.042, -201_300.0, 239.88, 19.0, 9.152e-2, -1.22e-5, -8.039e-9,
        ),
    ),
    (
        "c2h5oh",
        sp(
            46.069, -235_310.0, 282.59, 19.9, 20.96e-2, -10.38e-5, 20.05e-9,
        ),
    ),
    (
        "c2h6o",
        sp(
            46.069, -235_310.0, 282.59, 19.9, 20.96e-2, -10.38e-5, 20.05e-9,
        ),
    ),
];

/// Table lookup by an **already lowercased** key.
fn get(key: &str) -> Option<Species> {
    SPECIES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, s)| *s)
}

/// Whether the (lowercased) fluid spelling is an ideal-gas formula.
///
/// Takes the key verbatim, as the Java does — `is_ideal_gas("N2")` is `false`.
pub fn is_ideal_gas(fluid: &str) -> bool {
    get(fluid).is_some()
}

/// The tabulated species for `species` (case-insensitive), if any.
pub fn species(species: &str) -> Option<Species> {
    get(&species.to_ascii_lowercase())
}

/// Molar mass of a tabulated species [g/mol == kg/kmol], or `NaN` if unknown.
pub fn molar_mass_of(species: &str) -> f64 {
    get(&species.to_ascii_lowercase()).map_or(f64::NAN, |s| s.molar_mass)
}

/// Standard enthalpy of formation at 298.15 K [kJ/kmol], or `NaN` if unknown.
pub fn formation_enthalpy_of(species: &str) -> f64 {
    get(&species.to_ascii_lowercase()).map_or(f64::NAN, |s| s.hf)
}

/// Absolute molar enthalpy [J/mol == kJ/kmol] (incl. formation), `NaN` if
/// unknown.
pub fn molar_enthalpy(species: &str, t: f64) -> f64 {
    get(&species.to_ascii_lowercase()).map_or(f64::NAN, |s| h_molar(&s, t))
}

/// Molar heat capacity [J/mol-K == kJ/kmol-K], `NaN` if unknown.
pub fn molar_cp(species: &str, t: f64) -> f64 {
    get(&species.to_ascii_lowercase()).map_or(f64::NAN, |s| cp_molar(&s, t))
}

/// Absolute molar entropy at (T, p) [J/mol-K == kJ/kmol-K], `NaN` if unknown.
pub fn molar_entropy(species: &str, t: f64, p: f64) -> f64 {
    get(&species.to_ascii_lowercase()).map_or(f64::NAN, |s| s_molar(&s, t, p))
}

/// Molar enthalpy with formation reference [kJ/kmol].
fn h_molar(gas: &Species, t: f64) -> f64 {
    gas.hf
        + gas.a * (t - T_REF)
        + gas.b / 2.0 * (t * t - T_REF * T_REF)
        + gas.c / 3.0 * (t * t * t - T_REF * T_REF * T_REF)
        + gas.d / 4.0 * (t * t * t * t - T_REF * T_REF * T_REF * T_REF)
}

/// Molar cp [kJ/kmol-K].
fn cp_molar(gas: &Species, t: f64) -> f64 {
    gas.a + gas.b * t + gas.c * t * t + gas.d * t * t * t
}

/// Absolute molar entropy at (T, P) [kJ/kmol-K].
fn s_molar(gas: &Species, t: f64, p: f64) -> f64 {
    let integral = gas.a * libm::log(t / T_REF)
        + gas.b * (t - T_REF)
        + gas.c / 2.0 * (t * t - T_REF * T_REF)
        + gas.d / 3.0 * (t * t * t - T_REF * T_REF * T_REF);
    gas.s0 + integral - R_U * libm::log(p / P_REF)
}

/// kJ/kmol (or kJ/kmol-K) to J/kg (or J/kg-K).
fn per_mass(gas: &Species, molar: f64) -> f64 {
    molar * 1000.0 / gas.molar_mass
}

/// Java `Math.clamp(value, lo, hi)`: `min(hi, max(value, lo))`, so `NaN`
/// propagates. Rust's `f64::clamp` agrees on `NaN` but panics when `lo > hi`,
/// which the Java turns into an `IllegalArgumentException`; neither case can
/// arise from the literal bounds used here.
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

/// Inverts a monotone molar function of T by safeguarded Newton.
fn temperature_from(
    target: f64,
    f: impl Fn(f64) -> f64,
    slope: impl Fn(f64) -> f64,
) -> Result<f64> {
    let mut t = 1000.0;
    for _ in 0..100 {
        let error = f(t) - target;
        let step = error / slope(t);
        t = java_clamp(t - step, 10.0, 20_000.0);
        if step.abs() < 1e-9 * t.max(1.0) {
            return Ok(t);
        }
    }
    Err(FreesError::property(
        "Ideal-gas temperature lookup did not converge.",
    ))
}

/// Evaluates an encoded ideal-gas call.
///
/// `parts` is the `$`-split encoded call `[prop, output, fluid, indicator…]`
/// and `values` the indicator values in SI — the same shape the Java
/// `PropertyFunctions.evaluate` hands to `IdealGas.evaluate`. The caller is
/// expected to have checked [`is_ideal_gas`] on `parts[2]`; an unknown species
/// is reported rather than dereferenced (the Java would raise an NPE).
pub fn evaluate(output: &str, parts: &[&str], values: &[f64]) -> Result<f64> {
    let fluid = parts.get(2).copied().unwrap_or_default();
    let gas = get(fluid).ok_or_else(|| {
        FreesError::property(format!(
            "'{}' is not a tabulated ideal-gas species.",
            fluid.to_uppercase()
        ))
    })?;
    let indicators = if parts.len() > 3 {
        &parts[3..]
    } else {
        &[][..]
    };

    match output {
        "enthalpy" | "intenergy" | "cp" | "specheat" | "cv" => {
            let t = single_indicator(output, fluid, indicators, values, "t")?;
            require_positive_temperature(t, fluid)?;
            Ok(match output {
                "enthalpy" => per_mass(&gas, h_molar(&gas, t)),
                "intenergy" => per_mass(&gas, h_molar(&gas, t) - R_U * t),
                "cv" => per_mass(&gas, cp_molar(&gas, t) - R_U),
                _ => per_mass(&gas, cp_molar(&gas, t)),
            })
        }
        "entropy" | "gibbs" => {
            let [t, p] = temperature_pressure(output, fluid, indicators, values)?;
            require_positive_temperature(t, fluid)?;
            if output == "entropy" {
                Ok(per_mass(&gas, s_molar(&gas, t, p)))
            } else {
                let h = h_molar(&gas, t);
                let s = s_molar(&gas, t, p);
                Ok(per_mass(&gas, h - t * s))
            }
        }
        "volume" | "density" => {
            let [t, p] = temperature_pressure(output, fluid, indicators, values)?;
            let v = per_mass(&gas, R_U) * t / p;
            Ok(if output == "volume" { v } else { 1.0 / v })
        }
        "temperature" => {
            // The `values.len()` checks are not in the Java, which would let an
            // arity mismatch raise IndexOutOfBounds. A wasm build compiles
            // `panic = "abort"`, so an out-of-bounds index would take down the
            // whole module instead of returning a diagnostic; both engines
            // still refuse the call.
            if indicators.len() == 1 && indicators[0] == "h" && !values.is_empty() {
                let target = values[0] * gas.molar_mass / 1000.0;
                return temperature_from(target, |t| h_molar(&gas, t), |t| cp_molar(&gas, t));
            }
            if indicators.len() == 2
                && indicators[0] == "s"
                && indicators[1] == "p"
                && values.len() == 2
            {
                let target = values[0] * gas.molar_mass / 1000.0;
                let p = values[1];
                return temperature_from(
                    target,
                    |t| s_molar(&gas, t, p),
                    |t| cp_molar(&gas, t) / t,
                );
            }
            Err(FreesError::evaluation(format!(
                "Temperature({}, ...) takes h=... or s=..., P=... for an ideal gas.",
                fluid.to_uppercase()
            )))
        }
        "compressibility" | "compressibilityfactor" => Ok(1.0),
        _ => Err(FreesError::evaluation(format!(
            "Function '{output}' is not available for the ideal gas {}. \
             Use the full fluid name for real-fluid properties.",
            fluid.to_uppercase()
        ))),
    }
}

fn require_positive_temperature(t: f64, fluid: &str) -> Result<()> {
    if t <= 0.0 {
        return Err(FreesError::property(format!(
            "Ideal-gas properties of {} need an absolute temperature above 0 K, got {t}.",
            fluid.to_uppercase()
        )));
    }
    Ok(())
}

fn single_indicator(
    output: &str,
    fluid: &str,
    indicators: &[&str],
    values: &[f64],
    expected: &str,
) -> Result<f64> {
    if indicators.len() != 1 || indicators[0] != expected {
        let cap = capitalize(output);
        let up = fluid.to_uppercase();
        return Err(FreesError::evaluation(format!(
            "{cap}({up}, ...) is an ideal-gas function of temperature only, \
             e.g. {cap}({up}, T=300)"
        )));
    }
    values.first().copied().ok_or_else(|| {
        FreesError::evaluation(format!("{}(...) needs a value.", capitalize(output)))
    })
}

fn temperature_pressure(
    output: &str,
    fluid: &str,
    indicators: &[&str],
    values: &[f64],
) -> Result<[f64; 2]> {
    if indicators.len() == 2 && values.len() == 2 {
        if indicators[0] == "t" && indicators[1] == "p" {
            return Ok([values[0], values[1]]);
        }
        if indicators[0] == "p" && indicators[1] == "t" {
            return Ok([values[1], values[0]]);
        }
    }
    let cap = capitalize(output);
    let up = fluid.to_uppercase();
    Err(FreesError::evaluation(format!(
        "{cap}({up}, ...) needs T=... and P=... for an ideal gas, \
         e.g. {cap}({up}, T=300, P=101325)"
    )))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle values come from `tools/golden-dumper` (fixture
    /// `chem_idealgas`). Relative tolerance per `fixtures/README.md`: the
    /// closed-form polynomials are bit-reproducible, but `ln` is not specified
    /// across the JVM and Rust's libm, so entropy/Gibbs get the same 1e-9
    /// treatment as everything else.
    fn close(actual: f64, expected: f64) {
        let tol = 1e-9 * expected.abs().max(1e-3);
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual} (|Δ| = {})",
            (actual - expected).abs()
        );
    }

    fn ev(output: &str, fluid: &str, indicators: &[&str], values: &[f64]) -> f64 {
        let mut parts = vec!["prop", output, fluid];
        parts.extend_from_slice(indicators);
        evaluate(output, &parts, values).expect("ideal-gas call succeeds")
    }

    #[test]
    fn table_is_complete() {
        assert_eq!(SPECIES.len(), 26);
        for key in [
            "n2", "o2", "co2", "co", "h2o", "h2", "ch4", "c2h6", "c3h8", "c4h10", "c2h4", "c2h2",
            "so2", "no", "no2", "n2o", "oh", "h", "o", "n", "c8h18", "c12h26", "ch3oh", "ch4o",
            "c2h5oh", "c2h6o",
        ] {
            assert!(is_ideal_gas(key), "missing species {key}");
        }
        // The Java keys are lowercase and looked up verbatim.
        assert!(!is_ideal_gas("N2"));
        assert!(!is_ideal_gas("water"));
    }

    #[test]
    fn alias_spellings_are_the_same_species() {
        assert_eq!(species("ch3oh"), species("ch4o"));
        assert_eq!(species("c2h5oh"), species("c2h6o"));
    }

    #[test]
    fn accessors_are_case_insensitive_and_nan_for_unknowns() {
        assert_eq!(molar_mass_of("CO2"), 44.01);
        assert_eq!(molar_mass_of("co2"), 44.01);
        assert_eq!(formation_enthalpy_of("H2O"), -241_820.0);
        assert_eq!(formation_enthalpy_of("N2"), 0.0);
        assert!(molar_mass_of("kryptonite").is_nan());
        assert!(formation_enthalpy_of("kryptonite").is_nan());
        assert!(molar_enthalpy("kryptonite", 300.0).is_nan());
        assert!(molar_cp("kryptonite", 300.0).is_nan());
        assert!(molar_entropy("kryptonite", 300.0, 101325.0).is_nan());
    }

    #[test]
    fn enthalpy_matches_the_oracle() {
        close(ev("enthalpy", "n2", &["t"], &[500.0]), 211_795.14292401896);
        close(
            ev("enthalpy", "co2", &["t"], &[1000.0]),
            -8_183_557.9454096295,
        );
        close(
            ev("enthalpy", "h2o", &["t"], &[800.0]),
            -12_420_991.616341382,
        );
        close(
            ev("enthalpy", "ch4", &["t"], &[298.15]),
            -4_665_586.236988095,
        );
        close(
            ev("enthalpy", "c8h18", &["t"], &[600.0]),
            -1_140_075.014271301,
        );
    }

    #[test]
    fn internal_energy_matches_the_oracle() {
        close(ev("intenergy", "n2", &["t"], &[500.0]), 63_391.54459467188);
        close(
            ev("intenergy", "co2", &["t"], &[1000.0]),
            -8_372_480.008577093,
        );
    }

    #[test]
    fn heat_capacities_match_the_oracle() {
        close(ev("cp", "n2", &["t"], &[500.0]), 1062.921679220362);
        close(ev("cp", "co2", &["t"], &[1500.0]), 1327.2068847989096);
        close(ev("specheat", "h2", &["t"], &[300.0]), 14_321.413293650794);
        close(ev("cv", "n2", &["t"], &[500.0]), 766.1144825616677);
        close(ev("cv", "o2", &["t"], &[1200.0]), 855.3228538391824);
    }

    #[test]
    fn entropy_and_gibbs_match_the_oracle() {
        close(
            ev("entropy", "n2", &["t", "p"], &[500.0, 101_325.0]),
            7381.970763459769,
        );
        close(
            ev("entropy", "co2", &["t", "p"], &[1000.0, 200_000.0]),
            5988.112649457983,
        );
        // P=... T=... arrives in the other indicator order and must swap.
        close(
            ev("entropy", "o2", &["p", "t"], &[50_000.0, 700.0]),
            7421.198347199499,
        );
        close(
            ev("gibbs", "co2", &["t", "p"], &[1000.0, 101_325.0]),
            -14_300_134.611725716,
        );
    }

    #[test]
    fn volume_density_and_compressibility_match_the_oracle() {
        close(
            ev("volume", "n2", &["t", "p"], &[300.0, 101_325.0]),
            0.8787777843336616,
        );
        close(
            ev("density", "co2", &["t", "p"], &[350.0, 250_000.0]),
            3.780848580150037,
        );
        assert_eq!(
            ev("compressibility", "n2", &["t", "p"], &[300.0, 101_325.0]),
            1.0
        );
        assert_eq!(
            ev(
                "compressibilityfactor",
                "n2",
                &["t", "p"],
                &[300.0, 101_325.0]
            ),
            1.0
        );
    }

    #[test]
    fn inverse_temperature_lookups_match_the_oracle() {
        close(ev("temperature", "co2", &["h"], &[-8e6]), 1145.988962835369);
        close(
            ev("temperature", "n2", &["s", "p"], &[7000.0, 101_325.0]),
            347.7146614047314,
        );
    }

    #[test]
    fn wrong_indicators_and_unknown_outputs_are_refused() {
        let parts = ["prop", "enthalpy", "n2", "p"];
        assert!(evaluate("enthalpy", &parts, &[101_325.0]).is_err());

        let parts = ["prop", "entropy", "n2", "t"];
        assert!(evaluate("entropy", &parts, &[500.0]).is_err());

        let parts = ["prop", "temperature", "n2", "p"];
        assert!(evaluate("temperature", &parts, &[101_325.0]).is_err());

        // Arity mismatches must be diagnostics, never an out-of-bounds panic
        // (wasm builds abort on panic).
        let parts = ["prop", "temperature", "n2", "h"];
        assert!(evaluate("temperature", &parts, &[]).is_err());
        let parts = ["prop", "temperature", "n2", "s", "p"];
        assert!(evaluate("temperature", &parts, &[7000.0]).is_err());
        let parts = ["prop", "cp", "n2", "t"];
        assert!(evaluate("cp", &parts, &[]).is_err());
        let parts = ["prop", "entropy", "n2", "t", "p"];
        assert!(evaluate("entropy", &parts, &[500.0]).is_err());

        let parts = ["prop", "viscosity", "n2", "t", "p"];
        let err = evaluate("viscosity", &parts, &[300.0, 101_325.0])
            .unwrap_err()
            .to_string();
        assert!(err.contains("not available for the ideal gas N2"), "{err}");

        let parts = ["prop", "enthalpy", "water", "t"];
        assert!(evaluate("enthalpy", &parts, &[300.0]).is_err());
    }

    #[test]
    fn non_positive_temperature_is_refused() {
        let parts = ["prop", "enthalpy", "n2", "t"];
        let err = evaluate("enthalpy", &parts, &[0.0])
            .unwrap_err()
            .to_string();
        assert!(err.contains("above 0 K"), "{err}");
        assert!(evaluate("enthalpy", &parts, &[-5.0]).is_err());
        // Volume/density deliberately have no such guard in the Java.
        let parts = ["prop", "volume", "n2", "t", "p"];
        assert!(evaluate("volume", &parts, &[-5.0, 101_325.0]).is_ok());
    }

    #[test]
    fn formation_referenced_enthalpy_at_the_reference_state() {
        // h(T_REF) collapses to hf exactly: every polynomial term vanishes.
        assert_eq!(molar_enthalpy("co2", T_REF), -393_520.0);
        assert_eq!(molar_enthalpy("n2", T_REF), 0.0);
        // ... and the mass-basis CO2 figure the module doc quotes.
        close(
            ev("enthalpy", "co2", &["t"], &[T_REF]),
            -393_520.0 * 1000.0 / 44.01,
        );
    }

    #[test]
    fn java_clamp_propagates_nan_and_saturates() {
        assert!(java_clamp(f64::NAN, 10.0, 20.0).is_nan());
        assert_eq!(java_clamp(5.0, 10.0, 20.0), 10.0);
        assert_eq!(java_clamp(50.0, 10.0, 20.0), 20.0);
        assert_eq!(java_clamp(15.0, 10.0, 20.0), 15.0);
    }
}
