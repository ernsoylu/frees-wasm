//! Phase-split `(P, h)` property tables — the structure the flash surface
//! actually has, so a global interpolant never smears the vapour dome.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/SaturationSplitTable.java`
//! (283 LOC), plus the on-disk bundle format the browser build needs.
//!
//! # The split
//!
//! * **Saturation lines** (1-D in `ln P`): `Tsat`, `h_f`, `h_g`, `v_f`,
//!   `v_fg`, `s_f`, `s_fg` — cubic-Hermite interpolated with central-difference
//!   slopes.
//! * **Two-phase region**: *exact* mixture relations on those lines —
//!   `T = Tsat(P)`, `v = v_f + x·v_fg`, `s = s_f + x·s_fg` with
//!   `x = (h − h_f)/(h_g − h_f)`. No 2-D fit crosses the dome, ever.
//! * **Single-phase regions**: bicubic tables ([`super::phtable`]) in
//!   *dome-following* coordinates — vapour over `(P, h − h_g(P))`, liquid over
//!   `(P, h_f(P) − h)` — where the surface is smooth, so a coarse grid is
//!   accurate.
//!
//! Coverage is honest. Pressures outside the served band (up to just inside
//! `0.75·p_crit` — the fit extends further so the serve edge sits on interior
//! cells), superheat or subcooling deeper than the transformed rectangles, and
//! the low-pressure band where the liquid sliver is thinner than the grid all
//! return [`None`]. The Java caller then makes a direct native call; the
//! browser has no such fallback, so an uncovered point must surface as a
//! refusal, not a guess.
//!
//! # What this module adds over the Java
//!
//! [`SaturationSplitTable::eval`] returns the value **and both analytic
//! partials** (`∂/∂P`, `∂/∂h`), by differentiating exactly the expressions
//! [`value`](SaturationSplitTable::value) evaluates: the saturation-line
//! Hermite in `ln P`, the mixture relations, and the chain rule through the
//! dome-following coordinate. That is the whole point of decision D1 — Newton
//! gets analytic Jacobian entries instead of finite differences — and the Java
//! left it on the table because its callers only ever asked for values.
//!
//! ---
//!
//! # On-disk format: the `FREESSP1` bundle
//!
//! One bundle holds one fluid. It is a fixed header, then the eight saturation
//! lines, then three or six [`FREESPH1`](super::phtable) property-table
//! sections written back to back. Little-endian, no padding, no alignment
//! requirement — the same conventions as the section format, which is
//! documented in full at the top of [`super::phtable`].
//!
//! ```text
//! offset  size            field
//! ------  --------------  -----------------------------------------------
//!      0               8  magic, the ASCII bytes "FREESSP1"
//!      8               1  kind          = 0x02 (split-table bundle)
//!      9               1  bundle_flags  bit0 = liquid pieces present
//!                                       bits 1..7 reserved, must be 0
//!     10               2  reserved, must be 0
//!     12               4  n_sat : u32          saturation samples, >= 2
//!     16               4  fluid_name_len : u32 bytes of UTF-8 name
//!     20               8  p_min         : f64  [Pa]
//!     28               8  p_max         : f64  [Pa]
//!     36               8  p_serve_max   : f64  [Pa]
//!     44               8  p_liquid_min  : f64  [Pa] (+inf = never served)
//!     52               8  dh_vapor_max  : f64  [J/kg]
//!     60               8  dh_liquid_max : f64  [J/kg]
//!     68  fluid_name_len  fluid name, UTF-8
//!      .       8 * n_sat  log_p : f64  ln(P/Pa), STRICTLY INCREASING
//!      .       8 * n_sat  tsat  : f64  [K]
//!      .       8 * n_sat  hf    : f64  [J/kg]      saturated liquid enthalpy
//!      .       8 * n_sat  hg    : f64  [J/kg]      saturated vapour enthalpy
//!      .       8 * n_sat  vf    : f64  [m^3/kg]    saturated liquid volume
//!      .       8 * n_sat  vfg   : f64  [m^3/kg]    v_g − v_f
//!      .       8 * n_sat  sf    : f64  [J/(kg-K)]  saturated liquid entropy
//!      .       8 * n_sat  sfg   : f64  [J/(kg-K)]  s_g − s_f
//!      .               .  FREESPH1 section — vapour, ValueKind::Temperature
//!      .               .  FREESPH1 section — vapour, ValueKind::Density
//!      .               .  FREESPH1 section — vapour, ValueKind::Entropy
//!      .               .  [iff bit0] liquid Temperature / Density / Entropy
//! ```
//!
//! The section order is fixed: **T, then Dmass, then Smass**, vapour before
//! liquid. `decode` checks each section's declared [`ValueKind`] against that
//! order and refuses a bundle whose sections are permuted, so a generator
//! cannot silently emit density where temperature is expected.
//!
//! Every vapour section must declare `axis_h_kind = `[`AxisKind::Superheat`]
//! and every liquid section [`AxisKind::Subcooling`]; both must declare
//! `axis_p_kind = `[`AxisKind::Pressure`]. These are the coordinates
//! [`SaturationSplitTable::value`] transforms the query into, and a mismatch
//! would produce plausible numbers from the wrong surface.
//!
//! ## What a generator must guarantee
//!
//! * `log_p[i] = ln(p_i)` for a strictly increasing pressure sweep;
//!   `p_min = exp(log_p[0])` and `p_max = exp(log_p[n_sat-1])`.
//! * `p_min ≤ p_serve_max ≤ p_max`, and `h_g[i] > h_f[i]` at every sample —
//!   otherwise the two-phase quality is undefined and `decode` rejects the
//!   bundle.
//! * The vapour tables span `Δh ∈ [0, dh_vapor_max/0.9]` and the liquid tables
//!   `Δh ∈ [0, dh_liquid_max/0.9]`; service stops at the `*_max` values so the
//!   served edge sits on interior cells rather than on the one-sided stencils
//!   at the grid boundary. [`SaturationSplitTable::build`] does this, and a
//!   generator that produces bundles by other means should too.
//! * Every sample is finite. Non-finite nodes inside a section are back-filled
//!   by the section reader (and marked [`NODE_BACKFILLED`]); non-finite
//!   *saturation-line* entries are rejected outright, because every region's
//!   geometry is derived from them.
//!
//! ## What this module deliberately does not do
//!
//! The Java gates a fluid before serving it: `PhTableRegistry` samples 300
//! fixed-seed off-grid points, requires a worst-case relative error under
//! `1e-4`, and falls back to the native library for anything that fails. That
//! gate belongs with whatever wires these tables into `eval.rs`, and the
//! browser has no native fallback to fall back *to* — so the honest browser
//! behaviour is: serve where covered, refuse where not, and state the tabulated
//! error bound in the generated bundle's provenance. This module supplies the
//! coverage answer ([`SaturationSplitTable::region`]); it does not decide
//! policy.

// The saturation-line kernels index parallel arrays by a shared loop variable,
// mirroring the Java arrays they are transcribed from; iterator rewrites
// obscure that correspondence.
#![allow(clippy::needless_range_loop)]
// Float guards written `!(x > 0.0)` are negated on purpose: the negation makes
// NaN take the reject branch, which `x <= 0.0` would not. This matches the
// Java guards being ported.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::diag::{FreesError, Result};
use crate::props::phtable::{AxisKind, PhPropertyTable, Value, ValueKind};

/// Saturation-line samples, log-spaced in pressure. The Java's `SAT_POINTS`.
pub const SAT_POINTS: usize = 256;
/// Pressure nodes in each single-phase bicubic. The Java's `GRID_P`.
pub const GRID_P: usize = 96;
/// Dome-following `Δh` nodes in each single-phase bicubic. The Java's `GRID_DH`.
pub const GRID_DH: usize = 48;
/// Liquid coverage needs at least this much saturated-to-cold headroom [J/kg].
pub const MIN_LIQUID_DEPTH: f64 = 2.0e4;

/// Bundle magic: ASCII `FREESSP1`. The trailing digit is the format version.
pub const BUNDLE_MAGIC: &[u8; 8] = b"FREESSP1";
/// `kind` byte identifying a split-table bundle.
pub const BUNDLE_KIND: u8 = 0x02;
/// Fixed header length in bytes, before the fluid name.
pub const BUNDLE_HEADER_LEN: usize = 68;
/// `bundle_flags` bit 0 — the three liquid sections are present.
pub const BUNDLE_HAS_LIQUID: u8 = 0x01;

/// Generated-file magic: ASCII `FRPHTAB1`, the format `tools/table-gen` writes.
///
/// This is **not** [`BUNDLE_MAGIC`]. `FREESSP1` is this port's own
/// round-trippable serialisation of a table it built itself; `FRPHTAB1` is the
/// build-time artifact produced offline by native CoolProp, documented in
/// `tools/table-gen/README.md`. They carry the same physics at different
/// resolutions and, for the liquid piece, in different coordinates — see
/// [`LiquidCoord`].
pub const GENERATED_MAGIC: &[u8; 8] = b"FRPHTAB1";
/// Fixed header length of a `FRPHTAB1` file, before the string block.
pub const GENERATED_HEADER_LEN: usize = 136;
/// `FRPHTAB1` flags bit 0 — the liquid piece is present.
pub const GENERATED_HAS_LIQUID: u8 = 0x01;
/// `FRPHTAB1` flags bit 1 — the liquid depth axis is normalized.
pub const GENERATED_LIQUID_NORMALIZED: u8 = 0x02;

/// How a table measures depth into the subcooled-liquid sliver.
///
/// **This is the one place the port deliberately diverges from the Java**, and
/// the divergence is additive: both modes are implemented, the mode is a flag
/// on the artifact, and [`SaturationSplitTable::build`] — the line-for-line port
/// of the Java constructor — still produces [`Absolute`](LiquidCoord::Absolute)
/// and nothing else.
///
/// `SaturationSplitTable.java` measures liquid depth as `h_f(P) − h` and caps it
/// at one depth valid at *every* served pressure. That cap is set by the
/// thinnest sliver, which sits at low pressure, so at high pressure the
/// rectangle covers a small fraction of the liquid that exists — the reference
/// does not care because it falls through to a native CoolProp call, and this
/// port cannot. Decision D1 (`docs/decisions/0001-property-backend.md`) measured
/// the consequence: `rankine-cycle`'s 8 MPa pump-exit state is sixteen times
/// outside the absolute rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidCoord {
    /// `y = h_f(P) − h` [J/kg], capped at a single depth. The Java's.
    Absolute,
    /// `y = (h_f(P) − h) / (h_f(P) − h_cold(P))` ∈ [0, 1], which follows the
    /// sliver at every pressure at identical byte cost.
    Normalized,
}

