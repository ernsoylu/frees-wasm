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

use frees_core::components::cyclepath;
use frees_core::components::metadata::{self, VariableRow};
use frees_core::engine::{CheckReport, Solution};
use frees_core::eval::Intrinsic;
use frees_core::units::registry::{UnitRegistry, UnitSystem};
use frees_core::{FreesError, SolverSettings, VariableOverride};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

mod measurement;
mod repl;

// The calculated-signal entry point lives in its own module; re-exported so
// the crate's Rust-side surface is flat, as it is for everything else here.
// (The `.mf4` open/window/close entry points were removed with the MDF4
// feature — signals now reach `measurement_calc` inline.)
pub use measurement::measurement_calc;

/// Install the panic hook so a wasm trap arrives in the console as a readable
/// Rust backtrace instead of `unreachable executed`, and install the property
/// backend so the very first `fluids()` call already sees it.
///
/// Which backend that is is decision D9
/// (`docs/decisions/0009-rustprop-backend.md`): **rustprop**, and in this bundle
/// only rustprop — the crate turns `frees-core`'s `linked-tables` feature off,
/// so the `(P,h)` artifacts are not in the module at all.
///
/// `frees_core` also installs it lazily from `solve`/`check`, so the module is
/// correct without this; doing it at start-up keeps the one-off cost off the
/// first solve and out of the fluid-list round trip.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
    frees_core::props::tables::install_builtin_once();
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
    /// The Preferences "fill missing properties" switch. When on, the solved
    /// state points are completed from the property backend and the cycle path
    /// connecting them is returned — the Java
    /// `SolveController.resolveFillMissing`, which is the *only* producer of
    /// `cyclePath`.
    fill_missing: Option<bool>,
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
    /// The `±` column: a stated 1-sigma uncertainty, which makes the variable
    /// an uncertainty *source* for the propagation pass.
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
            uncertainty: dto.uncertainty,
        })
        .collect()
}

/// `SolverApiSupport.unitSystem`: parse the requested display system,
/// defaulting to SI on absence or an unknown value.
fn unit_system_of(request: &SolveRequest) -> UnitSystem {
    match request.display_unit_system.as_deref() {
        Some(name) => parse_unit_system(name),
        None => UnitSystem::Si,
    }
}

/// The unit-system name as the frontend spells it, defaulting to SI on an
/// unknown value (the Java `SolverApiSupport.unitSystem` fallback).
fn parse_unit_system(name: &str) -> UnitSystem {
    match name.to_ascii_uppercase().as_str() {
        "ENG_SI" => UnitSystem::EngSi,
        "ENGLISH" => UnitSystem::English,
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
        Ok(mut solution) => {
            // `SolveController.resolveFillMissing`, at the Java position:
            // *before* the variable DTOs are built, so the injected state
            // properties become ordinary result rows, and before
            // `ComponentMetadata.build`, which resolves its bindings against
            // those rows.
            let cycle_path = fill_missing(&mut solution, source, request.fill_missing);
            // `SolveContextCache.put` — the REPL evaluates against the last
            // successful solve, so the workspace is refreshed here and nowhere
            // else. A failed solve leaves the previous workspace in place,
            // exactly as the Java cache does.
            repl::store_session(source, &solution, system, &explicit_units);
            solve_success(
                &solution,
                &cycle_path,
                now_ms() - started,
                system,
                &explicit_units,
            )
        }
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
fn variable_rows(
    solution: &Solution,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> (Vec<VariableRow>, Vec<Option<f64>>) {
    solution
        .values
        .iter()
        .map(|(name, value)| {
            // `SolveController.toVariableDto`, exactly: the row's key into both
            // the units map and the explicit-units map is
            // `e.getKey().toLowerCase()` where `e.getKey()` is already the
            // **display** name, not the canonical one.
            //
            // For every scalar variable the two are the same string (`T_out` →
            // `t_out`), which is why this reads as a no-op. It is not one for an
            // expanded component member: the units map is keyed by the flat
            // solver name `n1$i` while the display name is `n1.i`, so the lookup
            // misses and the member reports `""`. That miss is the Java's
            // behaviour and it is deliberate to reproduce it — `""` renders as
            // the frontend's em-dash placeholder and, crucially, keeps the value
            // in SI, whereas honouring the unit here would silently convert port
            // members into the preferred display unit on a path where the
            // reference engine does not.
            //
            // The check endpoint is where the Java *does* surface them
            // (`CheckController.addComponentMemberUnits` re-keys the member
            // units by display name into `inferredUnits`), and `check()` below
            // matches that; the two endpoints disagree in the reference engine
            // and they disagree here for the same reason.
            let display = display_of(&solution.display_names, name);
            let key = display.to_ascii_lowercase();
            let unit = solution
                .inferred_units
                .get(&key)
                .map(String::as_str)
                .unwrap_or("");
            // `toVariableDto`: the uncertainty is looked up by the row's own
            // key — the display name lowercased — into `Result.uncertainties`.
            let si_unc = solution.uncertainties.get(&key).copied();
            let (display_value, display_unit, display_unc) =
                to_display(&key, *value, si_unc, unit, system, explicit_units);
            (
                VariableRow {
                    name: display.clone(),
                    value: display_value,
                    units: display_unit,
                },
                display_unc,
            )
        })
        .unzip()
}

fn variable_entries(rows: &[VariableRow], uncertainties: &[Option<f64>]) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            // `VariableDto.uncertainty` is `Double`, and the Java's
            // `toVariableDto` reads `result.uncertainties().get(canonicalName)`
            // — a plain map lookup. `partitionVariables` zero-fills that map
            // over every variable of the system, so a document with no source
            // reports **0.0, not null**; only a row that is not a system
            // variable at all (the synthetic `uncertaintyof$…` rows the second
            // pass publishes) misses and reports null. Transcribed as-is.
            let unc = uncertainties
                .get(index)
                .copied()
                .flatten()
                .filter(|u| u.is_finite());
            json!({
                "name": row.name,
                "value": row.value,
                "units": row.units,
                "uncertainty": unc,
            })
        })
        .collect()
}

/// `SolveController.resolveFillMissing` — back-fill the missing fluid
/// properties of every detected state point and build the cycle path that
/// connects them.
///
/// Returns the cycle path (empty when the switch is off, when no property
/// backend is installed, or when fewer than two state points exist). The
/// solution is mutated in place, which is this port's answer to the Java's
/// rebuilt `Result`: `resolve_missing_properties` reports the names it *added*,
/// and each of those gets its SI unit stamped from the property identity —
/// exactly the `result != rawResult` set-difference loop in `computeSolve`,
/// which exists because fill-missing injects variables the unit checker never
/// saw in the document text.
fn fill_missing(solution: &mut Solution, source: &str, requested: Option<bool>) -> Vec<Value> {
    if requested != Some(true) {
        return Vec::new();
    }
    let added = cyclepath::resolve_missing_properties(
        &mut solution.values,
        &mut solution.display_names,
        source,
        None,
        // `STATE TABLE` parses since Phase 8 (`Document::blocks.state_tables`),
        // but `fill_missing` is handed the source text rather than the parsed
        // document, so the per-circuit fluids are not threaded through yet and
        // the resolver still takes its legacy global-index path — the same
        // branch the Java takes for a document that declares no blocks.
        &[],
    );
    for name in &added {
        if let Some(unit) = cyclepath::si_unit_for_state_variable(name) {
            solution
                .inferred_units
                .entry(name.clone())
                .or_insert_with(|| unit.to_string());
        }
    }
    // `PropertyFunctions.detectFluid(cleanText)` with the Java's `"Water"`
    // default already folded in (see `detect_fluid`'s docs).
    let fluid = frees_core::props::propfun::detect_fluid(source);
    cyclepath::generate_cycle_path(&solution.values, fluid)
        .into_iter()
        .map(|point| {
            let map: Map<String, Value> = point
                .into_iter()
                .filter(|(_, v)| v.is_finite())
                .map(|(k, v)| (k, json!(v)))
                .collect();
            Value::Object(map)
        })
        .collect()
}

