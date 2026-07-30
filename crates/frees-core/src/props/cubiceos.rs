//! Cubic equations of state — Soave–Redlich–Kwong and Peng–Robinson.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/CubicEos.java`
//! (350 LOC), together with its data resource
//! `core/src/main/resources/eos_fluids.json` (vendored verbatim as
//! [`EOS_FLUIDS_JSON`], see `data/eos_fluids.json`).
//!
//! This is the **dependency-free** real-fluid backend: no CoolProp, no tables,
//! no network, so it is the one real-fluid path that is complete in the browser
//! on day one. It supports custom fluids and fluids CoolProp may lack, at the
//! accuracy of a two-parameter cubic EOS. P–v–T, the compressibility factor,
//! the enthalpy/entropy departure functions and the saturation pressure follow
//! the standard generalized-cubic formulation.
//!
//! # Reference state
//!
//! Enthalpy and entropy are returned on an **EOS-self-consistent** reference
//! (ideal-gas `h = 0`, `s = 0` at 298.15 K, 1 bar), so *differences* are
//! physical but absolute values do not match CoolProp's reference state. That
//! is the parent engine's behaviour and is preserved deliberately: "fixing" it
//! would silently move every `eos_enthalpy` result in existing documents.
//!
//! All outputs are SI mass-basis (Pa, m³/kg, J/kg, J/(kg·K)).
//!
//! # Verified against the Java oracle
//!
//! The tests at the bottom of this file replay 162 values produced by the real
//! Java engine through `tools/golden-dumper` — 9 fluids × 2 models × 9
//! quantities — and require agreement to 1e-12 relative.

// The generalized-cubic algebra below is transcribed operation-for-operation
// from the Java so the two engines round identically. Folding constants,
// rewriting repeated products as `powi`, or reassociating sums would move the
// last ulp, so it is not done even where clippy would prefer it.

use std::sync::OnceLock;

use crate::diag::{FreesError, Result};

/// The fluid coefficient table, vendored byte-for-byte from
/// `../frEES/backend/core/src/main/resources/eos_fluids.json`.
///
/// Refreshing it is a file copy; the reader below is deliberately strict, so a
/// schema change fails loudly at first use instead of producing wrong physics.
pub const EOS_FLUIDS_JSON: &str = include_str!("data/eos_fluids.json");

/// Universal gas constant [J/(mol·K)] — the Java literal, not a std constant.
const R: f64 = 8.314462618;
/// Ideal-gas reference temperature [K].
const T_REF: f64 = 298.15;
/// Ideal-gas reference pressure [Pa].
const P_REF: f64 = 1.0e5;

// ---------------------------------------------------------------------------
// Fluid table
// ---------------------------------------------------------------------------

/// Critical / ideal-gas parameters for one fluid.
///
/// `cp0` is the **molar** ideal-gas heat capacity `a + bT + cT² + dT³`
/// [J/(mol·K)]; `mw` is the molar mass [kg/kmol] (the Java's `M`).
#[derive(Debug, Clone, PartialEq)]
pub struct Fluid {
    /// The canonical (lowercase) table key, e.g. `carbondioxide`.
    pub name: String,
    /// Critical temperature [K].
    pub tc: f64,
    /// Critical pressure [Pa].
    pub pc: f64,
    /// Acentric factor [-].
    pub omega: f64,
    /// Molar mass [kg/kmol].
    pub mw: f64,
    /// Ideal-gas `cp0` polynomial coefficients [J/(mol·K)].
    pub cp0: [f64; 4],
}

/// The parsed contents of `eos_fluids.json`.
#[derive(Debug)]
struct FluidTable {
    /// Sorted by key, so lookups binary-search and error messages list the
    /// known names in the order the Java's `sorted()` produces.
    fluids: Vec<(String, Fluid)>,
    aliases: Vec<(String, String)>,
}

impl FluidTable {
    fn get(&self, key: &str) -> Option<&Fluid> {
        self.fluids
            .binary_search_by(|(k, _)| k.as_str().cmp(key))
            .ok()
            .map(|i| &self.fluids[i].1)
    }

    fn alias(&self, key: &str) -> Option<&str> {
        self.aliases
            .binary_search_by(|(k, _)| k.as_str().cmp(key))
            .ok()
            .map(|i| self.aliases[i].1.as_str())
    }

    fn known_names(&self) -> String {
        let names: Vec<&str> = self.fluids.iter().map(|(k, _)| k.as_str()).collect();
        names.join(", ")
    }
}

/// The embedded table, parsed once.
///
/// The parse *result* is cached, failure included, so a malformed resource
/// reports the same explicit error on every call instead of panicking inside a
/// lazy initializer.
fn table() -> Result<&'static FluidTable> {
    static TABLE: OnceLock<std::result::Result<FluidTable, String>> = OnceLock::new();
    TABLE
        .get_or_init(|| parse_fluid_table(EOS_FLUIDS_JSON))
        .as_ref()
        .map_err(|e| {
            FreesError::property(format!(
                "Cubic EOS: the embedded eos_fluids.json is malformed: {e}"
            ))
        })
}

/// Whether the (case-insensitive) token names an EOS fluid or alias.
///
/// Port of `CubicEos.isEosFluid`.
pub fn is_eos_fluid(token: &str) -> bool {
    let Ok(t) = table() else {
        return false;
    };
    let k = token.to_lowercase();
    t.get(&k).is_some() || t.alias(&k).is_some()
}

/// Every fluid key in the embedded table, sorted.
pub fn fluid_names() -> Vec<&'static str> {
    match table() {
        Ok(t) => t.fluids.iter().map(|(k, _)| k.as_str()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Resolves a user token (fluid name or alias, any case) to its parameters.
///
/// Port of the private `CubicEos.fluid`.
pub fn fluid(token: &str) -> Result<&'static Fluid> {
    let t = table()?;
    let lowered = token.to_lowercase();
    let key = t.alias(&lowered).unwrap_or(lowered.as_str());
    t.get(key).ok_or_else(|| {
        FreesError::property(format!(
            "Cubic EOS: unknown fluid '{token}'. Known fluids: {}.",
            t.known_names()
        ))
    })
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

/// Which two-parameter cubic to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// Soave–Redlich–Kwong.
    Srk,
    /// Peng–Robinson.
    Pr,
}

/// Parses the `model$` argument.
///
/// Port of the private `CubicEos.model`: lowercase, then strip every character
/// outside `a-z`, so `"Peng-Robinson"`, `"peng robinson"` and `"PengRobinson"`
/// all name the same model.
pub fn model(name: &str) -> Result<Model> {
    let k: String = name
        .trim()
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_lowercase)
        .collect();
    match k.as_str() {
        "srk" | "soave" | "rk" | "soaveredlichkwong" => Ok(Model::Srk),
        "pr" | "pengrobinson" | "peng" => Ok(Model::Pr),
        _ => Err(FreesError::property(format!(
            "Cubic EOS: model must be 'SRK' or 'PR', got '{name}'."
        ))),
    }
}