/// The three tabulated outputs, in the order the bundle stores them.
///
/// These are the Java's `PhTableRegistry.TABLE_OUTPUTS`: flash-heavy smooth
/// quantities. Quality is excluded (piecewise, and −1 outside the dome breaks
/// interpolation) and transport properties are rare enough to stay direct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Temperature [K] — CoolProp `"T"`.
    Temperature,
    /// Mass density [kg/m³] — CoolProp `"Dmass"`.
    Density,
    /// Specific entropy [J/(kg·K)] — CoolProp `"Smass"`.
    Entropy,
}

impl Output {
    /// The CoolProp output key, exactly as the Java `switch` spells it.
    pub fn key(self) -> &'static str {
        match self {
            Output::Temperature => "T",
            Output::Density => "Dmass",
            Output::Entropy => "Smass",
        }
    }

    /// Parses a CoolProp output key. Anything else is not tabulated.
    pub fn from_key(key: &str) -> Option<Output> {
        match key {
            "T" => Some(Output::Temperature),
            "Dmass" => Some(Output::Density),
            "Smass" => Some(Output::Entropy),
            _ => None,
        }
    }

    fn value_kind(self) -> ValueKind {
        match self {
            Output::Temperature => ValueKind::Temperature,
            Output::Density => ValueKind::Density,
            Output::Entropy => ValueKind::Entropy,
        }
    }

    /// Storage order within a bundle: T, Dmass, Smass. A generator must write
    /// its sections in this order; [`SaturationSplitTable::decode`] checks it.
    pub const ALL: [Output; 3] = [Output::Temperature, Output::Density, Output::Entropy];
}

/// Which branch of the split served — or would have served — a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// Inside the dome; exact mixture relations on the saturation lines.
    TwoPhase,
    /// Superheated vapour; bicubic over `(P, h − h_g(P))`.
    Vapor,
    /// Subcooled liquid; bicubic over `(P, h_f(P) − h)`.
    Liquid,
}

/// The second constraint of a property query, alongside pressure.
///
/// These are the three CoolProp input pairs `SaturationSplitTable`'s
/// constructor uses: `(P, Q)`, `(P, T)` and `(P, Hmass)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum At {
    /// Vapour quality [-] — `0.0` saturated liquid, `1.0` saturated vapour.
    Quality(f64),
    /// Temperature [K].
    Temperature(f64),
    /// Specific enthalpy [J/kg].
    Enthalpy(f64),
}

/// Whatever can answer `output(P, ·)` for one fluid while a bundle is being
/// built — natively, CoolProp behind `propsSIOrNaN`.
///
/// Non-finite means "cannot serve this point": the build treats that as fatal
/// on the saturation lines (their geometry drives everything else) and as a
/// back-fillable hole inside the single-phase rectangles, exactly as the Java
/// does.
///
/// Implemented for any `FnMut(&str, f64, At) -> f64`, so a generator can pass a
/// closure. Note this is a **build-time** convenience only: the contract
/// between the generator and the wasm build is the on-disk bundle, not this
/// trait.
pub trait PropSource {
    /// `output` at pressure `p` [Pa] and the second constraint `at`.
    fn prop(&mut self, output: &str, p: f64, at: At) -> f64;
}

impl<F> PropSource for F
where
    F: FnMut(&str, f64, At) -> f64,
{
    fn prop(&mut self, output: &str, p: f64, at: At) -> f64 {
        self(output, p, at)
    }
}

/// One fluid's three single-phase bicubics, in bundle order.
#[derive(Debug, Clone, PartialEq)]
struct Pieces {
    t: PhPropertyTable,
    d: PhPropertyTable,
    s: PhPropertyTable,
}

impl Pieces {
    fn get(&self, output: Output) -> &PhPropertyTable {
        match output {
            Output::Temperature => &self.t,
            Output::Density => &self.d,
            Output::Entropy => &self.s,
        }
    }

    fn each(&self) -> [&PhPropertyTable; 3] {
        [&self.t, &self.d, &self.s]
    }
}

/// Phase-split `(P, h)` property tables for one fluid.
#[derive(Debug, Clone, PartialEq)]
pub struct SaturationSplitTable {
    fluid: String,
    /// `ln(P/Pa)` at the saturation samples, strictly increasing.
    log_p: Vec<f64>,
    tsat: Vec<f64>,
    hf: Vec<f64>,
    hg: Vec<f64>,
    vf: Vec<f64>,
    vfg: Vec<f64>,
    sf: Vec<f64>,
    sfg: Vec<f64>,
    /// `h(P, T_low)` at the saturation samples — the cold end of the liquid
    /// sliver. Empty for an [`Absolute`](LiquidCoord::Absolute) table, which
    /// never needs it; carried by every `FRPHTAB1` artifact so a reader can
    /// convert between the two coordinates.
    h_cold: Vec<f64>,
    p_min: f64,
    p_max: f64,
    p_serve_max: f64,
    dh_vapor_max: f64,
    dh_liquid_max: f64,
    p_liquid_min: f64,
    liquid_coord: LiquidCoord,
    /// The fluid's own critical and triple constants, as CoolProp reported them
    /// to the generator. `None` for a table this port built itself, whose
    /// [`build`](SaturationSplitTable::build) never receives them.
    ///
    /// These are **not** grid geometry: `p_max` is `0.75·p_crit` by
    /// construction, and answering `P_crit` from it would be a wrong number.
    /// These four are the oracle's, verbatim.
    constants: Option<FluidConstants>,
    vapor: Pieces,
    liquid: Option<Pieces>,
}

/// A fluid's critical and triple-point constants, carried by a `FRPHTAB1` file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluidConstants {
    /// Critical pressure [Pa].
    pub p_crit: f64,
    /// Critical temperature [K].
    pub t_crit: f64,
    /// Triple-point pressure [Pa].
    pub p_triple: f64,
    /// Triple-point temperature [K].
    pub t_triple: f64,
}

fn bad_bundle(msg: impl Into<String>) -> FreesError {
    FreesError::property(format!("(P,h) split table: {}", msg.into()))
}

impl SaturationSplitTable {
    /// Builds the split tables for `fluid` by sampling `source`.
    ///
    /// * `h_top` — the enthalpy ceiling the vapour rectangle must fit under
    ///   (the Java's caller uses `Hmass` at `0.05·p_crit` and
    ///   `min(Tmax, 1.3·Tcrit)`).
    /// * `t_low` — the cold end used to size the liquid rectangle (the Java's
    ///   caller uses `Tmin + 1`).
    /// * `p_triple`, `p_crit` — the Java constructor reads these itself through
    ///   `Props1SI`; here they are explicit so [`PropSource`] stays a single
    ///   method.
    ///
    /// Every structural failure is an error, matching the Java constructor's
    /// `IllegalStateException`/`IllegalArgumentException` throws — the caller
    /// gates.
    ///
    /// Port of the `SaturationSplitTable(String, double, double)` constructor.
    pub fn build(
        fluid: &str,
        h_top: f64,
        t_low: f64,
        p_triple: f64,
        p_crit: f64,
        mut source: impl PropSource,
    ) -> Result<SaturationSplitTable> {
        let p_min = (p_triple * 1.2).max(p_crit * 1e-4);
        let p_max = p_crit * 0.75;
        if !(p_min > 0.0) || !(p_max > p_min) {
            return Err(bad_bundle("no subcritical band"));
        }

        let mut log_p = vec![0.0f64; SAT_POINTS];
        let mut tsat = vec![0.0f64; SAT_POINTS];
        let mut hf = vec![0.0f64; SAT_POINTS];
        let mut hg = vec![0.0f64; SAT_POINTS];
        let mut vf = vec![0.0f64; SAT_POINTS];
        let mut vfg = vec![0.0f64; SAT_POINTS];
        let mut sf = vec![0.0f64; SAT_POINTS];
        let mut sfg = vec![0.0f64; SAT_POINTS];
        let log_min = p_min.ln();
        let log_max = p_max.ln();
        for i in 0..SAT_POINTS {
            let p = (log_min + (log_max - log_min) * i as f64 / (SAT_POINTS as f64 - 1.0)).exp();
            log_p[i] = p.ln();
            tsat[i] = required(&mut source, "T", p, At::Quality(0.0))?;
            hf[i] = required(&mut source, "Hmass", p, At::Quality(0.0))?;
            hg[i] = required(&mut source, "Hmass", p, At::Quality(1.0))?;
            let df = required(&mut source, "Dmass", p, At::Quality(0.0))?;
            let dg = required(&mut source, "Dmass", p, At::Quality(1.0))?;
            vf[i] = 1.0 / df;
            vfg[i] = 1.0 / dg - 1.0 / df;
            sf[i] = required(&mut source, "Smass", p, At::Quality(0.0))?;
            sfg[i] = required(&mut source, "Smass", p, At::Quality(1.0))? - sf[i];
        }
        validate_sat_lines(&log_p, &hf, &hg, &vf, &vfg)?;

        // Vapour rectangle: superheat depth available at every pressure.
        let mut hg_max = 0.0f64;
        for v in &hg {
            hg_max = hg_max.max(*v);
        }
        let dh_vapor_max = 0.9 * (h_top - hg_max);
        if !(dh_vapor_max > 0.0) {
            return Err(bad_bundle("no superheat band under h_top"));
        }

        // Liquid rectangle: start where the liquid sliver is deep enough.
        let mut liquid_start: Option<usize> = None;
        let mut depth = f64::INFINITY;
        for i in 0..SAT_POINTS {
            if tsat[i] < t_low + 5.0 {
                continue;
            }
            let p = log_p[i].exp();
            let h_cold = source.prop("Hmass", p, At::Temperature(t_low));
            if !h_cold.is_finite() {
                continue;
            }
            let d = hf[i] - h_cold;
            if liquid_start.is_none() && d >= MIN_LIQUID_DEPTH {
                liquid_start = Some(i);
            }
            if liquid_start.is_some() {
                depth = depth.min(d);
            }
        }
        let (p_liquid_min, dh_liquid_max) = match liquid_start {
            Some(i) if depth.is_finite() => (log_p[i].exp(), 0.9 * depth),
            // Liquid is never served.
            _ => (f64::INFINITY, 0.0),
        };

        // The 2-D fits extend to p_max, but service stops short of it: the last
        // cells before a grid edge use one-sided stencils and carry the worst
        // error (empirically the near-critical vapour band), so the served
        // region keeps an anchored margin of fitted-but-unserved cells.
        let p_serve_max = p_max * 0.95;

        let p_grid = logspace(p_min, p_max, GRID_P)?;
        // Quadratic dh spacing: curvature concentrates at the dome edge
        // (dh -> 0), so the grid is densest exactly there.
        let dh_vapor = squarespace(dh_vapor_max / 0.9, GRID_DH)?;

        let sat = SatLines {
            log_p: &log_p,
            hf: &hf,
            hg: &hg,
        };
        let vapor = Pieces {
            t: build_piece(
                Output::Temperature,
                &p_grid,
                &dh_vapor,
                true,
                sat,
                &mut source,
            )?,
            d: build_piece(Output::Density, &p_grid, &dh_vapor, true, sat, &mut source)?,
            s: build_piece(Output::Entropy, &p_grid, &dh_vapor, true, sat, &mut source)?,
        };
        let liquid = if dh_liquid_max > 0.0 {
            let p_grid_l = logspace(p_liquid_min, p_max, GRID_P)?;
            let dh_liquid = squarespace(dh_liquid_max / 0.9, GRID_DH)?;
            Some(Pieces {
                t: build_piece(
                    Output::Temperature,
                    &p_grid_l,
                    &dh_liquid,
                    false,
                    sat,
                    &mut source,
                )?,
                d: build_piece(
                    Output::Density,
                    &p_grid_l,
                    &dh_liquid,
                    false,
                    sat,
                    &mut source,
                )?,
                s: build_piece(
                    Output::Entropy,
                    &p_grid_l,
                    &dh_liquid,
                    false,
                    sat,
                    &mut source,
                )?,
            })
        } else {
            None
        };

        Ok(SaturationSplitTable {
            fluid: fluid.to_string(),
            log_p,
            tsat,
            hf,
            hg,
            vf,
            vfg,
            sf,
            sfg,
            h_cold: Vec::new(),
            p_min,
            p_max,
            p_serve_max,
            dh_vapor_max,
            dh_liquid_max,
            p_liquid_min,
            liquid_coord: LiquidCoord::Absolute,
            constants: None,
            vapor,
            liquid,
        })
    }

