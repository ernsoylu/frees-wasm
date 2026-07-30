//! NASA-7 two-range ideal-gas thermochemistry.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/NasaThermo.java`
//! (140 LOC). Coefficients come from a standard combustion-mechanism dataset,
//! so enthalpies are absolute (formation-referenced): h(CO2, 298.15 K) =
//! −393.5 kJ/mol.
//!
//! Per species, two coefficient sets `{a1..a7}` cover a low and a high
//! temperature range:
//!
//! ```text
//!   cp/R   = a1 + a2 T + a3 T^2 + a4 T^3 + a5 T^4
//!   h/(RT) = a1 + a2 T/2 + a3 T^2/3 + a4 T^3/4 + a5 T^4/5 + a6/T
//!   s/R    = a1 ln T + a2 T + a3 T^2/2 + a4 T^3/3 + a5 T^4/4 + a7
//! ```
//!
//! Molar outputs are J/mol-K and J/mol. This complements
//! [`crate::props::idealgas`], whose cubic JANAF fits cover the fuels (octane,
//! alcohols) the mechanism lacks.
//!
//! # The table is data, and it is verified against the data
//!
//! The Java loads `core/src/main/resources/nasa7_species.json` at class-init
//! and parses it verbatim. That file is copied **byte for byte** to
//! `nasa7_species.json` next to this module. The `SPECIES` array below is
//! machine-generated from that copy rather than retyped, and
//! [`tests::table_matches_the_embedded_json`] re-parses the JSON at test time
//! and compares every field bit-for-bit — so a hand edit that drifts from the
//! data fails the build instead of silently producing wrong flame
//! temperatures.
//!
//! The JSON is *not* parsed at run time: `serde_json` is a dev-dependency of
//! this crate, and adding a runtime JSON parser to the wasm bundle to read a
//! 16-entry constant table would be a poor trade.

use crate::diag::{FreesError, Result};

/// Universal gas constant [J/mol-K] — the `NasaThermo` literal, which is the
/// full-precision CODATA value (unlike `IdealGas`'s truncated `8.31446`).
const R: f64 = 8.314462618;
/// Reference pressure for the tabulated entropy [Pa] (matches `IdealGas`).
const P_REF: f64 = 101_325.0;

/// One tabulated species. `sigma`/`eps_k` are `NaN` when the mechanism gives
/// no Lennard-Jones parameters.
#[derive(Debug, Clone, Copy)]
pub struct NasaSpecies {
    /// Molar mass [kg/kmol == g/mol].
    pub mw: f64,
    /// `[T_min, T_break, T_max]`; the break decides which coefficient set is used.
    pub t_ranges: [f64; 3],
    pub low: [f64; 7],
    pub high: [f64; 7],
    /// Lennard-Jones collision diameter σ [Å].
    pub sigma: f64,
    /// Lennard-Jones well depth ε/k [K].
    pub eps_k: f64,
}

impl NasaSpecies {
    /// `t <= tRanges[1] ? low : high` — the Java's exact break rule, so the
    /// break temperature itself uses the **low** set.
    fn coeffs_for(&self, t: f64) -> &[f64; 7] {
        if t <= self.t_ranges[1] {
            &self.low
        } else {
            &self.high
        }
    }
}

