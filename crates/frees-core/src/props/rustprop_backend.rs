//! [`RustpropBackend`] — the accuracy path of decision D8
//! (`docs/decisions/0008-coolprop-wasm.md`), served by **rustprop**, the
//! pure-Rust CoolProp 8 port, as a direct cargo dependency rather than an
//! emscripten `coolprop.wasm` blob.
//!
//! The four [`RealFluid`] calls forward 1:1 to `rustprop::props_si` /
//! `rustprop::ha_props_si`; every rustprop error becomes a
//! [`FreesError::Property`] carrying the rustprop message, which the engine
//! classifies exactly as it classifies the Java's
//! `PropertyEvaluationException`.
//!
//! Fluids go to rustprop **under their plain names, never an `IF97::`
//! prefix**: the frees parity oracle is the native CoolProp default backend,
//! i.e. IAPWS-95 HEOS for water, and IF97 diverges from it at ~1e-6 — two
//! orders looser than the tightest fixture tolerances.

use crate::diag::{FreesError, Result};
use crate::props::propfun::RealFluid;

/// A [`RealFluid`] answered by the rustprop engines linked into this build:
/// HEOS (Water, R134a, R1234yf, Air per the `rustprop-data` features in
/// `Cargo.toml`), the incompressible backend (the MEG/MPG glycols), and
/// humid air.
pub struct RustpropBackend;

/// The property-error mapping: message = the rustprop error's message
/// (rustprop's `Display` is upstream `what()`, message-only).
fn property_err(e: rustprop::Error) -> FreesError {
    FreesError::property(e.message())
}

impl RealFluid for RustpropBackend {
    fn props_si(
        &self,
        output: &str,
        name1: &str,
        value1: f64,
        name2: &str,
        value2: f64,
        fluid: &str,
    ) -> Result<f64> {
        rustprop::props_si(output, name1, value1, name2, value2, fluid).map_err(property_err)
    }

    fn props1_si(&self, fluid: &str, param: &str) -> Result<f64> {
        // rustprop's trivial route: a trivial output never needs a state
        // update, so the empty input names are deliberately never parsed —
        // this is exactly upstream's `Props1SI` shape.
        rustprop::props_si(param, "", 0.0, "", 0.0, fluid).map_err(property_err)
    }

    fn ha_props_si(
        &self,
        output: &str,
        name1: &str,
        value1: f64,
        name2: &str,
        value2: f64,
        name3: &str,
        value3: f64,
    ) -> Result<f64> {
        rustprop::ha_props_si(output, name1, value1, name2, value2, name3, value3)
            .map_err(property_err)
    }

    /// The fluids this build's `rustprop-data` features can serve **full
    /// states** for, spelled as [`crate::props::propfun::resolve_fluid`]
    /// produces them. The glycols are listed by family: `resolve_fluid`
    /// appends the mass fraction (`INCOMP::MEG[0.50]`), so the family is the
    /// serveable identity — the same spelling [`super::propfun::TableBackend`]
    /// keys its incompressible aux grids by.
    ///
    /// `Air` is deliberately absent: rustprop serves the pseudo-pure Air only
    /// at (P,T)/(Q,T)/(P,Q) until the remaining pseudo-pure flash pairs are
    /// ported, and this list feeds the property-diagram picker, which needs
    /// full states (the trait doc's rule). Air transport and `Z` at (T,P)
    /// still answer through [`Self::props_si`] when asked.
    fn served_fluids(&self) -> Option<Vec<String>> {
        Some(
            ["Water", "R134a", "R1234yf", "INCOMP::MEG", "INCOMP::MPG"]
                .map(String::from)
                .to_vec(),
        )
    }

    fn describe(&self) -> String {
        format!("rustprop (CoolProp {})", rustprop::UPSTREAM_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: RustpropBackend = RustpropBackend;

    /// The repo's own documented CoolProp ground truth (CLAUDE.md):
    /// `h = Enthalpy(Water, T=300 [K], P=101325 [Pa])` -> 112654.89965464505.
    #[test]
    fn water_enthalpy_matches_documented_ground_truth() {
        let h = B
            .props_si("Hmass", "T", 300.0, "P", 101325.0, "Water")
            .unwrap();
        let expected = 112654.89965464505;
        assert!(
            ((h - expected) / expected).abs() < 1e-9,
            "Hmass(Water, 300 K, 101325 Pa) = {h}, want {expected}"
        );
    }

    #[test]
    fn t_of_p_h_roundtrips() {
        for fluid in ["Water", "R134a", "R1234yf"] {
            let (t0, p) = (300.0, 101325.0);
            let h = B.props_si("Hmass", "T", t0, "P", p, fluid).unwrap();
            let t = B.props_si("T", "P", p, "Hmass", h, fluid).unwrap();
            assert!(
                (t - t0).abs() < 1e-6,
                "{fluid}: T(P, Hmass(T0, P)) = {t}, want {t0}"
            );
        }
    }

    #[test]
    fn saturated_water_viscosity_is_finite_and_positive() {
        let mu = B
            .props_si("viscosity", "P", 101325.0, "Q", 0.0, "Water")
            .unwrap();
        assert!(mu.is_finite() && mu > 0.0, "viscosity = {mu}");
    }

    /// Air is not in [`RealFluid::served_fluids`] (no full-state service), but
    /// transport and Z at (T,P) answer through `props_si` — and identically to
    /// rustprop asked directly, because the forward is 1:1.
    #[test]
    fn air_transport_and_z_match_rustprop_directly() {
        for output in ["viscosity", "Z"] {
            let via_backend = B
                .props_si(output, "T", 300.0, "P", 101325.0, "Air")
                .unwrap();
            let direct = rustprop::props_si(output, "T", 300.0, "P", 101325.0, "Air").unwrap();
            assert!(via_backend.is_finite() && via_backend > 0.0);
            assert_eq!(via_backend, direct, "{output}(Air, 300 K, 101325 Pa)");
        }
    }

    #[test]
    fn glycol_cp_and_viscosity_answer() {
        let fluid = "INCOMP::MEG[0.50]";
        let cp = B
            .props_si("Cpmass", "T", 300.0, "P", 101325.0, fluid)
            .unwrap();
        let mu = B
            .props_si("viscosity", "T", 300.0, "P", 101325.0, fluid)
            .unwrap();
        assert!(cp.is_finite() && cp > 0.0, "Cpmass = {cp}");
        assert!(mu.is_finite() && mu > 0.0, "viscosity = {mu}");
    }

    #[test]
    fn humid_air_enthalpy_answers() {
        let h = B
            .ha_props_si("H", "T", 298.15, "P", 101325.0, "R", 0.5)
            .unwrap();
        assert!(h.is_finite(), "HAPropsSI H = {h}");
    }

    #[test]
    fn props1_si_answers_water_tcrit() {
        let t = B.props1_si("Water", "Tcrit").unwrap();
        // rustprop reports the numerical critical point for superancillary
        // fluids, so compare loosely against IAPWS-95's 647.096 K.
        assert!((t - 647.096).abs() < 0.1, "Tcrit(Water) = {t}");
    }

    #[test]
    fn errors_classify_as_property_errors() {
        let unknown = B
            .props_si("Hmass", "T", 300.0, "P", 101325.0, "NotAFluid")
            .unwrap_err();
        assert!(matches!(unknown, FreesError::Property { .. }));

        let bad_pair = B
            .props_si("Hmass", "T", 300.0, "T", 300.0, "Water")
            .unwrap_err();
        assert!(matches!(bad_pair, FreesError::Property { .. }));
    }
}