    /// `output(P, h)` from the split tables, or [`None`] when the point lies
    /// outside covered territory.
    ///
    /// Port of `SaturationSplitTable.value`.
    pub fn value(&self, output: Output, p: f64, h: f64) -> Option<f64> {
        self.eval(output, p, h).map(|v| v.value)
    }

    /// As [`value`](Self::value), keyed by the CoolProp output string. Keys
    /// other than `"T"`, `"Dmass"` and `"Smass"` are not tabulated.
    pub fn value_by_key(&self, output_key: &str, p: f64, h: f64) -> Option<f64> {
        self.value(Output::from_key(output_key)?, p, h)
    }

    /// `output(P, h)` **and both analytic partials**, or [`None`] outside
    /// covered territory.
    ///
    /// The partials are the exact derivatives of the same expressions
    /// [`value`](Self::value) evaluates, so they are consistent with it to the
    /// last bit of the algebra — not a finite difference of it.
    pub fn eval(&self, output: Output, p: f64, h: f64) -> Option<Value> {
        let v = self.eval_covered(output, p, h)?;
        // **A `Some` from this method is a promise that the state was served.**
        //
        // The Java returns `Double.NaN` for "not covered" and `PhTableRegistry`
        // tests `isNaN` at every call site; this port replaced that convention
        // with `Option`, so a `Some(NaN)` would be the two conventions crossed —
        // a decline wearing the costume of an answer, which the solver would
        // then propagate into a `NaN` residual and report as convergence.
        //
        // Two ways it happens, both found by
        // `tests/props_robustness.rs::table_lookups_exactly_on_and_just_outside_every_grid_edge_are_bounded`:
        // a non-finite `p` or `h` defeats every range check below (all
        // comparisons against NaN are false, so the point falls through to the
        // liquid branch and interpolates at NaN), and a Hermite segment at an
        // infinite coordinate produces `inf - inf`.
        (v.value.is_finite() && v.d_value_d_p.is_finite() && v.d_value_d_h.is_finite()).then_some(v)
    }

    /// [`eval`](Self::eval) before the finiteness screen — the coverage logic
    /// alone.
    fn eval_covered(&self, output: Output, p: f64, h: f64) -> Option<Value> {
        if !p.is_finite() || !h.is_finite() {
            return None;
        }
        if p < self.p_min || p > self.p_serve_max {
            return None;
        }
        let (hfv, dhf_dp) = self.interp(&self.hf, p);
        let (hgv, dhg_dp) = self.interp(&self.hg, p);

        if h >= hfv && h <= hgv {
            // Two-phase: exact mixture relations on the saturation lines.
            let den = hgv - hfv;
            let x = (h - hfv) / den;
            // dx/dP from the quotient rule; dx/dh = 1/den.
            let dx_dp = (-dhf_dp * den - (h - hfv) * (dhg_dp - dhf_dp)) / (den * den);
            let dx_dh = 1.0 / den;
            return Some(match output {
                Output::Temperature => {
                    let (t, dt_dp) = self.interp(&self.tsat, p);
                    Value {
                        value: t,
                        d_value_d_p: dt_dp,
                        d_value_d_h: 0.0,
                    }
                }
                Output::Density => {
                    let (vfv, dvf_dp) = self.interp(&self.vf, p);
                    let (vfgv, dvfg_dp) = self.interp(&self.vfg, p);
                    let v = vfv + x * vfgv;
                    let dv_dp = dvf_dp + x * dvfg_dp + vfgv * dx_dp;
                    let dv_dh = vfgv * dx_dh;
                    let rho = 1.0 / v;
                    Value {
                        value: rho,
                        d_value_d_p: -rho * rho * dv_dp,
                        d_value_d_h: -rho * rho * dv_dh,
                    }
                }
                Output::Entropy => {
                    let (sfv, dsf_dp) = self.interp(&self.sf, p);
                    let (sfgv, dsfg_dp) = self.interp(&self.sfg, p);
                    Value {
                        value: sfv + x * sfgv,
                        d_value_d_p: dsf_dp + x * dsfg_dp + sfgv * dx_dp,
                        d_value_d_h: sfgv * dx_dh,
                    }
                }
            });
        }

        if h > hgv {
            // Superheated vapour in dome-following coordinates.
            let dh = h - hgv;
            if dh > self.dh_vapor_max {
                return None;
            }
            let v = self.vapor.get(output).eval(p, dh);
            // d(dh)/dP = -h_g'(P); d(dh)/dh = +1.
            return Some(Value {
                value: v.value,
                d_value_d_p: v.d_value_d_p - v.d_value_d_h * dhg_dp,
                d_value_d_h: v.d_value_d_h,
            });
        }

        // Subcooled liquid.
        let liquid = self.liquid.as_ref()?;
        if p < self.p_liquid_min {
            return None;
        }
        let (dh, ddh_dp, ddh_dh) = self.liquid_depth(p, h, hfv, dhf_dp);
        if dh > self.dh_liquid_max {
            return None;
        }
        let v = liquid.get(output).eval(p, dh);
        Some(Value {
            value: v.value,
            d_value_d_p: v.d_value_d_p + v.d_value_d_h * ddh_dp,
            d_value_d_h: v.d_value_d_h * ddh_dh,
        })
    }

    /// Which branch serves `(P, h)`, or [`None`] when nothing does.
    ///
    /// Cheap coverage answer for a caller that must decide *before* evaluating
    /// whether it can serve the point at all.
    pub fn region(&self, p: f64, h: f64) -> Option<Region> {
        // Same screen as `eval`, and for the same reason: NaN loses every
        // comparison below, so without it a NaN state reports as `Liquid`.
        if !p.is_finite() || !h.is_finite() {
            return None;
        }
        if p < self.p_min || p > self.p_serve_max {
            return None;
        }
        let (hfv, dhf_dp) = self.interp(&self.hf, p);
        let (hgv, _) = self.interp(&self.hg, p);
        if h >= hfv && h <= hgv {
            return Some(Region::TwoPhase);
        }
        if h > hgv {
            return if h - hgv > self.dh_vapor_max {
                None
            } else {
                Some(Region::Vapor)
            };
        }
        if self.liquid.is_none() || p < self.p_liquid_min {
            return None;
        }
        let (dh, _, _) = self.liquid_depth(p, h, hfv, dhf_dp);
        if dh > self.dh_liquid_max {
            return None;
        }
        Some(Region::Liquid)
    }

    /// The fluid this bundle describes.
    pub fn fluid(&self) -> &str {
        &self.fluid
    }

    /// Lowest tabulated pressure [Pa].
    pub fn p_min(&self) -> f64 {
        self.p_min
    }

    /// Highest **fitted** pressure [Pa] (`0.75·p_crit`).
    pub fn p_max(&self) -> f64 {
        self.p_max
    }

    /// Highest **served** pressure [Pa] (`0.95·p_max`).
    pub fn p_serve_max(&self) -> f64 {
        self.p_serve_max
    }

    /// Lowest pressure at which the liquid piece is served [Pa]; `+inf` when
    /// there is no liquid piece.
    pub fn p_liquid_min(&self) -> f64 {
        self.p_liquid_min
    }

    /// Deepest served superheat `h − h_g(P)` [J/kg].
    pub fn dh_vapor_max(&self) -> f64 {
        self.dh_vapor_max
    }

    /// Deepest served subcooling `h_f(P) − h` [J/kg]; `0` when liquid is never
    /// served.
    pub fn dh_liquid_max(&self) -> f64 {
        self.dh_liquid_max
    }

    /// Whether the bundle carries the three liquid pieces.
    pub fn has_liquid(&self) -> bool {
        self.liquid.is_some()
    }

    /// Saturation temperature [K] at `p`, from the interpolated line.
    pub fn tsat_at(&self, p: f64) -> f64 {
        self.interp(&self.tsat, p).0
    }

    /// Saturated-liquid enthalpy [J/kg] at `p`.
    pub fn hf_at(&self, p: f64) -> f64 {
        self.interp(&self.hf, p).0
    }

    /// Saturated-vapour enthalpy [J/kg] at `p`.
    pub fn hg_at(&self, p: f64) -> f64 {
        self.interp(&self.hg, p).0
    }

    /// The lowest enthalpy [J/kg] the liquid piece serves at `p`, or `h_f(P)`
    /// when there is no liquid piece.
    ///
    /// [`dh_liquid_max`](Self::dh_liquid_max) alone cannot answer this: it is
    /// [J/kg] for an [`Absolute`](LiquidCoord::Absolute) table and dimensionless
    /// for a [`Normalized`](LiquidCoord::Normalized) one, so a caller that
    /// subtracted it from `h_f` would be off by three orders of magnitude on
    /// half the tables in existence. Every inverse lookup goes through here.
    pub fn h_liquid_min_at(&self, p: f64) -> f64 {
        let hf = self.hf_at(p);
        if self.liquid.is_none() || p < self.p_liquid_min {
            return hf;
        }
        match self.liquid_coord {
            LiquidCoord::Absolute => hf - self.dh_liquid_max,
            LiquidCoord::Normalized => {
                hf - self.dh_liquid_max * (hf - self.interp(&self.h_cold, p).0)
            }
        }
    }