/// The species table, generated from `nasa7_species.json` (see the module doc).
const SPECIES: [(&str, NasaSpecies); 16] = [
    (
        "N2",
        NasaSpecies {
            mw: 28.014,
            t_ranges: [300.0, 1000.0, 5000.0],
            low: [
                3.298677,
                0.0014082404,
                -3.963222e-06,
                5.641515e-09,
                -2.444854e-12,
                -1020.8999,
                3.950372,
            ],
            high: [
                2.92664,
                0.0014879768,
                -5.68476e-07,
                1.0097038e-10,
                -6.753351e-15,
                -922.7977,
                5.980528,
            ],
            sigma: 3.621,
            eps_k: 97.53,
        },
    ),
    (
        "O2",
        NasaSpecies {
            mw: 31.998,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                3.78245636,
                -0.00299673416,
                9.84730201e-06,
                -9.68129509e-09,
                3.24372837e-12,
                -1063.94356,
                3.65767573,
            ],
            high: [
                3.28253784,
                0.00148308754,
                -7.57966669e-07,
                2.09470555e-10,
                -2.16717794e-14,
                -1088.45772,
                5.45323129,
            ],
            sigma: 3.458,
            eps_k: 107.4,
        },
    ),
    (
        "CO2",
        NasaSpecies {
            mw: 44.009,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                2.35677352,
                0.00898459677,
                -7.12356269e-06,
                2.45919022e-09,
                -1.43699548e-13,
                -48371.9697,
                9.90105222,
            ],
            high: [
                3.85746029,
                0.00441437026,
                -2.21481404e-06,
                5.23490188e-10,
                -4.72084164e-14,
                -48759.166,
                2.27163806,
            ],
            sigma: 3.763,
            eps_k: 244.0,
        },
    ),
    (
        "H2O",
        NasaSpecies {
            mw: 18.015,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                4.19864056,
                -0.0020364341,
                6.52040211e-06,
                -5.48797062e-09,
                1.77197817e-12,
                -30293.7267,
                -0.849032208,
            ],
            high: [
                3.03399249,
                0.00217691804,
                -1.64072518e-07,
                -9.7041987e-11,
                1.68200992e-14,
                -30004.2971,
                4.9667701,
            ],
            sigma: 2.605,
            eps_k: 572.4,
        },
    ),
    (
        "CO",
        NasaSpecies {
            mw: 28.01,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                3.57953347,
                -0.00061035368,
                1.01681433e-06,
                9.07005884e-10,
                -9.04424499e-13,
                -14344.086,
                3.50840928,
            ],
            high: [
                2.71518561,
                0.00206252743,
                -9.98825771e-07,
                2.30053008e-10,
                -2.03647716e-14,
                -14151.8724,
                7.81868772,
            ],
            sigma: 3.65,
            eps_k: 98.1,
        },
    ),
    (
        "H2",
        NasaSpecies {
            mw: 2.016,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                2.34433112,
                0.00798052075,
                -1.9478151e-05,
                2.01572094e-08,
                -7.37611761e-12,
                -917.935173,
                0.683010238,
            ],
            high: [
                3.3372792,
                -4.94024731e-05,
                4.99456778e-07,
                -1.79566394e-10,
                2.00255376e-14,
                -950.158922,
                -3.20502331,
            ],
            sigma: 2.92,
            eps_k: 38.0,
        },
    ),
    (
        "CH4",
        NasaSpecies {
            mw: 16.043,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                5.14987613,
                -0.0136709788,
                4.91800599e-05,
                -4.84743026e-08,
                1.66693956e-11,
                -10246.6476,
                -4.64130376,
            ],
            high: [
                0.074851495,
                0.0133909467,
                -5.73285809e-06,
                1.22292535e-09,
                -1.0181523e-13,
                -9468.34459,
                18.437318,
            ],
            sigma: 3.746,
            eps_k: 141.4,
        },
    ),
    (
        "C2H6",
        NasaSpecies {
            mw: 30.07,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                4.29142492,
                -0.0055015427,
                5.99438288e-05,
                -7.08466285e-08,
                2.68685771e-11,
                -11522.2055,
                2.66682316,
            ],
            high: [
                1.0718815,
                0.0216852677,
                -1.00256067e-05,
                2.21412001e-09,
                -1.9000289e-13,
                -11426.3932,
                15.1156107,
            ],
            sigma: 4.302,
            eps_k: 252.3,
        },
    ),
    (
        "C3H8",
        NasaSpecies {
            mw: 44.097,
            t_ranges: [300.0, 1000.0, 5000.0],
            low: [
                0.93355381,
                0.026424579,
                6.1059727e-06,
                -2.1977499e-08,
                9.5149253e-12,
                -13958.52,
                19.201691,
            ],
            high: [
                7.5341368,
                0.018872239,
                -6.2718491e-06,
                9.1475649e-10,
                -4.7838069e-14,
                -16467.516,
                -17.892349,
            ],
            sigma: 4.982,
            eps_k: 266.8,
        },
    ),
    (
        "C2H4",
        NasaSpecies {
            mw: 28.054,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                3.95920148,
                -0.00757052247,
                5.70990292e-05,
                -6.91588753e-08,
                2.69884373e-11,
                5089.77593,
                4.09733096,
            ],
            high: [
                2.03611116,
                0.0146454151,
                -6.71077915e-06,
                1.47222923e-09,
                -1.25706061e-13,
                4939.88614,
                10.3053693,
            ],
            sigma: 3.971,
            eps_k: 280.8,
        },
    ),
    (
        "C2H2",
        NasaSpecies {
            mw: 26.038,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                0.808681094,
                0.0233615629,
                -3.55171815e-05,
                2.80152437e-08,
                -8.50072974e-12,
                26428.9807,
                13.9397051,
            ],
            high: [
                4.14756964,
                0.00596166664,
                -2.37294852e-06,
                4.67412171e-10,
                -3.61235213e-14,
                25935.9992,
                -1.23028121,
            ],
            sigma: 4.1,
            eps_k: 209.0,
        },
    ),
    (
        "OH",
        NasaSpecies {
            mw: 17.007,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                3.99201543,
                -0.00240131752,
                4.61793841e-06,
                -3.88113333e-09,
                1.3641147e-12,
                3615.08056,
                -0.103925458,
            ],
            high: [
                3.09288767,
                0.000548429716,
                1.26505228e-07,
                -8.79461556e-11,
                1.17412376e-14,
                3858.657,
                4.4766961,
            ],
            sigma: 2.75,
            eps_k: 80.0,
        },
    ),
    (
        "H",
        NasaSpecies {
            mw: 1.008,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                2.5,
                7.05332819e-13,
                -1.99591964e-15,
                2.30081632e-18,
                -9.27732332e-22,
                25473.6599,
                -0.446682853,
            ],
            high: [
                2.50000001,
                -2.30842973e-11,
                1.61561948e-14,
                -4.73515235e-18,
                4.98197357e-22,
                25473.6599,
                -0.446682914,
            ],
            sigma: 2.05,
            eps_k: 145.0,
        },
    ),
    (
        "O",
        NasaSpecies {
            mw: 15.999,
            t_ranges: [200.0, 1000.0, 3500.0],
            low: [
                3.1682671,
                -0.00327931884,
                6.64306396e-06,
                -6.12806624e-09,
                2.11265971e-12,
                29122.2592,
                2.05193346,
            ],
            high: [
                2.56942078,
                -8.59741137e-05,
                4.19484589e-08,
                -1.00177799e-11,
                1.22833691e-15,
                29217.5791,
                4.78433864,
            ],
            sigma: 2.75,
            eps_k: 80.0,
        },
    ),
    (
        "N",
        NasaSpecies {
            mw: 14.007,
            t_ranges: [200.0, 1000.0, 6000.0],
            low: [2.5, 0.0, 0.0, 0.0, 0.0, 56104.637, 4.1939087],
            high: [
                2.4159429,
                0.00017489065,
                -1.1902369e-07,
                3.0226245e-11,
                -2.0360982e-15,
                56133.773,
                4.6496096,
            ],
            sigma: 3.298,
            eps_k: 71.4,
        },
    ),
    (
        "AR",
        NasaSpecies {
            mw: 39.948,
            t_ranges: [300.0, 1000.0, 5000.0],
            low: [2.5, 0.0, 0.0, 0.0, 0.0, -745.375, 4.366],
            high: [2.5, 0.0, 0.0, 0.0, 0.0, -745.375, 4.366],
            sigma: 3.33,
            eps_k: 136.5,
        },
    ),
];

