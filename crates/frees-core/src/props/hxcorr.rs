//! Heat-exchanger sizing correlations — UA (heat transfer) and dP (friction).
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/HxCorrelations.java`
//! (416 LOC), in full. A conventional "Nu + geometry" sizing engine:
//!
//! ```text
//!   Re = m·D_h/(A_flow·mu)   Pr = mu·cp/lambda   Nu = f(Re,Pr)   h = Nu·lambda/D_h
//!   UA = 1 / ( 1/(h_int·A_int) + R_wall + 1/(h_ext·A_ext) )
//! ```
//!
//! with a smooth laminar↔turbulent blend, two-phase boiling (Shah convective)
//! and condensation (Shah) factors on the refrigerant side, and a single-phase
//! Darcy / two-phase Chisholm (Lockhart–Martinelli) pressure drop. Everything
//! is scalar and stateless — these are evaluated *outside* a component and
//! injected into its `UA` / `dP` parameters.
//!
//! # Two halves, split at the fluid boundary
//!
//! Roughly two thirds of this file is pure algebra and ports directly. The rest
//! reaches through CoolProp for transport properties. Because the real-fluid
//! backend is a separate work item, every fluid-backed correlation appears
//! twice:
//!
//! * `*_from_state(...)` — the Java body *after* its `CoolProp.propsSI` calls,
//!   taking the resolved properties as arguments. Pure, and verified against
//!   the Java oracle by feeding it the very numbers CoolProp handed the Java.
//! * `htc_1phase(fluids, ...)` — the whole Java method, with the property
//!   lookups behind the [`Fluids`] trait, in the Java's call order.
//!
//! Wiring a real backend is then one `impl Fluids`, with no correlation
//! algebra to re-derive.
//!
//! The pressure-drop correlations call `FlowResistance.frictionFactor` and
//! `TwoPhase.lmPhi2` in the Java; here they call
//! [`crate::props::flowresist::friction_factor`] and
//! [`crate::props::twophase::lm_phi2`] — the same two methods, ported once.

// The Java's geometry/positivity guards are written `!(dh > 0)`: the negation
// makes a NaN argument take the *reject* branch, which `dh <= 0` would not.
// That NaN behaviour is the point, so clippy's rewrite suggestion is refused.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// Sizing correlations legitimately take a flow state *and* a geometry *and* a
// length; `dp_compact_core` has nine arguments in the Java and splitting it
// into a struct would break the one-to-one reading against the source.
#![allow(clippy::too_many_arguments)]

use crate::diag::{FreesError, Result};
use crate::props::flowresist::friction_factor;
use crate::props::twophase::lm_phi2;

// ---------------------------------------------------------------------------
// The fluid boundary
// ---------------------------------------------------------------------------

/// The property calls these correlations make.
///
/// Mirrors the Java's two entry points into `props/CoolProp.java` plus the
/// alias resolution `PropertyFunctions.resolveFluid` applies to a user token.
/// The real-fluid backend implements this; nothing in this module knows how a
/// property is obtained.
pub trait Fluids {
    /// `PropertyFunctions.resolveFluid(token)` — alias and glycol-mixture
    /// resolution. The Java always passes an already lower-cased token.
    fn resolve_fluid(&self, token: &str) -> Result<String>;

    /// `CoolProp.propsSI(output, name1, value1, name2, value2, fluid)`.
    fn props_si(
        &self,
        output: &str,
        name1: &str,
        value1: f64,
        name2: &str,
        value2: f64,
        fluid: &str,
    ) -> Result<f64>;

    /// `CoolProp.props1SI(fluid, param)` — the trivial (state-free) inputs
    /// such as `Pcrit`.
    fn props1_si(&self, fluid: &str, param: &str) -> Result<f64>;
}

// ---------------------------------------------------------------------------
// Nusselt: laminar (3.66, const-wall-T) <-> Gnielinski turbulent, blended
// ---------------------------------------------------------------------------

/// Single-phase Nusselt with a C¹ laminar↔turbulent blend over `Re` 2300–4000.
///
/// Package-private in the Java (`nuSinglePhase`); exposed here because the
/// `*_from_state` cores below are the natural unit of test.
pub fn nu_single_phase(re: f64, pr: f64) -> f64 {
    let nu_lam = 3.66;
    let re_eff = java_max(re, 1.0);
    let f = libm::pow(0.79 * libm::log(java_max(re_eff, 1e3)) - 1.64, -2.0); // Petukhov
    let mut nu_turb = (f / 8.0) * (re_eff - 1000.0) * pr
        / (1.0 + 12.7 * libm::sqrt(f / 8.0) * (libm::pow(pr, 2.0 / 3.0) - 1.0));
    nu_turb = java_max(nu_turb, nu_lam);
    smooth(nu_lam, nu_turb, re_eff, 2300.0, 4000.0)
}

/// Single-phase forced-convection coefficient `h` [W/m²K] from resolved
/// transport properties — the body of `htc1phase` after its CoolProp calls.
pub fn htc_1phase_from_state(
    mu: f64,
    k: f64,
    cp: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let re = mdot * dh / (a_flow * mu);
    let pr = mu * cp / k;
    Ok(nu_single_phase(re, pr) * k / dh)
}

/// Single-phase forced-convection coefficient `h` [W/m²K] from `(P,T)`, flow
/// and geometry.
pub fn htc_1phase(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    t: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let mu = fluids.props_si("viscosity", "P", p, "T", t, &f)?;
    let k = fluids.props_si("conductivity", "P", p, "T", t, &f)?;
    let cp = fluids.props_si("Cpmass", "P", p, "T", t, &f)?;
    let re = mdot * dh / (a_flow * mu);
    let pr = mu * cp / k;
    Ok(nu_single_phase(re, pr) * k / dh)
}

/// Liquid-only single-phase `h` [W/m²K] (all the mass as saturated liquid),
/// the base for the two-phase factors. Package-private in the Java.
pub fn htc_liquid_only(
    fluids: &dyn Fluids,
    fluid: &str,
    p: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    let mu = fluids.props_si("viscosity", "P", p, "Q", 0.0, fluid)?;
    let k = fluids.props_si("conductivity", "P", p, "Q", 0.0, fluid)?;
    let cp = fluids.props_si("Cpmass", "P", p, "Q", 0.0, fluid)?;
    Ok(htc_liquid_only_from_state(mu, k, cp, mdot, dh, a_flow))
}

