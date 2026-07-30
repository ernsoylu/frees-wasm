//! wasm-bindgen boundary for the frees engine.
//!
//! Deliberately thin: it owns the JS type conversions and nothing else. All
//! engine logic lives in `frees-core`, which knows nothing about wasm.
//!
//! Results cross the boundary as JSON strings rather than structured
//! `JsValue`s, and the strings are **exactly the REST wire shapes** the
//! frontend already parses (`web/src/api.ts` — `SolveResponse`,
//! `CheckResponse`; the Java originals are
//! `../frEES/backend/web/.../SolveController.java` and `CheckController.java`
//! over the DTOs in `core/.../api/SolveDtos.java`). The worker shim can feed
//! them to the same `JSON.parse` path the fetch layer used, with no
//! translation layer in between.
//!
//! Failure discipline (the Java controllers' contract): *every* document
//! problem is data, never a JS exception —
//!
//! * a syntax error is `success:false` / `solvable:false` with the
//!   `"Syntax error: …"` message, `errorLine` and `errors[]` (the Java
//!   400-with-body);
//! * a solver failure is `success:false` with the message in `error` and, when
//!   the engine named the failing Tarjan block, `failedBlockIndex` (the Java
//!   422 envelope);
//! * unit problems are warnings in `unitWarnings[]` and never block anything.

use std::collections::BTreeMap;

use frees_core::engine::{CheckReport, Solution};
use frees_core::units::registry::{UnitRegistry, UnitSystem};
use frees_core::{FreesError, SolverSettings, VariableOverride};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

/// Install the panic hook so a wasm trap arrives in the console as a readable
/// Rust backtrace instead of `unreachable executed`.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Engine version, for the worker handshake and the About dialog.
#[wasm_bindgen]
pub fn version() -> String {
    frees_core::VERSION.to_string()
}

// ---------------------------------------------------------------------------
// Request DTOs — the subset of the Java SolveRequest this port consumes
// ---------------------------------------------------------------------------

/// `{variableInfo: [...], stopCriteria: {...}}` — the request body
/// `POST /api/solve` and `POST /api/check` receive (`SolveController.SolveRequest`),
/// minus the fields whose machinery is not ported yet (`functionTables`,
/// `overrides`, `findAllSolutions`, …). Unknown fields are ignored, so the
/// frontend can keep sending its full request unchanged.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SolveRequest {
    variable_info: Vec<VariableInfoDto>,
    stop_criteria: Option<StopCriteriaDto>,
    /// `"SI"` | `"ENG_SI"` | `"ENGLISH"` — anything else falls back to SI, the
    /// Java `SolverApiSupport.unitSystem` catch.
    display_unit_system: Option<String>,
}

/// One row of the Variable Information window
/// (`SolverApiSupport.VariableInfoDto`; `VariableInfo` in `api.ts`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct VariableInfoDto {
    name: String,
    guess: Option<f64>,
    lower: Option<f64>,
    upper: Option<f64>,
    units: Option<String>,
    /// Accepted so the frontend's full rows deserialize; uncertainty
    /// propagation is not ported.
    #[allow(dead_code)]
    uncertainty: Option<f64>,
}

/// `StopCriteria` in `api.ts` (`SolverApiSupport.StopCriteriaDto`).
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StopCriteriaDto {
    max_iterations: Option<u32>,
    relative_residuals: Option<f64>,
    /// No counterpart in this port's Newton (its stop rule is residual-based);
    /// accepted and ignored.
    #[allow(dead_code)]
    change_in_variables: Option<f64>,
    /// No clock inside the solver on `wasm32-unknown-unknown`; accepted and
    /// ignored (the Java cap exists to protect a shared worker — in-browser
    /// the user only stalls their own tab).
    #[allow(dead_code)]
    elapsed_time_seconds: Option<f64>,
    #[allow(dead_code)]
    complex_mode: Option<bool>,
}

/// Server-side ceiling on the requested iteration budget
/// (`SolverApiSupport.MAX_ITERATIONS_CAP`).
const MAX_ITERATIONS_CAP: usize = 10_000;

fn parse_request(request_json: &str) -> Result<SolveRequest, String> {
    if request_json.trim().is_empty() {
        return Ok(SolveRequest::default());
    }
    serde_json::from_str(request_json).map_err(|e| format!("Invalid request: {e}"))
}

