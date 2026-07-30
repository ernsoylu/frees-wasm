//! One-term transient conduction — the computational Heisler/Gröber charts.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/core/HeislerCharts.java`
//! (114 LOC), in full. For a plane wall (half-thickness `L`), infinite cylinder
//! or sphere (radius `r0`) suddenly exposed to convection,
//!
//! ```text
//! theta* = (T - T_inf)/(T_i - T_inf) = C1 exp(-zeta1^2 Fo) * f(zeta1 x*)
//! ```
//!
//! with `Bi = h·Lc/k`, `Fo = alpha·t/Lc²` and `x*` the dimensionless position
//! (0 centre, 1 surface). The approximation is accurate for `Fo >= 0.2`.
//!
//! # The Java does not tabulate — it solves
//!
//! Textbooks print `zeta1(Bi)` and `C1(Bi)` as a table and interpolate. The
//! Java does **not**: it bisects the transcendental eigenvalue equation on
//! `(0, first-asymptote)` for 200 halvings (or until the bracket is under
//! `1e-12`) and evaluates `C1` in closed form from the root. This port mirrors
//! that solve, iteration count and bracket included — an interpolated table
//! would not reproduce the oracle.
//!
//! # Bessel functions
//!
//! The Java calls Apache Commons Math `BesselJ.value(0|1, ·)`. The eigenvalue
//! problem confines the argument to `zeta·x* <= 2.405` (the first zero of J₀),
//! where the ascending power series converges in ~15 terms and is exact to the
//! last bit — so the series replaces the Commons Math `rjbesl` port without
//! any loss. Verified against the oracle over all three geometries below.

use crate::diag::{FreesError, Result};

/// The three one-dimensional geometries the one-term solution covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    Wall,
    Cylinder,
    Sphere,
}

/// Resolves a user-supplied geometry spelling (`HeislerCharts.parse`).
///
/// Fails with the Java `SolverException` text — note it quotes the *original*
/// spelling, not the lower-cased key.
pub fn parse(geometry: &str) -> Result<Geometry> {
    match geometry.to_lowercase().as_str() {
        "wall" | "planewall" | "plane" | "slab" => Ok(Geometry::Wall),
        "cylinder" | "cyl" => Ok(Geometry::Cylinder),
        "sphere" | "ball" => Ok(Geometry::Sphere),
        _ => Err(FreesError::solver(format!(
            "Heisler geometry must be 'wall', 'cylinder' or 'sphere', got '{geometry}'."
        ))),
    }
}

/// Dimensionless temperature `theta*` at position `x_star` (0 centre, 1 surface).
pub fn temperature(geometry: &str, bi: f64, fo: f64, x_star: f64) -> Result<f64> {
    Ok(temperature_of(parse(geometry)?, bi, fo, x_star))
}

/// Fraction of the maximum possible heat transfer, `Q/Q0`, at Fourier `fo`.
pub fn heat_ratio(geometry: &str, bi: f64, fo: f64) -> Result<f64> {
    Ok(heat_ratio_of(parse(geometry)?, bi, fo))
}

/// [`temperature`] on an already-resolved [`Geometry`].
pub fn temperature_of(g: Geometry, bi: f64, fo: f64, x_star: f64) -> f64 {
    let zeta = first_eigenvalue(g, bi);
    let theta_centre = coefficient(g, zeta) * libm::exp(-zeta * zeta * fo);
    theta_centre * spatial(g, zeta, x_star)
}

/// [`heat_ratio`] on an already-resolved [`Geometry`].
pub fn heat_ratio_of(g: Geometry, bi: f64, fo: f64) -> f64 {
    let zeta = first_eigenvalue(g, bi);
    let theta_centre = coefficient(g, zeta) * libm::exp(-zeta * zeta * fo);
    match g {
        Geometry::Wall => 1.0 - theta_centre * libm::sin(zeta) / zeta,
        Geometry::Cylinder => 1.0 - 2.0 * theta_centre * bessel_j1(zeta) / zeta,
        Geometry::Sphere => {
            1.0 - 3.0 * theta_centre * (libm::sin(zeta) - zeta * libm::cos(zeta))
                / (zeta * zeta * zeta)
        }
    }
}

