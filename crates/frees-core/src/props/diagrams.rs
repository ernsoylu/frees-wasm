//! Thermodynamic property diagrams — saturation dome, isolines, markers.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/PropertyDiagrams.java`
//! (324 LOC), which backs `POST /api/plot/propplot`.
//!
//! Curves are produced by sweeping a CoolProp input pair robust across the
//! whole region: `(T,Q)` for the dome and quality lines, `(T,D)` for isotherms
//! and isochores, `(P,S)` for isobars/isentropes and `(P,H)` for isenthalps.
//! The two-phase region falls out of those flashes. **Failed points are emitted
//! as `None` so the client renders a line gap** — that is the mechanism by
//! which a partial property backend produces an honest, visibly incomplete
//! plot instead of a smooth wrong one.
//!
//! Every property call goes through [`crate::props::propfun`], so a diagram is
//! exactly as complete as the installed backend. With no backend the
//! constructor fails at `Ttriple` and the caller gets one error naming the
//! fluid, rather than a chart of NaNs.

use crate::diag::{FreesError, Result};
use crate::props::hx::java_double_to_string;
use crate::props::propfun::{props1_si, props_si_or_nan};

/// Supported diagram kinds; axis variables are `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Temperature–entropy.
    Ts,
    /// Pressure–enthalpy (log P).
    Ph,
    /// Pressure–volume (log both).
    Pv,
    /// Temperature–volume (log v).
    Tv,
    /// Enthalpy–entropy (Mollier).
    Hs,
    /// Pressure–temperature (the saturation curve alone).
    Pt,
}

impl Kind {
    /// The x-axis property symbol.
    pub fn x(self) -> &'static str {
        match self {
            Kind::Ts | Kind::Hs => "s",
            Kind::Ph => "h",
            Kind::Pv | Kind::Tv => "v",
            Kind::Pt => "T",
        }
    }

    /// The y-axis property symbol.
    pub fn y(self) -> &'static str {
        match self {
            Kind::Ts | Kind::Tv => "T",
            Kind::Ph | Kind::Pv | Kind::Pt => "P",
            Kind::Hs => "h",
        }
    }

    /// The Java enum constant name, which is what the wire payload carries.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ts => "TS",
            Kind::Ph => "PH",
            Kind::Pv => "PV",
            Kind::Tv => "TV",
            Kind::Hs => "HS",
            Kind::Pt => "PT",
        }
    }

    /// Port of `Kind.parse`: lower-cased with `-` stripped, so `T-s`, `ts` and
    /// `TS` all name the same diagram. `logph` is an accepted spelling of `ph`.
    pub fn parse(name: &str) -> Result<Kind> {
        let key = name.to_lowercase().replace('-', "");
        match key.as_str() {
            "ts" => Ok(Kind::Ts),
            "ph" | "logph" => Ok(Kind::Ph),
            "pv" => Ok(Kind::Pv),
            "tv" => Ok(Kind::Tv),
            "hs" => Ok(Kind::Hs),
            "pt" => Ok(Kind::Pt),
            _ => Err(FreesError::property(format!(
                "Unknown diagram type '{name}'. Supported: T-s, P-h, P-v, T-v, h-s, P-T"
            ))),
        }
    }
}

/// One curve: axis points (`None` for a gap) plus a legend label.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub family: String,
    pub label: String,
    pub x: Vec<Option<f64>>,
    pub y: Vec<Option<f64>>,
}

/// A single annotated point (the critical point).
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub label: String,
    pub x: f64,
    pub y: f64,
}

/// The full diagram payload for one fluid and diagram kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub fluid: String,
    pub kind: &'static str,
    pub x_property: &'static str,
    pub y_property: &'static str,
    pub x_log: bool,
    pub y_log: bool,
    pub dome: Vec<Curve>,
    pub isolines: Vec<Curve>,
    pub markers: Vec<Marker>,
}

const CURVE_POINTS: usize = 120;
const DOME_POINTS: usize = 200;

#[derive(Debug, Clone, Copy)]
struct Limits {
    t_triple: f64,
    t_crit: f64,
    p_triple: f64,
    p_crit: f64,
}

struct Generator {
    fluid: String,
    kind: Kind,
    limits: Limits,
}