fn settings_of(request: &SolveRequest) -> SolverSettings {
    let mut settings = SolverSettings::default();
    if let Some(stop) = &request.stop_criteria {
        if let Some(iterations) = stop.max_iterations {
            settings.max_iterations = (iterations as usize).clamp(1, MAX_ITERATIONS_CAP);
        }
        if let Some(tolerance) = stop.relative_residuals {
            if tolerance.is_finite() && tolerance > 0.0 {
                settings.rel_tolerance = tolerance;
            }
        }
    }
    settings
}

fn overrides_of(request: &SolveRequest) -> Vec<VariableOverride> {
    request
        .variable_info
        .iter()
        .filter(|dto| !dto.name.trim().is_empty())
        .map(|dto| VariableOverride {
            name: dto.name.clone(),
            guess: dto.guess,
            lower: dto.lower,
            upper: dto.upper,
            unit: dto.units.clone(),
        })
        .collect()
}

/// `SolverApiSupport.unitSystem`: parse the requested display system,
/// defaulting to SI on absence or an unknown value.
fn unit_system_of(request: &SolveRequest) -> UnitSystem {
    match request
        .display_unit_system
        .as_deref()
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("ENG_SI") => UnitSystem::EngSi,
        Some("ENGLISH") => UnitSystem::English,
        _ => UnitSystem::Si,
    }
}

