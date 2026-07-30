//! Bulk physical properties of common engineering solids.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/SolidProperties.java`
//! (128 LOC), in full — every material and every property, transcribed value
//! for value. These back the classic-solver-style material functions `k_`,
//! `rho_`, `c_`, `E_` and `nu_`.
//!
//! Values are representative room-temperature figures from standard
//! references. Thermal conductivity and specific heat take a linear correction
//! about 300 K for the nine metals that carry a reliable slope; everything else
//! is a constant. Surface-finish-dependent quantities (emissivity) and
//! liquid-only quantities (viscosity, vapour pressure) are deliberately absent
//! — a single material-level value would mislead.
//!
//! # Data, not code
//!
//! The Java stores two `Map.ofEntries` tables. This port stores two `const`
//! slices held in the *sorted* key order the Java's
//! `DB.keySet().stream().sorted()` produces, because that order is
//! user-visible: it is the "Known materials:" list in the unknown-material
//! error. [`tests::table_is_sorted`] pins it.

use crate::diag::{FreesError, Result};

/// `k` [W/m-K], `rho` [kg/m³], `c` [J/kg-K], `e` [Pa], `nu` [-].
///
/// `None` marks a property the table deliberately does not provide (brick has
/// no elastic data; wood/oak have no Poisson's ratio).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub k: Option<f64>,
    pub rho: Option<f64>,
    pub c: Option<f64>,
    pub e: Option<f64>,
    pub nu: Option<f64>,
}

const fn m(k: f64, rho: f64, c: f64, e: Option<f64>, nu: Option<f64>) -> Material {
    Material {
        k: Some(k),
        rho: Some(rho),
        c: Some(c),
        e,
        nu,
    }
}

/// The whole material table, in the sorted key order the error message uses.
pub const MATERIALS: &[(&str, Material)] = &[
    ("aluminium", m(237.0, 2702.0, 903.0, Some(70e9), Some(0.33))),
    ("aluminum", m(237.0, 2702.0, 903.0, Some(70e9), Some(0.33))),
    ("brass", m(110.0, 8530.0, 380.0, Some(100e9), Some(0.34))),
    ("brick", m(0.72, 1920.0, 835.0, None, None)),
    ("bronze", m(54.0, 8800.0, 380.0, Some(110e9), Some(0.34))),
    (
        "carbonsteel",
        m(60.5, 7854.0, 434.0, Some(200e9), Some(0.29)),
    ),
    ("concrete", m(1.4, 2300.0, 880.0, Some(30e9), Some(0.20))),
    ("copper", m(401.0, 8933.0, 385.0, Some(110e9), Some(0.34))),
    ("glass", m(1.4, 2500.0, 750.0, Some(70e9), Some(0.22))),
    ("gold", m(317.0, 19300.0, 129.0, Some(78e9), Some(0.44))),
    ("ice", m(2.22, 920.0, 2040.0, Some(9e9), Some(0.33))),
    ("iron", m(80.2, 7870.0, 447.0, Some(211e9), Some(0.29))),
    ("lead", m(35.3, 11340.0, 129.0, Some(16e9), Some(0.44))),
    (
        "magnesium",
        m(156.0, 1740.0, 1024.0, Some(45e9), Some(0.29)),
    ),
    ("nickel", m(90.7, 8900.0, 444.0, Some(200e9), Some(0.31))),
    ("oak", m(0.17, 700.0, 2310.0, Some(11e9), None)),
    ("silver", m(429.0, 10500.0, 235.0, Some(83e9), Some(0.37))),
    (
        "stainlesssteel",
        m(15.1, 7900.0, 477.0, Some(193e9), Some(0.30)),
    ),
    ("steel", m(60.5, 7854.0, 434.0, Some(200e9), Some(0.29))),
    ("titanium", m(21.9, 4500.0, 522.0, Some(116e9), Some(0.32))),
    (
        "tungsten",
        m(174.0, 19300.0, 132.0, Some(411e9), Some(0.28)),
    ),
    ("wood", m(0.17, 700.0, 2310.0, Some(11e9), None)),
    ("zinc", m(116.0, 7140.0, 389.0, Some(108e9), Some(0.25))),
];

/// Reference temperature the linear slopes are taken about [K].
pub const T_REF: f64 = 300.0;