/// Builds the full diagram payload for one fluid and diagram kind.
/// Port of `generate(fluid, kindName)`.
///
/// `fluid` is the **canonical CoolProp name** the Java controller passes
/// (`PropertyFunctions.plotFluids()` supplies the list); a document spelling is
/// resolved by [`crate::props::propfun::resolve_fluid`] first.
pub fn generate(fluid: &str, kind_name: &str) -> Result<Diagram> {
    let kind = Kind::parse(kind_name)?;
    // The Java constructor calls props1SI four times and lets the throw escape;
    // a missing backend therefore names itself here, once, instead of leaking
    // NaN into every curve.
    let limits = Limits {
        t_triple: props1_si(fluid, "Ttriple")?,
        t_crit: props1_si(fluid, "Tcrit")?,
        p_triple: props1_si(fluid, "ptriple")?.max(1.0),
        p_crit: props1_si(fluid, "pcrit")?,
    };
    let generator = Generator {
        fluid: fluid.to_string(),
        kind,
        limits,
    };
    Ok(generator.build())
}

impl Generator {
    fn build(&self) -> Diagram {
        let dome = self.saturation_dome();
        let mut isolines = Vec::new();
        if self.kind != Kind::Pt {
            isolines.extend(self.quality_lines());
        }
        match self.kind {
            Kind::Ts | Kind::Tv | Kind::Hs => isolines.extend(self.isobars()),
            Kind::Ph | Kind::Pv => {
                isolines.extend(self.isotherms());
                if self.kind == Kind::Ph {
                    isolines.extend(self.isentropes());
                }
            }
            Kind::Pt => { /* saturation curve only */ }
        }
        let x_log = self.kind == Kind::Pv || self.kind == Kind::Tv;
        let y_log = self.kind == Kind::Ph || self.kind == Kind::Pv;
        Diagram {
            fluid: self.fluid.clone(),
            kind: self.kind.name(),
            x_property: self.kind.x(),
            y_property: self.kind.y(),
            x_log,
            y_log,
            dome,
            isolines,
            markers: self.markers(),
        }
    }

    fn markers(&self) -> Vec<Marker> {
        let tc = self.limits.t_crit;
        vec![Marker {
            label: "Critical point".to_string(),
            x: self.axis_value_at_q(self.kind.x(), tc),
            y: self.axis_value_at_q(self.kind.y(), tc),
        }]
    }

    /// Axis property exactly at the critical temperature (the Q flash
    /// degenerates there, so the sample is nudged just below).
    fn axis_value_at_q(&self, axis: &str, t: f64) -> f64 {
        let t_safe = t.min(self.limits.t_crit * 0.999_999);
        props_si_or_nan(coolprop_key(axis), "T", t_safe, "Q", 0.5, &self.fluid)
    }

    fn saturation_dome(&self) -> Vec<Curve> {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        // Liquid branch up to the critical point, then back down the vapor one.
        for i in 0..DOME_POINTS {
            let t = self.dome_temperature(i);
            self.add_point(&mut xs, &mut ys, "T", t, "Q", 0.0);
        }
        for i in (0..DOME_POINTS).rev() {
            let t = self.dome_temperature(i);
            self.add_point(&mut xs, &mut ys, "T", t, "Q", 1.0);
        }
        if self.kind == Kind::Pt {
            // The P-T saturation line is single-valued; keep one branch.
            xs.truncate(DOME_POINTS);
            ys.truncate(DOME_POINTS);
        }
        vec![Curve {
            family: "dome".to_string(),
            label: "Saturation".to_string(),
            x: xs,
            y: ys,
        }]
    }

    /// Cluster dome samples near the critical point where curvature is high.
    fn dome_temperature(&self, i: usize) -> f64 {
        let u = i as f64 / (DOME_POINTS as f64 - 1.0);
        let shaped = 1.0 - (1.0 - u) * (1.0 - u);
        let t_max = self.limits.t_crit * 0.999_999;
        self.limits.t_triple + (t_max - self.limits.t_triple) * shaped
    }

    fn quality_lines(&self) -> Vec<Curve> {
        let mut out = Vec::new();
        for q in 1..=9 {
            let quality = f64::from(q) / 10.0;
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for i in 0..CURVE_POINTS {
                let u = i as f64 / (CURVE_POINTS as f64 - 1.0);
                let t = self.limits.t_triple
                    + (self.limits.t_crit * 0.9999 - self.limits.t_triple)
                        * (1.0 - (1.0 - u) * (1.0 - u));
                self.add_point(&mut xs, &mut ys, "T", t, "Q", quality);
            }
            out.push(Curve {
                family: "quality".to_string(),
                label: format!("x = {}", java_double_to_string(quality)),
                x: xs,
                y: ys,
            });
        }
        out
    }