/// Per-model constants: the `a`/`b` numeric coefficients and the `(eps, sigma)`
/// volume shifts of the generalized cubic.
#[derive(Debug, Clone, Copy)]
struct Constants {
    oa: f64,
    ob: f64,
    eps: f64,
    sigma: f64,
}

fn constants(m: Model) -> Constants {
    match m {
        Model::Srk => Constants {
            oa: 0.42748,
            ob: 0.08664,
            eps: 0.0,
            sigma: 1.0,
        },
        Model::Pr => Constants {
            oa: 0.45724,
            ob: 0.07780,
            eps: 1.0 - std::f64::consts::SQRT_2,
            sigma: 1.0 + std::f64::consts::SQRT_2,
        },
    }
}

/// The `m(omega)` alpha-function slope for each model.
fn m_factor(model: Model, omega: f64) -> f64 {
    match model {
        Model::Srk => 0.480 + 1.574 * omega - 0.176 * omega * omega,
        Model::Pr => 0.37464 + 1.54226 * omega - 0.26992 * omega * omega,
    }
}

/// Working set of EOS quantities at one temperature: `a(T)`, `b`, `da/dT`.
#[derive(Debug, Clone, Copy)]
struct Params {
    a: f64,
    b: f64,
    dadt: f64,
    c: Constants,
}

fn params(f: &Fluid, model: Model, t: f64) -> Params {
    let c = constants(model);
    let ac = c.oa * R * R * f.tc * f.tc / f.pc;
    let b = c.ob * R * f.tc / f.pc;
    let m = m_factor(model, f.omega);
    let sqrt_tr = (t / f.tc).sqrt();
    let alpha = (1.0 + m * (1.0 - sqrt_tr)) * (1.0 + m * (1.0 - sqrt_tr));
    let a = ac * alpha;
    // dalpha/dT = -m (1 + m(1 - sqrtTr)) / sqrt(T*Tc)
    let dadt = ac * (-m * (1.0 + m * (1.0 - sqrt_tr)) / (t * f.tc).sqrt());
    Params { a, b, dadt, c }
}

/// Pressure [Pa] from temperature [K] and **molar** volume [m³/mol].
fn pressure_molar(p: &Params, t: f64, v_molar: f64) -> f64 {
    R * t / (v_molar - p.b) - p.a / ((v_molar + p.c.eps * p.b) * (v_molar + p.c.sigma * p.b))
}

// ---------------------------------------------------------------------------
// Public property functions — the `eos_*` intrinsic family
// ---------------------------------------------------------------------------

/// Compressibility factor `Z` for the requested phase at `(T, P)`.
///
/// `phase` selects the root: anything whose trimmed lowercase form starts with
/// `"liq"` picks the smallest physical root, everything else the largest.
///
/// Port of `CubicEos.z` — the `eos_z(fluid$, model$, T, P, phase$)` intrinsic.
pub fn z(fluid_tok: &str, model_tok: &str, t: f64, p: f64, phase: &str) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let m = model(model_tok)?;
    z_with(f, m, t, p, phase)
}

/// `z` with the fluid and model already resolved — the saturation loop calls it
/// twice per iteration and re-resolving would be pure overhead.
fn z_with(f: &Fluid, m: Model, t: f64, p: f64, phase: &str) -> Result<f64> {
    let pr = params(f, m, t);
    let a = pr.a;
    let b = pr.b;
    let a_a = a * p / (R * R * t * t);
    let b_b = b * p / (R * t);
    let eps = pr.c.eps;
    let sig = pr.c.sigma;
    // Generalized cubic in Z:
    // Z^3 + c2 Z^2 + c1 Z + c0 = 0, with eps+sig and eps*sig from the model.
    let s = eps + sig;
    let q = eps * sig;
    let c2 = (s - 1.0) * b_b - 1.0;
    let c1 = a_a + q * b_b * b_b - s * b_b * (b_b + 1.0);
    let c0 = -(a_a * b_b + q * b_b * b_b * (b_b + 1.0));
    let roots = real_cubic_roots(c2, c1, c0);
    let vapor = !phase.trim().to_lowercase().starts_with("liq");
    let mut chosen: Option<f64> = None;
    for zr in roots.iter() {
        if zr > b_b {
            // physical: v > b
            let better = match chosen {
                None => true,
                Some(cur) => {
                    if vapor {
                        zr > cur
                    } else {
                        zr < cur
                    }
                }
            };
            if better {
                chosen = Some(zr);
            }
        }
    }
    chosen.ok_or_else(|| {
        FreesError::property(format!(
            "Cubic EOS: no physical root for {} at T={t} K, P={p} Pa.",
            f.name
        ))
    })
}

/// Specific volume [m³/kg] for the requested phase at `(T, P)`.
///
/// Port of `CubicEos.volume` — `eos_volume(fluid$, model$, T, P, phase$)`.
pub fn volume(fluid_tok: &str, model_tok: &str, t: f64, p: f64, phase: &str) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let zz = z(fluid_tok, model_tok, t, p, phase)?;
    let v_molar = zz * R * t / p; // m^3/mol
    Ok(v_molar / (f.mw / 1000.0)) // m^3/kg
}

/// Density [kg/m³] for the requested phase at `(T, P)`.
///
/// Port of `CubicEos.density` — `eos_density(fluid$, model$, T, P, phase$)`.
pub fn density(fluid_tok: &str, model_tok: &str, t: f64, p: f64, phase: &str) -> Result<f64> {
    Ok(1.0 / volume(fluid_tok, model_tok, t, p, phase)?)
}

/// Pressure [Pa] from temperature [K] and specific volume [m³/kg].
///
/// Port of `CubicEos.pressure` — `eos_pressure(fluid$, model$, T, v)`.
pub fn pressure(fluid_tok: &str, model_tok: &str, t: f64, v_specific: f64) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let m = model(model_tok)?;
    let pr = params(f, m, t);
    let v_molar = v_specific * (f.mw / 1000.0);
    Ok(pressure_molar(&pr, t, v_molar))
}

// ----- enthalpy / entropy --------------------------------------------------

/// Molar enthalpy departure `H_real − H_ideal` [J/mol] at `(T, P, Z)`.
fn enthalpy_departure_molar(pr: &Params, t: f64, z: f64, b_b: f64) -> f64 {
    let term = (t * pr.dadt - pr.a) / (pr.b * (pr.c.sigma - pr.c.eps)) * log_ratio(z, b_b, &pr.c);
    R * t * (z - 1.0) + term
}