/// Canonical mechanism key for a token (uppercased; argon spellings → `AR`).
fn key(token: &str) -> String {
    let k = token.to_uppercase();
    if k == "ARGON" {
        "AR".to_string()
    } else {
        k
    }
}

fn lookup(token: &str) -> Option<&'static NasaSpecies> {
    let k = key(token);
    SPECIES.iter().find(|(name, _)| *name == k).map(|(_, s)| s)
}

/// Whether NASA-7 coefficients are tabulated for the (case-insensitive) species.
pub fn has(token: &str) -> bool {
    lookup(token).is_some()
}

/// The tabulated species record, if any.
pub fn species(token: &str) -> Option<&'static NasaSpecies> {
    lookup(token)
}

fn require(token: &str) -> Result<&'static NasaSpecies> {
    lookup(token).ok_or_else(|| {
        FreesError::property(format!("NASA-7 thermo: no data for species '{token}'."))
    })
}

/// Molar mass [kg/kmol == g/mol].
pub fn molar_mass(token: &str) -> Result<f64> {
    Ok(require(token)?.mw)
}

/// Molar heat capacity at constant pressure [J/mol-K].
pub fn molar_cp(token: &str, t: f64) -> Result<f64> {
    let c = require(token)?.coeffs_for(t);
    Ok(R * (c[0] + c[1] * t + c[2] * t * t + c[3] * t * t * t + c[4] * t * t * t * t))
}