/// Eigenvalue-equation residual `f(zeta) - Bi` for the first root.
pub fn residual(g: Geometry, zeta: f64, bi: f64) -> f64 {
    match g {
        Geometry::Wall => zeta * libm::tan(zeta) - bi,
        Geometry::Cylinder => zeta * bessel_j1(zeta) / bessel_j0(zeta) - bi,
        Geometry::Sphere => 1.0 - zeta / libm::tan(zeta) - bi,
    }
}

/// Upper bound of the first-eigenvalue interval (the first asymptote).
pub fn upper_bound(g: Geometry) -> f64 {
    match g {
        Geometry::Wall => core::f64::consts::PI / 2.0,
        // Java literal 2.4048255576957727 — the same f64, spelled at the
        // shortest round-tripping length so `clippy::excessive_precision`
        // stays quiet.
        Geometry::Cylinder => 2.404_825_557_695_773, // first zero of J0
        Geometry::Sphere => core::f64::consts::PI,
    }
}

/// First eigenvalue `zeta1(Bi)`, by 200 bisections of [`residual`].
///
/// The residual increases monotonically from `-Bi` (at 0⁺) to `+inf` at the
/// asymptote, so plain sign bisection is safe and needs no bracketing search.
pub fn first_eigenvalue(g: Geometry, bi: f64) -> f64 {
    let mut lo = 1e-9;
    let mut hi = upper_bound(g) - 1e-9;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let f = residual(g, mid, bi);
        if f > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
        if hi - lo < 1e-12 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// One-term coefficient `C1(zeta1)`.
pub fn coefficient(g: Geometry, zeta: f64) -> f64 {
    match g {
        Geometry::Wall => 4.0 * libm::sin(zeta) / (2.0 * zeta + libm::sin(2.0 * zeta)),
        Geometry::Cylinder => {
            let j0 = bessel_j0(zeta);
            let j1 = bessel_j1(zeta);
            (2.0 / zeta) * j1 / (j0 * j0 + j1 * j1)
        }
        Geometry::Sphere => {
            4.0 * (libm::sin(zeta) - zeta * libm::cos(zeta)) / (2.0 * zeta - libm::sin(2.0 * zeta))
        }
    }
}

/// Spatial shape function `f(zeta1 x*)`.
pub fn spatial(g: Geometry, zeta: f64, x_star: f64) -> f64 {
    match g {
        Geometry::Wall => libm::cos(zeta * x_star),
        Geometry::Cylinder => bessel_j0(zeta * x_star),
        Geometry::Sphere => {
            if x_star == 0.0 {
                1.0
            } else {
                libm::sin(zeta * x_star) / (zeta * x_star)
            }
        }
    }
}

/// `J₀(x)` by the ascending power series — the Commons Math `BesselJ.value(0,·)`
/// stand-in, exact over the `|x| <= 2.405` this module ever asks for.
pub fn bessel_j0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..60_i32 {
        term *= -q / f64::from(k * k);
        sum += term;
        if libm::fabs(term) < 1e-18 * libm::fabs(sum) {
            break;
        }
    }
    sum
}

