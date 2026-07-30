//! ISA / U.S. Standard Atmosphere 1976 — T, P and ρ vs geopotential altitude.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/Atmosphere.java`
//! (45 LOC), in full. Two layers are modelled: the troposphere (0–11 km, lapse
//! rate 6.5 K/km) and the isothermal lower stratosphere (11–20 km, 216.65 K).
//! All SI — altitude m, `T` in K, `P` in Pa, `ρ` in kg/m³.
//!
//! Java holds the tropopause pressure in a `static final` initialised from
//! `Math.pow`; Rust cannot fold a `pow` into a `const`, so [`pressure`]
//! recomputes it from the identical expression on the stratospheric branch.
//! That is the same double, not an approximation — and `T_TROPO` is likewise
//! *computed* (`288.15 − 0.0065·11000` is `216.649_999_999_999_98`, not
//! `216.65`), which the oracle confirms.

/// Sea-level temperature [K].
pub const T0: f64 = 288.15;
/// Sea-level pressure [Pa].
pub const P0: f64 = 101_325.0;
/// Tropospheric lapse rate [K/m].
pub const LAPSE: f64 = 0.0065;
/// Specific gas constant of air [J/kg-K].
pub const R_AIR: f64 = 287.058;
/// Standard gravity [m/s²].
pub const G0: f64 = 9.80665;
/// Tropopause altitude [m].
pub const H_TROPO: f64 = 11_000.0;

/// Tropopause temperature [K] — `T0 − LAPSE·H_TROPO`, computed not rounded.
pub fn t_tropo() -> f64 {
    T0 - LAPSE * H_TROPO
}

/// Tropopause pressure [Pa] — the Java `P_TROPO` static initialiser.
pub fn p_tropo() -> f64 {
    P0 * libm::pow(t_tropo() / T0, G0 / (R_AIR * LAPSE))
}

/// ISA temperature [K] at geopotential altitude `alt` [m].
pub fn temperature(alt: f64) -> f64 {
    if alt <= H_TROPO {
        T0 - LAPSE * alt
    } else {
        t_tropo() // isothermal lower stratosphere
    }
}

/// ISA pressure [Pa] at geopotential altitude `alt` [m].
pub fn pressure(alt: f64) -> f64 {
    if alt <= H_TROPO {
        let t = T0 - LAPSE * alt;
        P0 * libm::pow(t / T0, G0 / (R_AIR * LAPSE))
    } else {
        p_tropo() * libm::exp(-G0 * (alt - H_TROPO) / (R_AIR * t_tropo()))
    }
}

/// ISA density [kg/m³] from the ideal-gas law at the layer `T` and `P`.
pub fn density(alt: f64) -> f64 {
    pressure(alt) / (R_AIR * temperature(alt))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values are the Java oracle's, dumped through
    /// `tools/golden-dumper` with `isa_t` / `isa_p` / `isa_rho`.
    fn eq(actual: f64, expected: f64) {
        assert_eq!(
            actual, expected,
            "expected the oracle's bits exactly, got {actual:e}"
        );
    }

    #[test]
    fn temperature_matches_the_oracle() {
        eq(temperature(0.0), 288.15);
        eq(temperature(1000.0), 281.65);
        eq(temperature(5000.0), 255.64999999999998);
        eq(temperature(11000.0), 216.64999999999998);
        eq(temperature(11000.0001), 216.64999999999998);
        eq(temperature(15000.0), 216.64999999999998);
        eq(temperature(20000.0), 216.64999999999998);
        eq(temperature(-500.0), 291.4);
    }

    #[test]
    fn pressure_matches_the_oracle() {
        eq(pressure(0.0), 101325.0);
        eq(pressure(1000.0), 89874.75552236482);
        eq(pressure(5000.0), 54020.49540145998);
        eq(pressure(11000.0), 22632.646369333983);
        eq(pressure(11000.0001), 22632.646012449506);
        eq(pressure(15000.0), 12045.011233214942);
        eq(pressure(20000.0), 5475.162948547324);
        eq(pressure(-500.0), 107477.3979377559);
    }

    #[test]
    fn density_matches_the_oracle() {
        eq(density(0.0), 1.2249781262066513);
        eq(density(1000.0), 1.1116250164638741);
        eq(density(5000.0), 0.7361106665094329);
        eq(density(11000.0), 0.36392089311454473);
        eq(density(15000.0), 0.19367736207400027);
        eq(density(20000.0), 0.08803770260303152);
        eq(density(-500.0), 1.2848663086923806);
    }

    #[test]
    fn tropopause_is_computed_not_rounded() {
        // The Java writes `T0 - LAPSE * H_TROPO`; the product is not 71.5 in
        // binary, so the constant is 216.649_999_999_999_98 and every
        // stratospheric value inherits that.
        assert_eq!(t_tropo(), 216.64999999999998);
        assert_ne!(t_tropo(), 216.65);
        assert_eq!(p_tropo(), pressure(H_TROPO));
    }

    #[test]
    fn the_tropopause_belongs_to_the_troposphere_branch() {
        // `alt <= H_TROPO` — the boundary uses the power law, and the
        // exponential branch starts an ulp above it.
        assert!(pressure(11000.0) > pressure(11000.0001));
    }
}