    fn isobars(&self) -> Vec<Curve> {
        nice_log_values(self.limits.p_triple * 2.0, self.limits.p_crit * 2.5, 7)
            .into_iter()
            .map(|p| self.sweep_entropy_at_pressure("isobar", &format_pressure(p), p))
            .collect()
    }

    fn isentropes(&self) -> Vec<Curve> {
        let s_min = props_si_or_nan("S", "T", self.limits.t_triple + 1.0, "Q", 0.0, &self.fluid);
        let s_max = props_si_or_nan("S", "T", self.limits.t_triple + 1.0, "Q", 1.0, &self.fluid);
        if s_min.is_nan() || s_max.is_nan() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let count = 7;
        for i in 1..=count {
            let s = s_min + (s_max - s_min) * f64::from(i) / (f64::from(count) + 1.0);
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for p in log_sweep(
                self.limits.p_triple * 1.2,
                self.limits.p_crit * 2.5,
                CURVE_POINTS,
            ) {
                self.add_point(&mut xs, &mut ys, "P", p, "S", s);
            }
            out.push(Curve {
                family: "isentrope".to_string(),
                label: format!("s = {} J/kg-K", java_round(s)),
                x: xs,
                y: ys,
            });
        }
        out
    }

    /// One isobar, swept in entropy between a cold and a hot anchor.
    ///
    /// The Java pins the cold anchor at `Ttriple + 0.5` and emits **no curve at
    /// all** — a zero-length array, not a line of gaps — when that one call
    /// fails. For every fluid the upstream picker offers that is harmless,
    /// because `Ttriple + 0.5` is a fluid state at every pressure *if the
    /// melting line leans the way water's and R134a's do*.
    ///
    /// CO2's does not, and the diagram is the poorer for it. Its triple point
    /// sits at 0.518 MPa with a steep melting line (`Tmelt` = 217.12 K at 3 MPa,
    /// 220.36 K at 18.44 MPa), so the cold anchor is inside the **solid** region
    /// for every pressure above ~2.9 MPa, and CoolProp refuses it by name:
    /// `"For now, we don't support T [217.092 K] below Tmelt(p) [217.55 K]"`.
    /// Two of CO2's three isobars come back empty — on a diagram where three is
    /// already the entire list, because `ptriple`/`pcrit` span 14x for CO2
    /// against water's 36000x.
    ///
    /// So this port walks the cold anchor up to the coldest temperature at this
    /// pressure the backend will actually answer, by bisecting the refusal
    /// boundary between the known-bad anchor and the known-good hot end (see
    /// `coldest_entropy_at_pressure`). That lands on the melting line, which is
    /// where a physical isobar starts.
    ///
    /// **Deliberate divergence, and strictly additive** (ledger item 38): the
    /// walk runs only after a refusal the Java turns into an empty curve, so it
    /// can only replace nothing with something. Water, R134a, R1234yf and Air
    /// all answer at the first anchor and are untouched — pinned by
    /// `the_cold_anchor_walk_is_inert_for_the_fluids_that_answer_at_once`.
    ///
    /// The hot anchor is deliberately *not* given the same treatment: `Tcrit *
    /// 1.15` sits comfortably inside every served fluid's EOS range, and never
    /// refused in the Wave-C1 sweep. Only the end that actually breaks is
    /// defended.
    fn sweep_entropy_at_pressure(&self, family: &str, label: &str, p: f64) -> Curve {
        let t_max = self.limits.t_crit * 1.15;
        let t_cold = self.limits.t_triple + 0.5;
        let mut s_low = props_si_or_nan("S", "P", p, "T", t_cold, &self.fluid);
        let s_high = props_si_or_nan("S", "P", p, "T", t_max, &self.fluid);
        if s_low.is_nan() && !s_high.is_nan() {
            s_low = self.coldest_entropy_at_pressure(p, t_cold, t_max);
        }
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        if !s_low.is_nan() && !s_high.is_nan() {
            for i in 0..CURVE_POINTS {
                let s = s_low + (s_high - s_low) * i as f64 / (CURVE_POINTS as f64 - 1.0);
                self.add_point(&mut xs, &mut ys, "P", p, "S", s);
            }
        }
        Curve {
            family: family.to_string(),
            label: label.to_string(),
            x: xs,
            y: ys,
        }
    }