/// Molar entropy departure `S_real − S_ideal` [J/(mol·K)] at `(T, P, Z)`.
fn entropy_departure_molar(pr: &Params, z: f64, b_b: f64) -> f64 {
    let term = pr.dadt / (pr.b * (pr.c.sigma - pr.c.eps)) * log_ratio(z, b_b, &pr.c);
    R * (z - b_b).ln() + term
}

/// `ln[(Z + sigma B)/(Z + eps B)]`, the recurring departure integral.
fn log_ratio(z: f64, b_b: f64, c: &Constants) -> f64 {
    ((z + c.sigma * b_b) / (z + c.eps * b_b)).ln()
}

/// Ideal-gas molar enthalpy relative to `T_REF` [J/mol].
fn ideal_enthalpy_molar(f: &Fluid, t: f64) -> f64 {
    let a = &f.cp0;
    a[0] * (t - T_REF)
        + a[1] / 2.0 * (t * t - T_REF * T_REF)
        + a[2] / 3.0 * (t * t * t - T_REF * T_REF * T_REF)
        + a[3] / 4.0 * (t * t * t * t - T_REF * T_REF * T_REF * T_REF)
}

/// Ideal-gas molar entropy relative to `(T_REF, P_REF)` [J/(mol·K)].
fn ideal_entropy_molar(f: &Fluid, t: f64, p: f64) -> f64 {
    let a = &f.cp0;
    let integral = a[0] * (t / T_REF).ln()
        + a[1] * (t - T_REF)
        + a[2] / 2.0 * (t * t - T_REF * T_REF)
        + a[3] / 3.0 * (t * t * t - T_REF * T_REF * T_REF);
    integral - R * (p / P_REF).ln()
}

/// Specific enthalpy [J/kg] at `(T, P)` for the requested phase.
///
/// Port of `CubicEos.enthalpy` — `eos_enthalpy(fluid$, model$, T, P, phase$)`.
pub fn enthalpy(fluid_tok: &str, model_tok: &str, t: f64, p: f64, phase: &str) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let m = model(model_tok)?;
    let pr = params(f, m, t);
    let zz = z(fluid_tok, model_tok, t, p, phase)?;
    let b_b = pr.b * p / (R * t);
    let h_molar = ideal_enthalpy_molar(f, t) + enthalpy_departure_molar(&pr, t, zz, b_b);
    Ok(h_molar / (f.mw / 1000.0))
}

/// Specific entropy [J/(kg·K)] at `(T, P)` for the requested phase.
///
/// Port of `CubicEos.entropy` — `eos_entropy(fluid$, model$, T, P, phase$)`.
pub fn entropy(fluid_tok: &str, model_tok: &str, t: f64, p: f64, phase: &str) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let m = model(model_tok)?;
    let pr = params(f, m, t);
    let zz = z(fluid_tok, model_tok, t, p, phase)?;
    let b_b = pr.b * p / (R * t);
    let s_molar = ideal_entropy_molar(f, t, p) + entropy_departure_molar(&pr, zz, b_b);
    Ok(s_molar / (f.mw / 1000.0))
}

// ----- saturation ----------------------------------------------------------

/// Saturation pressure [Pa] at temperature `t` (< Tc), by equating the liquid
/// and vapour fugacity coefficients (successive substitution, ≤ 100 sweeps).
///
/// Port of `CubicEos.saturationPressure` — `eos_psat(fluid$, model$, T)`.
pub fn saturation_pressure(fluid_tok: &str, model_tok: &str, t: f64) -> Result<f64> {
    let f = fluid(fluid_tok)?;
    let m = model(model_tok)?;
    if t >= f.tc {
        return Err(FreesError::property(format!(
            "Cubic EOS: saturation pressure needs T < Tc ({} K) for {}, got {t} K.",
            f.tc, f.name
        )));
    }
    let pr = params(f, m, t);
    // Initial guess: Wilson correlation for vapour pressure.
    let tr = t / f.tc;
    let mut p = f.pc * (5.373 * (1.0 + f.omega) * (1.0 - 1.0 / tr)).exp();
    for _ in 0..100 {
        let b_b = pr.b * p / (R * t);
        let a_a = pr.a * p / (R * R * t * t);
        let zv = z_with(f, m, t, p, "vapor")?;
        let zl = z_with(f, m, t, p, "liquid")?;
        let ln_phi_v = fugacity_coeff(zv, a_a, b_b, &pr.c);
        let ln_phi_l = fugacity_coeff(zl, a_a, b_b, &pr.c);
        let ratio = (ln_phi_l - ln_phi_v).exp(); // = phiL/phiV = fL/fV at same P
        let p_new = p * ratio;
        if (p_new - p).abs() < 1e-6 * p {
            return Ok(p_new);
        }
        p = p_new;
    }
    Ok(p)
}

/// `ln(phi) = Z − 1 − ln(Z − B) − A/(B(sigma−eps)) ln[(Z+sigma B)/(Z+eps B)]`.
fn fugacity_coeff(z: f64, a_a: f64, b_b: f64, c: &Constants) -> f64 {
    z - 1.0
        - (z - b_b).ln()
        - a_a / (b_b * (c.sigma - c.eps)) * ((z + c.sigma * b_b) / (z + c.eps * b_b)).ln()
}

// ----- cubic root solver ---------------------------------------------------

/// Up to three real roots, without a heap allocation.
struct RootBuf {
    roots: [f64; 3],
    len: usize,
}

impl RootBuf {
    fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        self.roots[..self.len].iter().copied()
    }
}

/// Real roots of `z³ + c2 z² + c1 z + c0 = 0` (1 or 3 real roots).
///
/// Port of the private `CubicEos.realCubicRoots`.
fn real_cubic_roots(c2: f64, c1: f64, c0: f64) -> RootBuf {
    // Depressed cubic t^3 + p t + q via z = t - c2/3.
    let shift = c2 / 3.0;
    let p = c1 - c2 * c2 / 3.0;
    let q = 2.0 * c2 * c2 * c2 / 27.0 - c2 * c1 / 3.0 + c0;
    let disc = q * q / 4.0 + p * p * p / 27.0;
    if disc > 0.0 {
        let sqrt_disc = disc.sqrt();
        let u = (-q / 2.0 + sqrt_disc).cbrt();
        let v = (-q / 2.0 - sqrt_disc).cbrt();
        return RootBuf {
            roots: [u + v - shift, f64::NAN, f64::NAN],
            len: 1,
        };
    }
    // Three real roots (disc <= 0): trigonometric solution.
    let r = (-p * p * p / 27.0).sqrt();
    let phi = (-q / (2.0 * r)).clamp(-1.0, 1.0).acos();
    let mag = 2.0 * (-p / 3.0).sqrt();
    RootBuf {
        roots: [
            mag * (phi / 3.0).cos() - shift,
            mag * ((phi + 2.0 * std::f64::consts::PI) / 3.0).cos() - shift,
            mag * ((phi + 4.0 * std::f64::consts::PI) / 3.0).cos() - shift,
        ],
        len: 3,
    }
}