/// `J₁(x)` by the ascending power series (see [`bessel_j0`]).
pub fn bessel_j1(x: f64) -> f64 {
    let q = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..60_i32 {
        term *= -q / f64::from(k * (k + 1));
        sum += term;
        if libm::fabs(term) < 1e-18 * libm::fabs(sum) {
            break;
        }
    }
    sum * x / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Java oracle is only reproducible to within the Bessel
    /// implementation's own last bits; 1e-14 relative is two orders tighter
    /// than the fixture tolerance and catches any structural divergence.
    fn close(actual: f64, expected: f64) {
        let tol = 1e-14 * libm::fabs(expected).max(1.0);
        assert!(
            libm::fabs(actual - expected) <= tol,
            "expected {expected:.17e}, got {actual:.17e} (delta {:.3e})",
            actual - expected
        );
    }

    // ---- theta*, plane wall (oracle: heisler_temp) -------------------------

    #[test]
    fn wall_temperature_matches_the_oracle() {
        close(
            temperature("wall", 3.3333333333333335, 0.22499999999999998, 0.0).unwrap(),
            0.8710062253809431,
        );
        close(
            temperature("wall", 3.3333333333333335, 0.22499999999999998, 1.0).unwrap(),
            0.29935683484980896,
        );
        close(
            temperature("planewall", 0.1, 1.0, 0.5).unwrap(),
            0.9112562796801947,
        );
        close(
            temperature("plane", 100.0, 0.4, 0.25).unwrap(),
            0.4476955401108924,
        );
        close(
            temperature("slab", 0.001, 2.0, 1.0).unwrap(),
            0.9976700628048353,
        );
        close(
            temperature("wall", 1000000.0, 0.3, 0.0).unwrap(),
            0.6073473722828968,
        );
        close(
            temperature("WALL", 1.0, 0.5, 0.5).unwrap(),
            0.7025364965121742,
        );
    }

    #[test]
    fn wall_heat_ratio_matches_the_oracle() {
        close(
            heat_ratio("wall", 3.3333333333333335, 0.22499999999999998).unwrap(),
            0.32952524994160104,
        );
        close(heat_ratio("wall", 0.1, 1.0).unwrap(), 0.09241292886418151);
        close(heat_ratio("slab", 100.0, 0.4).unwrap(), 0.6889569386808169);
        close(
            heat_ratio("wall", 0.001, 2.0).unwrap(),
            0.0019973583424026664,
        );
    }

    // ---- theta*, infinite cylinder (Bessel path) ---------------------------

    #[test]
    fn cylinder_temperature_matches_the_oracle() {
        close(
            temperature("cylinder", 1.0, 0.5, 0.5).unwrap(),
            0.4958980458136078,
        );
        close(
            temperature("cyl", 3.3333333333333335, 0.225, 0.0).unwrap(),
            0.6748766363151112,
        );
        close(
            temperature("cylinder", 0.1, 1.0, 1.0).unwrap(),
            0.802374985231977,
        );
        close(
            temperature("cylinder", 100.0, 0.4, 0.75).unwrap(),
            0.057776540213201964,
        );
        close(
            temperature("cylinder", 0.001, 2.0, 0.0).unwrap(),
            0.9962579459280481,
        );
    }

    #[test]
    fn cylinder_heat_ratio_matches_the_oracle() {
        close(
            heat_ratio("cylinder", 1.0, 0.5).unwrap(),
            0.5526190515382893,
        );
        close(
            heat_ratio("cyl", 3.3333333333333335, 0.225).unwrap(),
            0.5717991927096323,
        );
        close(
            heat_ratio("cylinder", 0.1, 1.0).unwrap(),
            0.17740057427878408,
        );
        close(
            heat_ratio("cylinder", 100.0, 0.4).unwrap(),
            0.9269570732009609,
        );
    }

    // ---- theta*, sphere ----------------------------------------------------

    #[test]
    fn sphere_temperature_matches_the_oracle() {
        close(
            temperature("sphere", 1.0, 0.5, 0.5).unwrap(),
            0.3338227251694543,
        );
        close(
            temperature("ball", 3.3333333333333335, 0.225, 0.0).unwrap(),
            0.47788432295422245,
        );
        close(
            temperature("sphere", 0.1, 1.0, 1.0).unwrap(),
            0.7303676791844198,
        );
        close(
            temperature("sphere", 100.0, 0.4, 0.75).unwrap(),
            0.012942902302301433,
        );
        close(
            temperature("sphere", 0.001, 2.0, 0.0).unwrap(),
            0.9943173433069892,
        );
    }

    #[test]
    fn sphere_heat_ratio_matches_the_oracle() {
        close(heat_ratio("sphere", 1.0, 0.5).unwrap(), 0.7129996667480328);
        close(
            heat_ratio("ball", 3.3333333333333335, 0.225).unwrap(),
            0.7393012408378744,
        );
        close(heat_ratio("sphere", 0.1, 1.0).unwrap(), 0.2549006097996286);
        close(
            heat_ratio("sphere", 100.0, 0.4).unwrap(),
            0.9869352516879994,
        );
    }

    // ---- structure ---------------------------------------------------------

    #[test]
    fn unknown_geometry_quotes_the_original_spelling() {
        let err = temperature("cone", 1.0, 0.5, 0.5).unwrap_err();
        assert_eq!(
            err,
            FreesError::solver(
                "Heisler geometry must be 'wall', 'cylinder' or 'sphere', got 'cone'."
            )
        );
        // The message must not lower-case the user's text.
        let err = heat_ratio("Cone", 1.0, 0.5).unwrap_err();
        assert!(format!("{err}").contains("got 'Cone'."));
    }

    #[test]
    fn every_alias_resolves() {
        for (spelling, want) in [
            ("wall", Geometry::Wall),
            ("planewall", Geometry::Wall),
            ("plane", Geometry::Wall),
            ("slab", Geometry::Wall),
            ("SLAB", Geometry::Wall),
            ("cylinder", Geometry::Cylinder),
            ("cyl", Geometry::Cylinder),
            ("Cyl", Geometry::Cylinder),
            ("sphere", Geometry::Sphere),
            ("ball", Geometry::Sphere),
            ("BALL", Geometry::Sphere),
        ] {
            assert_eq!(parse(spelling).unwrap(), want, "{spelling}");
        }
        // The Java lower-cases but does not strip punctuation (unlike the HX
        // arrangement resolver) — 'plane wall' is NOT an alias.
        assert!(parse("plane wall").is_err());
        assert!(parse("plane-wall").is_err());
    }

    #[test]
    fn bessel_series_matches_known_values() {
        // J0/J1 reference values (Abramowitz & Stegun 9.1), well inside the
        // range the eigenvalue solve exercises.
        close(bessel_j0(0.0), 1.0);
        close(bessel_j1(0.0), 0.0);
        close(bessel_j0(1.0), 0.7651976865579666);
        close(bessel_j1(1.0), 0.4400505857449335);
        close(bessel_j0(2.0), 0.22389077914123567);
        close(bessel_j1(2.0), 0.5767248077568734);
        // J0 vanishes at its first zero, which is exactly `upper_bound`.
        assert!(libm::fabs(bessel_j0(upper_bound(Geometry::Cylinder))) < 1e-15);
    }

    #[test]
    fn bessel_series_agrees_with_libm_over_the_eigenvalue_range() {
        // libm's j0/j1 are an independent implementation (fdlibm); agreement
        // to 1e-15 relative is the evidence that dropping Commons Math's
        // rjbesl for the series costs nothing.
        let mut x = 0.0;
        while x <= 2.5 {
            let (s0, s1) = (bessel_j0(x), bessel_j1(x));
            let (l0, l1) = (libm::j0(x), libm::j1(x));
            assert!(
                libm::fabs(s0 - l0) <= 1e-15 * libm::fabs(l0).max(1.0),
                "J0({x}): series {s0:e} vs libm {l0:e}"
            );
            assert!(
                libm::fabs(s1 - l1) <= 1e-15 * libm::fabs(l1).max(1.0),
                "J1({x}): series {s1:e} vs libm {l1:e}"
            );
            x += 0.01;
        }
    }

    #[test]
    fn eigenvalues_bracket_the_first_asymptote() {
        for g in [Geometry::Wall, Geometry::Cylinder, Geometry::Sphere] {
            for bi in [0.001, 0.1, 1.0, 10.0, 1e6] {
                let zeta = first_eigenvalue(g, bi);
                assert!(zeta > 0.0 && zeta < upper_bound(g), "{g:?} Bi={bi}");
            }
            // At moderate Bi the 1e-12 bracket lands on a true root. It cannot
            // at huge Bi: there the residual's slope is ~1/(asymptote − zeta)²,
            // so a 1e-12 bracket is worth ~1 in residual — a property of the
            // Java's bisection, not of this port.
            for bi in [0.001, 0.1, 1.0, 10.0] {
                let zeta = first_eigenvalue(g, bi);
                assert!(libm::fabs(residual(g, zeta, bi)) < 1e-7, "{g:?} Bi={bi}");
            }
            // Bi -> infinity drives zeta onto the asymptote (isothermal surface).
            assert!(first_eigenvalue(g, 1e12) > upper_bound(g) - 1e-6, "{g:?}");
        }
    }

    #[test]
    fn centre_is_the_one_term_coefficient() {
        // theta*(x*=0) = C1 exp(-zeta^2 Fo) for all three geometries.
        for g in [Geometry::Wall, Geometry::Cylinder, Geometry::Sphere] {
            let zeta = first_eigenvalue(g, 2.0);
            let want = coefficient(g, zeta) * libm::exp(-zeta * zeta * 0.7);
            close(temperature_of(g, 2.0, 0.7, 0.0), want);
        }
    }
}