    /// Entropy at the coldest temperature in `(t_refused, t_answered]` the
    /// backend will serve at pressure `p`, or `NaN` if it never does.
    ///
    /// Bisection is sound here because the predicate is **monotone in `T` at
    /// fixed `P`**: a state below the melting line is refused and every state
    /// above it is served, so "answers" flips exactly once across the bracket.
    /// The bracket starts known-bad/known-good, which is the invariant the loop
    /// maintains. 40 halvings take CO2's 133 K span below a nanokelvin — far
    /// finer than needed, and still only 40 flashes on a curve that would
    /// otherwise not exist.
    ///
    /// The returned value is always one the backend actually produced (never an
    /// extrapolation to the boundary), so a curve built on it is as real as any
    /// other point on the sweep.
    fn coldest_entropy_at_pressure(&self, p: f64, t_refused: f64, t_answered: f64) -> f64 {
        let mut lo = t_refused;
        let mut hi = t_answered;
        let mut coldest = f64::NAN;
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            let s = props_si_or_nan("S", "P", p, "T", mid, &self.fluid);
            if s.is_nan() {
                lo = mid;
            } else {
                hi = mid;
                coldest = s;
            }
        }
        coldest
    }

    /// The `P-h`/`P-v` isotherms, swept in density between a dilute and a dense
    /// anchor.
    ///
    /// This end of the same melting-line collision is left exactly as the Java
    /// wrote it, and the choice is deliberate. CO2's coldest isotherm (220 K)
    /// loses its **dense** anchor — `Dmass(T=220 K, P=pcrit*2.5)` is solid and
    /// refused — so the curve is empty. Walking that anchor down to the melting
    /// pressure the way `sweep_entropy_at_pressure` (above) walks its cold one
    /// would "work" and would still be worthless: `Psat(220 K)` is 0.599 MPa,
    /// below the dilute anchor at `ptriple*1.2` = 0.622 MPa, so the *entire*
    /// pressure window at 220 K is compressed liquid spanning 1166 -> ~1170
    /// kg/m3. The recovered isotherm would be a near-vertical 0.4 % stub, which
    /// is a worse answer than the honest absence. The other seven CO2 isotherms
    /// (240-360 K) are complete.
    fn isotherms(&self) -> Vec<Curve> {
        let mut out = Vec::new();
        for t in nice_linear_values(self.limits.t_triple, self.limits.t_crit * 1.2, 8) {
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            let d_gas = props_si_or_nan("D", "T", t, "P", self.limits.p_triple * 1.2, &self.fluid);
            let d_liq = props_si_or_nan("D", "T", t, "P", self.limits.p_crit * 2.5, &self.fluid);
            if !d_gas.is_nan() && !d_liq.is_nan() && d_gas > 0.0 && d_liq > d_gas {
                for d in log_sweep(d_gas, d_liq, CURVE_POINTS) {
                    self.add_point(&mut xs, &mut ys, "T", t, "D", d);
                }
            }
            out.push(Curve {
                family: "isotherm".to_string(),
                label: format!("T = {} K", java_round(t)),
                x: xs,
                y: ys,
            });
        }
        out
    }

    /// Appends the `(x, y)` axis values of the state given by the input pair,
    /// or a `(None, None)` gap when either axis cannot be evaluated.
    fn add_point(
        &self,
        xs: &mut Vec<Option<f64>>,
        ys: &mut Vec<Option<f64>>,
        key1: &str,
        v1: f64,
        key2: &str,
        v2: f64,
    ) {
        let x = self.state_prop(self.kind.x(), key1, v1, key2, v2);
        let y = self.state_prop(self.kind.y(), key1, v1, key2, v2);
        if x.is_nan() || y.is_nan() {
            xs.push(None);
            ys.push(None);
        } else {
            xs.push(Some(x));
            ys.push(Some(y));
        }
    }

    fn state_prop(&self, axis: &str, key1: &str, v1: f64, key2: &str, v2: f64) -> f64 {
        if axis == "v" {
            let d = props_si_or_nan("D", key1, v1, key2, v2, &self.fluid);
            // Java: `d > 0 ? 1.0 / d : NaN` — the comparison rejects NaN.
            return if d > 0.0 { 1.0 / d } else { f64::NAN };
        }
        props_si_or_nan(coolprop_key(axis), key1, v1, key2, v2, &self.fluid)
    }
}

/// Port of `coolPropKey`. The Java throws on an unknown axis; every caller here
/// passes a [`Kind`] axis, so the fallback is unreachable and returns the axis
/// verbatim for the backend to reject by name (a wasm build must not panic).
fn coolprop_key(axis: &str) -> &str {
    match axis {
        "s" => "Smass",
        "h" => "Hmass",
        "v" => "Dmass", // inverted in state_prop
        "T" => "T",
        "P" => "P",
        other => other,
    }
}