// ---------------------------------------------------------------------------
// eos_fluids.json reader
// ---------------------------------------------------------------------------
//
// The embedded resource is a fixed, hand-maintained document, so this is a
// small strict reader rather than a general JSON library: `frees-core` has no
// serde in its default feature set, and pulling one in for 1.8 KB of constants
// would cost wasm bytes the bundle budget cares about. Anything the reader does
// not understand — string escapes, booleans, null — is an explicit error, never
// a silent default.

/// Parses `eos_fluids.json` into the sorted lookup table.
fn parse_fluid_table(src: &str) -> std::result::Result<FluidTable, String> {
    let mut s = Scanner::new(src);
    let root = s.object()?;
    s.end()?;

    let mut fluids: Vec<(String, Fluid)> = Vec::new();
    let mut aliases: Vec<(String, String)> = Vec::new();

    let fluids_node = root
        .iter()
        .find(|(k, _)| *k == "fluids")
        .map(|(_, v)| v)
        .ok_or_else(|| "missing \"fluids\" object".to_string())?;
    let JsonValue::Object(entries) = fluids_node else {
        return Err("\"fluids\" must be an object".to_string());
    };
    for (name, node) in entries {
        let JsonValue::Object(fields) = node else {
            return Err(format!("fluid \"{name}\" must be an object"));
        };
        let num = |key: &str| -> std::result::Result<f64, String> {
            match fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v) {
                Some(JsonValue::Number(x)) => Ok(*x),
                Some(_) => Err(format!("fluid \"{name}\": \"{key}\" must be a number")),
                None => Err(format!("fluid \"{name}\": missing \"{key}\"")),
            }
        };
        let cp = match fields.iter().find(|(k, _)| *k == "cp").map(|(_, v)| v) {
            Some(JsonValue::Array(items)) => items,
            Some(_) => return Err(format!("fluid \"{name}\": \"cp\" must be an array")),
            None => return Err(format!("fluid \"{name}\": missing \"cp\"")),
        };
        if cp.len() != 4 {
            return Err(format!(
                "fluid \"{name}\": \"cp\" needs exactly 4 coefficients, found {}",
                cp.len()
            ));
        }
        let mut cp0 = [0.0f64; 4];
        for (slot, item) in cp0.iter_mut().zip(cp.iter()) {
            let JsonValue::Number(x) = item else {
                return Err(format!("fluid \"{name}\": \"cp\" entries must be numbers"));
            };
            *slot = *x;
        }
        fluids.push((
            (*name).to_string(),
            Fluid {
                name: (*name).to_string(),
                tc: num("Tc")?,
                pc: num("Pc")?,
                omega: num("omega")?,
                mw: num("M")?,
                cp0,
            },
        ));
    }
    if fluids.is_empty() {
        return Err("\"fluids\" is empty".to_string());
    }

    if let Some((_, JsonValue::Object(entries))) = root.iter().find(|(k, _)| *k == "aliases") {
        for (from, node) in entries {
            let JsonValue::Str(to) = node else {
                return Err(format!("alias \"{from}\" must map to a string"));
            };
            aliases.push(((*from).to_string(), (*to).to_string()));
        }
    }

    fluids.sort_by(|(a, _), (b, _)| a.cmp(b));
    aliases.sort_by(|(a, _), (b, _)| a.cmp(b));
    if fluids.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("duplicate fluid key".to_string());
    }
    if aliases.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("duplicate alias key".to_string());
    }
    for (from, to) in &aliases {
        if fluids
            .binary_search_by(|(k, _)| k.as_str().cmp(to.as_str()))
            .is_err()
        {
            return Err(format!("alias \"{from}\" points at unknown fluid \"{to}\""));
        }
    }
    Ok(FluidTable { fluids, aliases })
}