/// Absolute molar enthalpy (includes enthalpy of formation) [J/mol].
pub fn molar_enthalpy(token: &str, t: f64) -> Result<f64> {
    let c = require(token)?.coeffs_for(t);
    Ok(R * t
        * (c[0]
            + c[1] * t / 2.0
            + c[2] * t * t / 3.0
            + c[3] * t * t * t / 4.0
            + c[4] * t * t * t * t / 5.0
            + c[5] / t))
}

/// Absolute molar entropy at (T, partial pressure p) [J/mol-K].
pub fn molar_entropy(token: &str, t: f64, p: f64) -> Result<f64> {
    let c = require(token)?.coeffs_for(t);
    let s0 = R
        * (c[0] * libm::log(t)
            + c[1] * t
            + c[2] * t * t / 2.0
            + c[3] * t * t * t / 3.0
            + c[4] * t * t * t * t / 4.0
            + c[6]);
    Ok(s0 - R * libm::log(p / P_REF))
}

/// Whether Lennard-Jones transport parameters are tabulated for the species.
pub fn has_transport(token: &str) -> bool {
    lookup(token).is_some_and(|s| !s.sigma.is_nan() && !s.eps_k.is_nan())
}

/// Lennard-Jones collision diameter σ [Å].
pub fn collision_diameter(token: &str) -> Result<f64> {
    Ok(require(token)?.sigma)
}