/// Java `Math.round(double)` → `long`, i.e. `floor(x + 0.5)`.
fn java_round(v: f64) -> i64 {
    libm::floor(v + 0.5) as i64
}

fn log_sweep(from: f64, to: f64, points: usize) -> Vec<f64> {
    let log_from = libm::log(from);
    let log_to = libm::log(to);
    (0..points)
        .map(|i| libm::exp(log_from + (log_to - log_from) * i as f64 / (points as f64 - 1.0)))
        .collect()
}

/// Round values on a 1-2-5 progression covering `[from, to]`.
fn nice_log_values(from: f64, to: f64, max_count: usize) -> Vec<f64> {
    let mut candidates = Vec::new();
    let mut decade = libm::pow(10.0, libm::floor(libm::log10(from)));
    // The Java loop is unbounded; a non-finite `from` (no backend, NaN limits)
    // would spin forever, so the guard below is a port addition. It cannot
    // change any result the Java produces, because the Java only reaches this
    // code with finite limits.
    if !decade.is_finite() || decade <= 0.0 || !to.is_finite() {
        return candidates;
    }
    while decade <= to {
        for m in [1.0, 2.0, 5.0] {
            let v = m * decade;
            if v >= from && v <= to {
                candidates.push(v);
            }
        }
        decade *= 10.0;
    }
    thin(candidates, max_count)
}

/// Round step values (10/20/25/50… progression) covering `[from, to]`.
fn nice_linear_values(from: f64, to: f64, max_count: usize) -> Vec<f64> {
    let raw_step = (to - from) / max_count as f64;
    let magnitude = libm::pow(10.0, libm::floor(libm::log10(raw_step)));
    let mut step = magnitude;
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * magnitude >= raw_step {
            step = m * magnitude;
            break;
        }
    }
    let mut out = Vec::new();
    // Another port-only guard (see `nice_log_values`): the Java loop is
    // unbounded and would spin on a non-positive or non-finite step. Written as
    // two positive tests rather than `!(step > 0.0)` because nothing in the
    // Java depends on the negated form here.
    if !step.is_finite() || step <= 0.0 || !from.is_finite() || !to.is_finite() {
        return out;
    }
    let mut v = libm::ceil(from / step) * step;
    while v <= to {
        out.push(v);
        v += step;
    }
    out
}

/// Port of `thin`. The index arithmetic is done in **`f32`**, because the Java
/// is `Math.round((float) i * (values.size() - 1) / (maxCount - 1))` — an f64
/// version picks a different element at some sizes.
fn thin(values: Vec<f64>, max_count: usize) -> Vec<f64> {
    if values.len() <= max_count || max_count < 2 {
        return values;
    }
    let span = (values.len() - 1) as f32;
    let divisor = (max_count - 1) as f32;
    (0..max_count)
        .map(|i| {
            let idx = libm::floorf(i as f32 * span / divisor + 0.5) as usize;
            values[idx.min(values.len() - 1)]
        })
        .collect()
}

fn format_pressure(pascal: f64) -> String {
    if pascal >= 1e6 {
        return format!("{} MPa", trim_number(pascal / 1e6));
    }
    if pascal >= 1e3 {
        return format!("{} kPa", trim_number(pascal / 1e3));
    }
    format!("{} Pa", trim_number(pascal))
}