/// Linear temperature slopes about 300 K: `(dk/dT [W/m-K²], dc/dT [J/kg-K²])`.
///
/// Fits to standard tabulated data over roughly 250–600 K. Only the
/// well-characterised metals carry a slope; everything else is constant.
pub const SLOPES: &[(&str, (f64, f64))] = &[
    ("aluminium", (-0.02, 0.46)),
    ("aluminum", (-0.02, 0.46)),
    ("carbonsteel", (-0.04, 0.42)),
    ("copper", (-0.073, 0.107)),
    ("iron", (-0.085, 0.42)),
    ("nickel", (-0.10, 0.40)),
    ("steel", (-0.04, 0.42)),
    ("titanium", (-0.015, 0.29)),
    ("tungsten", (-0.15, 0.05)),
];

/// The material-function names this module serves (lower-cased, trailing `_`).
pub fn function_names() -> [&'static str; 5] {
    ["k_", "rho_", "c_", "e_", "nu_"]
}

/// The table entry for `name` (case-insensitive), or `None` if unknown.
pub fn material(name: &str) -> Option<Material> {
    let key = name.to_lowercase();
    MATERIALS
        .iter()
        .find(|(id, _)| *id == key)
        .map(|(_, mat)| *mat)
}

/// Property of a solid material at the 300 K reference — Java
/// `lookup(material, property)`.
pub fn lookup(material_name: &str, property: &str) -> Result<f64> {
    lookup_at(material_name, property, None)
}

/// Property of a solid material, optionally at temperature `temp_k` [K].
///
/// Thermal conductivity and specific heat receive a linear temperature
/// correction about 300 K where reliable slope data exists; the other
/// properties are treated as constants and ignore `temp_k` entirely.
///
/// `property` is the lower-cased function name with its trailing underscore
/// (`"k_"`, `"rho_"`, `"c_"`, `"e_"`, `"nu_"`) — matched exactly, as the Java
/// `switch` does, so any other spelling falls through to the
/// "not available" arm.
pub fn lookup_at(material_name: &str, property: &str, temp_k: Option<f64>) -> Result<f64> {
    let Some(mat) = material(material_name) else {
        return Err(FreesError::property(format!(
            "Unknown material '{material_name}'. Known materials: {}",
            known_materials()
        )));
    };
    let value = match property {
        "k_" => mat.k,
        "rho_" => mat.rho,
        "c_" => mat.c,
        "e_" => mat.e,
        "nu_" => mat.nu,
        _ => None,
    };
    let Some(value) = value else {
        return Err(FreesError::property(format!(
            "{} is not available for material '{material_name}'.",
            property_label(property)
        )));
    };
    Ok(apply_temperature(value, property, material_name, temp_k))
}

/// The comma-joined sorted key list the unknown-material error quotes.
pub fn known_materials() -> String {
    MATERIALS
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()
        .join(", ")
}

fn apply_temperature(value: f64, property: &str, material_name: &str, temp_k: Option<f64>) -> f64 {
    let Some(temp_k) = temp_k else {
        return value;
    };
    if property != "k_" && property != "c_" {
        return value;
    }
    let key = material_name.to_lowercase();
    let Some((_, (dkdt, dcdt))) = SLOPES.iter().find(|(id, _)| *id == key) else {
        return value;
    };
    let dvdt = if property == "k_" { *dkdt } else { *dcdt };
    value + dvdt * (temp_k - T_REF)
}