/// The subset of JSON the embedded resource uses.
#[derive(Debug)]
enum JsonValue<'a> {
    Str(&'a str),
    Number(f64),
    Array(Vec<JsonValue<'a>>),
    Object(Vec<(&'a str, JsonValue<'a>)>),
}

struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Scanner<'a> {
        Scanner {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> std::result::Result<u8, String> {
        self.skip_ws();
        self.bytes
            .get(self.pos)
            .copied()
            .ok_or_else(|| format!("unexpected end of input at byte {}", self.pos))
    }

    fn expect(&mut self, want: u8) -> std::result::Result<(), String> {
        let got = self.peek()?;
        if got != want {
            return Err(format!(
                "expected '{}' at byte {}, found '{}'",
                want as char, self.pos, got as char
            ));
        }
        self.pos += 1;
        Ok(())
    }

    fn end(&mut self) -> std::result::Result<(), String> {
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(format!("trailing input at byte {}", self.pos));
        }
        Ok(())
    }

    fn string(&mut self) -> std::result::Result<&'a str, String> {
        self.expect(b'"')?;
        let start = self.pos;
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'"' => {
                    let s = &self.src[start..self.pos];
                    self.pos += 1;
                    return Ok(s);
                }
                b'\\' => {
                    return Err(format!(
                        "escape sequences are not supported in eos_fluids.json (byte {})",
                        self.pos
                    ))
                }
                _ => self.pos += 1,
            }
        }
        Err("unterminated string".to_string())
    }

    fn number(&mut self) -> std::result::Result<f64, String> {
        self.skip_ws();
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.src[start..self.pos];
        text.parse::<f64>()
            .map_err(|_| format!("invalid number {text:?} at byte {start}"))
    }

    fn value(&mut self) -> std::result::Result<JsonValue<'a>, String> {
        match self.peek()? {
            b'"' => Ok(JsonValue::Str(self.string()?)),
            b'{' => Ok(JsonValue::Object(self.object()?)),
            b'[' => Ok(JsonValue::Array(self.array()?)),
            b't' | b'f' | b'n' => Err(format!(
                "booleans and null are not supported in eos_fluids.json (byte {})",
                self.pos
            )),
            _ => Ok(JsonValue::Number(self.number()?)),
        }
    }

    fn object(&mut self) -> std::result::Result<Vec<(&'a str, JsonValue<'a>)>, String> {
        self.expect(b'{')?;
        let mut out = Vec::new();
        if self.peek()? == b'}' {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            let value = self.value()?;
            out.push((key, value));
            match self.peek()? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Ok(out);
                }
                other => {
                    return Err(format!(
                        "expected ',' or '}}' at byte {}, found '{}'",
                        self.pos, other as char
                    ))
                }
            }
        }
    }

    fn array(&mut self) -> std::result::Result<Vec<JsonValue<'a>>, String> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        if self.peek()? == b']' {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            match self.peek()? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Ok(out);
                }
                other => {
                    return Err(format!(
                        "expected ',' or ']' at byte {}, found '{}'",
                        self.pos, other as char
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative agreement required against the Java oracle. The goldens below
    /// came out of the real engine, so anything looser than a few ulps would
    /// hide a genuine divergence.
    const ORACLE_TOL: f64 = 1e-12;

    fn assert_oracle(name: &str, actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        let ok = diff == 0.0 || diff <= ORACLE_TOL * expected.abs().max(actual.abs());
        assert!(
            ok,
            "{name}: got {actual:.17e}, Java oracle {expected:.17e} (rel {:.3e})",
            diff / expected.abs().max(f64::MIN_POSITIVE)
        );
    }

    // -- table ------------------------------------------------------------

    #[test]
    fn embedded_table_parses() {
        let t = table().expect("embedded eos_fluids.json parses");
        assert_eq!(t.fluids.len(), 9);
        assert_eq!(t.aliases.len(), 16);
        let water = t.get("water").expect("water present");
        assert_eq!(water.tc, 647.096);
        assert_eq!(water.pc, 22064000.0);
        assert_eq!(water.omega, 0.3443);
        assert_eq!(water.mw, 18.015);
        assert_eq!(water.cp0, [32.24, 0.1923e-2, 1.055e-5, -3.595e-9]);
    }

    #[test]
    fn known_names_are_sorted_like_the_java() {
        let t = table().unwrap();
        assert_eq!(
            t.known_names(),
            "ammonia, carbondioxide, ethane, methane, nbutane, nitrogen, oxygen, propane, water"
        );
    }

    #[test]
    fn aliases_and_case_folding_resolve() {
        assert_eq!(fluid("CO2").unwrap().name, "carbondioxide");
        assert_eq!(fluid("R744").unwrap().name, "carbondioxide");
        assert_eq!(fluid("Steam").unwrap().name, "water");
        assert_eq!(fluid("n-Butane").unwrap().name, "nbutane");
        assert_eq!(fluid("R717").unwrap().name, "ammonia");
        assert_eq!(fluid("NITROGEN").unwrap().name, "nitrogen");
    }

    #[test]
    fn is_eos_fluid_accepts_names_and_aliases() {
        assert!(is_eos_fluid("water"));
        assert!(is_eos_fluid("H2O"));
        assert!(is_eos_fluid("r290"));
        assert!(!is_eos_fluid("unobtainium"));
        assert!(!is_eos_fluid(""));
    }

    #[test]
    fn fluid_names_lists_every_key() {
        assert_eq!(
            fluid_names(),
            vec![
                "ammonia",
                "carbondioxide",
                "ethane",
                "methane",
                "nbutane",
                "nitrogen",
                "oxygen",
                "propane",
                "water"
            ]
        );
    }

    #[test]
    fn model_parsing_matches_the_java_normalisation() {
        for spelling in ["SRK", "srk", " Soave ", "rk", "Soave-Redlich-Kwong"] {
            assert_eq!(model(spelling).unwrap(), Model::Srk, "{spelling}");
        }
        for spelling in ["PR", "pr", "Peng-Robinson", "peng robinson", "PengRobinson"] {
            assert_eq!(model(spelling).unwrap(), Model::Pr, "{spelling}");
        }
    }

    // -- errors -----------------------------------------------------------

    #[test]
    fn unknown_fluid_is_a_property_error_listing_the_known_names() {
        let err = z("unobtainium", "PR", 300.0, 1e5, "vapor").unwrap_err();
        assert!(matches!(err, FreesError::Property { .. }), "{err}");
        let msg = err.to_string();
        assert!(msg.contains("unknown fluid 'unobtainium'"), "{msg}");
        assert!(msg.contains("ammonia, carbondioxide"), "{msg}");
    }

    #[test]
    fn unknown_model_is_a_property_error() {
        let err = z("co2", "vdw", 300.0, 1e5, "vapor").unwrap_err();
        assert!(matches!(err, FreesError::Property { .. }), "{err}");
        assert!(err.to_string().contains("'vdw'"), "{err}");
    }

    #[test]
    fn saturation_above_critical_is_rejected() {
        // Propane Tc = 369.89 K — the Java's CubicEosTest asserts this throw.
        let err = saturation_pressure("propane", "PR", 400.0).unwrap_err();
        assert!(matches!(err, FreesError::Property { .. }), "{err}");
        assert!(err.to_string().contains("T < Tc"), "{err}");
        assert!(saturation_pressure("propane", "PR", 369.88).is_ok());
    }

    #[test]
    fn malformed_resources_report_instead_of_panicking() {
        assert!(parse_fluid_table("{}").is_err());
        assert!(parse_fluid_table(r#"{"fluids": {}}"#).is_err());
        assert!(parse_fluid_table(r#"{"fluids": 3}"#).is_err());
        assert!(parse_fluid_table(r#"{"fluids": {"x": {"Tc": 1}}}"#).is_err());
        assert!(parse_fluid_table(
            r#"{"fluids":{"x":{"Tc":1,"Pc":2,"omega":0,"M":1,"cp":[1,2,3]}}}"#
        )
        .is_err());
        assert!(parse_fluid_table(
            r#"{"fluids":{"x":{"Tc":1,"Pc":2,"omega":0,"M":1,"cp":[1,2,3,4]}},"aliases":{"y":"z"}}"#
        )
        .is_err());
        assert!(parse_fluid_table(r#"{"fluids":{"x":true}}"#).is_err());
        assert!(parse_fluid_table(r#"{"fluids":{"x":{}},}"#).is_err());
        assert!(parse_fluid_table(r#"{"fluids":{"xA":{}}}"#).is_err());
        // A well-formed minimal document still parses.
        let ok = parse_fluid_table(
            r#"{"_c":"x","fluids":{"x":{"Tc":1,"Pc":2,"omega":0,"M":1,"cp":[1,2,3,4]}},
                "aliases":{"y":"x"}}"#,
        )
        .expect("minimal document");
        assert_eq!(ok.alias("y"), Some("x"));
        assert_eq!(ok.get("x").unwrap().cp0, [1.0, 2.0, 3.0, 4.0]);
    }

    // -- physics sanity (mirrors the Java CubicEosTest) --------------------

    #[test]
    fn ideal_gas_limit_at_low_pressure() {
        let zz = z("nitrogen", "PR", 400.0, 1.0e5, "vapor").unwrap();
        assert!((zz - 1.0).abs() < 5e-3, "Z = {zz}");
        let rho_ideal = 1.0e5 * 0.028013 / (8.314462618 * 400.0);
        let rho = density("nitrogen", "PR", 400.0, 1.0e5, "vapor").unwrap();
        assert!((rho - rho_ideal).abs() < 0.02 * rho_ideal, "rho = {rho}");
    }

    #[test]
    fn real_gas_compressibility_below_unity() {
        let z_pr = z("co2", "PR", 300.0, 6.0e6, "vapor").unwrap();
        let z_srk = z("co2", "SRK", 300.0, 6.0e6, "vapor").unwrap();
        assert!(z_pr > 0.0 && z_pr < 0.8, "PR Z = {z_pr}");
        assert!(z_srk > 0.0 && z_srk < 0.85, "SRK Z = {z_srk}");
    }

    #[test]
    fn pressure_volume_round_trip() {
        let v = volume("co2", "PR", 320.0, 5.0e6, "vapor").unwrap();
        let p = pressure("co2", "PR", 320.0, v).unwrap();
        assert!((p - 5.0e6).abs() < 1.0, "P = {p}");
        let v_liq = volume("propane", "PR", 300.0, 1.0e6, "liquid").unwrap();
        let v_vap = volume("propane", "PR", 300.0, 1.0e6, "vapor").unwrap();
        assert!(
            v_liq < v_vap,
            "liquid {v_liq} must be denser than vapour {v_vap}"
        );
    }

    #[test]
    fn enthalpy_rises_with_temperature() {
        let h300 = enthalpy("co2", "PR", 300.0, 1.0e6, "vapor").unwrap();
        let h350 = enthalpy("co2", "PR", 350.0, 1.0e6, "vapor").unwrap();
        assert!(h350 > h300, "{h300} -> {h350}");
    }

    #[test]
    fn phase_token_selects_the_root() {
        // Anything not starting with "liq" is vapour, per the Java.
        let liq = z("propane", "PR", 300.0, 1.0e6, "liquid").unwrap();
        for vapour_token in ["vapor", "vapour", "gas", "", "  V  "] {
            let v = z("propane", "PR", 300.0, 1.0e6, vapour_token).unwrap();
            assert!(v > liq, "{vapour_token:?}: {v} vs liquid {liq}");
        }
        for liquid_token in ["liquid", " LIQ ", "liq"] {
            let v = z("propane", "PR", 300.0, 1.0e6, liquid_token).unwrap();
            assert_eq!(v, liq, "{liquid_token:?}");
        }
    }

    #[test]
    fn single_root_region_serves_both_phase_requests() {
        // Well above Tc there is one real root; both phase tokens return it.
        let v = z("nitrogen", "PR", 400.0, 1.0e5, "vapor").unwrap();
        let l = z("nitrogen", "PR", 400.0, 1.0e5, "liquid").unwrap();
        assert_eq!(v, l);
    }

    #[test]
    fn cubic_roots_match_known_factorisations() {
        // (z-1)(z-2)(z-3) = z^3 - 6z^2 + 11z - 6
        let r = real_cubic_roots(-6.0, 11.0, -6.0);
        assert_eq!(r.len, 3);
        let mut got: Vec<f64> = r.iter().collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (g, want) in got.iter().zip([1.0, 2.0, 3.0]) {
            assert!((g - want).abs() < 1e-12, "{got:?}");
        }
        // z^3 + z + 1 has a single real root near -0.6823278.
        let r = real_cubic_roots(0.0, 1.0, 1.0);
        assert_eq!(r.len, 1);
        assert!(
            (r.roots[0] + 0.682_327_803_828_019_3).abs() < 1e-12,
            "{:?}",
            r.roots
        );
    }

    // -- Java-oracle goldens ----------------------------------------------
    //
    // Produced by `tools/golden-dumper/run.sh` over a generated sweep document
    // (9 fluids × 2 models × 9 quantities = 162 values). State points, as they
    // appear literally in that document:
    //     vapour  T = tv, P = pv     (0.95 Tc, 0.30 Pc)
    //     liquid  T = tl, P = pl     (0.70 Tc, 1.00 Pc)
    //     hot     T = th, P = ph     (1.20 Tc, 0.50 Pc)
    //     psat    T = ts             (0.80 Tc)

    struct Row {
        fluid: &'static str,
        model: &'static str,
        tv: f64,
        pv: f64,
        tl: f64,
        pl: f64,
        th: f64,
        ph: f64,
        ts: f64,
        zv: f64,
        zl: f64,
        vv: f64,
        dl: f64,
        h_hot: f64,
        s_hot: f64,
        h_liq: f64,
        s_liq: f64,
        psat: f64,
    }

    const ORACLE: &[Row] = &[
        Row {
            fluid: "nitrogen",
            model: "PR",
            tv: 119.8805,
            pv: 1018740.0,
            tl: 88.333,
            pl: 3395800.0,
            th: 151.428,
            ph: 1697900.0,
            ts: 100.952,
            zv: 0.8581334176688167,
            zl: 0.15040591300903045,
            vv: 0.029971920048585805,
            dl: 861.1516727864779,
            h_hot: -167451.06386805442,
            s_hot: -1610.9823755262635,
            h_liq: -404446.7639510549,
            s_liq: -3734.4380542577856,
            psat: 833555.4902537917,
        },
        Row {
            fluid: "nitrogen",
            model: "SRK",
            tv: 119.8805,
            pv: 1018740.0,
            tl: 88.333,
            pl: 3395800.0,
            th: 151.428,
            ph: 1697900.0,
            ts: 100.952,
            zv: 0.8697332534531226,
            zl: 0.16984098957916804,
            vv: 0.030377066082460595,
            dl: 762.6092140986367,
            h_hot: -166700.42826614474,
            s_hot: -1610.6331797463413,
            h_liq: -407527.791853506,
            s_liq: -3774.9087665661864,
            psat: 838633.1111053211,
        },
        Row {
            fluid: "oxygen",
            model: "PR",
            tv: 146.851,
            pv: 1512900.0,
            tl: 108.206,
            pl: 5043000.0,
            th: 185.496,
            ph: 2521500.0,
            ts: 123.664,
            zv: 0.8583654892203891,
            zl: 0.15089164406802924,
            vv: 0.021648943770168537,
            dl: 1188.7065507507975,
            h_hot: -118162.74595353885,
            s_hot: -1325.552425102448,
            h_liq: -365461.2454208131,
            s_liq: -3135.668518052686,
            psat: 1262707.8600096197,
        },
        Row {
            fluid: "oxygen",
            model: "SRK",
            tv: 146.851,
            pv: 1512900.0,
            tl: 108.206,
            pl: 5043000.0,
            th: 185.496,
            ph: 2521500.0,
            ts: 123.664,
            zv: 0.8699556006960216,
            zl: 0.17036945229990935,
            vv: 0.021941259426817142,
            dl: 1052.805437452941,
            h_hot: -117365.64208409387,
            s_hot: -1325.299284666011,
            h_liq: -368830.15631951136,
            s_liq: -3171.475922832614,
            psat: 1269385.9945228375,
        },
        Row {
            fluid: "carbondioxide",
            model: "PR",
            tv: 288.9235,
            pv: 2213190.0,
            tl: 212.891,
            pl: 7377300.0,
            th: 364.956,
            ph: 3688650.0,
            ts: 243.304,
            zv: 0.8553264581143982,
            zl: 0.14530233113509988,
            vv: 0.02109496897081061,
            dl: 1262.3644165865853,
            h_hot: 32264.315475849875,
            s_hot: -556.6029752279557,
            h_liq: -427184.3417771695,
            s_liq: -2247.952651602298,
            psat: 1422726.6514060162,
        },
        Row {
            fluid: "carbondioxide",
            model: "SRK",
            tv: 288.9235,
            pv: 2213190.0,
            tl: 212.891,
            pl: 7377300.0,
            th: 364.956,
            ph: 3688650.0,
            ts: 243.304,
            zv: 0.8670077531167295,
            zl: 0.16416633340939468,
            vv: 0.02138306546692076,
            dl: 1117.3088212588077,
            h_hot: 33526.15901461211,
            s_hot: -556.0115373846319,
            h_liq: -431670.2444974247,
            s_liq: -2273.5768307507356,
            psat: 1440411.2875370677,
        },
        Row {
            fluid: "methane",
            model: "PR",
            tv: 181.032,
            pv: 1379760.0,
            tl: 133.392,
            pl: 4599200.0,
            th: 228.672,
            ph: 2299600.0,
            ts: 152.448,
            zv: 0.8585331730232058,
            zl: 0.1512496524704347,
            vv: 0.05837911153593014,
            dl: 439.85499125534574,
            h_hot: -187879.10130941865,
            s_hot: -2303.8468582236546,
            h_liq: -799828.7044682078,
            s_liq: -5929.689988585332,
            psat: 1168213.001426703,
        },
        Row {
            fluid: "methane",
            model: "SRK",
            tv: 181.032,
            pv: 1379760.0,
            tl: 133.392,
            pl: 4599200.0,
            th: 228.672,
            ph: 2299600.0,
            ts: 152.448,
            zv: 0.8701159950087436,
            zl: 0.1707577369665031,
            vv: 0.0591667279936768,
            dl: 389.6041007957822,
            h_hot: -185933.50305513805,
            s_hot: -2303.4197858355446,
            h_liq: -808237.409563419,
            s_liq: -6001.712037809239,
            psat: 1173685.321556448,
        },
        Row {
            fluid: "ethane",
            model: "PR",
            tv: 290.054,
            pv: 1461660.0,
            tl: 213.724,
            pl: 4872200.0,
            th: 366.384,
            ph: 2436100.0,
            ts: 244.256,
            zv: 0.8571798643262427,
            zl: 0.14852127855739045,
            vv: 0.047033274355083865,
            dl: 555.1142519779797,
            h_hot: 93598.19598311755,
            s_hot: -559.6368858662341,
            h_liq: -589899.8260538473,
            s_liq: -3049.5582878800838,
            psat: 1102122.2611265087,
        },
        Row {
            fluid: "ethane",
            model: "SRK",
            tv: 290.054,
            pv: 1461660.0,
            tl: 213.724,
            pl: 4872200.0,
            th: 366.384,
            ph: 2436100.0,
            ts: 244.256,
            zv: 0.8688150759617281,
            zl: 0.1677720726231861,
            vv: 0.04767169590907287,
            dl: 491.4183699355746,
            h_hot: 95352.89203344357,
            s_hot: -559.1002539528042,
            h_liq: -596438.8260582753,
            s_liq: -3086.1621105799372,
            psat: 1112002.787650746,
        },
        Row {
            fluid: "propane",
            model: "PR",
            tv: 351.3955,
            pv: 1275360.0,
            tl: 258.923,
            pl: 4251200.0,
            th: 443.868,
            ph: 2125600.0,
            ts: 295.912,
            zv: 0.8563878736877646,
            zl: 0.1470796789845814,
            vv: 0.04448965657591393,
            dl: 592.0564175981413,
            h_hot: 262479.77557541267,
            s_hot: 167.11416699369306,
            h_liq: -464056.59198220825,
            s_liq: -1994.0704214718226,
            psat: 898383.230642454,
        },
        Row {
            fluid: "propane",
            model: "SRK",
            tv: 351.3955,
            pv: 1275360.0,
            tl: 258.923,
            pl: 4251200.0,
            th: 443.868,
            ph: 2125600.0,
            ts: 295.912,
            zv: 0.8680465984669401,
            zl: 0.1661689062363942,
            vv: 0.04509533150134817,
            dl: 524.0418909492936,
            h_hot: 263967.3535705602,
            s_hot: 167.58476973093735,
            h_liq: -469354.44926243654,
            s_liq: -2018.926799360416,
            psat: 908089.309144719,
        },
        Row {
            fluid: "nbutane",
            model: "PR",
            tv: 403.8735,
            pv: 1138800.0,
            tl: 297.591,
            pl: 3796000.0,
            th: 510.156,
            ph: 1898000.0,
            ts: 340.104,
            zv: 0.8556742825772023,
            zl: 0.14586653965890356,
            vv: 0.04341027547832282,
            dl: 611.3143177684061,
            h_hot: 431839.2209478854,
            s_hot: 683.0246559314114,
            h_liq: -371083.1397771703,
            s_liq: -1381.4418136076488,
            psat: 754368.5093834336,
        },
        Row {
            fluid: "nbutane",
            model: "SRK",
            tv: 403.8735,
            pv: 1138800.0,
            tl: 297.591,
            pl: 3796000.0,
            th: 510.156,
            ph: 1898000.0,
            ts: 340.104,
            zv: 0.8673493523703015,
            zl: 0.1648052558230973,
            vv: 0.04400257795400386,
            dl: 541.0646871149338,
            h_hot: 433162.94148557173,
            s_hot: 683.4446685610728,
            h_liq: -375755.1662198855,
            s_liq: -1400.5773763875472,
            psat: 763426.4825477854,
        },
        Row {
            fluid: "water",
            model: "PR",
            tv: 614.7412,
            pv: 6619200.0,
            tl: 452.9672,
            pl: 22064000.0,
            th: 776.5152,
            ph: 11032000.0,
            ts: 517.6768,
            zv: 0.8535985897809213,
            zl: 0.14273346174518264,
            vv: 0.036588147778028886,
            dl: 739.4209921210725,
            h_hot: 810734.1604680332,
            s_hot: -420.74710592686347,
            h_liq: -1807430.2519124458,
            s_liq: -4946.547714188361,
            psat: 3663856.570394032,
        },
        Row {
            fluid: "water",
            model: "SRK",
            tv: 614.7412,
            pv: 6619200.0,
            tl: 452.9672,
            pl: 22064000.0,
            th: 776.5152,
            ph: 11032000.0,
            ts: 517.6768,
            zv: 0.8652926099919505,
            zl: 0.16121700440791842,
            vv: 0.03708939338073098,
            dl: 654.6463152575135,
            h_hot: 817533.3428096928,
            s_hot: -418.9435803426131,
            h_liq: -1835353.3916043502,
            s_liq: -5018.144216690093,
            psat: 3710792.8550105058,
        },
        Row {
            fluid: "ammonia",
            model: "PR",
            tv: 385.13,
            pv: 3399900.0,
            tl: 283.78,
            pl: 11333000.0,
            th: 486.48,
            ph: 5666500.0,
            ts: 324.32,
            zv: 0.854908620315687,
            zl: 0.1446465744348185,
            vv: 0.04727754737602062,
            dl: 565.5368588004602,
            h_hot: 337455.63240280683,
            s_hot: -999.531665446259,
            h_liq: -1303115.144960645,
            s_liq: -5526.218574703799,
            psat: 2108105.261309934,
        },
        Row {
            fluid: "ammonia",
            model: "SRK",
            tv: 385.13,
            pv: 3399900.0,
            tl: 283.78,
            pl: 11333000.0,
            th: 486.48,
            ph: 5666500.0,
            ts: 324.32,
            zv: 0.8665958261890687,
            zl: 0.16341985268128817,
            vv: 0.04792386490779153,
            dl: 500.56934943915013,
            h_hot: 341845.54793190776,
            s_hot: -997.8976401846204,
            h_liq: -1319017.2243107073,
            s_liq: -5593.945946152391,
            psat: 2135068.3104775553,
        },
    ];

    #[test]
    fn matches_the_java_oracle_across_every_fluid_and_model() {
        assert_eq!(ORACLE.len(), 18, "9 fluids x 2 models");
        for row in ORACLE {
            let tag = |q: &str| format!("{q}[{} {}]", row.fluid, row.model);
            assert_oracle(
                &tag("zv"),
                z(row.fluid, row.model, row.tv, row.pv, "vapor").unwrap(),
                row.zv,
            );
            assert_oracle(
                &tag("zl"),
                z(row.fluid, row.model, row.tl, row.pl, "liquid").unwrap(),
                row.zl,
            );
            assert_oracle(
                &tag("vv"),
                volume(row.fluid, row.model, row.tv, row.pv, "vapor").unwrap(),
                row.vv,
            );
            assert_oracle(
                &tag("dl"),
                density(row.fluid, row.model, row.tl, row.pl, "liquid").unwrap(),
                row.dl,
            );
            assert_oracle(
                &tag("h_hot"),
                enthalpy(row.fluid, row.model, row.th, row.ph, "vapor").unwrap(),
                row.h_hot,
            );
            assert_oracle(
                &tag("s_hot"),
                entropy(row.fluid, row.model, row.th, row.ph, "vapor").unwrap(),
                row.s_hot,
            );
            assert_oracle(
                &tag("h_liq"),
                enthalpy(row.fluid, row.model, row.tl, row.pl, "liquid").unwrap(),
                row.h_liq,
            );
            assert_oracle(
                &tag("s_liq"),
                entropy(row.fluid, row.model, row.tl, row.pl, "liquid").unwrap(),
                row.s_liq,
            );
            assert_oracle(
                &tag("psat"),
                saturation_pressure(row.fluid, row.model, row.ts).unwrap(),
                row.psat,
            );
        }
    }

    /// The repo's own staged fixture, `fixtures/corpus-pending/cubic-eos-properties`.
    ///
    /// Its golden was generated by the Java engine before this port existed, so
    /// it is untouched third-party ground truth. Matching it means the fixture
    /// becomes promotable to `fixtures/corpus/` the moment `eval.rs` stops
    /// listing the `eos_*` family as unported.
    #[test]
    fn matches_the_staged_cubic_eos_properties_fixture() {
        let (t, p) = (320.0, 6_000_000.0);
        assert_oracle(
            "Z",
            z("co2", "PR", t, p, "vapor").unwrap(),
            0.6950257605383134,
        );
        assert_oracle(
            "rho",
            density("co2", "PR", t, p, "vapor").unwrap(),
            142.796497070525,
        );
        assert_oracle(
            "v",
            volume("co2", "PR", t, p, "vapor").unwrap(),
            0.007002972905603668,
        );
        assert_oracle(
            "h",
            enthalpy("co2", "PR", t, p, "vapor").unwrap(),
            -45003.83281558031,
        );
        assert_oracle(
            "Psat_300",
            saturation_pressure("co2", "PR", 300.0).unwrap(),
            6726910.38265417,
        );
    }

    /// A second, independent oracle document — spot values covering the
    /// `eos_pressure` path and the alias route, which the sweep does not.
    #[test]
    fn matches_the_java_oracle_on_the_spot_probe() {
        assert_oracle(
            "z_co2_pr",
            z("co2", "PR", 320.0, 4e6, "vapor").unwrap(),
            0.8087901683055375,
        );
        assert_oracle(
            "z_co2_srk",
            z("co2", "SRK", 320.0, 4e6, "vapor").unwrap(),
            0.8269502014519656,
        );
        assert_oracle(
            "z_n2_pr",
            z("nitrogen", "PR", 400.0, 1e5, "vapor").unwrap(),
            1.0001348038033382,
        );
        assert_oracle(
            "v_co2_pr",
            volume("co2", "PR", 320.0, 5e6, "vapor").unwrap(),
            0.0091181030578959,
        );
        assert_oracle(
            "rho_co2_pr",
            density("co2", "PR", 350.0, 5e6, "vapor").unwrap(),
            91.19221675969554,
        );
        assert_oracle(
            "h_co2_pr",
            enthalpy("co2", "PR", 350.0, 1e6, "vapor").unwrap(),
            37694.99149193417,
        );
        assert_oracle(
            "s_co2_pr",
            entropy("co2", "PR", 350.0, 1e6, "vapor").unwrap(),
            -310.5748308656568,
        );
        assert_oracle(
            "p_from_v",
            pressure("co2", "PR", 320.0, 0.011).unwrap(),
            4344014.539635953,
        );
        assert_oracle(
            "psat_prop",
            saturation_pressure("propane", "PR", 300.0).unwrap(),
            997556.2469315403,
        );
        assert_oracle(
            "psat_water_srk",
            saturation_pressure("water", "SRK", 400.0).unwrap(),
            233890.4225849489,
        );
        assert_oracle(
            "zl_prop",
            z("propane", "PR", 300.0, 1e6, "liquid").unwrap(),
            0.03475700877057791,
        );
        assert_oracle(
            "h_nh3",
            enthalpy("ammonia", "SRK", 300.0, 5e5, "vapor").unwrap(),
            -12826.058838730052,
        );
        // Alias route: r717 -> ammonia, identical result.
        assert_oracle(
            "s_nh3",
            entropy("r717", "SRK", 300.0, 5e5, "vapor").unwrap(),
            -809.5159048504212,
        );
    }
}
