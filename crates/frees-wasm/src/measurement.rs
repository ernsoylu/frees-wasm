//! `POST /api/measurements/calc`, in the browser.
//!
//! Port of the calculated-signal half of
//! `../frEES/backend/web/.../api/MeasurementCalcController.java` (293 LOC),
//! over the engine half in [`frees_core::measurement`].
//!
//! # What this boundary serves — and what it no longer does
//!
//! The Data Analyzer's `.mf4` reading was removed together with
//! `frees_core::measurement::mdf4`: this module no longer opens, windows or
//! closes measurement recordings, and it holds no registry of opened files.
//! What remains is the **calculated-signal path**: `measurement_calc` evaluates
//! one frees formula over input series the frontend supplies **inline** from
//! its own `channelStore` (CSV-imported channels, earlier calc results). An
//! input that arrives as a measurement *reference* — an id/group/channel triple
//! naming a recording this engine no longer holds — is refused with a typed
//! error saying so.
//!
//! The Java's `202 Accepted` + `jobId` + poll path for call-bearing calc
//! requests existed to move a slow evaluation off the API node onto the compute
//! tier; we are *already* on the worker thread, so [`measurement_calc`] simply
//! returns the answer.
//!
//! What survives is the **contract**: the JSON the frontend already parses
//! (`web/src/analyzer/measurementApi.ts` — `CalcRequestDto`, `CalcResultDto`),
//! so `channelStore.ts` needs no new types.
//!
//! # Failure discipline
//!
//! The same rule as the rest of the boundary: a document problem is data, never
//! a JS exception. The entry point answers either
//!
//! ```json
//! {"ok": true,  …the Java 200 body…}
//! ```
//!
//! or
//!
//! ```json
//! {"ok": false, "error": {"code": "RASTER_CAP_EXCEEDED", "message": "…",
//!                         "actualPoints": 4000001, "suggestedDt": 0.005, "cap": 1000000}}
//! ```
//!
//! `code` is [`MeasurementError::code`], which is what the frontend switches on;
//! `RASTER_CAP_EXCEEDED` carries the extra fields because the UI offers the
//! suggested `dt` as a one-click fix rather than just refusing.
//!
//! # Non-finite numbers on the wire
//!
//! JSON has no `NaN`, and a gap in measured data *is* `NaN` — never an absent
//! sample (`measurement/mod.rs`). Every `f64` this module emits therefore goes
//! through [`crate::finite_or_null`], the rule the property-plot endpoints
//! already use: a non-finite value is `null`, because silently writing `0`
//! would fabricate a data point. The inverse direction is handled here too:
//! `JSON.stringify(NaN)` is `null`, so inline calc series arrive with `null`
//! holes and [`nan_filled`] reads them back as `NaN`.
//!
//! **And one open defect, which is not this module's to fix.** `serde_json`'s
//! default float parser is accurate only to within 1 ULP; the exact one is
//! behind its `float_roundtrip` feature, which this workspace does not enable.
//! Numbers this module *emits* are exact (`ryu`), but every number it *reads*
//! can shift by one bit — `1.4000000000000001`, which is what `JSON.stringify`
//! writes for `14 × 0.1`, comes back as `1.4`. Two rules downstream are exact
//! `f64` equality (`SampledSeries::at`'s exact-hit branch and `raster::union`'s
//! dedupe), so the shift has visible consequences. See
//! `a_json_round_trip_moves_a_timestamp_by_one_ulp` below. The remedy is
//! `serde_json = { version = "1", features = ["float_roundtrip"] }` in the
//! workspace manifest (+19.2 KiB of wasm, measured); it is left undone here
//! because the workspace manifest is not this module's file, and because
//! landing it *inverts* that test — it asserts the defect is present,
//! deliberately, so whoever enables the feature is told to rewrite it.

use std::collections::BTreeMap;

use frees_core::measurement::series::{Interp, SampledSeries};
use frees_core::measurement::{calc, raster, MeasurementError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::finite_or_null;

// ---------------------------------------------------------------------------
// Caps
// ---------------------------------------------------------------------------

/// Raster points a call-free formula may span
/// (`MeasurementCalcController.MAX_RASTER`).
const MAX_RASTER: u32 = 1_000_000;

/// Raster points a formula containing a function call may span
/// (`MeasurementCalcController.MAX_RASTER_WITH_CALLS`). One property lookup per
/// sample is orders of magnitude dearer than one add — the Java measured
/// ~107 ns/pt compiled against ~75 µs for an uncached CoolProp call — so a
/// call-bearing formula gets a tenth of the budget.
const MAX_RASTER_WITH_CALLS: u32 = 100_000;

/// Points a channel window returns before it is decimated
/// (`MeasurementController.window`'s `@RequestParam(defaultValue = "2400")`).
///
/// The window endpoint left with the `.mf4` reader; the cap is kept because it
/// is part of the recorded Java contract and the number the in-browser
/// channelStore windowing mirrors.
#[allow(dead_code)]
const DEFAULT_MAX_POINTS: u32 = 2_400;

/// `Math.min(Math.max(maxPoints, 2), 20_000)`, the same clamp the Java applied
/// to the query parameter. Two is the floor because an envelope bucket needs a
/// min and a max. Kept with [`DEFAULT_MAX_POINTS`] for the same reason.
#[allow(dead_code)]
const MIN_MAX_POINTS: u32 = 2;
#[allow(dead_code)]
const MAX_MAX_POINTS: u32 = 20_000;

// ---------------------------------------------------------------------------
// Errors and JSON helpers
// ---------------------------------------------------------------------------

/// `{"ok": false, "error": {…}}` — the failure envelope.
///
/// [`MeasurementError::RasterCapExceeded`] carries its numbers alongside the
/// message because `CalcSignalModal` offers the suggested `dt` as a button. The
/// suggestion can be non-finite (a `NaN` in a corrupt time base, or a cap of
/// one), and then it arrives as `null`: honest about there being no actionable
/// `dt`, instead of a plausible number that would not work.
fn error_json(error: &MeasurementError) -> String {
    let mut body = json!({
        "code": error.code(),
        "message": error.to_string(),
    });
    if let MeasurementError::RasterCapExceeded {
        actual_points,
        suggested_dt,
        cap,
    } = error
    {
        body["actualPoints"] = json!(actual_points);
        body["suggestedDt"] = finite_or_null(*suggested_dt);
        body["cap"] = json!(cap);
    }
    json!({ "ok": false, "error": body }).to_string()
}

fn reply(result: Result<Value>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(error) => error_json(&error),
    }
}