    /// Cubic-Hermite on the `ln P` saturation grid with central-FD slopes,
    /// returning the value and `d/dP`.
    ///
    /// Port of the private `SaturationSplitTable.interp`; the derivative is the
    /// analytic slope of that same Hermite segment, converted from `d/d(ln P)`
    /// by the chain rule. Outside the grid the value clamps, so the derivative
    /// there is zero.
    fn interp(&self, f: &[f64], p: f64) -> (f64, f64) {
        let x = p.ln();
        let n = self.log_p.len();
        let mut lo = 0usize;
        let mut hi = n - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.log_p[mid] <= x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let x0 = self.log_p[lo];
        let x1 = self.log_p[hi];
        let dx = x1 - x0;
        let raw = (x - x0) / dx;
        let t = raw.clamp(0.0, 1.0);
        let m0 = self.slope(f, lo);
        let m1 = self.slope(f, hi);
        let t2 = t * t;
        let t3 = t2 * t;
        let value = (2.0 * t3 - 3.0 * t2 + 1.0) * f[lo]
            + (t3 - 2.0 * t2 + t) * dx * m0
            + (-2.0 * t3 + 3.0 * t2) * f[hi]
            + (t3 - t2) * dx * m1;
        // dvalue/dt of the same segment; zero once the clamp is active, and
        // NaN for a NaN pressure (where the value is NaN too — reporting a
        // finite 0.0 slope for a NaN value would let a Newton step believe it).
        let d_value_d_p = if raw.is_nan() {
            f64::NAN
        } else if !(0.0..=1.0).contains(&raw) {
            0.0
        } else {
            let dv_dt = (6.0 * t2 - 6.0 * t) * f[lo]
                + (3.0 * t2 - 4.0 * t + 1.0) * dx * m0
                + (-6.0 * t2 + 6.0 * t) * f[hi]
                + (3.0 * t2 - 2.0 * t) * dx * m1;
            // dt/dx = 1/dx and dx/dP = 1/P.
            dv_dt / dx / p
        };
        (value, d_value_d_p)
    }

    /// Central-difference slope in `ln P`; one-sided at the ends.
    ///
    /// Port of the private `SaturationSplitTable.slope`.
    fn slope(&self, f: &[f64], i: usize) -> f64 {
        let n = self.log_p.len();
        if i == 0 {
            return (f[1] - f[0]) / (self.log_p[1] - self.log_p[0]);
        }
        if i == n - 1 {
            return (f[n - 1] - f[n - 2]) / (self.log_p[n - 1] - self.log_p[n - 2]);
        }
        (f[i + 1] - f[i - 1]) / (self.log_p[i + 1] - self.log_p[i - 1])
    }

    // -- the on-disk bundle -------------------------------------------------

    /// The exact encoded length of this bundle in bytes.
    pub fn encoded_len(&self) -> usize {
        let mut n = BUNDLE_HEADER_LEN + self.fluid.len() + 8 * 8 * self.log_p.len();
        for t in self.vapor.each() {
            n += t.encoded_len();
        }
        if let Some(liquid) = &self.liquid {
            for t in liquid.each() {
                n += t.encoded_len();
            }
        }
        n
    }