/// Lennard-Jones potential well depth ε/k [K].
pub fn well_depth(token: &str) -> Result<f64> {
    Ok(require(token)?.eps_k)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The oracle records mass-basis values (`mix_cp('N2:1', 400)` etc.), which
    /// for a one-component mixture are exactly `molar / (mw/1000)`. Dividing
    /// here keeps the golden numbers in the test verbatim instead of
    /// pre-multiplied.
    fn mass_cp(token: &str, t: f64) -> f64 {
        molar_cp(token, t).unwrap() / (species(token).unwrap().mw / 1000.0)
    }

    fn mass_h(token: &str, t: f64) -> f64 {
        molar_enthalpy(token, t).unwrap() / (species(token).unwrap().mw / 1000.0)
    }

    fn mass_s(token: &str, t: f64, p: f64) -> f64 {
        molar_entropy(token, t, p).unwrap() / (species(token).unwrap().mw / 1000.0)
    }

    fn close(actual: f64, expected: f64) {
        let tol = 1e-9 * expected.abs().max(1e-3);
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual} (|Δ| = {})",
            (actual - expected).abs()
        );
    }

    /// `serde_json`'s default number parser is **not** correctly rounded — it
    /// reads the JSON's `-1.99591964e-15` (H, a3) as `-1.9959196400000004e-15`,
    /// one ULP off. Jackson, which is what the Java oracle uses, goes through
    /// `Double.parseDouble` and *is* correctly rounded, as is Rust's
    /// `f64::from_str`. So the generated literals follow the JVM, and the
    /// positional check below tolerates serde's one-ULP slack while
    /// [`every_generated_value_is_a_literal_from_the_json`] pins exactness
    /// against the file's own text.
    fn within_one_ulp(a: f64, b: f64) -> bool {
        if a.to_bits() == b.to_bits() {
            return true;
        }
        a.is_finite()
            && b.is_finite()
            && a.is_sign_positive() == b.is_sign_positive()
            && (i128::from(a.to_bits()) - i128::from(b.to_bits())).abs() <= 1
    }

    /// Numeric literals of the JSON, parsed by Rust's correctly-rounded
    /// `f64::from_str` instead of serde's fast path. String contents are
    /// skipped so the `_comment` prose contributes nothing.
    fn json_literals(raw: &str) -> Vec<f64> {
        let mut out = Vec::new();
        let mut token = String::new();
        let mut in_string = false;
        let mut escaped = false;
        let flush = |token: &mut String, out: &mut Vec<f64>| {
            if let Ok(v) = token.parse::<f64>() {
                out.push(v);
            }
            token.clear();
        };
        for ch in raw.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '0'..='9' | '.' | '-' | '+' | 'e' | 'E' => token.push(ch),
                _ => flush(&mut token, &mut out),
            }
        }
        flush(&mut token, &mut out);
        out
    }

    /// The table must be the JSON, field for field, in the right slots.
    /// `serde_json` is a dev-dependency, so this check costs the shipped
    /// bundle nothing.
    #[test]
    fn table_matches_the_embedded_json() {
        let raw = include_str!("nasa7_species.json");
        let root: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let json = root["species"].as_object().expect("species object");

        assert_eq!(
            json.len(),
            SPECIES.len(),
            "the generated table has a different species count than the JSON"
        );

        for (name, entry) in json {
            let (_, s) = SPECIES
                .iter()
                .find(|(k, _)| k == name)
                .unwrap_or_else(|| panic!("species {name} missing from the generated table"));

            let check = |got: f64, want: f64, what: &str| {
                assert!(
                    within_one_ulp(got, want),
                    "{name}: {what} generated {got:?} but JSON has {want:?}"
                );
            };

            check(s.mw, entry["M"].as_f64().unwrap(), "M");

            let tr = entry["Tranges"].as_array().unwrap();
            assert_eq!(tr.len(), 3, "{name}: Tranges length");
            for (i, v) in tr.iter().enumerate() {
                check(s.t_ranges[i], v.as_f64().unwrap(), &format!("Tranges[{i}]"));
            }

            for (set, coeffs) in [
                (&s.low, &entry["coeffs"][0]),
                (&s.high, &entry["coeffs"][1]),
            ] {
                let arr = coeffs.as_array().unwrap();
                assert_eq!(arr.len(), 7, "{name}: coefficient set length");
                for (i, v) in arr.iter().enumerate() {
                    check(set[i], v.as_f64().unwrap(), &format!("a{}", i + 1));
                }
            }

            match entry.get("sigma").and_then(|v| v.as_f64()) {
                Some(v) => check(s.sigma, v, "sigma"),
                None => assert!(s.sigma.is_nan(), "{name}: sigma should be NaN"),
            }
            match entry.get("epsk").and_then(|v| v.as_f64()) {
                Some(v) => check(s.eps_k, v, "epsk"),
                None => assert!(s.eps_k.is_nan(), "{name}: epsk should be NaN"),
            }
        }
    }

    /// Exactness half of the pair: every number in the generated table is
    /// bit-identical to a literal that actually appears in the JSON, parsed
    /// the way the JVM parses it. Together with the positional check above,
    /// a wrong digit *and* a wrong slot would both have to slip through.
    #[test]
    fn every_generated_value_is_a_literal_from_the_json() {
        let literals = json_literals(include_str!("nasa7_species.json"));
        assert!(literals.len() > 16 * 18, "scanner found too few literals");
        let present = |v: f64| literals.iter().any(|&x| x.to_bits() == v.to_bits());

        for (name, s) in SPECIES {
            let mut values = vec![s.mw, s.sigma, s.eps_k];
            values.extend_from_slice(&s.t_ranges);
            values.extend_from_slice(&s.low);
            values.extend_from_slice(&s.high);
            for v in values {
                assert!(present(v), "{name}: {v:?} is not a literal in the JSON");
            }
        }
    }

    #[test]
    fn the_embedded_json_is_the_reference_copy() {
        // The 16 mechanism species the Java ships. If the reference JSON ever
        // grows a species, this fails and the table must be regenerated.
        let names: Vec<&str> = SPECIES.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            vec![
                "N2", "O2", "CO2", "H2O", "CO", "H2", "CH4", "C2H6", "C3H8", "C2H4", "C2H2", "OH",
                "H", "O", "N", "AR"
            ]
        );
    }

    #[test]
    fn keys_are_case_insensitive_and_argon_is_aliased() {
        assert!(has("N2"));
        assert!(has("n2"));
        assert!(has("Co2"));
        assert!(has("AR"));
        assert!(has("Argon"));
        assert!(has("argon"));
        assert!(
            !has("C8H18"),
            "octane is an IdealGas fuel, not a mechanism species"
        );
        assert!(!has("Water"));
        assert_eq!(molar_mass("Argon").unwrap(), molar_mass("AR").unwrap());
    }

    #[test]
    fn unknown_species_is_a_property_error() {
        let err = molar_cp("kryptonite", 300.0).unwrap_err().to_string();
        assert!(err.contains("no data for species 'kryptonite'"), "{err}");
        assert!(molar_mass("kryptonite").is_err());
        assert!(molar_enthalpy("kryptonite", 300.0).is_err());
        assert!(molar_entropy("kryptonite", 300.0, 101_325.0).is_err());
        assert!(collision_diameter("kryptonite").is_err());
        assert!(well_depth("kryptonite").is_err());
        assert!(!has_transport("kryptonite"));
    }

    /// `coeffsFor` uses `<=`, so the break temperature itself takes the **low**
    /// set. The oracle pins N2 at exactly 1000 K.
    #[test]
    fn the_break_temperature_uses_the_low_coefficient_set() {
        let n2 = species("N2").unwrap();
        assert_eq!(n2.t_ranges[1], 1000.0);
        assert!(std::ptr::eq(n2.coeffs_for(1000.0), &n2.low));
        assert!(std::ptr::eq(n2.coeffs_for(1000.000001), &n2.high));
        close(mass_cp("N2", 1000.0), 1169.4847572427025);
    }

    #[test]
    fn molar_cp_matches_the_oracle() {
        close(mass_cp("N2", 400.0), 1046.6020030286302);
        close(mass_cp("N2", 2000.0), 1284.6545256241761);
        close(mass_cp("O2", 400.0), 941.3515095291774);
        close(mass_cp("O2", 2500.0), 1215.907280326406);
        close(mass_cp("CO2", 1500.0), 1326.9191748941912);
        close(mass_cp("H2O", 1500.0), 2625.109350631556);
        close(mass_cp("CO", 1500.0), 1257.0989615066396);
        close(mass_cp("H2", 1500.0), 16011.510172806735);
        close(mass_cp("CH4", 800.0), 3989.197036608699);
        close(mass_cp("C2H6", 800.0), 3590.713240704447);
        close(mass_cp("C3H8", 800.0), 3511.8971483676746);
        close(mass_cp("C2H4", 800.0), 2990.788714438257);
        close(mass_cp("C2H2", 800.0), 2436.044026739296);
        close(mass_cp("OH", 2000.0), 2043.566809429477);
        close(mass_cp("H", 2000.0), 20621.18703661885);
        close(mass_cp("O", 2000.0), 1301.6988073678126);
        close(mass_cp("N", 2000.0), 1483.3078648607634);
        close(mass_cp("AR", 2000.0), 520.3303430709924);
        close(mass_cp("Argon", 2000.0), 520.3303430709924);
    }

    #[test]
    fn molar_enthalpy_matches_the_oracle() {
        close(mass_h("N2", 400.0), 106_187.930_794_706_12);
        close(mass_h("N2", 2000.0), 2_003_721.968_609_804_5);
        close(mass_h("CO2", 298.15), -8_941_529.179_557_79);
        close(mass_h("H2O", 298.15), -13_423_514.938_712_63);
        close(mass_h("CH4", 298.15), -4_649_976.592_507_562);
        close(mass_h("OH", 2500.0), 6_518_824.350_024_584);
        close(mass_h("AR", 1000.0), 365_193.851_284_375_94);
    }

    /// The module doc's headline: the formation reference is baked into the
    /// coefficients, so h(CO2) at 298.15 K is −393.5 kJ/mol.
    #[test]
    fn enthalpy_is_formation_referenced() {
        let h = molar_enthalpy("CO2", 298.15).unwrap();
        assert!(
            (h - -393_500.0).abs() < 200.0,
            "h(CO2, 298.15) = {h} J/mol, expected about -393500"
        );
        let h2 = molar_enthalpy("H2O", 298.15).unwrap();
        assert!((h2 - -241_800.0).abs() < 200.0, "h(H2O, 298.15) = {h2}");
        // Elements in their reference state sit at ~0.
        assert!(molar_enthalpy("N2", 298.15).unwrap().abs() < 100.0);
        assert!(molar_enthalpy("O2", 298.15).unwrap().abs() < 100.0);
    }

    #[test]
    fn molar_entropy_matches_the_oracle() {
        close(mass_s("N2", 400.0, 101_325.0), 7142.478343540481);
        close(mass_s("N2", 400.0, 1_013_250.0), 6459.078697632612);
        close(mass_s("CO2", 2000.0, 101_325.0), 7027.698041300949);
        close(mass_s("H", 3000.0, 101_325.0), 161_416.350_844_599_16);
    }

    /// Entropy falls by `R ln(p/p_ref)` per mole; a decade of pressure is
    /// `R ln 10`. This is the identity the equilibrium solver leans on.
    #[test]
    fn entropy_pressure_term_is_r_ln_p() {
        let a = molar_entropy("N2", 400.0, 101_325.0).unwrap();
        let b = molar_entropy("N2", 400.0, 1_013_250.0).unwrap();
        close(a - b, R * libm::log(10.0));
    }

    #[test]
    fn transport_parameters_are_tabulated_for_every_species() {
        for (name, s) in SPECIES {
            assert!(has_transport(name), "{name} has no LJ parameters");
            assert_eq!(collision_diameter(name).unwrap(), s.sigma);
            assert_eq!(well_depth(name).unwrap(), s.eps_k);
        }
        assert_eq!(collision_diameter("N2").unwrap(), 3.621);
        assert_eq!(well_depth("N2").unwrap(), 97.53);
        assert_eq!(collision_diameter("Argon").unwrap(), 3.33);
        assert_eq!(well_depth("H2O").unwrap(), 572.4);
    }
}