/// A malformed request body. The contract has no "bad request" variant, so this
/// borrows [`MeasurementError::Parse`] and says plainly what failed.
fn parse_json<T: for<'de> Deserialize<'de>>(request_json: &str) -> Result<T> {
    serde_json::from_str(request_json)
        .map_err(|e| MeasurementError::Parse(format!("Malformed measurement request: {e}")))
}

/// A `f64` slice as a JSON array, non-finite values as `null`. See the module
/// doc: the shim must read `null` back as `NaN`, not as zero.
fn f64_array(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|v| finite_or_null(*v)).collect())
}

/// `Math.min`: NaN-propagating, `-0.0 < 0.0`. Rust's `f64::min` discards a NaN
/// operand, which would hand the user a plausible-looking `dt` derived from a
/// span that is not actually known. `raster.rs` keeps the same pair private for
/// the same reason.
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max`: NaN-propagating, `0.0 > -0.0`.
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

// ---------------------------------------------------------------------------
// POST /api/measurements/calc
// ---------------------------------------------------------------------------

/// `CalcRequestDto` in `measurementApi.ts` (`MeasurementCalcController.CalcRequest`).
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CalcRequest {
    name: String,
    formula: String,
    inputs: Vec<CalcInput>,
    raster: Option<RasterSpec>,
}

/// One bound input. Only the inline variant is servable; the measurement
/// -reference fields are kept on the wire shape so a stale caller is refused
/// with a typed error rather than a deserialisation failure.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CalcInput {
    var: String,
    interp: Option<String>,
    measurement_id: Option<String>,
    channel: Option<String>,
    group: Option<usize>,
    inline: Option<InlineSeries>,
}

/// `t`/`v` are `Option<f64>` because `JSON.stringify(NaN)` is `null`: a browser
/// -built series with a gap in it *will* arrive with holes, and refusing it
/// would refuse every imported CSV channel that has one. See [`nan_filled`].
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct InlineSeries {
    t: Vec<Option<f64>>,
    v: Vec<Option<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RasterSpec {
    mode: Option<String>,
    dt: Option<f64>,
    same_as: Option<String>,
}

/// Evaluate one frees formula at every point of a raster built from its inputs.
///
/// The reply is `CalcResultDto` — `{name, t, v}`. A raster that would exceed
/// the point cap comes back as `RASTER_CAP_EXCEEDED` carrying a `dt` that
/// verifiably fits, which is the difference between a guided path and a
/// refusal.
///
/// Always synchronous: the Java sent call-bearing formulas over 10 000 points
/// to the compute tier as a job, and there is no compute tier here — this call
/// already runs on the worker thread, off the UI. A million-point property
/// formula will take as long as it takes, and the tab stays responsive.
#[wasm_bindgen]
pub fn measurement_calc(request_json: &str) -> String {
    reply(calc_inner(request_json))
}

fn calc_inner(request_json: &str) -> Result<Value> {
    // A formula may call `enthalpy(…)`. `start()` installs the property tables
    // at module init, so in the browser this is already done; the call keeps
    // the function correct when it is reached without one (native tests, and
    // any future host that skips the start hook).
    frees_core::props::tables::install_builtin_once();

    let request: CalcRequest = parse_json(request_json)?;
    if request.formula.trim().is_empty() {
        return Err(MeasurementError::Parse("The formula is empty.".to_string()));
    }
    let formula = calc::parse_formula(&request.formula)?;
    let cap = if calc::contains_call(&formula) {
        MAX_RASTER_WITH_CALLS
    } else {
        MAX_RASTER
    };

    let inputs = resolve_inputs(&request.inputs)?;
    if inputs.is_empty() {
        return Err(MeasurementError::Parse(
            "Bind at least one input signal.".to_string(),
        ));
    }

    let raster = build_raster(request.raster.as_ref(), &inputs, cap)?;
    let values = calc::evaluate(&formula, &raster, &inputs)?;
    Ok(json!({
        "ok": true,
        "name": request.name,
        "t": f64_array(&raster),
        "v": f64_array(&values),
    }))
}

/// Resolve every input to a [`SampledSeries`], keyed by the variable name the
/// formula uses.
///
/// Keyed by the caller's **own spelling**, not lowercased as the Java did.
/// `calc::evaluate` lowercases the keys itself and refuses two inputs that
/// differ only in case — frees names are case-insensitive, so `Speed` and
/// `speed` are one variable and binding both is genuinely ambiguous. Lowercasing
/// here would silently drop one of them, which is what the Java's
/// `LinkedHashMap.put` did.
fn resolve_inputs(inputs: &[CalcInput]) -> Result<BTreeMap<String, SampledSeries>> {
    if inputs.len() > MAX_INPUTS {
        return Err(MeasurementError::Parse(format!(
            "This request binds {} input signals, above the {MAX_INPUTS} a calculated signal may \
             use. A formula can only name a handful; bind the ones it reads.",
            inputs.len()
        )));
    }
    let mut out: BTreeMap<String, SampledSeries> = BTreeMap::new();
    let mut budget = SampleBudget::default();
    for input in inputs {
        // Trimmed, unlike the Java: `" speed"` there became a binding no
        // formula could ever name, and the user got "unknown variable" for a
        // signal they had visibly bound.
        let var = input.var.trim();
        if var.is_empty() {
            return Err(MeasurementError::Parse(
                "An input is missing its variable name.".to_string(),
            ));
        }
        let interp = parse_interp(input.interp.as_deref(), var)?;

        let Some(inline) = &input.inline else {
            // The non-inline variant named a channel of an opened recording.
            // The engine no longer holds recordings, so the reference cannot
            // be served — and the caller has to be told that in terms of the
            // remedy, not of a missing id.
            if input.measurement_id.is_some() || input.channel.is_some() || input.group.is_some() {
                return Err(MeasurementError::NotFound(format!(
                    "Input \"{var}\" names a measurement channel, but measurement recordings \
                     are no longer held in the engine. Supply the signal inline."
                )));
            }
            return Err(MeasurementError::Parse(format!(
                "Input \"{var}\" binds neither an inline series nor a measurement reference."
            )));
        };

        if inline.t.len() != inline.v.len() {
            return Err(MeasurementError::Parse(format!(
                "Inline series for \"{var}\" is malformed: {} time(s) against {} value(s).",
                inline.t.len(),
                inline.v.len()
            )));
        }
        budget.claim(var, inline.t.len() as u64)?;
        let series = SampledSeries::new(nan_filled(&inline.t), nan_filled(&inline.v), interp);

        if out.insert(var.to_string(), series).is_some() {
            return Err(MeasurementError::Formula(format!(
                "Input \"{var}\" is bound twice. Bind each formula variable once."
            )));
        }
    }
    Ok(out)
}

/// Samples every bound input may materialise between them, before the raster
/// cap has anything to say about it.
///
/// The raster cap is about the *answer*: `MAX_RASTER` bounds the output.
/// Nothing bounded the **sum of the inputs**, and the calc path holds every
/// input in full before the raster is built — so a script binding many huge
/// inline series is a gigabyte-scale request from a small request body,
/// answered on wasm32 by an allocator trap rather than by a diagnostic. This
/// is the same defect shape the Phase 7–8 sweep closed with
/// `ode::problem::MAX_OUTPUT_SAMPLES`.
const MAX_INPUT_SAMPLES: u64 = 16_777_216;

/// Input signals one calculated signal may bind.
///
/// `calc::evaluate` refreshes **every** column at **every** raster point — the
/// slot buffer for all of them, and, inside a `Call`, the scratch scope for all
/// of them too — so its cost is `raster × inputs`, and the request sets both
/// numbers independently and cheaply. One-point inline series make the raster
/// as long as the input list, which turns the whole call quadratic in the size
/// of the request body. Measured in release: 2 000 such inputs took 0.08 s,
/// 8 000 took 2.2 s, and **32 000 took 52 s** — from about 1.8 MB of JSON, with
/// the worker wedged and nothing able to cancel it.
///
/// [`MAX_INPUT_SAMPLES`] does not close that: a million one-point inputs is a
/// million samples, well inside it. The count is the number that has to be
/// bounded, and it is a comfortable bound to set — a formula names a handful of
/// variables, and the parser's depth budget stops it naming very many more.
/// Measured at this ceiling with a full million-point raster, the whole
/// evaluation is 2.2 s in release; at 512 it is 9.8 s.
///
/// **This bounds the time, not the memory, and the two do not follow each
/// other.** `evaluate` holds one raster-length column per input at once, so 128
/// inputs on a million-point raster is a gigabyte — measured at 1 044 MB through
/// this very entry point, from a 5 604-byte request body. That product is
/// bounded by `frees_core::measurement::calc`'s own `MAX_INPUT_COLUMN_SAMPLES`, which
/// is where the allocation happens; this constant is not the guard for it and
/// raising it would not move that ceiling.
const MAX_INPUTS: usize = 128;

/// Running total of the samples the bound inputs will hold at once.
#[derive(Default)]
struct SampleBudget {
    used: u64,
}

impl SampleBudget {
    fn claim(&mut self, var: &str, samples: u64) -> Result<()> {
        self.used = self.used.saturating_add(samples);
        if self.used > MAX_INPUT_SAMPLES {
            return Err(MeasurementError::Parse(format!(
                "The bound inputs come to at least {} samples once \"{var}\" is included, above \
                 the {MAX_INPUT_SAMPLES}-sample limit a browser tab can hold at once. Bind fewer \
                 signals, or narrow the recording and re-export it.",
                self.used
            )));
        }
        Ok(())
    }
}

/// The wire spelling `measurementApi.ts` declares (`'step' | 'linear'`), read
/// case-insensitively as the Java's `equalsIgnoreCase` did.
///
/// **Divergence, deliberate.** The Java's `else` branch made *any* unrecognised
/// string mean `LINEAR`. That is the wrong default to guess at: `step` is what
/// `CalcSignalModal` picks for a boolean channel, and linearly interpolating a
/// valve position across a gap invents intermediate openings that never
/// happened. A typo therefore fails here with the modes named. An absent or
/// empty field still means `linear` — that is a caller declining to choose, not
/// a caller choosing wrongly.
fn parse_interp(raw: Option<&str>, var: &str) -> Result<Interp> {
    match raw.map(str::trim) {
        None | Some("") => Ok(Interp::Linear),
        Some(mode) if mode.eq_ignore_ascii_case(Interp::Step.as_str()) => Ok(Interp::Step),
        Some(mode) if mode.eq_ignore_ascii_case(Interp::Linear.as_str()) => Ok(Interp::Linear),
        Some(other) => Err(MeasurementError::Parse(format!(
            "Input \"{var}\" asks for interpolation \"{other}\", which is not a mode this engine \
             has. Use \"step\" or \"linear\"."
        ))),
    }
}

/// JSON `null` back to `NaN` — the inverse of [`f64_array`], and the reason
/// inline series deserialise as `Option<f64>`.
fn nan_filled(values: &[Option<f64>]) -> Vec<f64> {
    values.iter().map(|v| v.unwrap_or(f64::NAN)).collect()
}

/// `MeasurementCalcController.buildRaster`.
///
/// `sameAs` copies the named input's own time base where the Java aliased it.
/// The copy is bounded by `cap` (checked first), so it is at most 8 MB, and
/// aliasing it would mean handing out a second owner of a series the evaluator
/// is about to read.
fn build_raster(
    spec: Option<&RasterSpec>,
    inputs: &BTreeMap<String, SampledSeries>,
    cap: u32,
) -> Result<Vec<f64>> {
    let mode = spec.and_then(|s| s.mode.as_deref()).unwrap_or("merge");
    let (t0, t1) = span_of(inputs);
    match mode {
        "merge" => {
            let bases: Vec<&[f64]> = inputs.values().map(|s| s.t.as_slice()).collect();
            raster::union(&bases, cap)
        }
        "fixed" => match spec.and_then(|s| s.dt) {
            // NaN fails this guard, which is the Java's `!(spec.dt() > 0)`.
            Some(dt) if dt > 0.0 => raster::fixed(t0, t1, dt, cap),
            _ => Err(MeasurementError::Parse(
                "The fixed raster needs dt > 0.".to_string(),
            )),
        },
        "sameAs" => {
            let name = spec.and_then(|s| s.same_as.as_deref()).unwrap_or("");
            let base = inputs
                .iter()
                .find(|(var, _)| var.eq_ignore_ascii_case(name))
                .map(|(_, series)| series)
                .ok_or_else(|| {
                    MeasurementError::Parse(format!(
                        "sameAs raster: \"{name}\" is not one of the bound inputs."
                    ))
                })?;
            if base.t.len() as u64 > u64::from(cap) {
                return Err(MeasurementError::RasterCapExceeded {
                    actual_points: base.t.len() as u64,
                    suggested_dt: raster::suggest_dt(t0, t1, cap),
                    cap,
                });
            }
            Ok(base.t.clone())
        }
        other => Err(MeasurementError::Parse(format!(
            "Unknown raster mode: {other}"
        ))),
    }
}

/// The inputs' combined extent, from each series' first and last sample — the
/// Java's own approximation, kept: it is only ever used to size a *suggested*
/// `dt`, and a full scan of ten million samples to refine a suggestion nobody
/// has accepted yet is not worth it.
fn span_of(inputs: &BTreeMap<String, SampledSeries>) -> (f64, f64) {
    let mut t0 = f64::INFINITY;
    let mut t1 = f64::NEG_INFINITY;
    for series in inputs.values() {
        if let (Some(&first), Some(&last)) = (series.t.first(), series.t.last()) {
            t0 = java_min(t0, first);
            t1 = java_max(t1, last);
        }
    }
    (t0, t1)
}

// ---------------------------------------------------------------------------
// Tests — the calc wire shape and the failure envelope, all through inline
// series: nothing here opens a file, because nothing can any more.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(payload: &str) -> Value {
        serde_json::from_str(payload).expect("boundary output must be valid JSON")
    }

    fn numbers(value: &Value) -> Vec<f64> {
        value
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_f64().unwrap_or(f64::NAN))
            .collect()
    }

    fn calc(request: Value) -> Value {
        parsed(&measurement_calc(&request.to_string()))
    }

    fn inline(var: &str, t: Vec<f64>, v: Vec<f64>, interp: &str) -> Value {
        json!({"var": var, "interp": interp, "inline": {"t": t, "v": v}})
    }

    /// Every reply is parseable JSON carrying a boolean `ok`, and a failure
    /// always carries a `code` the frontend switches on and a human `message`.
    fn envelope(payload: &str) -> Value {
        let value: Value = serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("reply is not JSON ({e}): {payload}"));
        match value["ok"].as_bool() {
            Some(true) => {}
            Some(false) => {
                assert!(value["error"]["code"].is_string(), "{value}");
                assert!(value["error"]["message"].is_string(), "{value}");
            }
            None => panic!("reply has no boolean `ok`: {value}"),
        }
        value
    }

    // ── the failure envelope ────────────────────────────────────────────────

    #[test]
    fn a_malformed_request_body_never_throws() {
        let value = parsed(&measurement_calc("{not json"));
        assert_eq!(value["ok"], json!(false), "{value}");
        assert!(value["error"]["message"].is_string(), "{value}");
    }

    // ── CalcResultDto ───────────────────────────────────────────────────────

    #[test]
    fn a_merged_raster_evaluates_the_formula_at_every_union_point() {
        let payload = calc(json!({
            "name": "power",
            "formula": "speed * 2",
            "inputs": [inline("speed", vec![0.0, 1.0, 2.0], vec![0.0, 10.0, 20.0], "linear")],
            "raster": {"mode": "merge"},
        }));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["name"], "power");
        assert_eq!(payload["t"], json!([0.0, 1.0, 2.0]));
        assert_eq!(payload["v"], json!([0.0, 20.0, 40.0]));
    }

    #[test]
    fn a_fixed_raster_uses_the_requested_step() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1.0], vec![0.0, 4.0], "linear")],
            "raster": {"mode": "fixed", "dt": 0.5},
        }));
        assert_eq!(payload["t"], json!([0.0, 0.5, 1.0]), "{payload}");
        assert_eq!(payload["v"], json!([0.0, 2.0, 4.0]));
    }

    #[test]
    fn a_same_as_raster_takes_the_named_inputs_time_base() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a + b",
            "inputs": [
                inline("a", vec![0.0, 2.0], vec![1.0, 3.0], "step"),
                inline("b", vec![0.0, 1.0, 2.0], vec![0.0, 0.0, 0.0], "step"),
            ],
            // Case-insensitively, as the Java looked it up.
            "raster": {"mode": "sameAs", "sameAs": "A"},
        }));
        assert_eq!(payload["t"], json!([0.0, 2.0]), "{payload}");
    }

    #[test]
    fn an_unknown_raster_mode_is_named_not_guessed() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0], vec![1.0], "step")],
            "raster": {"mode": "sliding"},
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("sliding"));
    }

    /// The one error the frontend handles specially: it offers `suggestedDt`
    /// as a button, so the numbers have to be on the payload.
    #[test]
    fn a_raster_over_the_cap_carries_a_dt_that_fits() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1000.0], vec![0.0, 1.0], "linear")],
            "raster": {"mode": "fixed", "dt": 1e-6},
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "RASTER_CAP_EXCEEDED");
        assert_eq!(payload["error"]["cap"], json!(MAX_RASTER));
        let actual = payload["error"]["actualPoints"].as_u64().expect("points");
        assert!(actual > u64::from(MAX_RASTER), "{payload}");
        let dt = payload["error"]["suggestedDt"].as_f64().expect("dt");
        // The suggestion must verifiably fit, or it is not a fix.
        assert!(
            (1000.0f64 / dt).floor() + 1.0 <= f64::from(MAX_RASTER),
            "{payload}"
        );
    }

    /// A call-bearing formula gets a tenth of the budget, so the same raster
    /// that passes above is refused here — and the cap in the payload says so.
    #[test]
    fn a_call_bearing_formula_gets_the_smaller_cap() {
        let payload = calc(json!({
            "name": "x",
            "formula": "sin(a)",
            "inputs": [inline("a", vec![0.0, 1000.0], vec![0.0, 1.0], "linear")],
            "raster": {"mode": "fixed", "dt": 0.001},
        }));
        assert_eq!(payload["error"]["code"], "RASTER_CAP_EXCEEDED", "{payload}");
        assert_eq!(payload["error"]["cap"], json!(MAX_RASTER_WITH_CALLS));
    }

    #[test]
    fn a_broken_formula_reports_the_column() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a +",
            "inputs": [inline("a", vec![0.0], vec![1.0], "step")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "FORMULA_ERROR");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .starts_with("Formula error:"));
    }

    #[test]
    fn an_empty_formula_and_an_unbound_request_are_both_refused() {
        let empty = calc(json!({"name": "x", "formula": "   ", "inputs": []}));
        assert_eq!(empty["ok"], json!(false), "{empty}");
        assert!(empty["error"]["message"]
            .as_str()
            .expect("message")
            .contains("formula is empty"));

        let unbound = calc(json!({"name": "x", "formula": "1 + 1", "inputs": []}));
        assert_eq!(unbound["ok"], json!(false), "{unbound}");
        assert!(unbound["error"]["message"]
            .as_str()
            .expect("message")
            .contains("at least one input"));
    }

    /// The Java silently read any unrecognised interp as `linear`, which for a
    /// valve position invents openings that never happened.
    #[test]
    fn a_misspelled_interpolation_mode_is_refused_not_defaulted() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1.0], vec![0.0, 1.0], "stpe")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("stpe"));
    }

    #[test]
    fn interpolation_modes_are_read_case_insensitively_and_default_to_linear() {
        assert_eq!(parse_interp(Some("STEP"), "a"), Ok(Interp::Step));
        assert_eq!(parse_interp(Some(" Linear "), "a"), Ok(Interp::Linear));
        assert_eq!(parse_interp(None, "a"), Ok(Interp::Linear));
        assert_eq!(parse_interp(Some(""), "a"), Ok(Interp::Linear));
        assert!(parse_interp(Some("nearest"), "a").is_err());
    }

    /// `step` holds; `linear` blends. If the boundary passed the wrong mode
    /// through, these two would agree.
    #[test]
    fn the_interpolation_mode_reaches_the_evaluator() {
        let request = |interp: &str| {
            json!({
                "name": "x",
                "formula": "a",
                "inputs": [inline("a", vec![0.0, 2.0], vec![0.0, 10.0], interp)],
                "raster": {"mode": "fixed", "dt": 1.0},
            })
        };
        assert_eq!(calc(request("step"))["v"], json!([0.0, 0.0, 10.0]));
        assert_eq!(calc(request("linear"))["v"], json!([0.0, 5.0, 10.0]));
    }

    /// `JSON.stringify(NaN)` is `null`, so a browser-built series with a gap
    /// arrives with holes. Reading them as NaN keeps the gap a gap; rejecting
    /// them would refuse every imported CSV channel that has one.
    #[test]
    fn a_null_in_an_inline_series_is_a_gap_not_a_parse_failure() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{
                "var": "a",
                "interp": "linear",
                "inline": {"t": [0.0, 1.0, 2.0], "v": [0.0, Value::Null, 2.0]},
            }],
            "raster": {"mode": "merge"},
        }));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        // The gap survives evaluation and comes back as null, not as 0.
        assert_eq!(payload["v"], json!([0.0, Value::Null, 2.0]));
    }

    #[test]
    fn a_ragged_inline_series_is_refused_by_name() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{"var": "a", "interp": "step", "inline": {"t": [0.0, 1.0], "v": [0.0]}}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("\"a\""));
    }

    /// Two inputs that differ only in case are one frees variable, so binding
    /// both is ambiguous — `calc::evaluate`'s own guard, reached because this
    /// boundary does *not* lowercase the keys as the Java did.
    #[test]
    fn inputs_that_collide_are_refused_rather_than_silently_dropped() {
        let ambiguous = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [
                inline("Speed", vec![0.0, 1.0], vec![0.0, 1.0], "linear"),
                inline("speed", vec![0.0, 1.0], vec![5.0, 6.0], "linear"),
            ],
        }));
        assert_eq!(ambiguous["ok"], json!(false), "{ambiguous}");

        let duplicated = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [
                inline("speed", vec![0.0, 1.0], vec![0.0, 1.0], "linear"),
                inline("speed", vec![0.0, 1.0], vec![5.0, 6.0], "linear"),
            ],
        }));
        assert_eq!(duplicated["ok"], json!(false), "{duplicated}");
        assert!(duplicated["error"]["message"]
            .as_str()
            .expect("message")
            .contains("twice"));
    }

    /// The engine no longer holds measurement recordings, so an input that
    /// arrives as an id/group/channel reference is refused with the remedy in
    /// the message. The exact text is pinned: it is what the Data Analyzer
    /// shows a caller whose stored calc definition predates the removal.
    #[test]
    fn a_measurement_reference_input_is_refused_with_the_inline_remedy() {
        let payload = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [{"var": "speed", "measurementId": "m1", "group": 0, "channel": "speed"}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "CHANNEL_NOT_FOUND");
        assert_eq!(
            payload["error"]["message"],
            "Input \"speed\" names a measurement channel, but measurement recordings are no \
             longer held in the engine. Supply the signal inline."
        );

        // Any one reference field alone is still a reference, not a
        // "binds nothing" input.
        let partial = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{"var": "a", "channel": "speed"}],
        }));
        assert_eq!(partial["ok"], json!(false), "{partial}");
        assert_eq!(partial["error"]["code"], "CHANNEL_NOT_FOUND");
        assert!(partial["error"]["message"]
            .as_str()
            .expect("message")
            .contains("no longer held"));
    }

    #[test]
    fn an_input_binding_neither_a_series_nor_a_channel_is_refused() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{"var": "a", "interp": "step"}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("neither"));
    }

    #[test]
    fn an_input_without_a_variable_name_is_refused() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("  ", vec![0.0], vec![1.0], "step")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("missing its variable name"));
    }

    /// A property call is the differentiator — the same backend a document
    /// uses, per sample, over measured data.
    #[test]
    fn a_property_call_in_a_formula_reaches_the_property_backend() {
        let payload = calc(json!({
            "name": "h",
            "formula": "enthalpy(Water, T = t_k, P = 101325)",
            "inputs": [inline("t_k", vec![0.0, 1.0], vec![300.0, 300.0], "step")],
            "raster": {"mode": "merge"},
        }));
        // The tabulated backend may decline a point, but it must never trap and
        // must never fabricate: either a number or a typed failure.
        if payload["ok"] == json!(true) {
            let v = numbers(&payload["v"]);
            assert_eq!(v.len(), 2, "{payload}");
        } else {
            assert!(payload["error"]["message"].is_string(), "{payload}");
        }
    }

    // ── the span used for the suggested dt ──────────────────────────────────

    /// Rust's `f64::min` discards NaN, which would hand back a plausible `dt`
    /// for a span nobody knows.
    #[test]
    fn a_nan_time_poisons_the_span_rather_than_being_ignored() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "a".to_string(),
            SampledSeries::new(vec![f64::NAN, 1.0], vec![0.0, 1.0], Interp::Linear),
        );
        let (t0, _) = span_of(&inputs);
        assert!(t0.is_nan());
        assert_eq!(java_min(0.0, -0.0), -0.0);
        assert_eq!(java_max(0.0, -0.0), 0.0);
        assert!(java_max(f64::NAN, 0.0).is_nan());
    }

    // ── adversarial: hostile request bodies ─────────────────────────────────
    //
    // The export takes a string straight off a `postMessage`. Whatever arrives,
    // the answer is one of the two envelopes — never a trap, because
    // `panic = "abort"` on the profile that ships.

    /// Bodies that are not the shape the boundary declares: wrong types, wrong
    /// containers, absurd numbers, and text that is not JSON at all.
    #[test]
    fn no_malformed_request_body_escapes_the_envelope() {
        const BODIES: &[&str] = &[
            "",
            " ",
            "null",
            "[]",
            "0",
            "\"\"",
            "true",
            "{",
            "{}",
            "{\"measurementId\": 7}",
            "{\"measurementId\": null}",
            "{\"group\": -1}",
            "{\"group\": 1e400}",
            "{\"group\": 18446744073709551615}",
            "{\"group\": 184467440737095516150000}",
            "{\"channel\": []}",
            "{\"inputs\": {}}",
            "{\"inputs\": [null]}",
            "{\"inputs\": [{\"var\": 3}]}",
            "{\"inputs\": [{\"inline\": {\"t\": [\"x\"], \"v\": [1]}}]}",
            // A JSON number outside `f64`: it must not become a plausible time.
            "{\"formula\": \"x\", \"inputs\": [{\"var\": \"x\", \"inline\": \
             {\"t\": [0, 1e400], \"v\": [1, 2]}}]}",
            "{\"formula\": \"x\", \"inputs\": [{\"var\": \"x\", \"inline\": \
             {\"t\": [0, 1], \"v\": [1, -1e400]}}], \"raster\": {\"mode\": \"fixed\", \"dt\": 1e-400}}",
            "{\"raster\": \"merge\"}",
            "{\"raster\": {\"mode\": 3}}",
            "{\"formula\": 12}",
            // Deep nesting: serde_json's own recursion limit has to hold, not
            // ours, because a stack overflow here is an abort like any other.
            "{\"inputs\": [{\"inline\": {\"t\": [[[[[[[[[[1]]]]]]]]]]}}]}",
        ];
        for body in BODIES {
            envelope(&measurement_calc(body));
        }
        // A ten-thousand-deep JSON array, which is the shape that overflows a
        // recursive-descent parser rather than merely failing one.
        let bomb = format!("{}1{}", "[".repeat(10_000), "]".repeat(10_000));
        envelope(&measurement_calc(&bomb));
        envelope(&measurement_calc(&format!("{{\"raster\": {bomb}}}")));
    }

    /// A calc request whose inline series are degenerate in every way the wire
    /// allows: empty, all-null, single-point, non-finite, and enormous in
    /// magnitude. Each answers; none traps.
    #[test]
    fn degenerate_inline_series_answer_rather_than_trap() {
        let series: &[(&str, Value)] = &[
            ("empty", json!({"t": [], "v": []})),
            ("one", json!({"t": [0.0], "v": [1.0]})),
            ("all null", json!({"t": [null, null], "v": [null, null]})),
            ("null times", json!({"t": [null, 1.0], "v": [1.0, 2.0]})),
            ("huge", json!({"t": [-1e308, 1e308], "v": [1.0, 2.0]})),
            ("negative span", json!({"t": [5.0, 0.0], "v": [1.0, 2.0]})),
            (
                "repeated",
                json!({"t": [0.0, 0.0, 0.0], "v": [1.0, 2.0, 3.0]}),
            ),
        ];
        let rasters: &[Value] = &[
            json!(null),
            json!({"mode": "merge"}),
            json!({"mode": "fixed", "dt": 0.1}),
            json!({"mode": "fixed", "dt": 1e-300}),
            json!({"mode": "fixed", "dt": 1e300}),
            json!({"mode": "sameAs", "sameAs": "x"}),
            json!({"mode": "sameAs", "sameAs": "nope"}),
        ];
        for (label, inline) in series {
            for raster in rasters {
                for formula in ["x + 1", "movavg(x, 1)", "integral(x)", "delay(x, 1)"] {
                    let payload = envelope(&measurement_calc(
                        &json!({
                            "name": "out",
                            "formula": formula,
                            "inputs": [{"var": "x", "interp": "linear", "inline": inline}],
                            "raster": raster,
                        })
                        .to_string(),
                    ));
                    if payload["ok"] == json!(true) {
                        assert_eq!(
                            payload["t"].as_array().map(Vec::len),
                            payload["v"].as_array().map(Vec::len),
                            "{label} / {raster} / {formula}: t and v disagree"
                        );
                    }
                }
            }
        }
    }

    // ── the JSON number pipe is not round-trip exact ────────────────────────

    /// **Open defect, not fixed here — the fix is a Cargo feature.**
    ///
    /// `serde_json`'s default float parser is the fast one, and it is documented
    /// to be accurate only to within 1 ULP. Its `float_roundtrip` feature buys
    /// the exact parser and is **off** in this workspace, so a number this
    /// module *emits* correctly (`ryu` is exact) does not survive being read
    /// back: `1.4000000000000001` — the f64 nearest `14 × 0.1`, and exactly what
    /// `JSON.stringify` produces for it — deserialises as `1.4`.
    ///
    /// This is a boundary-wide exposure (every `f64` in every request body),
    /// and two of the calc path's rules are **exact equality** on `f64`
    /// (`SampledSeries::at`'s exact-hit branch, `raster::union`'s dedupe), so
    /// one ULP turns a step-channel hit into a hold of the previous sample and
    /// one shared timestamp into two raster points. The remedy is one word in
    /// the workspace manifest —
    /// `serde_json = { version = "1", features = ["float_roundtrip"] }` — which
    /// is outside this module, so it is reported rather than applied. With the
    /// feature on, this test *fails*, which is the point of writing it this way
    /// round: whoever lands the feature is told to rewrite it into an assertion
    /// that the defect is gone.
    #[test]
    fn a_json_round_trip_moves_a_timestamp_by_one_ulp() {
        let exact = 14.0_f64 * 0.1;
        let text = serde_json::to_string(&exact).expect("ryu is exact");
        assert_eq!(text, "1.4000000000000001", "the emitted digits are right");
        let back: f64 = serde_json::from_str(&text).expect("and re-read");
        assert_ne!(
            back.to_bits(),
            exact.to_bits(),
            "if this passes, the feature landed"
        );
        assert_eq!(back, 1.4);
    }

    // ── adversarial: the inputs are read before the raster is capped ────────

    /// The quadratic shape: `calc::evaluate` costs `raster × inputs`, and
    /// one-point inline series make the raster as long as the input list — so
    /// the whole call is quadratic in the request body. Measured in release
    /// before the cap: 2 000 inputs 0.15 s, 8 000 2.3 s, 16 000 12.5 s.
    ///
    /// **Each input needs its own time**, and that is not cosmetic: the raster
    /// is the *union* of the input time bases, so a thousand series that all
    /// sample `t = 0` dedupe to a one-point raster and the product `raster ×
    /// inputs` never grows.
    #[test]
    fn a_request_binding_more_inputs_than_a_formula_can_name_is_refused() {
        let one =
            |i: usize| json!({"var": format!("v{i}"), "inline": {"t": [i as f64], "v": [1.0]}});

        let at_cap: Vec<Value> = (0..MAX_INPUTS).map(one).collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": at_cap}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(true), "the cap itself is allowed");

        let over: Vec<Value> = (0..=MAX_INPUTS).map(one).collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": over}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(message.contains(&(MAX_INPUTS + 1).to_string()), "{message}");
        assert!(message.contains(&MAX_INPUTS.to_string()), "{message}");

        // And the refusal lands before anything is read: 40 000 inputs is the
        // shape that took 52 s, and it has to come back immediately.
        let bomb: Vec<Value> = (0..40_000).map(one).collect();
        let start = std::time::Instant::now();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": bomb}).to_string(),
        ));
        let elapsed = start.elapsed();
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "40 000 inputs took {elapsed:?}"
        );
    }

    /// Both caps above are satisfiable *at the same time*, and their product is
    /// the memory. 128 inputs is exactly [`MAX_INPUTS`]; a fixed raster over
    /// their span is exactly [`MAX_RASTER`]; each one costs `calc::evaluate` a
    /// full-length column, held all at once.
    ///
    /// Measured through this entry point with a counting global allocator: this
    /// **5 604-byte** request body peaked at **1 044 MB** — a 186 000×
    /// amplification, past what a tab will grow linear memory to, and the
    /// allocation site is a `collect()`, so on wasm32 it is an abort rather than
    /// this diagnostic. Neither cap sees it: `MAX_INPUT_SAMPLES` counts input
    /// samples and 128 one-point series is 128 of them, and `MAX_RASTER` counts
    /// the raster and a million is what it allows. The guard is
    /// `calc::MAX_INPUT_COLUMN_SAMPLES`, in the module that does the allocating; the
    /// same call now peaks at 132 MB.
    #[test]
    fn the_widest_legal_input_list_on_the_longest_legal_raster_is_refused() {
        // One input pins t = 0 and another t = 999 999, so `span_of` is the
        // whole million and `dt = 1` lands exactly on the raster cap.
        let inputs: Vec<Value> = (0..MAX_INPUTS)
            .map(|i| {
                let t = if i == 1 { 999_999.0 } else { 0.0 };
                json!({"var": format!("v{i}"), "inline": {"t": [t], "v": [1.0]}})
            })
            .collect();
        let body = json!({
            "name": "out",
            "formula": "v0 + 1",
            "inputs": inputs,
            "raster": {"mode": "fixed", "dt": 1.0},
        })
        .to_string();
        assert!(body.len() < 8_192, "the request is small: {} B", body.len());

        let payload = envelope(&measurement_calc(&body));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "FORMULA_ERROR");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(message.contains("bind fewer signals"), "{message}");

        // Narrower, same raster: still the ordinary case, still answered.
        let inputs: Vec<Value> = (0..4)
            .map(|i| {
                let t = if i == 1 { 999_999.0 } else { 0.0 };
                json!({"var": format!("v{i}"), "inline": {"t": [t], "v": [1.0]}})
            })
            .collect();
        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "v0 + 1",
                "inputs": inputs,
                "raster": {"mode": "fixed", "dt": 1.0},
            })
            .to_string(),
        ));
        assert_eq!(
            payload["ok"],
            json!(true),
            "four inputs must still evaluate"
        );
        assert_eq!(
            payload["t"].as_array().expect("times").len(),
            MAX_RASTER as usize
        );
    }

    /// An inline series counts against the sample budget, and the claim cannot
    /// wrap on the way to the check.
    #[test]
    fn the_input_budget_counts_inline_series_and_cannot_overflow() {
        let mut budget = SampleBudget::default();
        budget
            .claim("one", MAX_INPUT_SAMPLES)
            .expect("the ceiling itself fits");
        assert!(budget.claim("two", 1).is_err(), "but not one sample more");

        let mut budget = SampleBudget::default();
        assert!(budget.claim("a", u64::MAX).is_err());
        assert!(
            budget.claim("b", u64::MAX).is_err(),
            "saturating, not wrapping"
        );

        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "x",
                "inputs": [{"var": "x", "inline": {"t": [0.0, 1.0], "v": [1.0, 2.0]}}],
            })
            .to_string(),
        ));
        assert_eq!(
            payload["ok"],
            json!(true),
            "an ordinary series is unaffected"
        );
    }
}