    /// Serialises the bundle as `FREESSP1` bytes (see the module docs).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        out.extend_from_slice(BUNDLE_MAGIC);
        out.push(BUNDLE_KIND);
        out.push(if self.liquid.is_some() {
            BUNDLE_HAS_LIQUID
        } else {
            0
        });
        out.extend_from_slice(&[0u8; 2]); // reserved
        out.extend_from_slice(&(self.log_p.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.fluid.len() as u32).to_le_bytes());
        for x in [
            self.p_min,
            self.p_max,
            self.p_serve_max,
            self.p_liquid_min,
            self.dh_vapor_max,
            self.dh_liquid_max,
        ] {
            out.extend_from_slice(&x.to_le_bytes());
        }
        debug_assert_eq!(out.len(), BUNDLE_HEADER_LEN);
        out.extend_from_slice(self.fluid.as_bytes());
        for line in [
            &self.log_p,
            &self.tsat,
            &self.hf,
            &self.hg,
            &self.vf,
            &self.vfg,
            &self.sf,
            &self.sfg,
        ] {
            for x in line {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        for t in self.vapor.each() {
            t.encode_into(&mut out);
        }
        if let Some(liquid) = &self.liquid {
            for t in liquid.each() {
                t.encode_into(&mut out);
            }
        }
        out
    }

    /// Reads a `FREESSP1` bundle.
    ///
    /// This is what the wasm build calls on an `include_bytes!` slice. Every
    /// structural promise the module docs make of a generator is checked here;
    /// a bundle that breaks one is refused rather than served.
    pub fn decode(bytes: &[u8]) -> Result<SaturationSplitTable> {
        if bytes.len() < BUNDLE_HEADER_LEN {
            return Err(bad_bundle(format!(
                "need at least {BUNDLE_HEADER_LEN} header bytes, found {}",
                bytes.len()
            )));
        }
        if &bytes[0..8] != BUNDLE_MAGIC {
            return Err(bad_bundle(
                "bad magic — this is not a FREESSP1 split-table bundle",
            ));
        }
        if bytes[8] != BUNDLE_KIND {
            return Err(bad_bundle(format!(
                "bundle kind {:#04x} is not a split table",
                bytes[8]
            )));
        }
        let flags = bytes[9];
        if flags & !BUNDLE_HAS_LIQUID != 0 {
            return Err(bad_bundle(format!(
                "bundle flags {flags:#04x} set bits this format version reserves"
            )));
        }
        if bytes[10..12] != [0, 0] {
            return Err(bad_bundle("reserved header bytes must be zero"));
        }
        let n_sat = read_u32(bytes, 12) as usize;
        let name_len = read_u32(bytes, 16) as usize;
        if n_sat < 2 {
            return Err(bad_bundle(format!(
                "saturation grid needs at least 2 samples, header says {n_sat}"
            )));
        }
        let p_min = read_f64(bytes, 20);
        let p_max = read_f64(bytes, 28);
        let p_serve_max = read_f64(bytes, 36);
        let p_liquid_min = read_f64(bytes, 44);
        let dh_vapor_max = read_f64(bytes, 52);
        let dh_liquid_max = read_f64(bytes, 60);

        let body = BUNDLE_HEADER_LEN
            .checked_add(name_len)
            .and_then(|n| n_sat.checked_mul(8 * 8).and_then(|m| n.checked_add(m)))
            .ok_or_else(|| bad_bundle("declared size overflows"))?;
        if bytes.len() < body {
            return Err(bad_bundle(format!(
                "bundle declares {body} bytes of header and saturation lines, {} present",
                bytes.len()
            )));
        }
        let fluid = std::str::from_utf8(&bytes[BUNDLE_HEADER_LEN..BUNDLE_HEADER_LEN + name_len])
            .map_err(|_| bad_bundle("fluid name is not valid UTF-8"))?
            .to_string();

        let mut at = BUNDLE_HEADER_LEN + name_len;
        let log_p = read_f64_block(bytes, &mut at, n_sat);
        let tsat = read_f64_block(bytes, &mut at, n_sat);
        let hf = read_f64_block(bytes, &mut at, n_sat);
        let hg = read_f64_block(bytes, &mut at, n_sat);
        let vf = read_f64_block(bytes, &mut at, n_sat);
        let vfg = read_f64_block(bytes, &mut at, n_sat);
        let sf = read_f64_block(bytes, &mut at, n_sat);
        let sfg = read_f64_block(bytes, &mut at, n_sat);
        validate_sat_lines(&log_p, &hf, &hg, &vf, &vfg)?;
        for (name, line) in [("tsat", &tsat), ("sf", &sf), ("sfg", &sfg)] {
            if let Some(bad) = line.iter().position(|x| !x.is_finite()) {
                return Err(bad_bundle(format!(
                    "saturation line {name} is non-finite at sample {bad}"
                )));
            }
        }
        if !(p_min > 0.0) || !(p_serve_max >= p_min) || !(p_max >= p_serve_max) {
            return Err(bad_bundle(format!(
                "pressure band must satisfy 0 < p_min ({p_min}) <= p_serve_max \
                 ({p_serve_max}) <= p_max ({p_max})"
            )));
        }

        let vapor = read_pieces(bytes, &mut at, AxisKind::Superheat, "vapour")?;
        let liquid = if flags & BUNDLE_HAS_LIQUID != 0 {
            Some(read_pieces(bytes, &mut at, AxisKind::Subcooling, "liquid")?)
        } else {
            None
        };
        if at != bytes.len() {
            return Err(bad_bundle(format!(
                "{} trailing bytes after the last section",
                bytes.len() - at
            )));
        }
        if liquid.is_none() && dh_liquid_max != 0.0 {
            return Err(bad_bundle(
                "bundle has no liquid sections but declares a non-zero dh_liquid_max",
            ));
        }

        Ok(SaturationSplitTable {
            fluid,
            log_p,
            tsat,
            hf,
            hg,
            vf,
            vfg,
            sf,
            sfg,
            h_cold: Vec::new(),
            p_min,
            p_max,
            p_serve_max,
            dh_vapor_max,
            dh_liquid_max,
            p_liquid_min,
            liquid_coord: LiquidCoord::Absolute,
            constants: None,
            vapor,
            liquid,
        })
    }

    /// Reads a `FRPHTAB1` file — the artifact `tools/table-gen` produces offline
    /// from native CoolProp.
    ///
    /// This is what the wasm build calls on an `include_bytes!` slice. The
    /// format is specified in `tools/table-gen/README.md`; every structural
    /// promise it makes is checked here, and a file that breaks one is refused
    /// rather than served. Unlike [`decode`](Self::decode) — which reads a
    /// bundle this port itself wrote — the geometry is **not** assumed to be the
    /// Java's 256/96/48: `n_sat`, `n_p` and `n_dh` come from the header, and the
    /// liquid axis may be [`Normalized`](LiquidCoord::Normalized).
    pub fn decode_generated(bytes: &[u8]) -> Result<SaturationSplitTable> {
        if bytes.len() < GENERATED_HEADER_LEN {
            return Err(bad_generated(format!(
                "need at least {GENERATED_HEADER_LEN} header bytes, found {}",
                bytes.len()
            )));
        }
        if &bytes[0..8] != GENERATED_MAGIC {
            return Err(bad_generated(
                "bad magic — this is not a FRPHTAB1 generated property table",
            ));
        }
        let version = read_u16(bytes, 8);
        if version != 1 {
            return Err(bad_generated(format!(
                "format_version {version} is not 1; this build reads FRPHTAB1 version 1 only"
            )));
        }
        let elem = match bytes[10] {
            0 => Elem::F64,
            1 => Elem::F32,
            other => {
                return Err(bad_generated(format!(
                    "elem_kind {other} is neither 0 (f64) nor 1 (f32)"
                )))
            }
        };
        let flags = bytes[11];
        if flags & !(GENERATED_HAS_LIQUID | GENERATED_LIQUID_NORMALIZED) != 0 {
            return Err(bad_generated(format!(
                "flags {flags:#04x} set bits this format version reserves"
            )));
        }
        let has_liquid = flags & GENERATED_HAS_LIQUID != 0;
        let liquid_coord = if flags & GENERATED_LIQUID_NORMALIZED != 0 {
            LiquidCoord::Normalized
        } else {
            LiquidCoord::Absolute
        };

        let n_sat = read_u32(bytes, 12) as usize;
        let n_p = read_u32(bytes, 16) as usize;
        let n_dh = read_u32(bytes, 20) as usize;
        let n_props = read_u32(bytes, 24) as usize;
        let header_bytes = read_u32(bytes, 28) as usize;
        if n_sat < 2 {
            return Err(bad_generated(format!(
                "saturation grid needs at least 2 samples, header says {n_sat}"
            )));
        }
        if n_p < 2 || n_dh < 2 {
            return Err(bad_generated(format!(
                "each 2-D piece needs at least a 2x2 grid, header says {n_p}x{n_dh}"
            )));
        }
        if n_props != 3 {
            return Err(bad_generated(format!(
                "n_props must be 3 (T, Dmass, Smass), header says {n_props}"
            )));
        }
        if header_bytes < GENERATED_HEADER_LEN || header_bytes > bytes.len() {
            return Err(bad_generated(format!(
                "header_bytes {header_bytes} is outside [{GENERATED_HEADER_LEN}, {}]",
                bytes.len()
            )));
        }

        let p_min = read_f64(bytes, 32);
        let p_max = read_f64(bytes, 40);
        let p_serve_max = read_f64(bytes, 48);
        let p_liquid_min = read_f64(bytes, 56);
        let dh_vapor_max = read_f64(bytes, 64);
        let dh_liquid_max = read_f64(bytes, 72);
        // h_top (80) and t_low (88) are provenance for the generator's sizing
        // choices, not inputs to a query — the served box is already expressed by
        // the six scalars above, so they are read and dropped. The four that
        // follow are different: they are the fluid's own constants as CoolProp
        // reported them, not anything derived from the grid, and they are the
        // only oracle-accurate numbers in the file that a query can ask for
        // directly.
        let constants = FluidConstants {
            p_crit: read_f64(bytes, 96),
            t_crit: read_f64(bytes, 104),
            p_triple: read_f64(bytes, 112),
            t_triple: read_f64(bytes, 120),
        };
        for (name, x) in [
            ("p_crit", constants.p_crit),
            ("t_crit", constants.t_crit),
            ("p_triple", constants.p_triple),
            ("t_triple", constants.t_triple),
        ] {
            if !(x > 0.0) || !x.is_finite() {
                return Err(bad_generated(format!("{name} must be positive, found {x}")));
            }
        }
        let fluid_len = read_u16(bytes, 128) as usize;
        let cp_version_len = read_u16(bytes, 130) as usize;
        if GENERATED_HEADER_LEN + fluid_len + cp_version_len > header_bytes {
            return Err(bad_generated(format!(
                "string block ({fluid_len} + {cp_version_len} bytes) does not fit before \
                 header_bytes {header_bytes}"
            )));
        }
        let fluid =
            std::str::from_utf8(&bytes[GENERATED_HEADER_LEN..GENERATED_HEADER_LEN + fluid_len])
                .map_err(|_| bad_generated("fluid name is not valid UTF-8"))?
                .to_string();
        if fluid.trim().is_empty() {
            return Err(bad_generated("fluid name is empty"));
        }

        // Payload size, checked before a single element is read.
        let plane = n_p
            .checked_mul(n_dh)
            .ok_or_else(|| bad_generated("piece grid overflows"))?;
        let piece = plane
            .checked_mul(3)
            .and_then(|v| v.checked_add(n_p))
            .and_then(|v| v.checked_add(n_dh))
            .ok_or_else(|| bad_generated("piece size overflows"))?;
        let elems = n_sat
            .checked_mul(9)
            .and_then(|v| v.checked_add(piece))
            .and_then(|v| {
                if has_liquid {
                    v.checked_add(piece)
                } else {
                    Some(v)
                }
            })
            .ok_or_else(|| bad_generated("payload size overflows"))?;
        let want = elems
            .checked_mul(elem.width())
            .and_then(|v| v.checked_add(header_bytes))
            .ok_or_else(|| bad_generated("payload size overflows"))?;
        if bytes.len() != want {
            return Err(bad_generated(format!(
                "file declares {want} bytes ({header_bytes} header + {elems} \
                 {}-byte elements), {} present",
                elem.width(),
                bytes.len()
            )));
        }

        let mut at = header_bytes;
        let log_p = elem.block(bytes, &mut at, n_sat);
        let tsat = elem.block(bytes, &mut at, n_sat);
        let hf = elem.block(bytes, &mut at, n_sat);
        let hg = elem.block(bytes, &mut at, n_sat);
        let vf = elem.block(bytes, &mut at, n_sat);
        let vfg = elem.block(bytes, &mut at, n_sat);
        let sf = elem.block(bytes, &mut at, n_sat);
        let sfg = elem.block(bytes, &mut at, n_sat);
        let h_cold = elem.block(bytes, &mut at, n_sat);
        validate_sat_lines(&log_p, &hf, &hg, &vf, &vfg)?;
        for (name, line) in [("tsat", &tsat), ("sf", &sf), ("sfg", &sfg)] {
            if let Some(bad) = line.iter().position(|x| !x.is_finite()) {
                return Err(bad_generated(format!(
                    "saturation line {name} is non-finite at sample {bad}"
                )));
            }
        }
        if !(p_min > 0.0) || !(p_serve_max >= p_min) || !(p_max >= p_serve_max) {
            return Err(bad_generated(format!(
                "pressure band must satisfy 0 < p_min ({p_min}) <= p_serve_max \
                 ({p_serve_max}) <= p_max ({p_max})"
            )));
        }
        if !(dh_vapor_max > 0.0) {
            return Err(bad_generated(format!(
                "dh_vapor_max must be positive, found {dh_vapor_max}"
            )));
        }
        if has_liquid {
            // The normalized axis divides by `h_f − h_cold`; a sample where that
            // is not positive would put a pole inside the served box.
            for i in 0..n_sat {
                if !h_cold[i].is_finite() {
                    return Err(bad_generated(format!("h_cold is non-finite at sample {i}")));
                }
                if liquid_coord == LiquidCoord::Normalized
                    && log_p[i].exp() >= p_liquid_min
                    && !(hf[i] > h_cold[i])
                {
                    return Err(bad_generated(format!(
                        "normalized liquid depth needs h_f > h_cold at every served sample; \
                         sample {i} has h_f={}, h_cold={}",
                        hf[i], h_cold[i]
                    )));
                }
            }
            if !(dh_liquid_max > 0.0) {
                return Err(bad_generated(format!(
                    "a liquid piece is present but dh_liquid_max is {dh_liquid_max}"
                )));
            }
        } else if dh_liquid_max != 0.0 {
            return Err(bad_generated(
                "file has no liquid piece but declares a non-zero dh_liquid_max",
            ));
        }

        let vapor = elem.pieces(bytes, &mut at, n_p, n_dh, AxisKind::Superheat, "vapour")?;
        let liquid = if has_liquid {
            Some(elem.pieces(bytes, &mut at, n_p, n_dh, AxisKind::Subcooling, "liquid")?)
        } else {
            None
        };
        debug_assert_eq!(at, bytes.len());

        Ok(SaturationSplitTable {
            fluid,
            log_p,
            tsat,
            hf,
            hg,
            vf,
            vfg,
            sf,
            sfg,
            h_cold,
            p_min,
            p_max,
            p_serve_max,
            dh_vapor_max,
            dh_liquid_max,
            p_liquid_min,
            liquid_coord,
            constants: Some(constants),
            vapor,
            liquid,
        })
    }

    /// How this table measures depth into the liquid sliver.
    pub fn liquid_coord(&self) -> LiquidCoord {
        self.liquid_coord
    }

    /// The fluid's critical and triple constants, when the artifact carries
    /// them.
    pub fn constants(&self) -> Option<FluidConstants> {
        self.constants
    }

    /// The liquid depth coordinate at `(P, h)` and its two partials, in whatever
    /// [`LiquidCoord`] this table uses.
    ///
    /// `hfv`/`dhf_dp` are passed in because the caller has already interpolated
    /// them for the phase test.
    fn liquid_depth(&self, p: f64, h: f64, hfv: f64, dhf_dp: f64) -> (f64, f64, f64) {
        match self.liquid_coord {
            // d(dh)/dP = +h_f'(P); d(dh)/dh = -1.
            LiquidCoord::Absolute => (hfv - h, dhf_dp, -1.0),
            LiquidCoord::Normalized => {
                let (cold, dcold_dp) = self.interp(&self.h_cold, p);
                let den = hfv - cold;
                let y = (hfv - h) / den;
                let dden_dp = dhf_dp - dcold_dp;
                // y = (h_f - h)/den  =>  dy/dP = h_f'/den - y*den'/den.
                (y, dhf_dp / den - y * dden_dp / den, -1.0 / den)
            }
        }
    }
}

/// Element width of a `FRPHTAB1` payload, and the readers that go with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Elem {
    F32,
    F64,
}

impl Elem {
    fn width(self) -> usize {
        match self {
            Elem::F32 => 4,
            Elem::F64 => 8,
        }
    }

    /// `n` elements from `*at`, widened to `f64`. The payload is deliberately
    /// **not** zero-copy castable: a fetched `Vec<u8>` is 1-byte aligned and
    /// casting it would need `unsafe`, which this port does not use.
    fn block(self, bytes: &[u8], at: &mut usize, n: usize) -> Vec<f64> {
        let mut out = Vec::with_capacity(n);
        match self {
            Elem::F32 => {
                for _ in 0..n {
                    let mut buf = [0u8; 4];
                    buf.copy_from_slice(&bytes[*at..*at + 4]);
                    out.push(f32::from_le_bytes(buf) as f64);
                    *at += 4;
                }
            }
            Elem::F64 => {
                for _ in 0..n {
                    out.push(read_f64(bytes, *at));
                    *at += 8;
                }
            }
        }
        out
    }