fn property_label(property: &str) -> &str {
    match property {
        "k_" => "Thermal conductivity",
        "rho_" => "Density",
        "c_" => "Specific heat",
        "e_" => "Young's modulus",
        "nu_" => "Poisson's ratio",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(material_name: &str, property: &str) -> f64 {
        lookup(material_name, property).unwrap()
    }

    fn at(material_name: &str, property: &str, t: f64) -> f64 {
        lookup_at(material_name, property, Some(t)).unwrap()
    }

    // Every expectation below is the Java oracle's value, dumped through
    // `tools/golden-dumper` with `k_() / rho_() / c_() / E_() / nu_()`.

    #[test]
    fn conductivity_table_matches_the_oracle() {
        for (name, want) in [
            ("Aluminum", 237.0),
            ("Aluminium", 237.0),
            ("Brass", 110.0),
            ("Brick", 0.72),
            ("Bronze", 54.0),
            ("CarbonSteel", 60.5),
            ("Concrete", 1.4),
            ("Copper", 401.0),
            ("Glass", 1.4),
            ("Gold", 317.0),
            ("Ice", 2.22),
            ("Iron", 80.2),
            ("Lead", 35.3),
            ("Magnesium", 156.0),
            ("Nickel", 90.7),
            ("Oak", 0.17),
            ("Silver", 429.0),
            ("StainlessSteel", 15.1),
            ("Steel", 60.5),
            ("Titanium", 21.9),
            ("Tungsten", 174.0),
            ("Wood", 0.17),
            ("Zinc", 116.0),
        ] {
            assert_eq!(v(name, "k_"), want, "k_({name})");
        }
    }

    #[test]
    fn density_table_matches_the_oracle() {
        for (name, want) in [
            ("Aluminum", 2702.0),
            ("Aluminium", 2702.0),
            ("Brass", 8530.0),
            ("Brick", 1920.0),
            ("Bronze", 8800.0),
            ("CarbonSteel", 7854.0),
            ("Concrete", 2300.0),
            ("Copper", 8933.0),
            ("Glass", 2500.0),
            ("Gold", 19300.0),
            ("Ice", 920.0),
            ("Iron", 7870.0),
            ("Lead", 11340.0),
            ("Magnesium", 1740.0),
            ("Nickel", 8900.0),
            ("Oak", 700.0),
            ("Silver", 10500.0),
            ("StainlessSteel", 7900.0),
            ("Steel", 7854.0),
            ("Titanium", 4500.0),
            ("Tungsten", 19300.0),
            ("Wood", 700.0),
            ("Zinc", 7140.0),
        ] {
            assert_eq!(v(name, "rho_"), want, "rho_({name})");
        }
    }

    #[test]
    fn specific_heat_table_matches_the_oracle() {
        for (name, want) in [
            ("Aluminum", 903.0),
            ("Aluminium", 903.0),
            ("Brass", 380.0),
            ("Brick", 835.0),
            ("Bronze", 380.0),
            ("CarbonSteel", 434.0),
            ("Concrete", 880.0),
            ("Copper", 385.0),
            ("Glass", 750.0),
            ("Gold", 129.0),
            ("Ice", 2040.0),
            ("Iron", 447.0),
            ("Lead", 129.0),
            ("Magnesium", 1024.0),
            ("Nickel", 444.0),
            ("Oak", 2310.0),
            ("Silver", 235.0),
            ("StainlessSteel", 477.0),
            ("Steel", 434.0),
            ("Titanium", 522.0),
            ("Tungsten", 132.0),
            ("Wood", 2310.0),
            ("Zinc", 389.0),
        ] {
            assert_eq!(v(name, "c_"), want, "c_({name})");
        }
    }

    #[test]
    fn youngs_modulus_table_matches_the_oracle() {
        for (name, want) in [
            ("Aluminum", 70e9),
            ("Aluminium", 70e9),
            ("Brass", 100e9),
            ("Bronze", 110e9),
            ("CarbonSteel", 200e9),
            ("Concrete", 30e9),
            ("Copper", 110e9),
            ("Glass", 70e9),
            ("Gold", 78e9),
            ("Ice", 9e9),
            ("Iron", 211e9),
            ("Lead", 16e9),
            ("Magnesium", 45e9),
            ("Nickel", 200e9),
            ("Oak", 11e9),
            ("Silver", 83e9),
            ("StainlessSteel", 193e9),
            ("Steel", 200e9),
            ("Titanium", 116e9),
            ("Tungsten", 411e9),
            ("Wood", 11e9),
            ("Zinc", 108e9),
        ] {
            assert_eq!(v(name, "e_"), want, "E_({name})");
        }
    }

    #[test]
    fn poissons_ratio_table_matches_the_oracle() {
        for (name, want) in [
            ("Aluminum", 0.33),
            ("Aluminium", 0.33),
            ("Brass", 0.34),
            ("Bronze", 0.34),
            ("CarbonSteel", 0.29),
            ("Concrete", 0.2),
            ("Copper", 0.34),
            ("Glass", 0.22),
            ("Gold", 0.44),
            ("Ice", 0.33),
            ("Iron", 0.29),
            ("Lead", 0.44),
            ("Magnesium", 0.29),
            ("Nickel", 0.31),
            ("Silver", 0.37),
            ("StainlessSteel", 0.3),
            ("Steel", 0.29),
            ("Titanium", 0.32),
            ("Tungsten", 0.28),
            ("Zinc", 0.25),
        ] {
            assert_eq!(v(name, "nu_"), want, "nu_({name})");
        }
    }

    #[test]
    fn material_lookup_is_case_insensitive() {
        assert_eq!(v("aluminum", "k_"), 237.0);
        assert_eq!(v("ALUMINUM", "k_"), 237.0);
        assert_eq!(v("AlUmInUm", "k_"), 237.0);
    }

    #[test]
    fn temperature_slopes_match_the_oracle() {
        assert_eq!(at("Aluminum", "k_", 400.0), 235.0);
        assert_eq!(at("Aluminum", "c_", 400.0), 949.0);
        assert_eq!(at("Aluminium", "k_", 400.0), 235.0);
        assert_eq!(at("Aluminium", "c_", 400.0), 949.0);
        assert_eq!(at("Copper", "k_", 500.0), 386.4);
        assert_eq!(at("Copper", "c_", 500.0), 406.4);
        assert_eq!(at("Steel", "k_", 600.0), 48.5);
        assert_eq!(at("Steel", "c_", 600.0), 560.0);
        assert_eq!(at("CarbonSteel", "k_", 250.0), 62.5);
        assert_eq!(at("CarbonSteel", "c_", 250.0), 413.0);
        assert_eq!(at("Iron", "k_", 400.0), 71.7);
        assert_eq!(at("Iron", "c_", 400.0), 489.0);
        assert_eq!(at("Nickel", "k_", 400.0), 80.7);
        assert_eq!(at("Nickel", "c_", 400.0), 484.0);
        assert_eq!(at("Titanium", "k_", 400.0), 20.4);
        assert_eq!(at("Titanium", "c_", 400.0), 551.0);
        assert_eq!(at("Tungsten", "k_", 400.0), 159.0);
        assert_eq!(at("Tungsten", "c_", 400.0), 137.0);
        // The reference temperature is a no-op.
        assert_eq!(at("Aluminum", "k_", 300.0), 237.0);
    }

    #[test]
    fn materials_without_a_slope_stay_constant() {
        assert_eq!(at("StainlessSteel", "k_", 400.0), 15.1);
        assert_eq!(at("StainlessSteel", "c_", 400.0), 477.0);
        assert_eq!(at("Glass", "k_", 400.0), 1.4);
    }

    #[test]
    fn only_conductivity_and_specific_heat_see_the_temperature() {
        assert_eq!(at("Aluminum", "rho_", 400.0), 2702.0);
        assert_eq!(at("Aluminum", "e_", 400.0), 70e9);
        assert_eq!(at("Aluminum", "nu_", 400.0), 0.33);
    }

    #[test]
    fn unknown_material_lists_the_sorted_table() {
        let err = lookup("Unobtainium", "k_").unwrap_err();
        assert_eq!(
            err,
            FreesError::property(
                "Unknown material 'Unobtainium'. Known materials: aluminium, aluminum, brass, \
                 brick, bronze, carbonsteel, concrete, copper, glass, gold, ice, iron, lead, \
                 magnesium, nickel, oak, silver, stainlesssteel, steel, titanium, tungsten, \
                 wood, zinc"
            )
        );
    }

    #[test]
    fn absent_properties_name_themselves() {
        assert_eq!(
            lookup("Brick", "e_").unwrap_err(),
            FreesError::property("Young's modulus is not available for material 'Brick'.")
        );
        assert_eq!(
            lookup("Brick", "nu_").unwrap_err(),
            FreesError::property("Poisson's ratio is not available for material 'Brick'.")
        );
        assert_eq!(
            lookup("Wood", "nu_").unwrap_err(),
            FreesError::property("Poisson's ratio is not available for material 'Wood'.")
        );
        assert_eq!(
            lookup("Oak", "nu_").unwrap_err(),
            FreesError::property("Poisson's ratio is not available for material 'Oak'.")
        );
    }

    #[test]
    fn an_unknown_property_falls_through_to_its_own_name() {
        // The Java `switch (property)` has no default value, so an unmatched
        // key yields null and `propertyLabel` echoes the key back.
        assert_eq!(
            lookup("Steel", "emissivity_").unwrap_err(),
            FreesError::property("emissivity_ is not available for material 'Steel'.")
        );
        // The switch is exact — the label is not case-folded.
        assert_eq!(
            lookup("Steel", "K_").unwrap_err(),
            FreesError::property("K_ is not available for material 'Steel'.")
        );
    }

    #[test]
    fn table_is_sorted() {
        // The order is user-visible in the unknown-material error.
        let keys: Vec<&str> = MATERIALS.iter().map(|(id, _)| *id).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        assert_eq!(keys.len(), 23);
        let slope_keys: Vec<&str> = SLOPES.iter().map(|(id, _)| *id).collect();
        let mut slopes_sorted = slope_keys.clone();
        slopes_sorted.sort_unstable();
        assert_eq!(slope_keys, slopes_sorted);
        assert_eq!(slope_keys.len(), 9);
        // Every slope key must name a real material.
        for key in slope_keys {
            assert!(material(key).is_some(), "{key}");
        }
    }

    #[test]
    fn aliases_carry_identical_data() {
        assert_eq!(material("aluminum"), material("aluminium"));
        assert_eq!(material("steel"), material("carbonsteel"));
        assert_eq!(material("wood"), material("oak"));
    }

    #[test]
    fn function_names_are_the_five_material_functions() {
        assert_eq!(function_names(), ["k_", "rho_", "c_", "e_", "nu_"]);
    }
}