/// `SolverApiSupport.unitsByVariable`: the Variable Information window's
/// explicit units, keyed by lowercase name — the units that win over every
/// derived or preferred display unit.
fn explicit_units_of(request: &SolveRequest) -> BTreeMap<String, String> {
    request
        .variable_info
        .iter()
        .filter(|dto| !dto.name.trim().is_empty())
        .filter_map(|dto| {
            dto.units
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(|u| (dto.name.trim().to_ascii_lowercase(), u.to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Clock — core leaves `stats.elapsed_ms` unset because wasm32-unknown-unknown
// has no std clock; the boundary measures with whatever the host offers.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// `Date.now()` — coarse, but present in every JS host (worker included)
    /// without needing a `js-sys` dependency.
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now_ms() -> f64;
}

fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        date_now_ms()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------
// POST /api/solve
// ---------------------------------------------------------------------------

/// Solve a frees document. `request_json` is the `SolveRequest` body (subset
/// honoured: `variableInfo`, `stopCriteria`; `""`/`"{}"` mean defaults).
/// Returns a `SolveResponse` JSON string — success or failure, never a JS
/// exception.
#[wasm_bindgen]
pub fn solve(source: &str, request_json: &str) -> String {
    let request = match parse_request(request_json) {
        Ok(request) => request,
        // A malformed request never reached the engine, so the failure carries
        // no block diagnostics — the synthetic wrapper keeps the envelope
        // builder on one signature.
        Err(message) => {
            let failure = frees_core::SolveFailure::from(FreesError::evaluation(message.clone()));
            return solve_failure(message, None, &failure, 0.0);
        }
    };
    let settings = settings_of(&request);
    let overrides = overrides_of(&request);

    let system = unit_system_of(&request);
    let explicit_units = explicit_units_of(&request);

    let started = now_ms();
    match frees_core::solve_with(source, &settings, &overrides) {
        Ok(solution) => solve_success(&solution, now_ms() - started, system, &explicit_units),
        Err(failure) => match &failure.error {
            FreesError::Parse { .. } => {
                // The Java 400: "Syntax error:" + message + the 1-based line.
                let line = failure.span().map(|span| span.line_col(source).0);
                solve_failure(
                    format!("Syntax error: {}", failure.to_string_message()),
                    line,
                    &failure,
                    now_ms() - started,
                )
            }
            // The Java 422 envelope, from the structured failure the engine
            // now carries (`SolveFailure` mirrors `SolverException`'s
            // `FailureState` + `partialResult`) — no message parsing.
            _ => solve_failure(
                failure.to_string_message(),
                None,
                &failure,
                now_ms() - started,
            ),
        },
    }
}

/// `variables[]` — `{name, value, units}` per solved unknown: the display
/// spelling, the value **in its display unit**, and that unit's name. A unit
/// the checker *derived as dimensionless* arrives from core as `"-"` (the
/// checker's explicit marker, same as Java's `UnitChecker`); a variable
/// **absent** from the units map gets `""`, the Java `toVariableDto` fallback
/// (`unitsByLowerName.getOrDefault(canonicalName, "")`) — the frontend renders
/// falsy units as its own em-dash placeholder.
fn variable_entries(
    solution: &Solution,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> Vec<Value> {
    solution
        .values
        .iter()
        .map(|(name, value)| {
            let unit = solution
                .inferred_units
                .get(name)
                .map(String::as_str)
                .unwrap_or("");
            let (display_value, display_unit) =
                to_display(name, *value, unit, system, explicit_units);
            json!({
                "name": display_of(&solution.display_names, name),
                "value": display_value,
                "units": display_unit,
            })
        })
        .collect()
}

/// Port of `SolverApiSupport.toDisplay` / `convertToDisplayUnit`: converts an
/// SI value into its display unit.
///
/// Precedence, verbatim from the Java:
/// 1. a blank or `"-"` unit passes through untouched (nothing to convert);
/// 2. an **explicit** unit from the Variable Information window
///    (`explicitUnits`, keyed lowercase) wins in every system — the value is
///    converted *into that unit's scale* and its text is kept;
/// 3. otherwise the system's preferred display unit for the dimension
///    (`UnitRegistry.preferredDisplayUnit`; the SI table is empty, so SI keeps
///    the recorded unit);
/// 4. otherwise the recorded unit itself (factor-1 SI names pass through);
/// 5. an unparseable unit string falls back to the SI value with the raw text.
fn to_display(
    name_lower: &str,
    si_value: f64,
    unit: &str,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> (f64, String) {
    if unit.is_empty() || unit == "-" {
        return (si_value, unit.to_string());
    }
    // The explicit unit's own text is what gets displayed, so it is what gets
    // parsed — the Java uses the same `unit` for both because `unitsByLowerName`
    // already overlays explicit units; this port overlays here instead.
    let effective = explicit_units
        .get(name_lower)
        .map(String::as_str)
        .unwrap_or(unit);
    let Ok(recorded) = UnitRegistry::parse_with_offset(effective) else {
        return (si_value, effective.to_string());
    };
    if explicit_units.contains_key(name_lower) {
        return (recorded.from_si(si_value), effective.to_string());
    }
    match UnitRegistry::preferred_display_unit(&recorded.dims, system) {
        Some(preferred) => {
            let value = (si_value - preferred.offset) / preferred.factor;
            (value, preferred.name)
        }
        None => (recorded.from_si(si_value), effective.to_string()),
    }
}

fn display_of<'a>(display_names: &'a BTreeMap<String, String>, name: &'a String) -> &'a String {
    display_names.get(name).unwrap_or(name)
}

fn solve_success(
    solution: &Solution,
    elapsed_ms: f64,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> String {
    let variables = variable_entries(solution, system, explicit_units);

    let blocks: Vec<Value> = solution
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            json!({
                "index": index,
                "equations": solution.block_equations.get(index).cloned().unwrap_or_default(),
                "variables": block
                    .variables
                    .iter()
                    .map(|v| display_of(&solution.display_names, v))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    // Non-finite residuals are dropped rather than serialized: JSON has no
    // NaN, and `api.ts` types `value` as a required number. (The Java failure
    // envelope applies the same finite filter.)
    let residuals: Vec<Value> = solution
        .residuals
        .iter()
        .filter(|r| r.residual.is_finite())
        .map(|r| json!({ "equation": r.equation, "value": r.residual }))
        .collect();

    let stats = json!({
        "equations": solution.residuals.len(),
        "unknowns": solution.values.len(),
        "blocks": solution.blocks.len(),
        "iterations": solution.stats.iterations,
        "elapsedMillis": elapsed_ms.max(0.0).round() as u64,
        "maxResidual": solution.stats.max_residual,
    });

    json!({
        "success": true,
        "variables": variables,
        "blocks": blocks,
        "residuals": residuals,
        "stats": stats,
        // A single solve yields exactly one solution (the Java Result contract:
        // "single-solve returns exactly one"); all-roots solving is not ported.
        "solutions": [{
            "variables": variables,
            "maxResidual": solution.stats.max_residual,
        }],
        "unitWarnings": solution.unit_warnings,
        "error": null,
        "errorLine": null,
        "failedBlockIndex": null,
    })
    .to_string()
}

/// The `SolveResponse.failure` envelope. When the failure carries partial
/// diagnostics (a block-loop stall), the Java 422 shape is reproduced: the
/// full block structure, the finite residuals at the stalled iterate, populated
/// stats, and `failedBlockIndex` — so `SolveDiagnostics` can render which block
/// gave up and what the residuals looked like. Pre-block failures (syntax,
/// structural) keep the empty envelope with `stats: null`, exactly like the
/// Java `SolveResponse.failure`.
fn solve_failure(
    error: String,
    error_line: Option<usize>,
    failure: &frees_core::SolveFailure,
    elapsed_ms: f64,
) -> String {
    let (blocks, residuals, stats) = match &failure.partial {
        Some(partial) => {
            let blocks: Vec<Value> = partial
                .blocks
                .iter()
                .enumerate()
                .map(|(index, block)| {
                    json!({
                        "index": index,
                        "equations": partial
                            .block_equations
                            .get(index)
                            .cloned()
                            .unwrap_or_default(),
                        "variables": block
                            .variables
                            .iter()
                            .map(|v| display_of(&partial.display_names, v))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect();
            // The Java failure envelope filters residuals to finite values
            // (`Double.isFinite`) — NaN marks equations the stall left
            // unevaluable, and JSON has no NaN literal.
            let residuals: Vec<Value> = partial
                .residuals
                .iter()
                .filter(|r| r.residual.is_finite())
                .map(|r| json!({ "equation": r.equation, "value": r.residual }))
                .collect();
            let stats = json!({
                "equations": partial.residuals.len(),
                "unknowns": partial.display_names.len(),
                "blocks": partial.blocks.len(),
                "iterations": partial.stats.iterations,
                "elapsedMillis": elapsed_ms.max(0.0).round() as u64,
                "maxResidual": partial.stats.max_residual,
            });
            (blocks, residuals, stats)
        }
        None => (Vec::new(), Vec::new(), Value::Null),
    };

    json!({
        "success": false,
        "variables": [],
        "blocks": blocks,
        "residuals": residuals,
        "stats": stats,
        "solutions": [],
        "unitWarnings": [],
        "error": error,
        "errorLine": error_line,
        "failedBlockIndex": failure.failed_block_index,
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// POST /api/check
// ---------------------------------------------------------------------------

/// Check a document without solving. Returns a `CheckResponse` JSON string;
/// syntax errors arrive as `solvable:false` data with `errorLine`/`errors[]`,
/// exactly as `api.ts` expects from the Java 400-with-body.
#[wasm_bindgen]
pub fn check(source: &str, request_json: &str) -> String {
    let request = match parse_request(request_json) {
        Ok(request) => request,
        Err(message) => return check_failure(message),
    };
    let overrides = overrides_of(&request);

    match frees_core::check_with(source, &overrides) {
        Ok(report) => check_response(&report),
        // Only non-document problems (an invalid override row) surface as Err;
        // shaped like the Java 500-with-body, which api.ts reads the same way.
        Err(err) => check_failure(err.to_string_message()),
    }
}

fn check_response(report: &CheckReport) -> String {
    // `variables` carries display spellings (the Java CheckResult maps through
    // displayNames), and `inferredUnits` is keyed the same way — the frontend
    // looks units up with the names from `variables`.
    let variables: Vec<&String> = report
        .variables
        .iter()
        .map(|v| display_of(&report.display_names, v))
        .collect();

    let inferred_units: Map<String, Value> = report
        .inferred_units
        .iter()
        .map(|(name, unit)| {
            (
                display_of(&report.display_names, name).clone(),
                Value::String(unit.clone()),
            )
        })
        .collect();

    let errors: Vec<Value> = report
        .errors
        .iter()
        .map(|e| json!({ "line": e.line, "column": e.column, "message": e.message }))
        .collect();

    json!({
        "solvable": report.solvable,
        "equations": report.equation_count,
        "unknowns": report.unknown_count,
        "variables": variables,
        "unitWarnings": report.unit_warnings,
        "inferredUnits": inferred_units,
        "message": report.message,
        "errorLine": report.error_line,
        "errors": errors,
    })
    .to_string()
}

/// The empty-bodied CheckResponse the Java error paths return (bad request,
/// invalid override): counts at zero and the reason in `message`.
fn check_failure(message: String) -> String {
    json!({
        "solvable": false,
        "equations": 0,
        "unknowns": 0,
        "variables": [],
        "unitWarnings": [],
        "inferredUnits": {},
        "message": message,
        "errorLine": null,
        "errors": [],
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Tests — every assertion here is about the *wire shape*: the exact camelCase
// keys and JSON types `web/src/api.ts` declares.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{check, solve, version};
    use serde_json::Value;

    fn parsed(payload: &str) -> Value {
        serde_json::from_str(payload).expect("boundary output must be valid JSON")
    }

    /// The named field exists AND has the expected JSON type — `api.ts` types
    /// are structural, so both halves matter.
    fn assert_key(value: &Value, key: &str, check: impl Fn(&Value) -> bool) {
        let field = value
            .get(key)
            .unwrap_or_else(|| panic!("missing key {key:?} in {value}"));
        assert!(check(field), "key {key:?} has the wrong type: {field}");
    }

    fn variable<'a>(response: &'a Value, name: &str) -> &'a Value {
        response["variables"]
            .as_array()
            .expect("variables must be an array")
            .iter()
            .find(|v| v["name"] == name)
            .unwrap_or_else(|| panic!("no variable {name:?} in {response}"))
    }

    #[test]
    fn version_is_a_semver_string() {
        assert!(version().split('.').count() >= 3);
    }

    // ── SolveResponse: displayUnitSystem conversion (SolverApiSupport.toDisplay) ──

    #[test]
    fn si_system_passes_si_values_through() {
        // No explicit units, SI system: the preferred table is empty, so the
        // recorded (factor-1 SI) unit passes through — 140 kPa stays 140000 Pa.
        let v = parsed(&solve(
            "P = 140 [kPa]\nQ = P * 2\n",
            r#"{"displayUnitSystem": "SI"}"#,
        ));
        assert_eq!(variable(&v, "P")["value"], 140000.0, "{v}");
        assert_eq!(variable(&v, "P")["units"], "Pa", "{v}");
    }

    #[test]
    fn explicit_variable_info_units_win_in_every_system() {
        // The Variable Information window declares kPa: the value converts into
        // that unit and its text is kept — even in the SI system (the Java
        // explicitUnits branch runs before the preferred-table lookup).
        let request = r#"{
            "displayUnitSystem": "SI",
            "variableInfo": [{"name": "P", "units": "kPa"}]
        }"#;
        let v = parsed(&solve("P = 140 [kPa]\nQ = P * 2\n", request));
        assert_eq!(variable(&v, "P")["value"], 140.0, "{v}");
        assert_eq!(variable(&v, "P")["units"], "kPa", "{v}");
        // Q has no explicit unit and keeps the SI path.
        assert_eq!(variable(&v, "Q")["value"], 280000.0, "{v}");
    }

    #[test]
    fn eng_si_prefers_kpa_for_pressure() {
        let v = parsed(&solve(
            "P = 140 [kPa]\n",
            r#"{"displayUnitSystem": "ENG_SI"}"#,
        ));
        assert_eq!(variable(&v, "P")["value"], 140.0, "{v}");
        assert_eq!(variable(&v, "P")["units"], "kPa", "{v}");
    }

    #[test]
    fn english_prefers_psi_for_pressure() {
        let v = parsed(&solve(
            "P = 140 [kPa]\n",
            r#"{"displayUnitSystem": "ENGLISH"}"#,
        ));
        let value = variable(&v, "P")["value"].as_f64().unwrap();
        assert!((value - 140000.0 / 6894.757293168).abs() < 1e-9, "{v}");
        assert_eq!(variable(&v, "P")["units"], "psi", "{v}");
    }

    #[test]
    fn unknown_system_and_dimensionless_fall_back_cleanly() {
        // An unrecognised system string is the Java valueOf catch → SI.
        let v = parsed(&solve("x = 2\n", r#"{"displayUnitSystem": "METRIC"}"#));
        assert_eq!(variable(&v, "x")["value"], 2.0, "{v}");
        // A dimensionless "-" and an absent unit both pass through untouched.
        let units = variable(&v, "x")["units"].as_str().unwrap();
        assert!(units.is_empty() || units == "-", "{v}");
    }

    // ── SolveResponse: a solving document with units ───────────────────────

    #[test]
    fn solve_emits_the_solve_response_wire_shape() {
        let out = solve("P = 140 [kPa]\nQ = P * 2\n", "{}");
        let v = parsed(&out);

        assert_eq!(v["success"], true);
        assert_key(&v, "variables", Value::is_array);
        assert_key(&v, "blocks", Value::is_array);
        assert_key(&v, "residuals", Value::is_array);
        assert_key(&v, "stats", Value::is_object);
        assert_key(&v, "solutions", Value::is_array);
        assert_key(&v, "unitWarnings", Value::is_array);
        assert_key(&v, "error", Value::is_null);

        // VariableResult: {name, value, units} — display spelling, SI value,
        // declared/derived unit.
        let p = variable(&v, "P");
        assert_key(p, "name", Value::is_string);
        assert_key(p, "value", Value::is_f64);
        assert_key(p, "units", Value::is_string);
        assert_eq!(p["value"].as_f64().unwrap(), 140_000.0);
        assert_eq!(p["units"], "Pa");
        // Q's unit is dimensionally derived, not declared anywhere.
        assert_eq!(variable(&v, "Q")["units"], "Pa");

        // BlockResult: {index, equations, variables}.
        let block = &v["blocks"][0];
        assert_key(block, "index", Value::is_u64);
        assert_key(block, "equations", Value::is_array);
        assert_key(block, "variables", Value::is_array);
        assert_eq!(block["index"], 0);
        assert_eq!(block["equations"][0], "P = 140 [kPa]");
        assert_eq!(block["variables"][0], "P");

        // ResidualResult: {equation, value}.
        let residual = &v["residuals"][0];
        assert_key(residual, "equation", Value::is_string);
        assert_key(residual, "value", Value::is_f64);

        // SolveStats: all six camelCase fields, all numbers.
        let stats = &v["stats"];
        assert_eq!(stats["equations"], 2);
        assert_eq!(stats["unknowns"], 2);
        assert_eq!(stats["blocks"], 2);
        assert_key(stats, "iterations", Value::is_u64);
        assert_key(stats, "elapsedMillis", Value::is_u64);
        assert_key(stats, "maxResidual", Value::is_f64);

        // SolutionResult: {variables, maxResidual}, exactly one for a single solve.
        let solutions = v["solutions"].as_array().unwrap();
        assert_eq!(solutions.len(), 1);
        assert_key(&solutions[0], "variables", Value::is_array);
        assert_key(&solutions[0], "maxResidual", Value::is_f64);
    }

    #[test]
    fn solve_reports_unit_warnings_without_blocking() {
        let out = solve("x = 2 [m]\ny = 3 [s]\nz = x + y\n", "{}");
        let v = parsed(&out);
        assert_eq!(v["success"], true);
        let warnings = v["unitWarnings"].as_array().unwrap();
        assert!(!warnings.is_empty());
        assert!(
            warnings[0].as_str().unwrap().contains("[m]"),
            "{warnings:?}"
        );
    }

    // ── SolveResponse: a parse error ───────────────────────────────────────

    #[test]
    fn a_parse_error_is_a_failure_envelope_not_an_exception() {
        let out = solve("a = 1\nb = = 2\n", "{}");
        let v = parsed(&out);

        assert_eq!(v["success"], false);
        assert!(
            v["error"].as_str().unwrap().starts_with("Syntax error: "),
            "{v}"
        );
        // 1-based editor line of the offending statement.
        assert_eq!(v["errorLine"], 2);
        assert_key(&v, "stats", Value::is_null);
        assert_key(&v, "variables", Value::is_array);
        assert_eq!(v["variables"].as_array().unwrap().len(), 0);
        assert_key(&v, "solutions", Value::is_array);
        assert_key(&v, "unitWarnings", Value::is_array);
        assert_key(&v, "failedBlockIndex", Value::is_null);
    }

    // ── SolveResponse: a DOF failure and a failed block ────────────────────

    #[test]
    fn a_dof_failure_is_a_failure_envelope_with_no_error_line() {
        let out = solve("m + n = 5\n", "{}");
        let v = parsed(&out);

        assert_eq!(v["success"], false);
        assert!(
            v["error"].as_str().unwrap().contains("underspecified"),
            "{v}"
        );
        // A whole-system problem points at no single line and no block.
        assert_key(&v, "errorLine", Value::is_null);
        assert_key(&v, "failedBlockIndex", Value::is_null);
        assert_key(&v, "stats", Value::is_null);
    }

    #[test]
    fn a_nonconvergent_block_names_its_failed_block_index() {
        // exp(x) = -1 has no real root; block 0 gives up.
        let out = solve("exp(x) = -1\n", "{}");
        let v = parsed(&out);
        assert_eq!(v["success"], false);
        assert_eq!(v["failedBlockIndex"], 0);
        assert_key(&v, "error", Value::is_string);
    }

    // ── SolveResponse: variableInfo overrides ──────────────────────────────

    #[test]
    fn variable_info_guesses_steer_the_solve() {
        // x^2 = 9 has two roots; the external guess picks the negative one.
        let request = r#"{
            "variableInfo": [
                {"name": "x", "guess": -3, "lower": null, "upper": null,
                 "units": null, "uncertainty": null}
            ],
            "stopCriteria": {"maxIterations": 250, "relativeResiduals": 1e-12,
                             "changeInVariables": 1e-15, "elapsedTimeSeconds": 3600}
        }"#;
        let v = parsed(&solve("x ^ 2 = 9\n", request));
        assert_eq!(v["success"], true, "{v}");
        let x = variable(&v, "x")["value"].as_f64().unwrap();
        assert!((x + 3.0).abs() < 1e-9, "expected -3, got {x}");
    }

    #[test]
    fn an_invalid_override_is_a_failure_envelope() {
        let request = r#"{"variableInfo": [{"name": "x", "guess": 5, "lower": 10, "upper": 0}]}"#;
        let v = parsed(&solve("x = 1\n", request));
        assert_eq!(v["success"], false);
        assert_key(&v, "error", Value::is_string);
    }

    #[test]
    fn garbage_request_json_is_a_failure_envelope() {
        let v = parsed(&solve("x = 1\n", "{not json"));
        assert_eq!(v["success"], false);
        assert!(v["error"].as_str().unwrap().contains("Invalid request"));
    }

    // ── CheckResponse ──────────────────────────────────────────────────────

    #[test]
    fn check_emits_the_check_response_wire_shape() {
        let out = check("Tin = 300 [K]\nT_out = Tin * 2\n", "{}");
        let v = parsed(&out);

        assert_eq!(v["solvable"], true);
        assert_eq!(v["equations"], 2);
        assert_eq!(v["unknowns"], 2);
        assert_key(&v, "variables", Value::is_array);
        assert_key(&v, "unitWarnings", Value::is_array);
        assert_key(&v, "inferredUnits", Value::is_object);
        assert_key(&v, "message", Value::is_string);
        assert_key(&v, "errorLine", Value::is_null);
        assert_key(&v, "errors", Value::is_array);

        // Display spellings, not lowercase canonical names …
        let names: Vec<&str> = v["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["T_out", "Tin"]);
        // … and inferredUnits keyed the same way, so the frontend's lookup by
        // the names in `variables` connects.
        assert_eq!(v["inferredUnits"]["Tin"], "K");
        assert_eq!(v["inferredUnits"]["T_out"], "K");
    }

    #[test]
    fn check_reports_a_syntax_error_with_line_and_errors() {
        let v = parsed(&check("a = 1\nb = 2\nc = = 3\n", "{}"));
        assert_eq!(v["solvable"], false);
        assert!(
            v["message"].as_str().unwrap().starts_with("Syntax error: "),
            "{v}"
        );
        assert_eq!(v["errorLine"], 3);
        let error = &v["errors"][0];
        assert_eq!(error["line"], 3);
        assert_key(error, "column", Value::is_u64);
        assert!(error["column"].as_u64().unwrap() >= 1);
        assert_key(error, "message", Value::is_string);
    }

    #[test]
    fn check_reports_a_dof_failure_as_unsolvable_data() {
        let v = parsed(&check("m + n = 5\n", "{}"));
        assert_eq!(v["solvable"], false);
        assert_eq!(v["equations"], 1);
        assert_eq!(v["unknowns"], 2);
        assert!(
            v["message"].as_str().unwrap().contains("underspecified"),
            "{v}"
        );
    }

    #[test]
    fn check_validates_variable_info_like_the_solve_path() {
        let request = r#"{"variableInfo": [{"name": "x", "guess": 5, "lower": 10, "upper": 0}]}"#;
        let v = parsed(&check("x = 1\n", request));
        assert_eq!(v["solvable"], false);
        assert_key(&v, "message", Value::is_string);
        assert_key(&v, "inferredUnits", Value::is_object);
    }
}