    /// One `FRPHTAB1` piece: `p_grid`, `y_grid`, then the T / Dmass / Smass
    /// planes, each row-major over `(P, y)`.
    fn pieces(
        self,
        bytes: &[u8],
        at: &mut usize,
        n_p: usize,
        n_dh: usize,
        axis_h_kind: AxisKind,
        what: &str,
    ) -> Result<Pieces> {
        let p_grid = self.block(bytes, at, n_p);
        let y_grid = self.block(bytes, at, n_dh);
        let mut built = [None, None, None];
        for (slot, output) in built.iter_mut().zip(Output::ALL) {
            let plane = self.block(bytes, at, n_p * n_dh);
            *slot = Some(
                PhPropertyTable::from_nodes(
                    p_grid.clone(),
                    y_grid.clone(),
                    plane,
                    Vec::new(),
                    AxisKind::Pressure,
                    axis_h_kind,
                    output.value_kind(),
                )
                .map_err(|e| {
                    bad_generated(format!(
                        "{what} {:?} piece: {}",
                        output,
                        e.to_string_message()
                    ))
                })?,
            );
        }
        let [t, d, s] = built;
        Ok(Pieces {
            t: t.expect("plane 0 read"),
            d: d.expect("plane 1 read"),
            s: s.expect("plane 2 read"),
        })
    }
}

fn bad_generated(msg: impl Into<String>) -> FreesError {
    FreesError::property(format!("generated (P,h) table: {}", msg.into()))
}

/// The three saturation lines a single-phase piece needs during its build.
#[derive(Debug, Clone, Copy)]
struct SatLines<'a> {
    log_p: &'a [f64],
    hf: &'a [f64],
    hg: &'a [f64],
}

impl SatLines<'_> {
    /// The same Hermite the finished table uses, so the coordinate transform at
    /// build time and at serve time agree exactly.
    fn interp(&self, f: &[f64], p: f64) -> f64 {
        let x = p.ln();
        let n = self.log_p.len();
        let mut lo = 0usize;
        let mut hi = n - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.log_p[mid] <= x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let x0 = self.log_p[lo];
        let x1 = self.log_p[hi];
        let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        let m0 = self.slope(f, lo);
        let m1 = self.slope(f, hi);
        let dx = x1 - x0;
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * f[lo]
            + (t3 - 2.0 * t2 + t) * dx * m0
            + (-2.0 * t3 + 3.0 * t2) * f[hi]
            + (t3 - t2) * dx * m1
    }

    fn slope(&self, f: &[f64], i: usize) -> f64 {
        let n = self.log_p.len();
        if i == 0 {
            return (f[1] - f[0]) / (self.log_p[1] - self.log_p[0]);
        }
        if i == n - 1 {
            return (f[n - 1] - f[n - 2]) / (self.log_p[n - 1] - self.log_p[n - 2]);
        }
        (f[i + 1] - f[i - 1]) / (self.log_p[i + 1] - self.log_p[i - 1])
    }
}

/// Port of the private `SaturationSplitTable.buildPiece`.
fn build_piece(
    output: Output,
    p_grid: &[f64],
    dh_grid: &[f64],
    vapor: bool,
    sat: SatLines<'_>,
    source: &mut impl PropSource,
) -> Result<PhPropertyTable> {
    let key = output.key();
    let table = PhPropertyTable::build(p_grid, dh_grid, |p, dh| {
        let h = if vapor {
            sat.interp(sat.hg, p) + dh
        } else {
            sat.interp(sat.hf, p) - dh
        };
        source.prop(key, p, At::Enthalpy(h))
    })?;
    Ok(table.with_kinds(
        AxisKind::Pressure,
        if vapor {
            AxisKind::Superheat
        } else {
            AxisKind::Subcooling
        },
        output.value_kind(),
    ))
}

/// A sample the build cannot proceed without — the Java calls `propsSI` (which
/// throws) rather than `propsSIOrNaN` for exactly these.
fn required(source: &mut impl PropSource, output: &str, p: f64, at: At) -> Result<f64> {
    let v = source.prop(output, p, at);
    if !v.is_finite() {
        return Err(bad_bundle(format!(
            "the source could not supply {output} at P={p} Pa, {at:?}"
        )));
    }
    Ok(v)
}

fn validate_sat_lines(
    log_p: &[f64],
    hf: &[f64],
    hg: &[f64],
    vf: &[f64],
    vfg: &[f64],
) -> Result<()> {
    let n = log_p.len();
    for line in [hf, hg, vf, vfg] {
        if line.len() != n {
            return Err(bad_bundle("saturation lines must all have the same length"));
        }
    }
    for i in 0..n {
        if !log_p[i].is_finite() {
            return Err(bad_bundle(format!("log_p is non-finite at sample {i}")));
        }
        if i > 0 && !(log_p[i] > log_p[i - 1]) {
            return Err(bad_bundle("log_p must be strictly increasing"));
        }
        if !hf[i].is_finite() || !hg[i].is_finite() {
            return Err(bad_bundle(format!(
                "saturation enthalpies are non-finite at sample {i}"
            )));
        }
        if !(hg[i] > hf[i]) {
            return Err(bad_bundle(format!(
                "h_g must exceed h_f at every sample; sample {i} has h_f={}, h_g={}",
                hf[i], hg[i]
            )));
        }
        if !vf[i].is_finite() || !vfg[i].is_finite() {
            return Err(bad_bundle(format!(
                "saturation volumes are non-finite at sample {i}"
            )));
        }
    }
    Ok(())
}

fn read_pieces(bytes: &[u8], at: &mut usize, axis_h_kind: AxisKind, what: &str) -> Result<Pieces> {
    let mut read = [None, None, None];
    for (slot, output) in read.iter_mut().zip(Output::ALL) {
        let (table, used) = PhPropertyTable::decode_prefix(&bytes[*at..])?;
        if table.value_kind() != output.value_kind() {
            return Err(bad_bundle(format!(
                "{what} sections must be ordered T, Dmass, Smass; found {:?} where {:?} was expected",
                table.value_kind(),
                output.value_kind()
            )));
        }
        if table.axis_p_kind() != AxisKind::Pressure || table.axis_h_kind() != axis_h_kind {
            return Err(bad_bundle(format!(
                "{what} section {:?} must be over (Pressure, {axis_h_kind:?}), found ({:?}, {:?})",
                output,
                table.axis_p_kind(),
                table.axis_h_kind()
            )));
        }
        *slot = Some(table);
        *at += used;
    }
    let [t, d, s] = read;
    Ok(Pieces {
        t: t.expect("section 0 read"),
        d: d.expect("section 1 read"),
        s: s.expect("section 2 read"),
    })
}

/// `n` points from `lo` to `hi` inclusive, geometric in pressure.
///
/// Port of the private `SaturationSplitTable.logspace`.
pub fn logspace(lo: f64, hi: f64, n: usize) -> Result<Vec<f64>> {
    if n < 2 {
        return Err(bad_bundle("log grid needs at least 2 points"));
    }
    if !(lo > 0.0) || !(hi > lo) {
        return Err(bad_bundle(format!(
            "log grid needs 0 < lo ({lo}) < hi ({hi})"
        )));
    }
    let a = lo.ln();
    let b = hi.ln();
    let mut out = vec![0.0f64; n];
    for i in 0..n {
        out[i] = (a + (b - a) * i as f64 / (n as f64 - 1.0)).exp();
    }
    Ok(out)
}

/// `n` points from `0` to `max` inclusive, quadratically spaced so the grid is
/// densest at `0` — the dome edge.
///
/// Port of the private `SaturationSplitTable.squarespace`.
pub fn squarespace(max: f64, n: usize) -> Result<Vec<f64>> {
    if n < 2 {
        return Err(bad_bundle("square grid needs at least 2 points"));
    }
    if !(max > 0.0) {
        return Err(bad_bundle(format!("square grid needs max > 0, got {max}")));
    }
    let mut out = vec![0.0f64; n];
    for i in 0..n {
        let t = i as f64 / (n as f64 - 1.0);
        out[i] = max * t * t;
    }
    Ok(out)
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    let mut buf = [0u8; 2];
    buf.copy_from_slice(&bytes[at..at + 2]);
    u16::from_le_bytes(buf)
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(buf)
}

fn read_f64(bytes: &[u8], at: usize) -> f64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[at..at + 8]);
    f64::from_le_bytes(buf)
}