fn trim_number(v: f64) -> String {
    if v == libm::rint(v) {
        return format!("{}", v as i64);
    }
    java_double_to_string(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::propfun::{self, RealFluid};
    use std::sync::Arc;

    #[test]
    fn kind_parsing_accepts_every_java_spelling() {
        for (text, kind) in [
            ("T-s", Kind::Ts),
            ("ts", Kind::Ts),
            ("TS", Kind::Ts),
            ("P-h", Kind::Ph),
            ("logph", Kind::Ph),
            ("log-ph", Kind::Ph),
            ("P-v", Kind::Pv),
            ("T-v", Kind::Tv),
            ("h-s", Kind::Hs),
            ("P-T", Kind::Pt),
        ] {
            assert_eq!(Kind::parse(text).unwrap(), kind, "{text}");
        }
        let err = Kind::parse("q-w").unwrap_err().to_string();
        assert!(err.contains("Unknown diagram type 'q-w'"), "{err}");
        assert!(err.contains("T-s, P-h, P-v, T-v, h-s, P-T"), "{err}");
    }

    #[test]
    fn axes_and_log_flags_match_the_java_enum() {
        assert_eq!((Kind::Ts.x(), Kind::Ts.y()), ("s", "T"));
        assert_eq!((Kind::Ph.x(), Kind::Ph.y()), ("h", "P"));
        assert_eq!((Kind::Pv.x(), Kind::Pv.y()), ("v", "P"));
        assert_eq!((Kind::Tv.x(), Kind::Tv.y()), ("v", "T"));
        assert_eq!((Kind::Hs.x(), Kind::Hs.y()), ("s", "h"));
        assert_eq!((Kind::Pt.x(), Kind::Pt.y()), ("T", "P"));
        assert_eq!(Kind::Ph.name(), "PH");
    }

    #[test]
    fn nice_log_values_walk_the_one_two_five_progression() {
        assert_eq!(
            nice_log_values(1.0, 100.0, 10),
            vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]
        );
        // Thinned to 3: indices round(0*6/2)=0, round(1*6/2)=3, round(2*6/2)=6.
        assert_eq!(nice_log_values(1.0, 100.0, 3), vec![1.0, 10.0, 100.0]);
        // Non-finite limits (no backend) yield nothing rather than spinning.
        assert!(nice_log_values(f64::NAN, 10.0, 5).is_empty());
        assert!(nice_log_values(1.0, f64::NAN, 5).is_empty());
    }

    #[test]
    fn nice_linear_values_pick_a_round_step() {
        // raw step 100/8 = 12.5 -> magnitude 10 -> first m with m*10 >= 12.5 is
        // 2 -> step 20, from ceil(0/20)*20 = 0.
        assert_eq!(
            nice_linear_values(0.0, 100.0, 8),
            vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]
        );
        assert!(nice_linear_values(0.0, f64::NAN, 8).is_empty());
    }

    #[test]
    fn thin_reproduces_the_java_float_index_arithmetic() {
        let values: Vec<f64> = (0..10).map(f64::from).collect();
        assert_eq!(thin(values.clone(), 4), vec![0.0, 3.0, 6.0, 9.0]);
        assert_eq!(thin(values.clone(), 20), values);
    }

    #[test]
    fn pressure_labels_match_the_java_formatter() {
        assert_eq!(format_pressure(1.0e6), "1 MPa");
        assert_eq!(format_pressure(2.5e6), "2.5 MPa");
        assert_eq!(format_pressure(1.0e5), "100 kPa");
        assert_eq!(format_pressure(101_325.0), "101.325 kPa");
        assert_eq!(format_pressure(611.657), "611.657 Pa");
        assert_eq!(format_pressure(500.0), "500 Pa");
    }

    #[test]
    fn java_round_is_floor_of_x_plus_half() {
        assert_eq!(java_round(372.5), 373);
        assert_eq!(java_round(372.4999), 372);
        assert_eq!(java_round(-0.5), 0);
    }

    /// With no property backend the constructor fails once, naming the fluid —
    /// it does not hand back a diagram full of gaps.
    #[test]
    fn generate_without_a_backend_fails_at_the_constants() {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::uninstall();
        let err = generate("Water", "T-s").unwrap_err().to_string();
        restore_after(previous);
        assert!(err.contains("Water"), "{err}");
        assert!(err.contains("Ttriple"), "{err}");
    }

    fn restore_after(previous: Option<Arc<dyn RealFluid>>) {
        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }
    }

    /// An analytic stand-in fluid: enough of a `PropsSI` to exercise every
    /// sweep and prove the curve *structure* (families, counts, gap encoding)
    /// is the Java's. The numbers are not physical and are not asserted on.
    struct ToyFluid;

    impl ToyFluid {
        fn tsat(p: f64) -> f64 {
            300.0 + 30.0 * libm::log(p / 1.0e5)
        }
    }

    impl RealFluid for ToyFluid {
        fn props1_si(&self, _fluid: &str, param: &str) -> Result<f64> {
            match param {
                "Ttriple" => Ok(280.0),
                "Tcrit" => Ok(500.0),
                "ptriple" => Ok(1.0e4),
                "pcrit" => Ok(4.0e6),
                other => Err(FreesError::property(format!("no {other}"))),
            }
        }

        fn props_si(
            &self,
            output: &str,
            name1: &str,
            value1: f64,
            name2: &str,
            value2: f64,
            _fluid: &str,
        ) -> Result<f64> {
            let mut t = f64::NAN;
            let mut p = f64::NAN;
            let mut q = f64::NAN;
            for (k, v) in [(name1, value1), (name2, value2)] {
                match k {
                    "T" => t = v,
                    "P" => p = v,
                    "Q" => q = v,
                    // Entropy/density inputs are inverted analytically.
                    "S" => t = libm::exp(v / 1000.0),
                    "D" => p = v * 300.0,
                    _ => {}
                }
            }
            if q.is_finite() {
                // A (T,Q) or (P,Q) flash sits on the saturation line.
                if t.is_finite() {
                    p = 1.0e5 * libm::exp((t - 300.0) / 30.0);
                } else if p.is_finite() {
                    t = Self::tsat(p);
                }
            }
            if !t.is_finite() || !p.is_finite() {
                return Err(FreesError::property("toy: underdetermined".to_string()));
            }
            let value = match output {
                "T" => t,
                "P" => p,
                "Smass" | "S" => 1000.0 * libm::log(t),
                "Hmass" | "H" => 1000.0 * t,
                "Dmass" | "D" => p / (287.0 * t),
                other => return Err(FreesError::property(format!("toy: no {other}"))),
            };
            Ok(value)
        }
    }

    fn with_toy<T>(body: impl FnOnce() -> T) -> T {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::install(Arc::new(ToyFluid));
        let out = body();
        restore_after(previous);
        out
    }

    #[test]
    fn a_ts_diagram_has_the_java_curve_inventory() {
        with_toy(|| {
            let d = generate("Toy", "T-s").unwrap();
            assert_eq!(d.kind, "TS");
            assert_eq!((d.x_property, d.y_property), ("s", "T"));
            assert!(!d.x_log && !d.y_log);
            // One dome curve, both branches: 2 x DOME_POINTS samples.
            assert_eq!(d.dome.len(), 1);
            assert_eq!(d.dome[0].family, "dome");
            assert_eq!(d.dome[0].x.len(), 2 * DOME_POINTS);
            assert_eq!(d.dome[0].y.len(), 2 * DOME_POINTS);
            // Nine quality lines plus the isobars.
            let quality: Vec<&Curve> = d
                .isolines
                .iter()
                .filter(|c| c.family == "quality")
                .collect();
            assert_eq!(quality.len(), 9);
            assert_eq!(quality[0].label, "x = 0.1");
            assert_eq!(quality[8].label, "x = 0.9");
            assert!(quality.iter().all(|c| c.x.len() == CURVE_POINTS));
            let isobars: Vec<&Curve> = d.isolines.iter().filter(|c| c.family == "isobar").collect();
            assert!(
                !isobars.is_empty() && isobars.len() <= 7,
                "{}",
                isobars.len()
            );
            assert_eq!(d.markers.len(), 1);
            assert_eq!(d.markers[0].label, "Critical point");
        });
    }

    #[test]
    fn a_ph_diagram_adds_isotherms_and_isentropes_and_logs_the_y_axis() {
        with_toy(|| {
            let d = generate("Toy", "P-h").unwrap();
            assert!(!d.x_log && d.y_log);
            assert!(d.isolines.iter().any(|c| c.family == "isotherm"));
            assert_eq!(
                d.isolines
                    .iter()
                    .filter(|c| c.family == "isentrope")
                    .count(),
                7
            );
            assert!(!d.isolines.iter().any(|c| c.family == "isobar"));
        });
    }

    #[test]
    fn a_pt_diagram_keeps_one_dome_branch_and_no_quality_lines() {
        with_toy(|| {
            let d = generate("Toy", "P-T").unwrap();
            assert_eq!(d.dome[0].x.len(), DOME_POINTS);
            assert!(d.isolines.is_empty());
        });
    }

    /// The cold-anchor walk (ledger item 38) must not move an anchor that
    /// already answers — that is what makes it strictly additive rather than a
    /// change to every fluid's chart. `ToyFluid` answers everywhere, so every
    /// isobar must still start at exactly `S(P, Ttriple + 0.5)`, the Java's
    /// anchor, to the last bit.
    #[test]
    fn the_cold_anchor_walk_is_inert_for_the_fluids_that_answer_at_once() {
        with_toy(|| {
            let d = generate("Toy", "T-s").unwrap();
            // The toy's Ttriple is 280.0, so the Java anchor is 280.5 K and the
            // T-s x-axis carries the entropy fed into the (P,S) flash.
            let java_anchor = 1000.0 * libm::log(280.5);
            for isobar in d.isolines.iter().filter(|c| c.family == "isobar") {
                assert_eq!(
                    isobar.x[0],
                    Some(java_anchor),
                    "isobar '{}' moved off the Java cold anchor",
                    isobar.label
                );
            }
        });
    }

    /// ...and it must rescue a curve the Java drops, for a fluid whose melting
    /// line refuses that anchor. `Frozen` is CO2's geometry in miniature: a
    /// steep melting line out of a high-pressure triple point, refusing exactly
    /// as CoolProp does.
    #[test]
    fn the_cold_anchor_walk_rescues_an_isobar_the_java_leaves_empty() {
        struct Frozen;
        impl Frozen {
            /// `Tmelt(p)`: 280 K at the 5e5 Pa triple point, rising steeply.
            fn t_melt(p: f64) -> f64 {
                280.0 + (p - 5.0e5) / 1.0e6
            }
        }
        impl RealFluid for Frozen {
            fn props1_si(&self, _fluid: &str, param: &str) -> Result<f64> {
                match param {
                    "Ttriple" => Ok(280.0),
                    "Tcrit" => Ok(500.0),
                    "ptriple" => Ok(5.0e5),
                    "pcrit" => Ok(4.0e6),
                    other => Err(FreesError::property(format!("no {other}"))),
                }
            }
            fn props_si(
                &self,
                output: &str,
                n1: &str,
                v1: f64,
                n2: &str,
                v2: f64,
                fluid: &str,
            ) -> Result<f64> {
                // Refuse solid states on the (P,T) route, exactly as CoolProp
                // does ("we don't support T below Tmelt(p)"); otherwise defer to
                // the analytic toy.
                let (p, t) = match (n1, n2) {
                    ("P", "T") => (v1, v2),
                    ("T", "P") => (v2, v1),
                    _ => (f64::NAN, f64::NAN),
                };
                if t.is_finite() && t < Frozen::t_melt(p) {
                    return Err(FreesError::property(format!(
                        "we don't support T [{t} K] below Tmelt(p) [{} K]",
                        Frozen::t_melt(p)
                    )));
                }
                ToyFluid.props_si(output, n1, v1, n2, v2, fluid)
            }
        }

        let _guard = propfun::test_swap_guard();
        let previous = propfun::install(Arc::new(Frozen));
        let d = generate("Frozen", "T-s").unwrap();
        restore_after(previous);

        let isobars: Vec<&Curve> = d.isolines.iter().filter(|c| c.family == "isobar").collect();
        assert!(!isobars.is_empty());
        // Every isobar is drawn, including the ones whose 280.5 K anchor is
        // solid (any p above 1.5e6, where Tmelt > 280.5).
        for c in &isobars {
            assert_eq!(c.x.len(), CURVE_POINTS, "isobar '{}' is empty", c.label);
            assert!(
                c.x.iter().any(Option::is_some),
                "isobar '{}' is all gaps",
                c.label
            );
        }
        // The rescued anchor sits on the melting line, not at the Java's fixed
        // 280.5 K: for the 2 MPa isobar, Tmelt = 281.5 K.
        let two_mpa = isobars
            .iter()
            .find(|c| c.label == "2 MPa")
            .expect("a 2 MPa isobar");
        let anchor_t = libm::exp(two_mpa.x[0].unwrap() / 1000.0);
        assert!(
            (anchor_t - 281.5).abs() < 1.0e-6,
            "2 MPa isobar starts at {anchor_t} K, want Tmelt = 281.5 K"
        );
    }

    /// The gap encoding is the whole reason a partial backend is safe: a point
    /// the backend declines becomes `(None, None)`, never an interpolated
    /// guess.
    #[test]
    fn declined_points_become_null_gaps_in_both_axes() {
        struct HalfBlind;
        impl RealFluid for HalfBlind {
            fn props1_si(&self, _fluid: &str, param: &str) -> Result<f64> {
                match param {
                    "Ttriple" => Ok(280.0),
                    "Tcrit" => Ok(500.0),
                    "ptriple" => Ok(1.0e4),
                    "pcrit" => Ok(4.0e6),
                    _ => Err(FreesError::property("no".to_string())),
                }
            }
            fn props_si(
                &self,
                _output: &str,
                _n1: &str,
                _v1: f64,
                _n2: &str,
                _v2: f64,
                _fluid: &str,
            ) -> Result<f64> {
                Err(FreesError::property("declined".to_string()))
            }
        }
        let _guard = propfun::test_swap_guard();
        let previous = propfun::install(Arc::new(HalfBlind));
        let d = generate("Blind", "T-s").unwrap();
        restore_after(previous);
        assert!(d.dome[0].x.iter().all(Option::is_none));
        assert!(d.dome[0].y.iter().all(Option::is_none));
        assert_eq!(d.dome[0].x.len(), d.dome[0].y.len());
        assert!(d.markers[0].x.is_nan());
    }
}