/// [`htc_liquid_only`] from resolved liquid properties. Note this is the one
/// place the Java does **not** call `guardGeom` — its callers already did.
pub fn htc_liquid_only_from_state(
    mu: f64,
    k: f64,
    cp: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> f64 {
    let re = mdot * dh / (a_flow * mu);
    let pr = mu * cp / k;
    nu_single_phase(re, pr) * k / dh
}

/// Flow-boiling coefficient `h` [W/m²K] — Shah's convective limit
/// (heat-flux-free): `h = h_lo · max(1, 1.8/Co^0.8)`,
/// `Co = ((1−x)/x)^0.8·sqrt(rho_g/rho_l)`. Quality is clipped to `[0.01, 0.99]`.
pub fn htc_evap_from_state(
    mu_l: f64,
    k_l: f64,
    cp_l: f64,
    rho_l: f64,
    rho_g: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let hlo = htc_liquid_only_from_state(mu_l, k_l, cp_l, mdot, dh, a_flow);
    let xx = clip(x, 0.01, 0.99);
    let co = libm::pow((1.0 - xx) / xx, 0.8) * libm::sqrt(rho_g / rho_l);
    Ok(hlo * java_max(1.0, 1.8 / libm::pow(co, 0.8)))
}

/// [`htc_evap_from_state`] with the CoolProp lookups in the Java's order.
pub fn htc_evap(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let hlo = htc_liquid_only(fluids, &f, p, mdot, dh, a_flow)?;
    let rhol = fluids.props_si("Dmass", "P", p, "Q", 0.0, &f)?;
    let rhog = fluids.props_si("Dmass", "P", p, "Q", 1.0, &f)?;
    let xx = clip(x, 0.01, 0.99);
    let co = libm::pow((1.0 - xx) / xx, 0.8) * libm::sqrt(rhog / rhol);
    Ok(hlo * java_max(1.0, 1.8 / libm::pow(co, 0.8)))
}

/// Condensation coefficient `h` [W/m²K] — the Shah condensation correlation
/// `h = h_lo · [(1−x)^0.8 + 3.8·x^0.76·(1−x)^0.04 / pr^0.38]`, `pr = P/Pcrit`.
pub fn htc_cond_from_state(
    mu_l: f64,
    k_l: f64,
    cp_l: f64,
    p: f64,
    p_crit: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let hlo = htc_liquid_only_from_state(mu_l, k_l, cp_l, mdot, dh, a_flow);
    let pr = p / p_crit;
    let xx = clip(x, 0.01, 0.99);
    let factor = libm::pow(1.0 - xx, 0.8)
        + 3.8 * libm::pow(xx, 0.76) * libm::pow(1.0 - xx, 0.04) / libm::pow(pr, 0.38);
    Ok(hlo * factor)
}

/// [`htc_cond_from_state`] with the CoolProp lookups in the Java's order.
pub fn htc_cond(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let hlo = htc_liquid_only(fluids, &f, p, mdot, dh, a_flow)?;
    let pr = p / fluids.props1_si(&f, "Pcrit")?;
    let xx = clip(x, 0.01, 0.99);
    let factor = libm::pow(1.0 - xx, 0.8)
        + 3.8 * libm::pow(xx, 0.76) * libm::pow(1.0 - xx, 0.04) / libm::pow(pr, 0.38);
    Ok(hlo * factor)
}

/// Overall conductance `UA` [W/K] in series: internal film, wall, external film.
pub fn ua_hx(h1: f64, a1: f64, h2: f64, a2: f64, r_wall: f64) -> Result<f64> {
    if h1 <= 0.0 || a1 <= 0.0 || h2 <= 0.0 || a2 <= 0.0 {
        return Err(FreesError::property(
            "ua_hx: film coefficients and areas must be positive.",
        ));
    }
    Ok(1.0 / (1.0 / (h1 * a1) + r_wall + 1.0 / (h2 * a2)))
}

/// Single-phase Darcy pressure drop `dP` [Pa] over length `l`, from resolved
/// density and viscosity.
pub fn dp_1phase_from_state(
    rho: f64,
    mu: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    Ok(darcy_drop(rho, mu, mdot, dh, a_flow, l))
}

/// Single-phase Darcy pressure drop `dP` [Pa] over length `l`.
pub fn dp_1phase(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    t: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let rho = fluids.props_si("Dmass", "P", p, "T", t, &f)?;
    let mu = fluids.props_si("viscosity", "P", p, "T", t, &f)?;
    let v = mdot / (rho * a_flow);
    let re = rho * libm::fabs(v) * dh / mu;
    let fr = friction_factor(re, 0.0);
    Ok(fr * (l / dh) * rho * v * libm::fabs(v) / 2.0)
}

/// Two-phase frictional `dP` [Pa] = liquid-only Darcy drop × Chisholm
/// (turbulent–turbulent) two-phase multiplier (Lockhart–Martinelli).
pub fn dp_2phase_from_state(
    rho_l: f64,
    mu_l: f64,
    rho_g: f64,
    mu_g: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let v = mdot / (rho_l * a_flow);
    let re = rho_l * libm::fabs(v) * dh / mu_l;
    let fr = friction_factor(re, 0.0);
    let dp_lo = fr * (l / dh) * rho_l * v * libm::fabs(v) / 2.0;
    let xx = clip(x, 0.01, 0.99);
    let xtt =
        libm::pow((1.0 - xx) / xx, 0.9) * libm::sqrt(rho_g / rho_l) * libm::pow(mu_l / mu_g, 0.1);
    Ok(dp_lo * lm_phi2(xtt, 20.0)?) // C=20 turbulent-turbulent
}

/// [`dp_2phase_from_state`] with the CoolProp lookups in the Java's order.
pub fn dp_2phase(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let rhol = fluids.props_si("Dmass", "P", p, "Q", 0.0, &f)?;
    let mul = fluids.props_si("viscosity", "P", p, "Q", 0.0, &f)?;
    let rhog = fluids.props_si("Dmass", "P", p, "Q", 1.0, &f)?;
    let mug = fluids.props_si("viscosity", "P", p, "Q", 1.0, &f)?;
    dp_2phase_from_state(rhol, mul, rhog, mug, x, mdot, dh, a_flow, l)
}

// ---------------------------------------------------------------------------
// External / air-side convection (compact finned HX, tube banks)
// ---------------------------------------------------------------------------

/// Žukauskas tube-bank cross-flow Nusselt (forced, gases):
/// `Nu = 0.27·Re^0.63·Pr^0.36`.
pub fn nu_zukauskas(re: f64, pr: f64) -> f64 {
    0.27 * libm::pow(java_max(re, 1.0), 0.63) * libm::pow(pr, 0.36)
}

/// Colburn j-factor Nusselt for compact surfaces: `Nu = j·Re·Pr^(1/3)`.
pub fn nu_colburn(j: f64, re: f64, pr: f64) -> f64 {
    j * re * libm::pow(pr, 1.0 / 3.0)
}

/// Churchill–Chu natural-convection Nusselt (vertical surface) from Rayleigh `ra`.
pub fn nu_churchill_chu(ra: f64, pr: f64) -> f64 {
    let d = libm::pow(1.0 + libm::pow(0.492 / pr, 9.0 / 16.0), 8.0 / 27.0);
    let term = 0.825 + 0.387 * libm::pow(java_max(ra, 0.0), 1.0 / 6.0) / d;
    term * term
}

/// Cubic free+forced convection blend `Nu = (Nu1³ + Nu2³)^(1/3)`.
pub fn nu_blend(nu1: f64, nu2: f64) -> f64 {
    libm::cbrt(nu1 * nu1 * nu1 + nu2 * nu2 * nu2)
}

/// Air-side / external-flow film coefficient `h` [W/m²K] over a finned tube
/// bank (Žukauskas), characteristic length = tube outer diameter `d`, from
/// resolved transport properties.
pub fn htc_ext_air_from_state(
    mu: f64,
    k: f64,
    cp: f64,
    mdot: f64,
    d: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(d, a_flow)?;
    let re = mdot * d / (a_flow * mu);
    let pr = mu * cp / k;
    Ok(nu_zukauskas(re, pr) * k / d)
}

/// [`htc_ext_air_from_state`] with the CoolProp lookups in the Java's order.
pub fn htc_ext_air(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    t: f64,
    mdot: f64,
    d: f64,
    a_flow: f64,
) -> Result<f64> {
    guard_geom(d, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let mu = fluids.props_si("viscosity", "P", p, "T", t, &f)?;
    let k = fluids.props_si("conductivity", "P", p, "T", t, &f)?;
    let cp = fluids.props_si("Cpmass", "P", p, "T", t, &f)?;
    let re = mdot * d / (a_flow * mu);
    let pr = mu * cp / k;
    Ok(nu_zukauskas(re, pr) * k / d)
}

// ---------------------------------------------------------------------------
// Geometry resolution: primary dimensions -> D_h, A_conv, sigma, eta_surf
// ---------------------------------------------------------------------------

/// Compact hydraulic diameter `D_h = 4·A_flow·L / A_total`.
pub fn hx_dh(a_flow: f64, a_total: f64, l: f64) -> Result<f64> {
    if !(a_total > 0.0) {
        return Err(FreesError::property("hx_dh: total area must be > 0."));
    }
    Ok(4.0 * a_flow * l / a_total)
}

/// Convective area from the compact identity `A = 4·A_flow·L / D_h`.
pub fn hx_aconv(a_flow: f64, l: f64, dh: f64) -> Result<f64> {
    if !(dh > 0.0) {
        return Err(FreesError::property("hx_aconv: D_h must be > 0."));
    }
    Ok(4.0 * a_flow * l / dh)
}

/// Free-flow (contraction) ratio `sigma = A_flow / A_frontal`.
pub fn hx_sigma(a_flow: f64, a_frontal: f64) -> Result<f64> {
    if !(a_frontal > 0.0) {
        return Err(FreesError::property("hx_sigma: frontal area must be > 0."));
    }
    Ok(a_flow / a_frontal)
}

/// Overall fin-surface efficiency `eta_surf = 1 − (A_fin/A_total)(1 − eta_fin)`.
pub fn hx_eta_surf(a_fin: f64, a_total: f64, eta_fin: f64) -> Result<f64> {
    if !(a_total > 0.0) {
        return Err(FreesError::property("hx_eta_surf: total area must be > 0."));
    }
    Ok(1.0 - (a_fin / a_total) * (1.0 - eta_fin))
}

// ---------------------------------------------------------------------------
// Pressure drop: Müller–Steinhagen two-phase + compact-core entrance/exit
// ---------------------------------------------------------------------------

/// Müller–Steinhagen–Heck two-phase frictional `dP` [Pa]:
/// `dP = [A + 2(B−A)x](1−x)^(1/3) + B·x³`, `A`/`B` = all-liquid / all-gas Darcy
/// drop. Quality is clipped to `[0, 1]`.
pub fn dp_mueller_steinhagen_from_state(
    rho_l: f64,
    mu_l: f64,
    rho_g: f64,
    mu_g: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let a = darcy_drop(rho_l, mu_l, mdot, dh, a_flow, l);
    let b = darcy_drop(rho_g, mu_g, mdot, dh, a_flow, l);
    let xx = clip(x, 0.0, 1.0);
    Ok((a + 2.0 * (b - a) * xx) * libm::pow(1.0 - xx, 1.0 / 3.0) + b * libm::pow(xx, 3.0))
}

/// [`dp_mueller_steinhagen_from_state`] with the CoolProp lookups in the Java's
/// order.
pub fn dp_mueller_steinhagen(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    x: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let f = fluids.resolve_fluid(&fluid_tok.to_lowercase())?;
    let a = darcy_drop(
        fluids.props_si("Dmass", "P", p, "Q", 0.0, &f)?,
        fluids.props_si("viscosity", "P", p, "Q", 0.0, &f)?,
        mdot,
        dh,
        a_flow,
        l,
    );
    let b = darcy_drop(
        fluids.props_si("Dmass", "P", p, "Q", 1.0, &f)?,
        fluids.props_si("viscosity", "P", p, "Q", 1.0, &f)?,
        mdot,
        dh,
        a_flow,
        l,
    );
    let xx = clip(x, 0.0, 1.0);
    Ok((a + 2.0 * (b - a) * xx) * libm::pow(1.0 - xx, 1.0 / 3.0) + b * libm::pow(xx, 3.0))
}

fn darcy_drop(rho: f64, mu: f64, mdot: f64, dh: f64, a_flow: f64, l: f64) -> f64 {
    let v = mdot / (rho * a_flow);
    let re = rho * libm::fabs(v) * dh / mu;
    friction_factor(re, 0.0) * (l / dh) * rho * v * libm::fabs(v) / 2.0
}

/// Compact-core `dP` [Pa] (full four-term form): entrance contraction `kc`,
/// flow acceleration (`rho_in → rho_out`), the CORE-FRICTION term
/// `f·(A/Ac)·(rho_in/rho_mean)`, and exit expansion `ke`, for a free-flow ratio
/// `sigma`.
///
/// `a_over_ac` = total heat-transfer area / free-flow area; `fanning` = Fanning
/// friction factor (e.g. `f_fin`). The friction term is usually dominant and
/// must not be dropped.
pub fn dp_compact_core(
    g: f64,
    rho_in: f64,
    rho_out: f64,
    rho_mean: f64,
    sigma: f64,
    fanning: f64,
    a_over_ac: f64,
    kc: f64,
    ke: f64,
) -> Result<f64> {
    if !(rho_in > 0.0) || !(rho_out > 0.0) || !(rho_mean > 0.0) {
        return Err(FreesError::property(
            "dp_compact_core: densities must be > 0.",
        ));
    }
    let s2 = sigma * sigma;
    Ok((g * g / (2.0 * rho_in))
        * ((kc + 1.0 - s2)
            + 2.0 * (rho_in / rho_out - 1.0)
            + fanning * a_over_ac * (rho_in / rho_mean)
            - (1.0 - s2 - ke) * (rho_in / rho_out)))
}

// ---------------------------------------------------------------------------
// Remaining correlations (advisory item A)
// ---------------------------------------------------------------------------

/// Žukauskas tube-bank cross-flow Nusselt with arrangement + Re-band `C,m`.
///
/// `arr` = `"inline"` | `"staggered"`; `Pr^0.36` (`Pr/Pr_w ≈ 1` for gases).
/// Anything not starting with `stag` is in-line.
pub fn nu_tube_bank(arr: &str, re: f64, pr: f64) -> f64 {
    let staggered = arr.to_lowercase().starts_with("stag");
    let re_eff = java_max(re, 1.0);
    let (c, m) = if re_eff < 100.0 {
        (if staggered { 0.90 } else { 0.80 }, 0.40)
    } else if re_eff < 1000.0 {
        (0.51, 0.50)
    } else if re_eff < 2e5 {
        (
            if staggered { 0.40 } else { 0.27 },
            if staggered { 0.60 } else { 0.63 },
        )
    } else {
        // Standard tube-bank constants (in-line 0.21, not 0.021).
        (if staggered { 0.022 } else { 0.21 }, 0.84)
    };
    c * libm::pow(re_eff, m) * libm::pow(pr, 0.36)
}

/// Hilpert single-cylinder cross-flow Nusselt: `Nu = C·Re^m·Pr^(1/3)`, with
/// `C,m` by Re band.
pub fn nu_hilpert(re: f64, pr: f64) -> f64 {
    let re_eff = java_max(re, 0.4);
    let (c, m) = if re_eff < 4.0 {
        (0.989, 0.330)
    } else if re_eff < 40.0 {
        (0.911, 0.385)
    } else if re_eff < 4000.0 {
        (0.683, 0.466)
    } else if re_eff < 4e4 {
        (0.193, 0.618)
    } else {
        (0.027, 0.805)
    };
    c * libm::pow(re_eff, m) * libm::pow(pr, 1.0 / 3.0)
}

/// Chevron plate-HX Nusselt: `Nu = C(beta)·Re^m·Pr^(1/3)`; `C,m` rise with the
/// chevron angle `beta` (30°→60°), a Martin/Kumar-style fit. `beta` is clipped
/// to `[30, 60]`.
pub fn nu_plate(re: f64, pr: f64, beta_deg: f64) -> f64 {
    let b = clip(beta_deg, 30.0, 60.0);
    let f = (b - 30.0) / 30.0;
    let c = 0.2 + 0.2 * f;
    let m = 0.6 + 0.14 * f;
    c * libm::pow(java_max(re, 1.0), m) * libm::pow(pr, 1.0 / 3.0)
}

/// Developed fin length (fin-and-tube geometry).
pub fn hx_fin_len(depth: f64, t: f64, fin_density: f64, h_tube: f64) -> f64 {
    let a = h_tube - 2.0 * t;
    let b = 1.0 / (2.0 * fin_density);
    2.0 * (depth - 2.0 * t) * fin_density * libm::sqrt(a * a + b * b)
}

/// Primary (tube-wall) area.
pub fn hx_area_direct(w: f64, tube_count: f64, h_tube: f64, depth: f64, t: f64) -> f64 {
    2.0 * w * tube_count * ((h_tube - 2.0 * t) + (depth - 2.0 * t))
}

/// Secondary (fin) area.
pub fn hx_area_indirect(w: f64, tube_count: f64, fin_len: f64) -> f64 {
    2.0 * w * tube_count * fin_len
}

/// Two-phase gravitational (static-head) pressure change [Pa] over length `l`
/// at inclination `theta_deg` from horizontal:
/// `(alpha·rho_g + (1−alpha)·rho_l)·g·L·sin(theta)`.
pub fn dp_gravity(rho_l: f64, rho_g: f64, alpha: f64, l: f64, theta_deg: f64) -> f64 {
    let rho_mix = alpha * rho_g + (1.0 - alpha) * rho_l;
    rho_mix * 9.80665 * l * libm::sin(to_radians(theta_deg))
}

// ---------------------------------------------------------------------------
// Gap-completion correlations
// ---------------------------------------------------------------------------

/// Mass flux `G = mdot / A_flow` [kg/m²s].
pub fn mass_flux(mdot: f64, a_flow: f64) -> Result<f64> {
    if !(a_flow > 0.0) {
        return Err(FreesError::property("mass_flux: A_flow must be > 0."));
    }
    Ok(mdot / a_flow)
}

/// Colburn j-factor for a compact fin surface — the "j data table" as a
/// representative Re power-law per surface type: plain / wavy / louvered /
/// offset-strip. Use with [`nu_colburn`].
///
/// Coefficients are calibrated to standard compact-surface data magnitudes at
/// `Re ≈ 1000` (plain ≈ 0.005, wavy ≈ 0.008, louvered ≈ 0.011, offset ≈ 0.019)
/// with the physical ordering offset > louvered > wavy > plain (interrupted
/// fins break the boundary layer → higher j); uniform exponent −0.4.
pub fn j_fin(surface: &str, re: f64) -> f64 {
    let r = java_max(re, 1.0);
    match fin_surface(surface) {
        "wavy" => 0.130 * libm::pow(r, -0.40),
        "louvered" => 0.174 * libm::pow(r, -0.40),
        "offset" => 0.300 * libm::pow(r, -0.40),
        _ => 0.080 * libm::pow(r, -0.40), // plain
    }
}

/// Fanning friction factor for a compact fin surface (the air-side `dP`
/// analogue of [`j_fin`]); apply as `dP = 4·f·(L/D_h)·G²/(2 rho)`.
///
/// Calibrated to standard compact-surface data magnitudes at `Re ≈ 1000`
/// (plain ≈ 0.019, wavy ≈ 0.035, louvered ≈ 0.053, offset ≈ 0.071); uniform
/// exponent −0.3.
pub fn f_fin(surface: &str, re: f64) -> f64 {
    let r = java_max(re, 1.0);
    match fin_surface(surface) {
        "wavy" => 0.280 * libm::pow(r, -0.30),
        "louvered" => 0.420 * libm::pow(r, -0.30),
        "offset" => 0.560 * libm::pow(r, -0.30),
        _ => 0.150 * libm::pow(r, -0.30), // plain
    }
}

/// Java `finSurface` — lower-cases the spelling; anything unrecognised is
/// plain, which is the `default` arm of both switches above.
fn fin_surface(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "wavy" => "wavy",
        "louvered" => "louvered",
        "offset" => "offset",
        _ => "plain",
    }
}

/// Gungor–Winterton flow-boiling two-phase Nusselt from the liquid-only Nu:
/// `Nu_tp = Nu_l·(1 + 24000·Bo^1.16 + 1.37·(1/X_tt)^0.86)`. `Bo` is the boiling
/// number `q/(G·h_fg)` — pass 0 for the convective limit.
pub fn nu_gungor_winterton(nu_l: f64, xtt: f64, bo: f64) -> f64 {
    let e = 1.0
        + 24000.0 * libm::pow(java_max(bo, 0.0), 1.16)
        + 1.37 * libm::pow(1.0 / java_max(xtt, 1e-6), 0.86);
    nu_l * e
}

/// Traviss in-tube condensation Nusselt:
/// `Nu = 0.15·Pr_l·Re_l^0.9 / F_T · (1/X_tt + 2.85/X_tt^0.476)`,
/// `F_T = 5·Pr_l + 5·ln(1+5·Pr_l) + 2.5·ln(0.00313·Re_l^0.812)`.
pub fn nu_traviss(re_l: f64, pr_l: f64, xtt: f64) -> f64 {
    let r = java_max(re_l, 1.0);
    let mut ft = 5.0 * pr_l
        + 5.0 * libm::log(1.0 + 5.0 * pr_l)
        + 2.5 * libm::log(0.00313 * libm::pow(r, 0.812));
    ft = java_max(ft, 1e-3);
    let x = java_max(xtt, 1e-6);
    0.15 * pr_l * libm::pow(r, 0.9) / ft * (1.0 / x + 2.85 / libm::pow(x, 0.476))
}

/// Quality-integrated two-phase frictional `dP` [Pa]: the local two-phase drop
/// (the [`dp_2phase`] basis) integrated across `x_in → x_out` over `n` equal
/// cells — the cell-by-cell average the lumped single-point [`dp_2phase`]
/// cannot capture.
pub fn dp_2phase_avg_from_state(
    rho_l: f64,
    mu_l: f64,
    rho_g: f64,
    mu_g: f64,
    x_in: f64,
    x_out: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
    n: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let cells = cell_count(n);
    let seg = l / f64::from(cells);
    let mut total = 0.0;
    let mut i = 0;
    while i < cells {
        let x_mid = x_in + (x_out - x_in) * (f64::from(i) + 0.5) / f64::from(cells);
        total += dp_2phase_from_state(rho_l, mu_l, rho_g, mu_g, x_mid, mdot, dh, a_flow, seg)?;
        i += 1;
    }
    Ok(total)
}

/// [`dp_2phase_avg_from_state`] with the CoolProp lookups in the Java's order —
/// which repeats them per cell, exactly as the Java does.
pub fn dp_2phase_avg(
    fluids: &dyn Fluids,
    fluid_tok: &str,
    p: f64,
    x_in: f64,
    x_out: f64,
    mdot: f64,
    dh: f64,
    a_flow: f64,
    l: f64,
    n: f64,
) -> Result<f64> {
    guard_geom(dh, a_flow)?;
    let cells = cell_count(n);
    let seg = l / f64::from(cells);
    let mut total = 0.0;
    let mut i = 0;
    while i < cells {
        let x_mid = x_in + (x_out - x_in) * (f64::from(i) + 0.5) / f64::from(cells);
        total += dp_2phase(fluids, fluid_tok, p, x_mid, mdot, dh, a_flow, seg)?;
        i += 1;
    }
    Ok(total)
}

/// `(int) Math.max(1, Math.round(n))` — the Java's cell count, narrowing and
/// all.
fn cell_count(n: f64) -> i32 {
    let rounded = java_round(n);
    let clamped = if rounded > 1 { rounded } else { 1 };
    clamped as i32
}

/// `Math.round(double)`: the closest `long`, ties toward positive infinity.
/// NaN gives 0 and out-of-range saturates, matching the JLS narrowing that
/// Rust's `as` cast performs.
fn java_round(a: f64) -> i64 {
    if a.is_nan() {
        return 0;
    }
    let floor = libm::floor(a);
    (if a - floor >= 0.5 { floor + 1.0 } else { floor }) as i64
}

// ---------------------------------------------------------------------------
// Shared guards and small helpers (Java: smooth / clip / guardGeom)
// ---------------------------------------------------------------------------

/// Smoothstep (C¹) blend from `lo` to `hi` across `x1 → x2`.
fn smooth(lo: f64, hi: f64, x: f64, x1: f64, x2: f64) -> f64 {
    if x <= x1 {
        return lo;
    }
    if x >= x2 {
        return hi;
    }
    let t = (x - x1) / (x2 - x1);
    lo + (hi - lo) * t * t * (3.0 - 2.0 * t)
}

/// `v` clipped to `[a, b]` — the Java's ternary, so NaN falls through unchanged.
fn clip(v: f64, a: f64, b: f64) -> f64 {
    if v < a {
        a
    } else if v > b {
        b
    } else {
        v
    }
}

/// The shared geometry guard every fluid-backed correlation opens with.
pub fn guard_geom(dh: f64, a_flow: f64) -> Result<()> {
    if !(dh > 0.0) || !(a_flow > 0.0) {
        return Err(FreesError::property(
            "hx correlation: hydraulic diameter D_h and free-flow area A_flow must be > 0.",
        ));
    }
    Ok(())
}

/// `Math.max` (NaN-propagating, `0.0 > -0.0`).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

/// `Math.toRadians` — `deg / 180 · pi`, associated exactly as the JDK does.
fn to_radians(deg: f64) -> f64 {
    deg / 180.0 * core::f64::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every expectation is the Java oracle's value (`tools/golden-dumper`).
    fn close(actual: f64, expected: f64) {
        let tol = 1e-13 * libm::fabs(expected).max(1.0);
        assert!(
            libm::fabs(actual - expected) <= tol,
            "expected {expected:.17e}, got {actual:.17e}"
        );
    }

    // ---- the fluid state the Java's CoolProp handed these correlations -----
    //
    // Dumped from the same oracle run that produced the expectations below, so
    // the `*_from_state` cores are checked against Java end values, not
    // against a re-derivation.

    const W_MU: f64 = 0.0005767262693751609;
    const W_K: f64 = 0.6369957248212325;
    const W_CP: f64 = 4180.534790491714;
    const W_RHO: f64 = 989.4268355836397;

    const A_MU: f64 = 1.853734050902612e-05;
    const A_K: f64 = 0.026384465709828872;
    const A_CP: f64 = 1006.3739076641027;
    const A_RHO: f64 = 1.1769955883877592;

    const R_MUL: f64 = 0.0002186519451136908;
    const R_KL: f64 = 0.08512805394166044;
    const R_CPL: f64 = 1389.409471704562;
    const R_RHOL: f64 = 1240.7746009216569;
    const R_MUG: f64 = 1.1319456032008952e-05;
    const R_RHOG: f64 = 24.317378810052126;
    const R_PCRIT: f64 = 4059276.3737910665;

    /// A [`Fluids`] that replays exactly the CoolProp answers the Java saw.
    struct OracleFluids;

    impl Fluids for OracleFluids {
        fn resolve_fluid(&self, token: &str) -> Result<String> {
            Ok(match token {
                "water" => "Water",
                "air" => "Air",
                "r134a" => "R134a",
                other => other,
            }
            .to_string())
        }

        fn props_si(
            &self,
            output: &str,
            name1: &str,
            value1: f64,
            name2: &str,
            value2: f64,
            fluid: &str,
        ) -> Result<f64> {
            assert_eq!(name1, "P", "the Java always passes P first");
            let key = (fluid, output, name2, value2, value1);
            Ok(match key {
                ("Water", "viscosity", "T", 320.0, 101325.0) => W_MU,
                ("Water", "conductivity", "T", 320.0, 101325.0) => W_K,
                ("Water", "Cpmass", "T", 320.0, 101325.0) => W_CP,
                ("Water", "Dmass", "T", 320.0, 101325.0) => W_RHO,
                ("Air", "viscosity", "T", 300.0, 101325.0) => A_MU,
                ("Air", "conductivity", "T", 300.0, 101325.0) => A_K,
                ("Air", "Cpmass", "T", 300.0, 101325.0) => A_CP,
                ("Air", "Dmass", "T", 300.0, 101325.0) => A_RHO,
                ("R134a", "viscosity", "Q", 0.0, 500000.0) => R_MUL,
                ("R134a", "conductivity", "Q", 0.0, 500000.0) => R_KL,
                ("R134a", "Cpmass", "Q", 0.0, 500000.0) => R_CPL,
                ("R134a", "Dmass", "Q", 0.0, 500000.0) => R_RHOL,
                ("R134a", "viscosity", "Q", 1.0, 500000.0) => R_MUG,
                ("R134a", "Dmass", "Q", 1.0, 500000.0) => R_RHOG,
                other => panic!("test double has no entry for {other:?}"),
            })
        }

        fn props1_si(&self, fluid: &str, param: &str) -> Result<f64> {
            match (fluid, param) {
                ("R134a", "Pcrit") => Ok(R_PCRIT),
                other => panic!("test double has no entry for {other:?}"),
            }
        }
    }

    // ---- single phase ------------------------------------------------------

    #[test]
    fn htc_1phase_matches_the_oracle() {
        close(
            htc_1phase(&OracleFluids, "Water", 101325.0, 320.0, 0.05, 0.01, 0.0002).unwrap(),
            1773.213525785477,
        );
        close(
            htc_1phase_from_state(W_MU, W_K, W_CP, 0.05, 0.01, 0.0002).unwrap(),
            1773.213525785477,
        );
        close(
            htc_1phase(&OracleFluids, "Air", 101325.0, 300.0, 0.4, 0.012, 0.05).unwrap(),
            37.880966338541526,
        );
        close(
            htc_1phase_from_state(A_MU, A_K, A_CP, 0.4, 0.012, 0.05).unwrap(),
            37.880966338541526,
        );
    }

    #[test]
    fn nu_single_phase_blends_across_the_transition_band() {
        // Below 2300 the laminar constant, above 4000 the Gnielinski/Petukhov
        // form, C¹ smoothstep between — and never below 3.66.
        assert_eq!(nu_single_phase(1.0, 5.0), 3.66);
        assert_eq!(nu_single_phase(2300.0, 5.0), 3.66);
        assert_eq!(nu_single_phase(-1e9, 5.0), 3.66);
        let mid = nu_single_phase(3150.0, 5.0);
        assert!(mid > 3.66 && mid < nu_single_phase(4000.0, 5.0));
        for re in [1.0, 500.0, 2300.0, 3000.0, 4000.0, 1e5, 1e7] {
            assert!(nu_single_phase(re, 5.0) >= 3.66, "Re={re}");
        }
    }

    #[test]
    fn dp_1phase_matches_the_oracle() {
        close(
            dp_1phase(
                &OracleFluids,
                "Water",
                101325.0,
                320.0,
                0.05,
                0.01,
                0.0002,
                2.5,
            )
            .unwrap(),
            307.7399551248669,
        );
        close(
            dp_1phase_from_state(W_RHO, W_MU, 0.05, 0.01, 0.0002, 2.5).unwrap(),
            307.7399551248669,
        );
        close(
            dp_1phase(&OracleFluids, "Air", 101325.0, 300.0, 0.4, 0.012, 0.05, 1.2).unwrap(),
            100.64322564183759,
        );
        close(
            dp_1phase_from_state(A_RHO, A_MU, 0.4, 0.012, 0.05, 1.2).unwrap(),
            100.64322564183759,
        );
    }

    #[test]
    fn htc_ext_air_matches_the_oracle() {
        close(
            htc_ext_air(&OracleFluids, "Air", 101325.0, 300.0, 0.4, 0.012, 0.05).unwrap(),
            114.62962632532441,
        );
        close(
            htc_ext_air_from_state(A_MU, A_K, A_CP, 0.4, 0.012, 0.05).unwrap(),
            114.62962632532441,
        );
    }

    // ---- two phase ---------------------------------------------------------

    #[test]
    fn htc_evap_matches_the_oracle() {
        let f = &OracleFluids;
        close(
            htc_evap(f, "R134a", 500000.0, 0.3, 0.02, 0.008, 0.0001).unwrap(),
            2465.764627976024,
        );
        close(
            htc_evap_from_state(R_MUL, R_KL, R_CPL, R_RHOL, R_RHOG, 0.3, 0.02, 0.008, 0.0001)
                .unwrap(),
            2465.764627976024,
        );
        // Quality clipped to [0.01, 0.99].
        close(
            htc_evap(f, "R134a", 500000.0, 0.001, 0.02, 0.008, 0.0001).unwrap(),
            488.7354055926543,
        );
        close(
            htc_evap(f, "R134a", 500000.0, 0.999, 0.02, 0.008, 0.0001).unwrap(),
            80290.33784793603,
        );
        close(
            htc_evap(f, "R134a", 500000.0, 0.01, 0.02, 0.008, 0.0001).unwrap(),
            488.7354055926543,
        );
    }

    #[test]
    fn htc_cond_matches_the_oracle() {
        let f = &OracleFluids;
        close(
            htc_cond(f, "R134a", 500000.0, 0.7, 0.02, 0.008, 0.0001).unwrap(),
            3177.5547909630272,
        );
        close(
            htc_cond_from_state(
                R_MUL, R_KL, R_CPL, 500000.0, R_PCRIT, 0.7, 0.02, 0.008, 0.0001,
            )
            .unwrap(),
            3177.5547909630272,
        );
        close(
            htc_cond(f, "R134a", 500000.0, 0.0, 0.02, 0.008, 0.0001).unwrap(),
            609.0682446594748,
        );
        close(
            htc_cond(f, "R134a", 500000.0, 1.0, 0.02, 0.008, 0.0001).unwrap(),
            3409.6400109137635,
        );
    }

    #[test]
    fn dp_2phase_matches_the_oracle() {
        let f = &OracleFluids;
        close(
            dp_2phase(f, "R134a", 500000.0, 0.3, 0.02, 0.008, 0.0001, 1.5).unwrap(),
            5757.613174695314,
        );
        close(
            dp_2phase_from_state(R_RHOL, R_MUL, R_RHOG, R_MUG, 0.3, 0.02, 0.008, 0.0001, 1.5)
                .unwrap(),
            5757.613174695314,
        );
        close(
            dp_2phase(f, "R134a", 500000.0, 0.95, 0.02, 0.008, 0.0001, 1.5).unwrap(),
            726901.6174244224,
        );
    }

    #[test]
    fn dp_mueller_steinhagen_matches_the_oracle() {
        let f = &OracleFluids;
        close(
            dp_mueller_steinhagen(f, "R134a", 500000.0, 0.3, 0.02, 0.008, 0.0001, 1.5).unwrap(),
            1482.3714015637033,
        );
        close(
            dp_mueller_steinhagen_from_state(
                R_RHOL, R_MUL, R_RHOG, R_MUG, 0.3, 0.02, 0.008, 0.0001, 1.5,
            )
            .unwrap(),
            1482.3714015637033,
        );
        // x = 0 is the all-liquid Darcy drop, x = 1 the all-gas one.
        close(
            dp_mueller_steinhagen(f, "R134a", 500000.0, 0.0, 0.02, 0.008, 0.0001, 1.5).unwrap(),
            101.54054467562034,
        );
        close(
            dp_mueller_steinhagen(f, "R134a", 500000.0, 1.0, 0.02, 0.008, 0.0001, 1.5).unwrap(),
            2583.881614970487,
        );
    }

    #[test]
    fn dp_2phase_avg_matches_the_oracle() {
        let f = &OracleFluids;
        close(
            dp_2phase_avg(
                f, "R134a", 500000.0, 0.1, 0.9, 0.02, 0.008, 0.0001, 1.5, 8.0,
            )
            .unwrap(),
            29560.89355719473,
        );
        close(
            dp_2phase_avg_from_state(
                R_RHOL, R_MUL, R_RHOG, R_MUG, 0.1, 0.9, 0.02, 0.008, 0.0001, 1.5, 8.0,
            )
            .unwrap(),
            29560.89355719473,
        );
        close(
            dp_2phase_avg(
                f, "R134a", 500000.0, 0.1, 0.9, 0.02, 0.008, 0.0001, 1.5, 1.0,
            )
            .unwrap(),
            13755.8282874031,
        );
    }

    #[test]
    fn dp_2phase_avg_floors_the_cell_count_at_one() {
        let f = &OracleFluids;
        // (int) Math.max(1, Math.round(0)) == 1.
        close(
            dp_2phase_avg(
                f, "R134a", 500000.0, 0.1, 0.9, 0.02, 0.008, 0.0001, 1.5, 0.0,
            )
            .unwrap(),
            13755.8282874031,
        );
        assert_eq!(cell_count(0.0), 1);
        assert_eq!(cell_count(-17.0), 1);
        assert_eq!(cell_count(0.4), 1);
        assert_eq!(cell_count(0.5), 1);
        assert_eq!(cell_count(1.5), 2);
        assert_eq!(cell_count(2.49), 2);
        assert_eq!(cell_count(f64::NAN), 1);
        // Java's Math.round is *not* floor(a + 0.5): the JDK fixed that in 7.
        assert_eq!(java_round(0.49999999999999994), 0);
    }

    #[test]
    fn ua_hx_matches_the_oracle() {
        close(
            ua_hx(1200.0, 0.8, 60.0, 6.5, 0.0002).unwrap(),
            262.7589691763517,
        );
        close(ua_hx(1.0, 1.0, 1.0, 1.0, 0.0).unwrap(), 0.5);
    }

    // ---- pure correlations -------------------------------------------------

    #[test]
    fn zukauskas_colburn_churchill_chu_and_blend_match_the_oracle() {
        close(nu_zukauskas(20000.0, 0.71), 122.3110422561271);
        close(nu_zukauskas(0.2, 5.0), 0.4819399915661922);
        close(nu_zukauskas(1.0, 1.0), 0.27);
        close(nu_colburn(0.008, 1200.0, 0.71), 8.564276548278093);
        close(nu_colburn(0.02, 500.0, 7.0), 19.12931182772389);
        close(nu_churchill_chu(1e8, 0.71), 61.06517223358536);
        close(nu_churchill_chu(1e4, 7.0), 6.333474332938017);
        close(nu_churchill_chu(0.0, 0.71), 0.6806249999999999);
        close(nu_churchill_chu(-5.0, 0.71), 0.6806249999999999);
        close(nu_blend(12.0, 30.0), 30.62681233200878);
        close(nu_blend(0.0, 5.0), 5.0);
    }

    #[test]
    fn geometry_identities_match_the_oracle() {
        close(hx_dh(0.02, 4.0, 1.5).unwrap(), 0.03);
        close(hx_aconv(0.02, 1.5, 0.003).unwrap(), 40.0);
        close(hx_sigma(0.02, 0.08).unwrap(), 0.25);
        close(hx_eta_surf(3.0, 4.0, 0.85).unwrap(), 0.8875);
        close(hx_fin_len(0.025, 0.0001, 500.0, 0.002), 0.05106632549929554);
        close(hx_area_direct(0.5, 30.0, 0.002, 0.025, 0.0001), 0.798);
        close(hx_area_indirect(0.5, 30.0, 0.1234), 3.702);
        close(mass_flux(0.05, 0.002).unwrap(), 25.0);
    }

    #[test]
    fn hx_dh_and_hx_aconv_are_inverses() {
        let dh = hx_dh(0.02, 4.0, 1.5).unwrap();
        close(hx_aconv(0.02, 1.5, dh).unwrap(), 4.0);
    }

    #[test]
    fn dp_compact_core_matches_the_oracle() {
        close(
            dp_compact_core(12.0, 1.18, 1.05, 1.11, 0.45, 0.02, 60.0, 0.35, 0.25).unwrap(),
            125.42088868529544,
        );
        close(
            dp_compact_core(30.0, 1.2, 1.2, 1.2, 1.0, 0.005, 100.0, 0.0, 0.0).unwrap(),
            187.5,
        );
    }

    #[test]
    fn tube_bank_bands_match_the_oracle() {
        close(nu_tube_bank("inline", 50.0, 0.71), 3.3816682054151035);
        close(nu_tube_bank("staggered", 50.0, 0.71), 3.804376731091991);
        close(nu_tube_bank("inline", 500.0, 0.71), 10.0811060592778);
        close(nu_tube_bank("staggered", 500.0, 0.71), 10.0811060592778);
        close(nu_tube_bank("inline", 20000.0, 0.71), 122.3110422561271);
        close(nu_tube_bank("staggered", 20000.0, 0.71), 134.6266361008529);
        close(nu_tube_bank("inline", 300000.0, 0.71), 7403.79106115974);
        close(nu_tube_bank("staggered", 300000.0, 0.71), 775.6352540262584);
        close(nu_tube_bank("Stag", 500000.0, 0.71), 1191.2707817835892);
        // Anything not starting with "stag" is in-line; Re floors at 1.
        close(nu_tube_bank("anything", 0.01, 0.71), 0.7072012058644764);
    }

    #[test]
    fn hilpert_bands_match_the_oracle() {
        close(nu_hilpert(0.1, 0.71), 0.652071979466922);
        close(nu_hilpert(2.0, 0.71), 1.1090615263002104);
        close(nu_hilpert(20.0, 0.71), 2.575338823188933);
        close(nu_hilpert(2000.0, 0.71), 21.043604514377858);
        close(nu_hilpert(20000.0, 0.71), 78.34536277878139);
        close(nu_hilpert(200000.0, 0.71), 445.7715630604863);
    }

    #[test]
    fn plate_matches_the_oracle_and_clips_the_chevron_angle() {
        close(nu_plate(2000.0, 4.0, 30.0), 30.362299284382438);
        close(nu_plate(2000.0, 4.0, 45.0), 77.53504005648657);
        close(nu_plate(2000.0, 4.0, 60.0), 175.9984535866888);
        close(nu_plate(2000.0, 4.0, 10.0), 30.362299284382438);
        close(nu_plate(2000.0, 4.0, 90.0), 175.9984535866888);
        close(nu_plate(0.5, 4.0, 45.0), 0.47622031559045985);
    }

    #[test]
    fn dp_gravity_matches_the_oracle() {
        close(dp_gravity(1000.0, 20.0, 0.8, 3.0, 90.0), 6354.7091999999975);
        close(dp_gravity(1000.0, 20.0, 0.8, 3.0, 30.0), 3177.3545999999983);
        close(
            dp_gravity(1000.0, 20.0, 0.0, 3.0, -45.0),
            -20803.046147169163,
        );
        // Horizontal: no static head.
        close(dp_gravity(1000.0, 20.0, 0.5, 3.0, 0.0), 0.0);
    }

    #[test]
    fn fin_surfaces_match_the_oracle() {
        close(j_fin("plain", 1000.0), 0.005047658755841546);
        close(j_fin("wavy", 1000.0), 0.00820244547824251);
        close(j_fin("louvered", 1000.0), 0.01097865779395536);
        close(j_fin("offset", 1000.0), 0.018928720334405794);
        close(j_fin("OFFSET", 1000.0), 0.018928720334405794);
        close(j_fin("unknown", 1000.0), 0.005047658755841546);
        close(j_fin("plain", 0.5), 0.08);
        close(f_fin("plain", 1000.0), 0.018883881176912507);
        close(f_fin("wavy", 1000.0), 0.03524991153023669);
        close(f_fin("louvered", 1000.0), 0.052874867295355024);
        close(f_fin("offset", 1000.0), 0.07049982306047338);
        close(f_fin("Wavy", 1000.0), 0.03524991153023669);
        close(f_fin("unknown", 1000.0), 0.018883881176912507);
        close(f_fin("plain", 0.5), 0.15);
    }

    #[test]
    fn fin_surfaces_keep_the_physical_ordering() {
        // Interrupted fins break the boundary layer, so j and f both rise.
        for re in [200.0, 1000.0, 5000.0] {
            assert!(j_fin("plain", re) < j_fin("wavy", re));
            assert!(j_fin("wavy", re) < j_fin("louvered", re));
            assert!(j_fin("louvered", re) < j_fin("offset", re));
            assert!(f_fin("plain", re) < f_fin("wavy", re));
            assert!(f_fin("wavy", re) < f_fin("louvered", re));
            assert!(f_fin("louvered", re) < f_fin("offset", re));
        }
    }

    #[test]
    fn gungor_winterton_and_traviss_match_the_oracle() {
        close(nu_gungor_winterton(120.0, 0.5, 0.0), 418.3922982682825);
        close(nu_gungor_winterton(120.0, 0.5, 1e-4), 484.3692866679944);
        close(nu_gungor_winterton(120.0, 1e-9, 0.0), 23763149.831063047);
        // A negative boiling number floors at 0 — same as Bo = 0.
        close(nu_gungor_winterton(120.0, 0.5, -1.0), 418.3922982682825);
        close(nu_traviss(20000.0, 3.0, 0.5), 577.068473824192);
        close(nu_traviss(0.5, 3.0, 0.5), 0.18578018170867452);
        close(nu_traviss(20000.0, 3.0, 1e-9), 96956349.80542885);
    }

    // ---- guards ------------------------------------------------------------

    #[test]
    fn guards_carry_the_java_text() {
        assert_eq!(
            htc_1phase(&OracleFluids, "Water", 101325.0, 320.0, 0.05, 0.0, 0.0002).unwrap_err(),
            FreesError::property(
                "hx correlation: hydraulic diameter D_h and free-flow area A_flow must be > 0."
            )
        );
        assert_eq!(
            ua_hx(0.0, 1.0, 1.0, 1.0, 0.0).unwrap_err(),
            FreesError::property("ua_hx: film coefficients and areas must be positive.")
        );
        assert_eq!(
            hx_dh(0.02, 0.0, 1.5).unwrap_err(),
            FreesError::property("hx_dh: total area must be > 0.")
        );
        assert_eq!(
            hx_aconv(0.02, 1.5, 0.0).unwrap_err(),
            FreesError::property("hx_aconv: D_h must be > 0.")
        );
        assert_eq!(
            hx_sigma(0.02, 0.0).unwrap_err(),
            FreesError::property("hx_sigma: frontal area must be > 0.")
        );
        assert_eq!(
            hx_eta_surf(3.0, 0.0, 0.85).unwrap_err(),
            FreesError::property("hx_eta_surf: total area must be > 0.")
        );
        assert_eq!(
            dp_compact_core(12.0, 0.0, 1.05, 1.11, 0.45, 0.02, 60.0, 0.35, 0.25).unwrap_err(),
            FreesError::property("dp_compact_core: densities must be > 0.")
        );
        assert_eq!(
            mass_flux(0.05, 0.0).unwrap_err(),
            FreesError::property("mass_flux: A_flow must be > 0.")
        );
    }

    #[test]
    fn geometry_guards_reject_nan_but_ua_hx_does_not() {
        // `!(dh > 0)` rejects NaN…
        assert!(guard_geom(f64::NAN, 1.0).is_err());
        assert!(guard_geom(1.0, f64::NAN).is_err());
        assert!(hx_dh(0.02, f64::NAN, 1.5).is_err());
        assert!(mass_flux(1.0, f64::NAN).is_err());
        assert!(dp_compact_core(12.0, f64::NAN, 1.0, 1.0, 0.4, 0.02, 60.0, 0.3, 0.2).is_err());
        // …while `h1 <= 0` lets it through, exactly as the Java does.
        assert!(ua_hx(f64::NAN, 1.0, 1.0, 1.0, 0.0).unwrap().is_nan());
    }

    #[test]
    fn pressure_drops_go_through_the_shared_friction_factor() {
        // `dp_1phase` is `f·(L/D_h)·rho·v|v|/2` with f from
        // `props::flowresist` — not a private copy. Reconstruct it here so a
        // future change to either side shows up as a failure.
        let (mdot, dh, a_flow, l) = (0.05, 0.01, 0.0002, 2.5);
        let v = mdot / (W_RHO * a_flow);
        let re = W_RHO * libm::fabs(v) * dh / W_MU;
        let want = friction_factor(re, 0.0) * (l / dh) * W_RHO * v * libm::fabs(v) / 2.0;
        close(want, 307.7399551248669);
        close(
            dp_1phase_from_state(W_RHO, W_MU, mdot, dh, a_flow, l).unwrap(),
            want,
        );
    }

    #[test]
    fn the_two_phase_multiplier_is_chisholm_with_c_equal_20() {
        // `dp_2phase` = liquid-only Darcy drop × `props::twophase::lm_phi2`.
        let (x, mdot, dh, a_flow, l) = (0.3, 0.02, 0.008, 0.0001, 1.5);
        let v = mdot / (R_RHOL * a_flow);
        let re = R_RHOL * libm::fabs(v) * dh / R_MUL;
        let dp_lo = friction_factor(re, 0.0) * (l / dh) * R_RHOL * v * libm::fabs(v) / 2.0;
        let xtt = libm::pow((1.0 - x) / x, 0.9)
            * libm::sqrt(R_RHOG / R_RHOL)
            * libm::pow(R_MUL / R_MUG, 0.1);
        close(dp_lo * lm_phi2(xtt, 20.0).unwrap(), 5757.613174695314);
    }

    #[test]
    fn clip_and_smooth_keep_their_java_shapes() {
        assert_eq!(clip(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clip(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(clip(2.0, 0.0, 1.0), 1.0);
        // The Java ternary lets NaN fall through unclamped.
        assert!(clip(f64::NAN, 0.0, 1.0).is_nan());
        assert_eq!(smooth(1.0, 2.0, 0.0, 0.0, 1.0), 1.0);
        assert_eq!(smooth(1.0, 2.0, 1.0, 0.0, 1.0), 2.0);
        assert_eq!(smooth(1.0, 2.0, 0.5, 0.0, 1.0), 1.5);
    }

    #[test]
    fn to_radians_matches_the_jdk_association() {
        assert_eq!(to_radians(180.0), core::f64::consts::PI);
        assert_eq!(to_radians(90.0), core::f64::consts::PI / 2.0);
        assert_eq!(to_radians(30.0), 0.5235987755982988);
    }
}