fn read_f64_block(bytes: &[u8], at: &mut usize, n: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(read_f64(bytes, *at));
        *at += 8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // A synthetic fluid with a closed-form dome.
    //
    // Every relation below is exact and mutually consistent, so the two-phase
    // branch can be checked against algebra rather than a tolerance, and the
    // single-phase bicubics can be checked against the function they sampled.
    // Numbers are loosely R134a-shaped but nothing here claims to be a real
    // fluid — this exercises the *machinery*, which is what this module owns.
    // ---------------------------------------------------------------------

    const P_TRIPLE: f64 = 3.9e3;
    const P_CRIT: f64 = 4.06e6;
    const RGAS: f64 = 81.5; // J/(kg-K), R134a-ish
    const CP_VAP: f64 = 900.0; // J/(kg-K)
    const CP_LIQ: f64 = 1400.0; // J/(kg-K)
    const RHO_LIQ_REF: f64 = 1290.0; // kg/m^3 at T_REF_L
    const T_REF_L: f64 = 250.0;
    const BETA_L: f64 = 2.6e-3; // 1/K, liquid expansivity

    fn tsat(p: f64) -> f64 {
        // Clausius-Clapeyron-ish: 1/T linear in ln p.
        let a = 1.0 / 374.2; // 1/Tc
        let b = 4.6e-5;
        1.0 / (a + b * (P_CRIT / p).ln())
    }

    fn hf(p: f64) -> f64 {
        2.0e5 + CP_LIQ * (tsat(p) - T_REF_L)
    }

    fn hfg(p: f64) -> f64 {
        // Shrinks toward the critical point.
        2.1e5 * (1.0 - tsat(p) / 374.2).powf(0.38)
    }

    fn hg(p: f64) -> f64 {
        hf(p) + hfg(p)
    }

    fn rho_liq(t: f64) -> f64 {
        RHO_LIQ_REF * (1.0 - BETA_L * (t - T_REF_L))
    }

    fn rho_vap(p: f64, t: f64) -> f64 {
        p / (RGAS * t)
    }

    fn s_f(p: f64) -> f64 {
        1000.0 + CP_LIQ * (tsat(p) / T_REF_L).ln()
    }

    fn s_g(p: f64) -> f64 {
        s_f(p) + hfg(p) / tsat(p)
    }

    /// `output(P, at)` for the synthetic fluid, in the `PropSource` shape.
    fn synthetic(output: &str, p: f64, at: At) -> f64 {
        match at {
            At::Quality(q) => {
                let ts = tsat(p);
                match output {
                    "T" => ts,
                    "Hmass" => hf(p) + q * hfg(p),
                    "Dmass" => {
                        let v = 1.0 / rho_liq(ts) + q * (1.0 / rho_vap(p, ts) - 1.0 / rho_liq(ts));
                        1.0 / v
                    }
                    "Smass" => s_f(p) + q * (s_g(p) - s_f(p)),
                    _ => f64::NAN,
                }
            }
            At::Temperature(t) => {
                // Only the subcooled-liquid probe uses this.
                if output == "Hmass" {
                    hf(p) - CP_LIQ * (tsat(p) - t)
                } else {
                    f64::NAN
                }
            }
            At::Enthalpy(h) => {
                let ts = tsat(p);
                let hfv = hf(p);
                let hgv = hg(p);
                if h > hgv {
                    let t = ts + (h - hgv) / CP_VAP;
                    match output {
                        "T" => t,
                        "Dmass" => rho_vap(p, t),
                        "Smass" => s_g(p) + CP_VAP * (t / ts).ln(),
                        _ => f64::NAN,
                    }
                } else if h < hfv {
                    let t = ts - (hfv - h) / CP_LIQ;
                    match output {
                        "T" => t,
                        "Dmass" => rho_liq(t),
                        "Smass" => s_f(p) + CP_LIQ * (t / ts).ln(),
                        _ => f64::NAN,
                    }
                } else {
                    let x = (h - hfv) / (hgv - hfv);
                    match output {
                        "T" => ts,
                        "Dmass" => {
                            let v =
                                1.0 / rho_liq(ts) + x * (1.0 / rho_vap(p, ts) - 1.0 / rho_liq(ts));
                            1.0 / v
                        }
                        "Smass" => s_f(p) + x * (s_g(p) - s_f(p)),
                        _ => f64::NAN,
                    }
                }
            }
        }
    }

    fn build_synthetic() -> SaturationSplitTable {
        let t_low = 200.0;
        // h_top: superheated vapour high above the dome, as the Java's caller
        // computes it (Hmass at 0.05 p_crit and a hot temperature).
        let p_probe = P_CRIT * 0.05;
        let h_top = hg(p_probe) + CP_VAP * (1.3 * 374.2 - tsat(p_probe));
        SaturationSplitTable::build("synthetic", h_top, t_low, P_TRIPLE, P_CRIT, synthetic)
            .expect("synthetic bundle builds")
    }

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs().max(1e-9)
    }

    // -- geometry ---------------------------------------------------------

    #[test]
    fn build_derives_the_java_geometry() {
        let t = build_synthetic();
        assert_eq!(t.fluid(), "synthetic");
        assert_eq!(t.p_min(), (P_TRIPLE * 1.2f64).max(P_CRIT * 1e-4));
        assert_eq!(t.p_max(), P_CRIT * 0.75);
        assert_eq!(t.p_serve_max(), t.p_max() * 0.95);
        assert!(t.dh_vapor_max() > 0.0);
        assert!(
            t.has_liquid(),
            "the synthetic fluid has a deep liquid sliver"
        );
        assert!(t.dh_liquid_max() >= 0.9 * MIN_LIQUID_DEPTH);
        // `p_liquid_min` is `exp(log_p[i])`, and the Java stores `log_p[i]` as
        // `ln(p_i)`, so the first sample round-trips to within an ulp of
        // `p_min` rather than exactly onto it. Do not tighten this to `>=`.
        assert!(t.p_liquid_min() >= t.p_min() * (1.0 - 1e-12));
        assert!(t.p_liquid_min() < t.p_max());
    }

    #[test]
    fn saturation_lines_reproduce_the_source() {
        let t = build_synthetic();
        for f in [0.02, 0.1, 0.37, 0.6, 0.9] {
            let p = t.p_min() + f * (t.p_serve_max() - t.p_min());
            assert!(rel(t.tsat_at(p), tsat(p)) < 1e-6, "Tsat at {p}");
            assert!(rel(t.hf_at(p), hf(p)) < 1e-6, "hf at {p}");
            assert!(rel(t.hg_at(p), hg(p)) < 1e-6, "hg at {p}");
        }
    }

    // -- the three regions -------------------------------------------------

    #[test]
    fn two_phase_uses_exact_mixture_relations() {
        let t = build_synthetic();
        for f in [0.05, 0.3, 0.7] {
            let p = t.p_min() + f * (t.p_serve_max() - t.p_min());
            for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
                // The dome boundary the table serves is its *interpolated*
                // h_f/h_g, not the source's; building the query off `hf(p)`
                // directly puts x = 0 a hair on the liquid side.
                let h = t.hf_at(p) + x * (t.hg_at(p) - t.hf_at(p));
                assert_eq!(t.region(p, h), Some(Region::TwoPhase), "x = {x}");
                let temp = t.value(Output::Temperature, p, h).unwrap();
                assert!(rel(temp, tsat(p)) < 1e-6, "T at x={x}: {temp}");
                let rho = t.value(Output::Density, p, h).unwrap();
                assert!(
                    rel(rho, synthetic("Dmass", p, At::Enthalpy(h))) < 1e-4,
                    "rho at x={x}"
                );
                let s = t.value(Output::Entropy, p, h).unwrap();
                assert!(
                    rel(s, synthetic("Smass", p, At::Enthalpy(h))) < 1e-5,
                    "s at x={x}"
                );
            }
        }
    }

    #[test]
    fn superheated_vapour_tracks_the_source() {
        let t = build_synthetic();
        for f in [0.05, 0.25, 0.55, 0.85] {
            let p = t.p_min() + f * (t.p_serve_max() - t.p_min());
            for frac in [0.02, 0.2, 0.6, 0.95] {
                let dh = frac * t.dh_vapor_max();
                let h = t.hg_at(p) + dh;
                assert_eq!(t.region(p, h), Some(Region::Vapor), "p={p} dh={dh}");
                for out in Output::ALL {
                    let got = t.value(out, p, h).unwrap();
                    let want = synthetic(out.key(), p, At::Enthalpy(h));
                    assert!(
                        rel(got, want) < 2e-3,
                        "{out:?} at p={p} dh={dh}: {got} vs {want}"
                    );
                }
            }
        }
    }

    #[test]
    fn subcooled_liquid_tracks_the_source() {
        let t = build_synthetic();
        let lo = t.p_liquid_min();
        for f in [0.05, 0.4, 0.8] {
            let p = lo + f * (t.p_serve_max() - lo);
            for frac in [0.05, 0.4, 0.9] {
                let dh = frac * t.dh_liquid_max();
                let h = t.hf_at(p) - dh;
                assert_eq!(t.region(p, h), Some(Region::Liquid), "p={p} dh={dh}");
                for out in Output::ALL {
                    let got = t.value(out, p, h).unwrap();
                    let want = synthetic(out.key(), p, At::Enthalpy(h));
                    assert!(
                        rel(got, want) < 2e-3,
                        "{out:?} at p={p} dh={dh}: {got} vs {want}"
                    );
                }
            }
        }
    }

    // -- coverage is honest ------------------------------------------------

    #[test]
    fn uncovered_points_return_none() {
        let t = build_synthetic();
        let p = t.p_min() + 0.4 * (t.p_serve_max() - t.p_min());
        let h = t.hg_at(p) + 0.5 * t.dh_vapor_max();

        // Pressure outside the served band.
        assert_eq!(t.value(Output::Temperature, t.p_min() * 0.5, h), None);
        assert_eq!(
            t.value(Output::Temperature, t.p_serve_max() * 1.01, h),
            None
        );
        assert_eq!(
            t.region(t.p_max(), h),
            None,
            "p_max is fitted but not served"
        );

        // Superheat deeper than the vapour rectangle.
        let too_hot = t.hg_at(p) + t.dh_vapor_max() * 1.001;
        assert_eq!(t.value(Output::Density, p, too_hot), None);
        assert_eq!(t.region(p, too_hot), None);

        // Subcooling deeper than the liquid rectangle.
        let too_cold = t.hf_at(p) - t.dh_liquid_max() * 1.001;
        assert_eq!(t.value(Output::Density, p, too_cold), None);
        assert_eq!(t.region(p, too_cold), None);

        // Below the liquid band's pressure floor there is no liquid service.
        if t.p_liquid_min() > t.p_min() {
            let low = t.p_min() * 1.000_001;
            let cold = t.hf_at(low) - 0.5 * t.dh_liquid_max();
            assert_eq!(t.region(low, cold), None);
            assert_eq!(t.value(Output::Temperature, low, cold), None);
        }
    }

    #[test]
    fn a_fluid_with_no_liquid_headroom_still_serves_vapour() {
        // t_low just under the saturation temperature leaves no liquid sliver
        // deep enough, so the Java's `liquidStart` never latches.
        let p_probe = P_CRIT * 0.05;
        let h_top = hg(p_probe) + CP_VAP * (1.3 * 374.2 - tsat(p_probe));
        let t = SaturationSplitTable::build(
            "synthetic",
            h_top,
            370.0, // above almost every Tsat in the band
            P_TRIPLE,
            P_CRIT,
            synthetic,
        )
        .unwrap();
        assert!(!t.has_liquid());
        assert_eq!(t.dh_liquid_max(), 0.0);
        assert_eq!(t.p_liquid_min(), f64::INFINITY);
        let p = t.p_min() + 0.4 * (t.p_serve_max() - t.p_min());
        assert!(t
            .value(Output::Temperature, p, t.hg_at(p) + 1.0e4)
            .is_some());
        assert_eq!(t.value(Output::Temperature, p, t.hf_at(p) - 1.0e4), None);
    }

    #[test]
    fn structural_failures_are_errors() {
        // No subcritical band.
        assert!(
            SaturationSplitTable::build("x", 1e6, 200.0, 1e6, 1.0e6, synthetic).is_err(),
            "p_min >= p_max must be refused"
        );
        // h_top under the dome leaves no superheat band.
        assert!(
            SaturationSplitTable::build("x", 0.0, 200.0, P_TRIPLE, P_CRIT, synthetic).is_err(),
            "no superheat band must be refused"
        );
        // A source that cannot answer is fatal on the saturation lines.
        assert!(SaturationSplitTable::build(
            "x",
            1e6,
            200.0,
            P_TRIPLE,
            P_CRIT,
            |_: &str, _: f64, _: At| { f64::NAN }
        )
        .is_err());
    }

    // -- analytic derivatives ----------------------------------------------

    /// Central difference of `value` — the thing the analytic partials must
    /// agree with, since they claim to be derivatives of exactly that surface.
    fn fd(t: &SaturationSplitTable, out: Output, p: f64, h: f64, dp: f64, dh: f64) -> (f64, f64) {
        let d_p =
            (t.value(out, p + dp, h).unwrap() - t.value(out, p - dp, h).unwrap()) / (2.0 * dp);
        let d_h =
            (t.value(out, p, h + dh).unwrap() - t.value(out, p, h - dh).unwrap()) / (2.0 * dh);
        (d_p, d_h)
    }

    #[test]
    fn two_phase_partials_match_a_finite_difference_of_the_value() {
        let t = build_synthetic();
        for f in [0.2, 0.5, 0.8] {
            let p = t.p_min() + f * (t.p_serve_max() - t.p_min());
            for x in [0.15, 0.5, 0.85] {
                let h = t.hf_at(p) + x * (t.hg_at(p) - t.hf_at(p));
                for out in Output::ALL {
                    let v = t.eval(out, p, h).unwrap();
                    let (fd_p, fd_h) = fd(&t, out, p, h, p * 1e-6, 5.0);
                    assert!(
                        rel(v.d_value_d_p, fd_p) < 1e-4 || (v.d_value_d_p - fd_p).abs() < 1e-9,
                        "{out:?} dP at p={p} x={x}: {} vs {fd_p}",
                        v.d_value_d_p
                    );
                    assert!(
                        rel(v.d_value_d_h, fd_h) < 1e-4 || (v.d_value_d_h - fd_h).abs() < 1e-9,
                        "{out:?} dh at p={p} x={x}: {} vs {fd_h}",
                        v.d_value_d_h
                    );
                }
            }
        }
    }

    #[test]
    fn vapour_partials_match_a_finite_difference_of_the_value() {
        let t = build_synthetic();
        for f in [0.2, 0.5, 0.8] {
            let p = t.p_min() + f * (t.p_serve_max() - t.p_min());
            for frac in [0.15, 0.45, 0.8] {
                let h = t.hg_at(p) + frac * t.dh_vapor_max();
                for out in Output::ALL {
                    let v = t.eval(out, p, h).unwrap();
                    let (fd_p, fd_h) = fd(&t, out, p, h, p * 1e-6, 20.0);
                    assert!(
                        rel(v.d_value_d_p, fd_p) < 1e-4,
                        "{out:?} dP at p={p}: {} vs {fd_p}",
                        v.d_value_d_p
                    );
                    assert!(
                        rel(v.d_value_d_h, fd_h) < 1e-4,
                        "{out:?} dh at p={p}: {} vs {fd_h}",
                        v.d_value_d_h
                    );
                }
            }
        }
    }

    #[test]
    fn liquid_partials_match_a_finite_difference_of_the_value() {
        let t = build_synthetic();
        let lo = t.p_liquid_min();
        for f in [0.2, 0.5, 0.8] {
            let p = lo + f * (t.p_serve_max() - lo);
            for frac in [0.2, 0.5, 0.8] {
                let h = t.hf_at(p) - frac * t.dh_liquid_max();
                for out in Output::ALL {
                    let v = t.eval(out, p, h).unwrap();
                    let (fd_p, fd_h) = fd(&t, out, p, h, p * 1e-6, 20.0);
                    assert!(
                        rel(v.d_value_d_p, fd_p) < 1e-4,
                        "{out:?} dP at p={p}: {} vs {fd_p}",
                        v.d_value_d_p
                    );
                    assert!(
                        rel(v.d_value_d_h, fd_h) < 1e-4,
                        "{out:?} dh at p={p}: {} vs {fd_h}",
                        v.d_value_d_h
                    );
                }
            }
        }
    }

    #[test]
    fn two_phase_temperature_is_flat_in_enthalpy() {
        let t = build_synthetic();
        let p = t.p_min() + 0.5 * (t.p_serve_max() - t.p_min());
        let h = t.hf_at(p) + 0.5 * (t.hg_at(p) - t.hf_at(p));
        let v = t.eval(Output::Temperature, p, h).unwrap();
        assert_eq!(v.d_value_d_h, 0.0);
        assert!(v.d_value_d_p > 0.0, "Tsat rises with pressure");
    }

    // -- the on-disk bundle ------------------------------------------------

    #[test]
    fn bundle_header_layout_is_what_the_documentation_says() {
        let t = build_synthetic();
        let bytes = t.encode();
        assert_eq!(&bytes[0..8], BUNDLE_MAGIC);
        assert_eq!(bytes[8], BUNDLE_KIND);
        assert_eq!(bytes[9], BUNDLE_HAS_LIQUID);
        assert_eq!(&bytes[10..12], &[0, 0]);
        assert_eq!(read_u32(&bytes, 12), SAT_POINTS as u32);
        assert_eq!(read_u32(&bytes, 16), "synthetic".len() as u32);
        assert_eq!(read_f64(&bytes, 20), t.p_min());
        assert_eq!(read_f64(&bytes, 28), t.p_max());
        assert_eq!(read_f64(&bytes, 36), t.p_serve_max());
        assert_eq!(read_f64(&bytes, 44), t.p_liquid_min());
        assert_eq!(read_f64(&bytes, 52), t.dh_vapor_max());
        assert_eq!(read_f64(&bytes, 60), t.dh_liquid_max());
        assert_eq!(
            &bytes[BUNDLE_HEADER_LEN..BUNDLE_HEADER_LEN + 9],
            b"synthetic"
        );
        assert_eq!(bytes.len(), t.encoded_len());
    }

    #[test]
    fn bundle_round_trips_and_serves_an_identical_surface() {
        let t = build_synthetic();
        let bytes = t.encode();
        let back = SaturationSplitTable::decode(&bytes).unwrap();
        assert_eq!(t, back);
        assert_eq!(back.encode(), bytes);
        let p = t.p_min() + 0.42 * (t.p_serve_max() - t.p_min());
        for h in [
            t.hf_at(p) - 0.3 * t.dh_liquid_max(),
            t.hf_at(p) + 0.5 * (t.hg_at(p) - t.hf_at(p)),
            t.hg_at(p) + 0.3 * t.dh_vapor_max(),
        ] {
            for out in Output::ALL {
                assert_eq!(t.eval(out, p, h), back.eval(out, p, h), "{out:?} at h={h}");
            }
        }
    }

    #[test]
    fn a_liquid_free_bundle_round_trips() {
        let p_probe = P_CRIT * 0.05;
        let h_top = hg(p_probe) + CP_VAP * (1.3 * 374.2 - tsat(p_probe));
        let t =
            SaturationSplitTable::build("dry", h_top, 370.0, P_TRIPLE, P_CRIT, synthetic).unwrap();
        assert!(!t.has_liquid());
        let bytes = t.encode();
        assert_eq!(bytes[9], 0);
        let back = SaturationSplitTable::decode(&bytes).unwrap();
        assert_eq!(t, back);
        assert!(!back.has_liquid());
    }

    #[test]
    fn decode_rejects_corrupt_bundles() {
        let good = build_synthetic().encode();

        assert!(SaturationSplitTable::decode(&good[..40]).is_err());

        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[8] = 0x01; // a section kind, not a bundle kind
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[9] = 0x02; // reserved bundle flag
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[11] = 1; // reserved header byte
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[12..16].copy_from_slice(&1u32.to_le_bytes()); // n_sat < 2
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[20..28].copy_from_slice(&(-1.0f64).to_le_bytes()); // p_min <= 0
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[36..44].copy_from_slice(&1.0e12f64.to_le_bytes()); // p_serve_max > p_max
        assert!(SaturationSplitTable::decode(&bad).is_err());

        let mut extra = good.clone();
        extra.push(0);
        assert!(SaturationSplitTable::decode(&extra).is_err());

        // h_g <= h_f at a sample: the quality would be undefined.
        let mut bad = good.clone();
        let hf_at = BUNDLE_HEADER_LEN + "synthetic".len() + 2 * 8 * SAT_POINTS;
        let hg_at = hf_at + 8 * SAT_POINTS;
        let hf_bytes = read_f64(&bad, hf_at).to_le_bytes();
        bad[hg_at..hg_at + 8].copy_from_slice(&hf_bytes);
        assert!(SaturationSplitTable::decode(&bad).is_err());

        // A non-increasing log_p line.
        let mut bad = good.clone();
        let log_at = BUNDLE_HEADER_LEN + "synthetic".len() + 8;
        bad[log_at..log_at + 8].copy_from_slice(&(-99.0f64).to_le_bytes());
        assert!(SaturationSplitTable::decode(&bad).is_err());
    }

    #[test]
    fn decode_rejects_permuted_or_mislabelled_sections() {
        let t = build_synthetic();
        // Rebuild a bundle whose vapour sections are ordered D, T, S.
        let mut bytes = t.encode();
        let head = BUNDLE_HEADER_LEN + t.fluid().len() + 8 * 8 * SAT_POINTS;
        let (first, used_t) = PhPropertyTable::decode_prefix(&bytes[head..]).unwrap();
        let (second, used_d) = PhPropertyTable::decode_prefix(&bytes[head + used_t..]).unwrap();
        let mut swapped = bytes[..head].to_vec();
        second.encode_into(&mut swapped);
        first.encode_into(&mut swapped);
        swapped.extend_from_slice(&bytes[head + used_t + used_d..]);
        assert!(SaturationSplitTable::decode(&swapped).is_err());

        // Mislabel a vapour section's second axis as Subcooling.
        let axis_byte = head + 11;
        bytes[axis_byte] = 3; // AxisKind::Subcooling
        assert!(SaturationSplitTable::decode(&bytes).is_err());
    }

    // -- grid helpers ------------------------------------------------------

    #[test]
    fn grid_helpers_match_the_java() {
        let g = logspace(1e4, 1e6, 5).unwrap();
        assert!((g[0] - 1e4).abs() < 1e-6);
        assert!((g[4] - 1e6).abs() < 1e-3);
        assert!((g[2] - 1e5).abs() < 1e-6);
        for w in g.windows(2) {
            assert!(w[1] > w[0]);
        }
        let s = squarespace(400.0, 5).unwrap();
        assert_eq!(s[0], 0.0);
        assert_eq!(s[4], 400.0);
        assert_eq!(s[2], 100.0);
        assert!(logspace(0.0, 1.0, 4).is_err());
        assert!(logspace(1.0, 1.0, 4).is_err());
        assert!(logspace(1.0, 2.0, 1).is_err());
        assert!(squarespace(0.0, 4).is_err());
        assert!(squarespace(1.0, 1).is_err());
    }

    #[test]
    fn output_keys_round_trip() {
        for out in Output::ALL {
            assert_eq!(Output::from_key(out.key()), Some(out));
        }
        assert_eq!(Output::from_key("Q"), None);
        assert_eq!(Output::from_key("D"), None); // mass-basis keys only
        let t = build_synthetic();
        let p = t.p_min() + 0.5 * (t.p_serve_max() - t.p_min());
        let h = t.hg_at(p) + 0.2 * t.dh_vapor_max();
        assert_eq!(
            t.value_by_key("Dmass", p, h),
            t.value(Output::Density, p, h)
        );
        assert_eq!(t.value_by_key("Q", p, h), None);
    }
}