/// `ComponentMetadata.build(cleanText, variableDtos)` — the per-instance
/// datasheet payload.
///
/// The Java re-parses the document here; this port carries the top-level
/// instance list forward on [`Solution`] instead (`component_instances`), so the
/// boundary neither re-lexes nor risks disagreeing with the solve. `vars` are
/// the **display-converted** rows, as the Java passes its `variableDtos` — a
/// binding like `UA = UA_chl_r` shows the symbol *and* the value the user sees.
fn component_entries(solution: &Solution, vars: &[VariableRow]) -> Vec<Value> {
    metadata::build(&solution.component_instances, vars)
        .into_iter()
        .map(|component| {
            json!({
                "name": component.name,
                "type": component.type_name,
                "params": component
                    .params
                    .into_iter()
                    .map(|param| json!({
                        "name": param.name,
                        "ref": param.reference,
                        "value": param.value.filter(|v| v.is_finite()),
                        "units": param.units,
                    }))
                    .collect::<Vec<_>>(),
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
    si_unc: Option<f64>,
    unit: &str,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> (f64, String, Option<f64>) {
    // A stated sigma scales by the display unit's FACTOR only — it is an
    // interval width, so the additive offset of e.g. [C] must not touch it.
    // Port of `SolverApiSupport.convertToDisplayUnit`, which divides
    // `displayUnc` by the factor and never subtracts the offset.
    if unit.is_empty() || unit == "-" {
        return (si_value, unit.to_string(), si_unc);
    }
    // The explicit unit's own text is what gets displayed, so it is what gets
    // parsed — the Java uses the same `unit` for both because `unitsByLowerName`
    // already overlays explicit units; this port overlays here instead.
    let effective = explicit_units
        .get(name_lower)
        .map(String::as_str)
        .unwrap_or(unit);
    let Ok(recorded) = UnitRegistry::parse_with_offset(effective) else {
        return (si_value, effective.to_string(), si_unc);
    };
    if explicit_units.contains_key(name_lower) {
        return (
            recorded.from_si(si_value),
            effective.to_string(),
            si_unc.map(|u| u / recorded.factor),
        );
    }
    match UnitRegistry::preferred_display_unit(&recorded.dims, system) {
        Some(preferred) => {
            let value = (si_value - preferred.offset) / preferred.factor;
            (value, preferred.name, si_unc.map(|u| u / preferred.factor))
        }
        None => (
            recorded.from_si(si_value),
            effective.to_string(),
            si_unc.map(|u| u / recorded.factor),
        ),
    }
}

fn display_of<'a>(display_names: &'a BTreeMap<String, String>, name: &'a String) -> &'a String {
    display_names.get(name).unwrap_or(name)
}

fn solve_success(
    solution: &Solution,
    cycle_path: &[Value],
    elapsed_ms: f64,
    system: UnitSystem,
    explicit_units: &BTreeMap<String, String>,
) -> String {
    let (rows, row_uncertainties) = variable_rows(solution, system, explicit_units);
    let variables = variable_entries(&rows, &row_uncertainties);
    let components = component_entries(solution, &rows);

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
        // Empty unless the request asked to fill missing properties — the Java
        // `resolveFillMissing` short-circuits to `List.of()` otherwise.
        "cyclePath": cycle_path,
        // One entry per top-level COMPONENT instance; empty for a document with
        // no component layer.
        "components": components,
        // One entry per solved DYNAMIC block — the Java `Result.odeTables`,
        // shaped as `OdeTableDto` so the Tables window renders it through the
        // same path as a parametric table and the Plots window can graph it.
        "odeTables": ode_tables(solution),
        // One entry per `PLOT '…' … END` block — the Java
        // `SolveController`'s `plotsOf(parsed.plots())`. `App.tsx` maps each
        // through `plotDefToSpec`, so this is what makes a declared plot
        // render.
        "definedPlots": plot_defs(&solution.plots),
        // Tornado breakdown: per dependent variable, its propagated sigma and
        // each source's signed contribution, largest sigma first — the Java
        // `SolveController.uncertaintyBreakdownOf`. Empty when the document
        // declares no uncertainty.
        "uncertaintyBreakdown": uncertainty_breakdown(solution),
    })
    .to_string()
}

/// `uncertaintyBreakdown[]` — port of `SolveController.uncertaintyBreakdownOf`.
///
/// One entry per variable that has contributions, display-named, its sources
/// capped at 16 and the entries ranked by propagated sigma, largest first.
/// Values are **SI**: the Java ranks and emits `result.uncertainties()`
/// straight out of the propagation, without the display-unit conversion
/// `toVariableDto` applies to the variable rows.
fn uncertainty_breakdown(solution: &Solution) -> Vec<Value> {
    let mut entries: Vec<(f64, Value)> = solution
        .uncertainty_contributions
        .iter()
        .map(|(name, sources)| {
            let total = solution.uncertainties.get(name).copied().unwrap_or(0.0);
            (
                total,
                json!({
                    "variable": display_of(&solution.display_names, name),
                    "total": total,
                    "sources": sources
                        .iter()
                        .take(16)
                        .map(|c| json!({
                            "source": display_of(&solution.display_names, &c.source),
                            "value": c.value,
                        }))
                        .collect::<Vec<_>>(),
                }),
            )
        })
        .collect();
    entries.sort_by(|a, b| b.0.total_cmp(&a.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

/// `definedPlots[]` — port of `SolveDtos.plotsOf`, which is a straight
/// `new PlotDefDto(p.name(), p.attributes())` per declared plot.
///
/// The attribute map crosses as `Record<string, string[]>`: keys already
/// lowercased by the parser, values in first-insertion order (the Java's
/// `LinkedHashMap`). A JSON object does not *promise* key order, but
/// `plotDefToSpec` looks attributes up by name, so only the per-key value
/// order is load-bearing and that is a JSON array.
fn plot_defs(plots: &[frees_core::parser::blocks::PlotDef]) -> Vec<Value> {
    plots
        .iter()
        .map(|plot| {
            let attributes: Map<String, Value> = plot
                .attributes
                .iter()
                .map(|(key, values)| (key.to_string(), json!(values)))
                .collect();
            json!({ "name": plot.name, "attributes": attributes })
        })
        .collect()
}

/// `odeTables[]` — the sampled trajectory of every solved `DYNAMIC` block.
///
/// The DTO calls the column headers `vars` (matching `ParametricTableDto`) and
/// carries a parallel `units` array, so the grid can label its columns without
/// a second lookup. **Rows are SI**, like every value core produces: the ODE
/// table is not run through the display-unit conversion `variable_rows` applies,
/// because a trajectory column is not a result *variable* and the Java response
/// does not convert it either.
///
/// A non-finite cell becomes `null` rather than a JSON literal — `NaN` is not
/// representable in JSON and the DTO types the cell as `number | null`. Same
/// rule the residual list already applies.
fn ode_tables(solution: &Solution) -> Vec<Value> {
    solution
        .ode_tables
        .iter()
        .map(|table| {
            let units: Vec<&str> = table
                .columns
                .iter()
                .map(|column| {
                    solution
                        .inferred_units
                        .get(column.as_str())
                        .map(String::as_str)
                        .unwrap_or("")
                })
                .collect();
            json!({
                "name": table.name,
                "vars": table.columns,
                "units": units,
                "rows": table
                    .rows
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|v| if v.is_finite() { json!(v) } else { Value::Null })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>(),
                "events": table
                    .events
                    .iter()
                    .map(|hit| json!({ "name": hit.name, "time": hit.time }))
                    .collect::<Vec<_>>(),
                "method": table.method,
                "stopped": table.stopped,
                "endTime": table.end_time,
            })
        })
        .collect()
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
                "unknowns": partial.unknown_count,
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
        // The Java `CheckController` reports the declared plots too, which is
        // what lets the Plots tab populate before the first solve —
        // `App.tsx`'s `result?.definedPlots ?? checkResult?.definedPlots`.
        "definedPlots": plot_defs(&report.plots),
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
        "definedPlots": [],
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// GET /api/reference
// ---------------------------------------------------------------------------

/// The Help window's language reference, read live from the engine's own
/// tables so it can never drift from what the solver accepts.
///
/// Wire shape is the Java `ReferenceController.reference()` map, key for key:
///
/// ```json
/// {"units":     [{"symbol": "kPa", "dimension": "Pa", "siFactor": 1000.0}],
///  "constants": [{"name": "R#", "value": 8.314, "unit": "J/mol-K",
///                 "description": "Universal (molar) gas constant"}],
///  "functions": [{"name": "atan2", "signature": "atan2(a, b)",
///                 "description": "", "category": "Built-in"}]}
/// ```
///
/// Sources: [`UnitRegistry::all_units`] (sorted by dimension then symbol, as
/// `UnitRegistry.listUnits()`), `eval::CONSTANTS` (declaration order, as
/// `ConstantsRegistry.list()`), and `eval::INTRINSICS` (sorted by name).
///
/// One field the Java has and this does not: `FunctionRegistry.FunctionInfo`
/// hand-writes a `description` and a `category` per function, and the Rust
/// dispatch table carries neither — its rows are `(name, arity, body)`. Rather
/// than invent prose, `description` is empty and `category` is the one label
/// the table genuinely supports. `signature` is generated from the arity, so
/// the argument *count* is real even though the argument *names* are
/// positional placeholders.
#[wasm_bindgen]
pub fn reference() -> String {
    json!({
        "units": reference_units(),
        "constants": reference_constants(),
        "functions": reference_functions(),
    })
    .to_string()
}

/// Every registered unit: symbol, the SI dimension it measures, and the factor
/// to SI base units.
fn reference_units() -> Vec<Value> {
    UnitRegistry::all_units()
        .into_iter()
        .map(|unit| {
            json!({
                "symbol": unit.symbol,
                "dimension": unit.dimension,
                "siFactor": unit.si_factor,
            })
        })
        .collect()
}

/// The `#`-suffixed built-ins. A dimensionless constant carries `"-"`, not
/// `null` — the Java maps `unit == null` to `"-"` and the Help table prints the
/// cell verbatim.
fn reference_constants() -> Vec<Value> {
    frees_core::eval::CONSTANTS
        .iter()
        .map(|constant| {
            json!({
                "name": constant.name,
                "value": constant.value,
                "unit": constant.unit.unwrap_or("-"),
                "description": constant.description,
            })
        })
        .collect()
}

/// The intrinsic dispatch table — the authoritative answer to "does this
/// browser engine actually have that function?", which is what the static
/// frontend catalogs cannot know.
fn reference_functions() -> Vec<Value> {
    let mut intrinsics: Vec<&Intrinsic> = frees_core::eval::INTRINSICS.iter().collect();
    intrinsics.sort_unstable_by_key(|intrinsic| intrinsic.name);
    intrinsics
        .into_iter()
        .map(|intrinsic| {
            json!({
                "name": intrinsic.name,
                "signature": signature_of(intrinsic.name, intrinsic.arity),
                "description": "",
                "category": "Built-in",
            })
        })
        .collect()
}

/// `a, b, c` — positional placeholders for `n` arguments. The registry stores
/// no argument names, so the position is all there is to say.
fn placeholders(n: usize) -> String {
    (0..n)
        .map(|i| match u8::try_from(i) {
            Ok(offset) if offset < 26 => char::from(b'a' + offset).to_string(),
            _ => format!("x{}", i + 1),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A call form built from the arity: exact counts render literally, optional
/// tails as `[, c]`, open tails as `, ...`, and a fixed set of allowed counts
/// as `|`-separated alternatives (`if` takes 3 or 5, never 4).
fn signature_of(name: &str, arity: frees_core::eval::Arity) -> String {
    use frees_core::eval::Arity;
    match arity {
        Arity::Exact(n) => format!("{name}({})", placeholders(n)),
        Arity::Range(lo, hi) => {
            // placeholders(lo) is a prefix of placeholders(hi) by construction,
            // so the optional tail is what the longer form adds.
            let required = placeholders(lo);
            let full = placeholders(hi);
            let optional = full.get(required.len()..).unwrap_or("");
            format!("{name}({required}[{optional}])")
        }
        Arity::OneOf(counts) => counts
            .iter()
            .map(|n| format!("{name}({})", placeholders(*n)))
            .collect::<Vec<_>>()
            .join(" | "),
        Arity::AtLeast(0) | Arity::Any => format!("{name}(...)"),
        Arity::AtLeast(lo) => format!("{name}({}, ...)", placeholders(lo)),
    }
}

// ---------------------------------------------------------------------------
// Property plots — GET /api/plot/fluids, POST /api/plot/propplot,
//                  POST /api/plot/psychart  (`PlotController.java`)
// ---------------------------------------------------------------------------

/// `GET /api/plot/fluids` — `{available, fluids[]}`.
///
/// `PlotController.fluids()` returns
/// `CoolProp.isAvailable() ? plotFluids() : List.of()`, so a build with no
/// real-fluid backend reports an empty list and `available: false`. The
/// frontend already renders that state ("no fluids available") — it is the
/// truth, not a stub.
///
/// **One deliberate narrowing**: the list is `plot_fluids_available()`, not
/// `plot_fluids()`. The Java's list is every fluid CoolProp knows, because
/// CoolProp serves every one of them; a build whose backend serves a subset would
/// have a picker failing on the rest. A backend that serves everything gets the
/// Java list back verbatim.
///
/// Since D9 the subset is rustprop's `served_fluids` — Water, R134a, R1234yf and
/// the two glycol families — so `Air` left the picker with the `air.fraux`
/// transport grid it was the only backing for. See
/// `props/rustprop_backend.rs::served_fluids` for why (pseudo-pure Air has no
/// full-state service yet), and note that `backend` below now reports rustprop
/// rather than the table list.
#[wasm_bindgen]
pub fn fluids() -> String {
    frees_core::props::tables::install_builtin_once();
    let available = frees_core::props::propfun::is_available();
    let names: Vec<&str> = frees_core::props::propfun::plot_fluids_available();
    json!({
        "available": available,
        "fluids": names,
        // Not in the Java (which never needed to explain itself, having the
        // library loaded): what this build's property backend actually is.
        "backend": frees_core::props::propfun::backend_description(),
    })
    .to_string()
}

/// `POST /api/plot/propplot` — the saturation dome, isolines and markers for
/// one fluid, in the `DiagramResponse` shape `api.ts` declares.
///
/// Failure is **data**, never a trap: `{"error": "…"}` mirrors the Java
/// controller's `badRequest(ErrorResponse)` body, and `engineClient` turns it
/// into the rejected promise `PlotCard` already catches.
#[wasm_bindgen]
pub fn property_diagram(fluid: &str, kind: &str) -> String {
    frees_core::props::tables::install_builtin_once();
    if fluid.trim().is_empty() {
        return json!({"error": "A fluid name is required"}).to_string();
    }
    // A document spelling ("water", "R134a") resolves to the canonical CoolProp
    // name; an already-canonical name passes through unchanged.
    let canonical = frees_core::props::propfun::resolve_fluid(&fluid.to_lowercase())
        .unwrap_or_else(|_| fluid.to_string());
    match frees_core::props::diagrams::generate(&canonical, kind) {
        Ok(diagram) => diagram_json(&diagram).to_string(),
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

/// `POST /api/plot/psychart` — the psychrometric chart.
///
/// `request_json` is `{"pressure": …, "tMin": …, "tMax": …}`; any field may be
/// absent or `null`, which selects the Java default (1 atm, 0–50 °C).
#[wasm_bindgen]
pub fn psychrometric_chart(request_json: &str) -> String {
    #[derive(Debug, Default, Deserialize)]
    #[serde(default, rename_all = "camelCase")]
    struct PsychartRequest {
        pressure: Option<f64>,
        t_min: Option<f64>,
        t_max: Option<f64>,
    }

    let request: PsychartRequest = if request_json.trim().is_empty() {
        PsychartRequest::default()
    } else {
        match serde_json::from_str(request_json) {
            Ok(parsed) => parsed,
            Err(e) => return json!({ "error": format!("Malformed request: {e}") }).to_string(),
        }
    };
    match frees_core::props::psychro::generate(request.pressure, request.t_min, request.t_max) {
        Ok(chart) => json!({
            "pressure": chart.pressure,
            "tMin": chart.t_min,
            "tMax": chart.t_max,
            "curves": chart.curves.iter().map(curve_json).collect::<Vec<Value>>(),
        })
        .to_string(),
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn diagram_json(diagram: &frees_core::props::diagrams::Diagram) -> Value {
    json!({
        "fluid": diagram.fluid,
        "kind": diagram.kind,
        "xProperty": diagram.x_property,
        "yProperty": diagram.y_property,
        "xLog": diagram.x_log,
        "yLog": diagram.y_log,
        "dome": diagram.dome.iter().map(curve_json).collect::<Vec<Value>>(),
        "isolines": diagram.isolines.iter().map(curve_json).collect::<Vec<Value>>(),
        "markers": diagram.markers.iter().map(|m| json!({
            "label": m.label,
            "x": finite_or_null(m.x),
            "y": finite_or_null(m.y),
        })).collect::<Vec<Value>>(),
    })
}

/// A curve with its gaps intact. `None` becomes JSON `null`, which is exactly
/// what Plotly reads as a line break — the mechanism that lets a partial
/// property backend draw an honestly broken curve.
fn curve_json(curve: &frees_core::props::diagrams::Curve) -> Value {
    json!({
        "family": curve.family,
        "label": curve.label,
        "x": curve.x.iter().map(|v| point_json(*v)).collect::<Vec<Value>>(),
        "y": curve.y.iter().map(|v| point_json(*v)).collect::<Vec<Value>>(),
    })
}

fn point_json(value: Option<f64>) -> Value {
    value.map_or(Value::Null, finite_or_null)
}

/// `serde_json` cannot represent NaN/Inf, and silently turning one into `0`
/// would be a fabricated data point. Both become `null`, the same gap a
/// declined lookup produces.
fn finite_or_null(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// The same rule for a value that is already optional: `None` and a
/// non-finite number both become JSON `null`.
fn json_number(value: f64) -> Option<Value> {
    serde_json::Number::from_f64(value).map(Value::Number)
}

// ---------------------------------------------------------------------------
// POST /api/repl/evaluate and /api/repl/clear
// ---------------------------------------------------------------------------

/// Evaluate one REPL line against the workspace the last successful
/// [`solve`] stored. `request_json` is `{"expression": "...",
/// "unitSystem": "SI"}`; the reply is a `ReplResponse` JSON string.
///
/// Never throws: `ReplTerminal.tsx` awaits this without a `catch`, so a
/// malformed request is reported as `success: false` like everything else.
/// See [`repl`] for what is ported and the one unit divergence.
#[wasm_bindgen]
pub fn repl_evaluate(request_json: &str) -> String {
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct ReplRequest {
        expression: String,
        unit_system: Option<String>,
    }
    let request: ReplRequest = match serde_json::from_str(request_json) {
        Ok(request) => request,
        Err(err) => {
            return json!({
                "success": false,
                "value": Value::Null,
                "text": Value::Null,
                "units": Value::Null,
                "uncertainty": Value::Null,
                "error": format!("Malformed REPL request: {err}"),
                "name": Value::Null,
                "assignedVariables": [],
            })
            .to_string()
        }
    };
    let system = request
        .unit_system
        .as_deref()
        .map_or(UnitSystem::Si, parse_unit_system);
    repl::evaluate(&request.expression, system)
}

/// Drop the REPL's own variable overlays — all of them, or the one named.
/// `name_json` is `""`/`"null"` for "all", otherwise a JSON string.
/// Fire-and-forget: `App.tsx` calls it with `void`.
#[wasm_bindgen]
pub fn repl_clear(name_json: &str) {
    let name: Option<String> = serde_json::from_str(name_json).unwrap_or(None);
    repl::clear(name.as_deref());
}

// ---------------------------------------------------------------------------
// Tests — every assertion here is about the *wire shape*: the exact camelCase
// keys and JSON types `web/src/api.ts` declares.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        check, fluids, placeholders, property_diagram, psychrometric_chart, reference,
        signature_of, solve, version,
    };
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

    // ── definedPlots (SolveDtos.plotsOf) ──────────────────────────────────

    /// The `PLOT '…' … END` payload `plots/fromCode.ts::plotDefToSpec`
    /// consumes: a name and `Record<string, string[]>` attributes, keys
    /// lowercased, every value an array even when the user wrote one.
    #[test]
    fn a_declared_plot_reaches_the_frontend_as_a_plot_def_dto() {
        let source = "speed = 10\npower = 2 * speed\n\
                      PLOT 'Power curve'\n  kind = xy\n  x = speed\n  y = power\n  \
                      xlabel = 'Speed'\n  logx = true\nEND\n";
        for payload in [solve(source, "{}"), check(source, "{}")] {
            let v = parsed(&payload);
            let plots = v["definedPlots"].as_array().expect("definedPlots array");
            assert_eq!(plots.len(), 1, "{v}");
            assert_eq!(plots[0]["name"], "Power curve", "{v}");
            let attributes = plots[0]["attributes"]
                .as_object()
                .expect("attributes object");
            assert_eq!(attributes["kind"], serde_json::json!(["xy"]), "{v}");
            assert_eq!(attributes["x"], serde_json::json!(["speed"]), "{v}");
            assert_eq!(attributes["y"], serde_json::json!(["power"]), "{v}");
            assert_eq!(attributes["xlabel"], serde_json::json!(["Speed"]), "{v}");
            assert_eq!(attributes["logx"], serde_json::json!(["true"]), "{v}");
        }
    }

    #[test]
    fn a_document_with_no_plot_block_reports_an_empty_list() {
        assert_eq!(
            parsed(&solve("x = 1\n", "{}"))["definedPlots"],
            serde_json::json!([])
        );
        assert_eq!(
            parsed(&check("x = 1\n", "{}"))["definedPlots"],
            serde_json::json!([])
        );
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

        // Both component fields are always *present* arrays, empty for a
        // document with no component layer and no fill-missing request —
        // `api.ts` reads them with `?? []`, but a typed absent field and a
        // typed empty one are different contracts and this pins the second.
        assert_key(&v, "cyclePath", Value::is_array);
        assert_key(&v, "components", Value::is_array);
        assert!(v["cyclePath"].as_array().unwrap().is_empty(), "{v}");
        assert!(v["components"].as_array().unwrap().is_empty(), "{v}");
    }

    // ── SolveResponse.components — the datasheet payload ───────────────────

    /// A 12 V source across a 10/20 ohm divider, written in the library's own
    /// `connect(...)` idiom: i = 0.4 A, v_mid = 8 V. `R2` binds its parameter to
    /// a top-level symbol so the datasheet has a resolved binding to show.
    const RESISTOR_NETWORK: &str = "\
VoltageSource V1(E = 12)
Resistor      R1(R = 10)
Resistor      R2(R = R_load)
Ground        G1()
connect(V1.p, R1.a)
connect(R1.b, R2.a)
connect(R2.b, V1.n, G1.port)
R_load = 20
";

    #[test]
    fn solve_emits_component_metadata_for_a_component_document() {
        let v = parsed(&solve(RESISTOR_NETWORK, "{}"));
        assert_eq!(v["success"], true, "{v}");

        let components = v["components"].as_array().expect("components array");
        let names: Vec<&str> = components
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["V1", "R1", "R2", "G1"], "{v}");

        // ComponentResult: {name, type, params}, identities in their SOURCE
        // spelling (the AST lowercases both for registry lookup).
        let r2 = &components[2];
        assert_eq!(r2["type"], "Resistor", "{v}");
        let params = r2["params"].as_array().expect("params array");
        assert_eq!(params.len(), 1, "{v}");
        // ComponentParamResult: {name, ref, value, units}. `ref` is the binding
        // as written; `value` is resolved from the solved rows, so a shared
        // symbol shows both the symbol and its number.
        assert_eq!(params[0]["name"], "r", "{v}");
        assert_eq!(params[0]["ref"], "R_load", "{v}");
        assert_eq!(params[0]["value"], 20.0, "{v}");

        // A literal binding resolves to its own number.
        assert_eq!(components[1]["params"][0]["value"], 10.0, "{v}");
        // A component with no parameters still reports an (empty) list.
        assert!(
            components[3]["params"].as_array().unwrap().is_empty(),
            "{v}"
        );
    }

    #[test]
    fn component_port_members_reach_the_variables_table() {
        let v = parsed(&solve(RESISTOR_NETWORK, "{}"));
        // The expanded port members surface as ordinary result rows under their
        // dotted display spelling, which is what the schematic readouts group by.
        assert_eq!(variable(&v, "r1.b.v")["value"], 8.0, "{v}");
        assert_eq!(variable(&v, "r1.a.i")["value"], 0.4, "{v}");
        // …with an EMPTY unit on the solve path, because `toVariableDto` keys
        // `unitsByLowerName` by the display name (`n1.i`) while the member units
        // live under the flat name (`n1$i`). Verified against the reference
        // engine, whose `unitsByLowerName` for this exact document is
        // `{n1$i=A, n1$v=V, n2$i=A, n2$v=V, n3$i=A, n3$v=V, r_load=Ω}` — the
        // member entries are there, and the dotted lookup misses every one.
        assert_eq!(variable(&v, "r1.b.v")["units"], "", "{v}");
        assert_eq!(variable(&v, "r1.a.i")["units"], "", "{v}");
        // A variable the document itself named keys identically either way, so
        // the derived Ω does arrive.
        assert_eq!(variable(&v, "R_load")["units"], "Ω", "{v}");
    }

    #[test]
    fn check_reports_component_member_units_where_solve_does_not() {
        // `CheckController.addComponentMemberUnits`: the check endpoint puts the
        // members back into `inferredUnits`, keyed the way the payload keys
        // them, so the Variable Explorer and the schematic are not dimensionless.
        let v = parsed(&check(RESISTOR_NETWORK, "{}"));
        assert_eq!(v["solvable"], true, "{v}");
        // `check_response` keys `inferredUnits` by the *display* name, exactly
        // as `addComponentMemberUnits` does, so the frontend can look a unit up
        // with the name it got in `variables`.
        let units = &v["inferredUnits"];
        assert_eq!(units["r1.a.i"], "A", "{v}");
        assert_eq!(units["r1.b.v"], "V", "{v}");
    }

    #[test]
    fn a_component_document_checks_as_solvable() {
        let v = parsed(&check(RESISTOR_NETWORK, "{}"));
        assert_eq!(v["solvable"], true, "{v}");
        // 7 equations / 7 unknowns once the two resistors and the ground have
        // expanded — the check endpoint must run the same expansion the solve
        // does, or a component document would report as empty and solvable.
        assert_eq!(v["equations"], v["unknowns"], "{v}");
        assert!(v["equations"].as_u64().unwrap() >= 14, "{v}");
    }

    #[test]
    fn the_unit_warning_of_a_component_body_matches_the_reference_engine() {
        // Grounding the port members lets the checker see V on the left and A on
        // the right of `a.V - b.V = R * a.I` before R has a dimension, and it
        // warns. The reference engine emits the same single warning for this
        // document (probed directly against the core jar); unit problems are
        // warnings by the parent invariant and never block a solve.
        let v = parsed(&solve(RESISTOR_NETWORK, "{}"));
        assert_eq!(v["success"], true, "{v}");
        let warnings = v["unitWarnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1, "{v}");
        let text = warnings[0].as_str().unwrap();
        assert!(text.contains("COMPONENT resistor r1"), "{text}");
        assert!(text.contains("do not match the right side [A]"), "{text}");
    }

    // ── SolveResponse.cyclePath — the property-plot overlay ────────────────

    /// Two Water state points, enough for `generateCyclePath` to interpolate a
    /// segment between them (it needs at least two).
    const TWO_STATES: &str = "\
T1 = 320 [K]
P1 = 101325 [Pa]
T2 = 400 [K]
P2 = 101325 [Pa]
";

    #[test]
    fn cycle_path_is_empty_unless_fill_missing_is_requested() {
        let v = parsed(&solve(TWO_STATES, "{}"));
        assert!(v["cyclePath"].as_array().unwrap().is_empty(), "{v}");
        let v = parsed(&solve(TWO_STATES, r#"{"fillMissing": false}"#));
        assert!(v["cyclePath"].as_array().unwrap().is_empty(), "{v}");
    }

    #[test]
    fn fill_missing_completes_the_states_and_returns_a_cycle_path() {
        let v = parsed(&solve(TWO_STATES, r#"{"fillMissing": true}"#));
        assert_eq!(v["success"], true, "{v}");

        // The back-filled properties arrive as ordinary rows, unit-stamped from
        // the property identity (nothing in the text could derive them).
        let h1 = variable(&v, "h1");
        assert!(h1["value"].as_f64().unwrap() > 0.0, "{v}");
        assert_eq!(h1["units"], "J/kg", "{v}");
        assert_eq!(variable(&v, "s1")["units"], "J/kg-K", "{v}");
        assert_eq!(variable(&v, "rho1")["units"], "kg/m^3", "{v}");

        // The path is a list of {property: value} maps the plot layer overlays.
        let path = v["cyclePath"].as_array().expect("cyclePath array");
        assert!(path.len() >= 2, "expected an interpolated path, got {v}");
        for point in path {
            assert!(point.is_object(), "{v}");
            for value in point.as_object().unwrap().values() {
                assert!(value.is_f64(), "cyclePath carries only finite numbers: {v}");
            }
        }
        // The first point is state 1 itself.
        assert!(path[0].get("T").is_some(), "{v}");
        assert!(path[0].get("P").is_some(), "{v}");
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
        // `stats.unknowns` is the count of unknowns in the stalled system (the
        // Java partial `Stats.variableCount`), not the size of the display-name
        // map — that map is the whole `ParseResult.displayNames` and can name
        // more identifiers than the system has unknowns.
        assert_eq!(v["stats"]["unknowns"], 1);
    }

    /// A partial failure whose document has more *named* identifiers than
    /// unknowns: the FUNCTION formal `n` and the body's `k` both carry display
    /// names, while the only unknown is `y`.
    #[test]
    fn a_partial_failure_counts_unknowns_not_display_names() {
        let out = solve(
            "FUNCTION Bad(n)\n  Bad := exp(n)\nEND\n\ny = Bad(y) + 1\n",
            "{}",
        );
        let v = parsed(&out);
        assert_eq!(v["success"], false);
        assert_eq!(v["stats"]["unknowns"], 1);
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

    // ── LanguageReference (ReferenceController.reference) ─────────────────

    /// The three arrays `web/src/api.ts` declares, all non-empty — an empty
    /// reference is exactly the stub this replaced.
    #[test]
    fn reference_has_the_three_populated_arrays() {
        let v = parsed(&reference());
        for key in ["units", "constants", "functions"] {
            assert_key(&v, key, Value::is_array);
            assert!(
                !v[key].as_array().unwrap().is_empty(),
                "{key} must not be empty"
            );
        }
    }

    /// `UnitInfo` in api.ts: symbol/dimension strings + a numeric siFactor.
    #[test]
    fn reference_units_carry_the_unit_info_fields() {
        let v = parsed(&reference());
        let units = v["units"].as_array().unwrap();
        for unit in units {
            assert_key(unit, "symbol", Value::is_string);
            assert_key(unit, "dimension", Value::is_string);
            assert_key(unit, "siFactor", Value::is_number);
        }
        let kpa = units
            .iter()
            .find(|u| u["symbol"] == "kpa")
            .unwrap_or_else(|| panic!("kpa missing from {} units", units.len()));
        assert_eq!(kpa["dimension"], "Pa");
        assert_eq!(kpa["siFactor"], 1000.0);
    }

    /// The Java sorts by dimension then symbol and the Help page groups on that
    /// order — a dimension must not appear in two separate runs.
    #[test]
    fn reference_units_stay_grouped_by_dimension() {
        let v = parsed(&reference());
        let dimensions: Vec<&str> = v["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["dimension"].as_str().unwrap())
            .collect();
        let mut sorted = dimensions.clone();
        sorted.sort_unstable();
        assert_eq!(dimensions, sorted, "units must be sorted by dimension");
    }

    /// `ConstantInfo` in api.ts, and the Java's `unit == null -> "-"` mapping.
    #[test]
    fn reference_constants_match_the_java_constant_info() {
        let v = parsed(&reference());
        let constants = v["constants"].as_array().unwrap();
        for constant in constants {
            assert_key(constant, "name", Value::is_string);
            assert_key(constant, "value", Value::is_number);
            assert_key(constant, "unit", Value::is_string);
            assert_key(constant, "description", Value::is_string);
            assert!(
                constant["name"].as_str().unwrap().ends_with('#'),
                "constants are #-suffixed: {constant}"
            );
        }
        // ReferenceControllerTest asserts exactly this row.
        let r = constants
            .iter()
            .find(|c| c["name"] == "R#")
            .expect("R# must be in the constants reference");
        assert_eq!(r["unit"], "J/mol-K");
        // A dimensionless constant carries "-", never null.
        let pi = constants.iter().find(|c| c["name"] == "pi#").unwrap();
        assert_eq!(pi["unit"], "-");
    }

    /// `FunctionInfo` in api.ts, sorted by name, one row per dispatch entry.
    #[test]
    fn reference_functions_mirror_the_intrinsic_registry() {
        let v = parsed(&reference());
        let functions = v["functions"].as_array().unwrap();
        assert_eq!(functions.len(), frees_core::eval::INTRINSICS.len());

        let names: Vec<&str> = functions
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "functions must be sorted by name");
        assert_eq!(names, frees_core::eval::intrinsic_names());

        for function in functions {
            assert_key(function, "name", Value::is_string);
            assert_key(function, "signature", Value::is_string);
            assert_key(function, "description", Value::is_string);
            assert_key(function, "category", Value::is_string);
            let name = function["name"].as_str().unwrap();
            let signature = function["signature"].as_str().unwrap();
            assert!(
                signature.starts_with(&format!("{name}(")),
                "signature must open with the call: {function}"
            );
        }
    }

    /// The signature is generated from the arity, so the argument *count* is
    /// real even though the names are placeholders.
    #[test]
    fn reference_signatures_render_every_arity_shape() {
        use frees_core::eval::Arity;
        assert_eq!(signature_of("pi", Arity::Exact(0)), "pi()");
        assert_eq!(signature_of("sin", Arity::Exact(1)), "sin(a)");
        assert_eq!(signature_of("atan2", Arity::Exact(2)), "atan2(a, b)");
        assert_eq!(signature_of("round", Arity::Range(1, 2)), "round(a[, b])");
        assert_eq!(
            signature_of("if", Arity::OneOf(&[3, 5])),
            "if(a, b, c) | if(a, b, c, d, e)"
        );
        assert_eq!(signature_of("max", Arity::AtLeast(1)), "max(a, ...)");
        assert_eq!(signature_of("sum", Arity::Any), "sum(...)");
        assert_eq!(signature_of("noop", Arity::AtLeast(0)), "noop(...)");
        // Past the alphabet the placeholders keep going without colliding.
        assert!(placeholders(27).ends_with("z, x27"));
    }

    // ── Property plots (PlotController) ──────────────────────────────────────

    /// The property backend is process-global (it mirrors the Java's single
    /// loaded `CoolProp.LIB`), so every test that reads or swaps it holds this.
    static PROPERTY_BACKEND: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn backend_guard() -> std::sync::MutexGuard<'static, ()> {
        PROPERTY_BACKEND.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `GET /api/plot/fluids`. This build ships no real-fluid backend, so the
    /// Java's own `CoolProp.isAvailable() ? plotFluids() : List.of()` branch
    /// says `available:false` with an empty list — which is exactly what
    /// `api.ts` documents the old backend returned in the same situation.
    #[test]
    fn the_fluid_list_reports_availability_honestly() {
        let _guard = backend_guard();
        let payload = parsed(&fluids());
        assert_key(&payload, "available", Value::is_boolean);
        assert_key(&payload, "fluids", Value::is_array);
        assert_key(&payload, "backend", Value::is_string);
        let available = payload["available"].as_bool().expect("boolean");
        let names = payload["fluids"].as_array().expect("array");
        assert_eq!(
            available,
            !names.is_empty(),
            "the list and the flag must agree: {payload}"
        );
        assert!(names.iter().all(Value::is_string), "{payload}");
        if !available {
            assert!(
                payload["backend"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("none"),
                "{payload}"
            );
        }
    }

    /// A property diagram for a fluid this build cannot serve is a *data*
    /// failure, not a trap: the `{"error": …}` body the Java controller returns
    /// as 400.
    ///
    /// The fluid used to be Water and the reason used to be "no backend at
    /// all". Since Phase 5 links the generated tables, `property_diagram` always
    /// has a backend; the honest un-servable case is now a fluid outside the two
    /// that are tabulated. The error must still name it.
    #[test]
    fn a_property_diagram_for_an_untabulated_fluid_is_an_error_body() {
        let _guard = backend_guard();
        let payload = parsed(&property_diagram("Ammonia", "T-s"));
        assert_key(&payload, "error", Value::is_string);
        let message = payload["error"].as_str().expect("string");
        assert!(message.contains("Ammonia"), "{message}");
    }

    /// The linked tables are a *real* diagram source, not just a non-error:
    /// water's T–s dome has to come back with finite numbers on it.
    #[test]
    fn the_linked_tables_draw_a_water_dome_with_real_numbers_on_it() {
        let _guard = backend_guard();
        frees_core::props::tables::install_builtin_once();
        let payload = parsed(&property_diagram("Water", "T-s"));
        assert!(payload.get("error").is_none(), "{payload}");
        let dome = payload["dome"][0].clone();
        let ys: Vec<Option<f64>> = serde_json::from_value(dome["y"].clone()).expect("y array");
        let finite: Vec<f64> = ys.iter().flatten().copied().collect();
        assert!(
            finite.len() > 100,
            "the dome should be mostly drawable, got {} of {} points",
            finite.len(),
            ys.len()
        );
        // T–s: the y axis is temperature, and water's dome lives between the
        // triple point and the critical point.
        let lo = finite.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = finite.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(lo > 250.0 && hi < 648.0, "dome T range {lo}..{hi}");
        // And the fluid list only offers what can actually be drawn.
        let listed = parsed(&fluids());
        let names: Vec<String> =
            serde_json::from_value(listed["fluids"].clone()).expect("fluids array");
        // Air JOINED this list at Wave-2 integration (2026-08-18), and the
        // reason it used to be excluded is worth keeping straight. Under the
        // pre-D9 `(P,h)` TableBackend, Air was a transport-only fluid (D7's
        // `(P,T)` grid) with no dome and no entropy axis, so every point of its
        // diagram would have failed. This crate now installs rustprop
        // unconditionally (see its `Cargo.toml`), and rustprop draws Air's dome
        // for real: both saturation branches answer across 65-130 K with the
        // pseudo-pure glide intact — measured at 80 K, Q=0 gives
        // s = 26.64 J/kg/K at 114.6 kPa and Q=1 gives 2601.32 J/kg/K at
        // 82.3 kPa. So the picker offering Air is correct, not a leak.
        assert_eq!(names, ["Air", "R1234yf", "R134a", "Water"], "{listed}");
        assert_eq!(listed["available"], Value::Bool(true));
    }

    /// The end-to-end proof that the plot path is *wired*, not just shaped: with
    /// a property backend installed, `fluids()` publishes the real list and
    /// `property_diagram()` returns a chart with real numbers in it.
    ///
    /// The backend here is a closed-form stand-in, not physics — the point is
    /// the plumbing (engine table -> diagram generator -> JSON -> `api.ts`),
    /// which is exactly the part a missing CoolProp cannot exercise.
    #[test]
    fn with_a_backend_installed_the_fluid_list_and_a_diagram_both_carry_real_data() {
        use frees_core::props::propfun::{self, RealFluid};
        use frees_core::FreesError;
        use std::sync::Arc;

        struct Toy;
        impl RealFluid for Toy {
            fn props1_si(&self, _fluid: &str, param: &str) -> Result<f64, FreesError> {
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
            ) -> Result<f64, FreesError> {
                let mut t = f64::NAN;
                let mut p = f64::NAN;
                let mut q = f64::NAN;
                for (k, v) in [(name1, value1), (name2, value2)] {
                    match k {
                        "T" => t = v,
                        "P" => p = v,
                        "Q" => q = v,
                        "S" => t = (v / 1000.0).exp(),
                        "D" => p = v * 300.0,
                        _ => {}
                    }
                }
                if q.is_finite() {
                    if t.is_finite() {
                        p = 1.0e5 * ((t - 300.0) / 30.0).exp();
                    } else if p.is_finite() {
                        t = 300.0 + 30.0 * (p / 1.0e5).ln();
                    }
                }
                if !t.is_finite() || !p.is_finite() {
                    return Err(FreesError::property("toy: underdetermined".to_string()));
                }
                Ok(match output {
                    "T" => t,
                    "P" => p,
                    "Smass" | "S" => 1000.0 * t.ln(),
                    "Hmass" | "H" => 1000.0 * t,
                    "Dmass" | "D" => p / (287.0 * t),
                    other => return Err(FreesError::property(format!("toy: no {other}"))),
                })
            }
        }

        let _guard = backend_guard();
        let previous = propfun::install(Arc::new(Toy));

        let list = parsed(&fluids());
        let diagram = parsed(&property_diagram("water", "T-s"));

        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }

        // The fluid list is now the real plotFluids() surface.
        assert_eq!(list["available"], true);
        let names: Vec<&str> = list["fluids"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(names.contains(&"Water"), "{list}");
        assert!(names.contains(&"R134a"), "{list}");
        assert!(!names.contains(&"HumidAir"), "{list}");

        // The diagram is a chart, not an error body — and the document spelling
        // "water" resolved to the canonical "Water".
        assert!(diagram.get("error").is_none(), "{diagram}");
        assert_eq!(diagram["fluid"], "Water");
        assert_eq!(diagram["kind"], "TS");
        let dome = diagram["dome"].as_array().expect("array");
        assert_eq!(dome.len(), 1);
        let xs = dome[0]["x"].as_array().expect("array");
        let ys = dome[0]["y"].as_array().expect("array");
        assert_eq!(xs.len(), ys.len());
        assert_eq!(xs.len(), 400, "both dome branches");
        assert!(
            xs.iter().filter(|v| v.is_number()).count() > 300,
            "most dome points must be real numbers: {}",
            dome[0]
        );
        // Quality lines and isobars are present with real points.
        let families: std::collections::BTreeSet<&str> = diagram["isolines"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|c| c["family"].as_str())
            .collect();
        assert!(families.contains("quality"), "{families:?}");
        assert!(families.contains("isobar"), "{families:?}");
        assert_eq!(diagram["markers"][0]["label"], "Critical point");
        assert!(diagram["markers"][0]["x"].is_number(), "{diagram}");
    }

    #[test]
    fn a_property_diagram_validates_its_inputs_before_the_backend() {
        let blank = parsed(&property_diagram("   ", "T-s"));
        assert_eq!(blank["error"], "A fluid name is required");
        let kind = parsed(&property_diagram("Water", "q-w"));
        assert!(
            kind["error"]
                .as_str()
                .unwrap_or_default()
                .contains("Unknown diagram type 'q-w'"),
            "{kind}"
        );
    }

    /// The psychrometric chart is generated whatever the backend can do; with
    /// none, every point is a JSON `null` gap. The shape must still be exactly
    /// `PsychartResponse`, because the chart component parses it either way.
    #[test]
    fn the_psychrometric_chart_has_the_psychart_response_shape() {
        let payload = parsed(&psychrometric_chart("{}"));
        assert_key(&payload, "pressure", Value::is_number);
        assert_key(&payload, "tMin", Value::is_number);
        assert_key(&payload, "tMax", Value::is_number);
        assert_key(&payload, "curves", Value::is_array);
        assert_eq!(payload["pressure"], 101_325.0);
        let curves = payload["curves"].as_array().expect("array");
        assert!(!curves.is_empty(), "{payload}");
        for curve in curves {
            assert_key(curve, "family", Value::is_string);
            assert_key(curve, "label", Value::is_string);
            assert_key(curve, "x", Value::is_array);
            assert_key(curve, "y", Value::is_array);
            let xs = curve["x"].as_array().expect("array");
            let ys = curve["y"].as_array().expect("array");
            assert_eq!(xs.len(), ys.len(), "{curve}");
            // Every element is a number or an explicit null — never a string,
            // never a fabricated 0.
            assert!(
                xs.iter().chain(ys).all(|v| v.is_number() || v.is_null()),
                "{curve}"
            );
        }
    }

    #[test]
    fn the_psychrometric_chart_accepts_an_explicit_window_and_refuses_a_bad_one() {
        let payload = parsed(&psychrometric_chart(
            r#"{"pressure": 90000, "tMin": 283.15, "tMax": 303.15}"#,
        ));
        assert_eq!(payload["pressure"], 90_000.0);
        assert_eq!(payload["tMin"], 283.15);
        let bad = parsed(&psychrometric_chart(r#"{"pressure": 500}"#));
        assert!(
            bad["error"]
                .as_str()
                .unwrap_or_default()
                .contains("pressure > 1 kPa"),
            "{bad}"
        );
        let malformed = parsed(&psychrometric_chart("{not json"));
        assert!(
            malformed["error"]
                .as_str()
                .unwrap_or_default()
                .contains("Malformed request"),
            "{malformed}"
        );
        // An empty body is the all-defaults chart, like an absent request body.
        assert_eq!(parsed(&psychrometric_chart(""))["pressure"], 101_325.0);
    }

    // ── Uncertainty on the wire (VariableDto.uncertainty + uncertaintyBreakdown) ──

    #[test]
    fn an_in_text_uncertainty_reaches_the_variable_rows_and_the_breakdown() {
        // y = x^2 with u(x) = 0.1 propagates to |dy/dx|*u = 2*3*0.1 = 0.6.
        // The Java oracle answers 0.6000000039736431 (a finite-difference
        // Jacobian, not the closed form); fixtures/golden/av_unc_square_law.json
        // pins that number, so this only checks it reaches the DTO.
        let v = parsed(&solve(
            "x = 3\nUncertaintyOf(x) = 0.1\ny = x^2\nuy = UncertaintyOf(y)\n",
            "{}",
        ));
        assert_eq!(v["success"], true, "{v}");
        let ux = variable(&v, "x")["uncertainty"].as_f64().expect("u(x)");
        assert!((ux - 0.1).abs() < 1e-12, "u(x) = {ux}");
        let uy = variable(&v, "y")["uncertainty"].as_f64().expect("u(y)");
        assert!((uy - 0.6).abs() < 1e-6, "u(y) = {uy}");

        let breakdown = v["uncertaintyBreakdown"]
            .as_array()
            .expect("uncertaintyBreakdown must be an array");
        let entry = breakdown
            .iter()
            .find(|e| e["variable"] == "y")
            .unwrap_or_else(|| panic!("no breakdown for y in {v}"));
        assert!(
            (entry["total"].as_f64().unwrap() - 0.6).abs() < 1e-6,
            "{entry}"
        );
        let sources = entry["sources"].as_array().expect("sources");
        assert_eq!(sources.len(), 1, "{entry}");
        assert_eq!(sources[0]["source"], "x");
    }

    #[test]
    fn a_document_with_no_uncertainty_reports_zero_and_an_empty_breakdown() {
        // Not null: `partitionVariables` zero-fills `uncertainties` over every
        // variable before it looks at a single spec, so the Java DTO carries
        // 0.0 for a document that declares nothing. The breakdown *is* empty —
        // `contributions` stays untouched when there is no source.
        let v = parsed(&solve("x = 3\ny = x^2\n", "{}"));
        assert_eq!(v["success"], true, "{v}");
        assert_eq!(variable(&v, "y")["uncertainty"].as_f64(), Some(0.0), "{v}");
        assert_eq!(
            v["uncertaintyBreakdown"].as_array().map(Vec::len),
            Some(0),
            "{v}"
        );
    }

    #[test]
    fn a_variable_info_uncertainty_is_a_source_and_scales_by_the_unit_factor() {
        // The `±` column of the Variable Information window. `1 [kPa]` of
        // uncertainty is 1000 Pa — the unit's FACTOR applies, its offset does
        // not, because a sigma is an interval width.
        let request = r#"{"variableInfo": [
            {"name": "p", "guess": 1, "units": "kPa", "uncertainty": 1}
        ]}"#;
        let v = parsed(&solve(
            "p = 200 [kPa]\nq = 2*p\nuq = UncertaintyOf(q)\n",
            request,
        ));
        assert_eq!(v["success"], true, "{v}");
        let uq = variable(&v, "uq")["value"].as_f64().expect("uq");
        assert!((uq - 2000.0).abs() < 1e-6, "u(q) = {uq}");
    }
}
