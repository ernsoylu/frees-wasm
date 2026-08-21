//! `POST /api/solve/table` — the Tables workbook's GUI Solve, in-browser.
//!
//! Port of `SolveController.computeSolveTable` and its helpers: the sweep
//! driver itself (`run_sweep`, `build_columns`, `columns_converged`,
//! `mentions_parametric_accessor`) was ported to
//! `frees_core::analysis::parametric` in Phase 8 and had **zero call sites**
//! until this module — ledger item 23's "with no boundary, there is no place
//! for the Java controllers' input validation to live" is closed here for the
//! table surface, with the Java caps transcribed and the wasm-native ones
//! marked as such.
//!
//! Same contract as every export in this crate: JSON string in, JSON string
//! out, never a JS exception. A cap breach or a blank document is *data* — the
//! envelope carries a top-level `"error"` beside empty `results`, which is the
//! same string the Java's 400/422 body would carry.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use frees_core::analysis::parametric::{run_sweep, RowJob, RowOutcome};
use frees_core::components::cyclepath;
use frees_core::parser::blocks::ParametricTable;

use crate::{
    explicit_units_of, now_ms, overrides_of, settings_of, unit_system_of, variable_entries,
    variable_rows, SolveRequest, StopCriteriaDto, VariableInfoDto,
};

// ---------------------------------------------------------------------------
// Caps. The first two are the Java controller's, transcribed with their
// values and messages; the third is wasm-native (no Java analogue — the Java
// has a horizontal compute tier behind a queue, this build has one worker),
// in the spirit of `measurement.rs`'s `MAX_INPUTS`.
// ---------------------------------------------------------------------------

/// `frees.solver.max-table-rows` (default) — `SolveController`.
const MAX_TABLE_ROWS: usize = 5_000;

/// `frees.solver.max-table-seconds` (default) — the *cooperative* budget for
/// the whole request: checked between rows and per row inside each accessor
/// pass, never mid-solve, exactly like the Java `TableDeadline`.
const MAX_TABLE_SECONDS: f64 = 120.0;

/// wasm-native: a bound on the declared column count, so the dense grid this
/// boundary builds from the sparse row maps cannot be inflated independently
/// of the row cap.
const MAX_TABLE_COLUMNS: usize = 256;

/// `SolveController.tooManyRowsMessage`, verbatim.
fn too_many_rows_message(rows: usize) -> String {
    format!(
        "The parametric table has too many rows ({rows}; limit {MAX_TABLE_ROWS}). \
         Reduce the run count."
    )
}

/// `SolveController.deadlineExceededMessage`, verbatim.
fn deadline_message() -> String {
    format!(
        "The parametric run exceeded its {}-second budget and was stopped. Reduce \
         the number of runs, or tighten the stop criteria so each run converges \
         faster.",
        MAX_TABLE_SECONDS as u64
    )
}

// ---------------------------------------------------------------------------
// Request DTOs — the Java `SolveTableRequest` minus `text` (which rides as
// the `source` argument, like `solve`'s). `functionTables` is accepted and
// ignored, the same treatment the `solve` export gives it.
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SolveTableRequest {
    stop_criteria: Option<StopCriteriaDto>,
    variable_info: Vec<VariableInfoDto>,
    display_unit_system: Option<String>,
    table: Option<TableDto>,
}

/// `SolveController.TableDto`: `variables` (the declared column order) and
/// `rows` as sparse maps — an absent or `null` value means the cell is blank,
/// i.e. an output the solve fills in.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TableDto {
    variables: Vec<String>,
    rows: Vec<BTreeMap<String, Option<f64>>>,
}

/// What the side-channel keeps per run beside `RowOutcome` (which carries
/// only the SI values the accessor columns need): the display-unit values and
/// DTOs the response is built from, and the per-row solve stats. The accessor
/// loop re-runs every row on every pass, so slot `run - 1` is simply
/// overwritten and the final pass wins.
struct RowSide {
    success: bool,
    values: BTreeMap<String, f64>,
    error: Option<String>,
    variables: Vec<Value>,
    iterations: usize,
    max_residual: f64,
    equations: usize,
    unknowns: usize,
}

/// Solve a Tables-workbook parametric sweep. `request_json` is the
/// `SolveTableRequest` body; returns a `SolveTableResponse` JSON string.
#[wasm_bindgen]
pub fn solve_table(source: &str, request_json: &str) -> String {
    match solve_table_inner(source, request_json) {
        Ok(value) => value.to_string(),
        Err(message) => json!({
            "results": [],
            "stats": Value::Null,
            "variables": [],
            "error": message,
        })
        .to_string(),
    }
}

fn solve_table_inner(source: &str, request_json: &str) -> Result<Value, String> {
    frees_core::props::tables::install_builtin_once();

    let request: SolveTableRequest = if request_json.trim().is_empty() {
        SolveTableRequest::default()
    } else {
        serde_json::from_str(request_json).map_err(|e| format!("Invalid request: {e}"))?
    };

    // The Java 400s: blank text, missing table. Same strings as data.
    if source.trim().is_empty() {
        return Err("The document is empty.".to_string());
    }
    let Some(table_dto) = request.table else {
        return Err("The request carries no table.".to_string());
    };
    if table_dto.rows.len() > MAX_TABLE_ROWS {
        return Err(too_many_rows_message(table_dto.rows.len()));
    }
    if table_dto.variables.len() > MAX_TABLE_COLUMNS {
        return Err(format!(
            "The parametric table has too many columns ({}; limit {MAX_TABLE_COLUMNS}). \
             Reduce the declared variables.",
            table_dto.variables.len()
        ));
    }

    // The Java parse gate before anything is enqueued: a syntax error answers
    // once, not once per row.
    if let Err(failure) = frees_core::parse_document(source) {
        return Err(format!("Syntax error: {}", failure.to_string_message()));
    }

    // The dense grid `ParametricTable` wants, from the sparse row maps: each
    // row aligned to `variables`, absent/null ⇒ `None` (an output cell). Key
    // lookup is exact, as the Java's `row.get(var)` is; the workbook keys its
    // rows by the declared names, so nothing else arrives in practice.
    let table = ParametricTable {
        name: String::new(),
        vars: table_dto.variables.clone(),
        rows: table_dto
            .rows
            .iter()
            .map(|row| {
                table_dto
                    .variables
                    .iter()
                    .map(|var| row.get(var).copied().flatten().filter(|v| v.is_finite()))
                    .collect()
            })
            .collect(),
    };

    // The `solve` helpers, reused verbatim through a facade request.
    let facade = SolveRequest {
        variable_info: request.variable_info,
        stop_criteria: request.stop_criteria,
        display_unit_system: request.display_unit_system.clone(),
        fill_missing: None,
    };
    let settings = settings_of(&facade);
    let overrides = overrides_of(&facade);
    let system = unit_system_of(&facade);
    let explicit_units = explicit_units_of(&facade);

    let started = now_ms();
    let run_count = table.run_count();
    let mut sides: Vec<Option<RowSide>> = Vec::new();
    sides.resize_with(run_count, || None);
    let mut deadline_hit = false;

    let sweep = run_sweep(&table, source, |job: RowJob<'_>| {
        // The cooperative deadline, at the Java's two check sites collapsed
        // into one: entry to every row solve, on every pass.
        if deadline_hit || (now_ms() - started) / 1000.0 > MAX_TABLE_SECONDS {
            deadline_hit = true;
            return RowOutcome::failed(deadline_message());
        }
        let slot = &mut sides[job.run - 1];
        match frees_core::solve_with_parametric(&job.source, &settings, &overrides, job.accessors) {
            Ok(mut solution) => {
                // The Java table row fills missing properties unconditionally,
                // scoped to the table's own columns — not gated on the
                // Preferences switch like `/api/solve`.
                let added = cyclepath::resolve_missing_properties(
                    &mut solution.values,
                    &mut solution.display_names,
                    &job.source,
                    Some(&table.vars),
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
                let (rows, uncertainties) = variable_rows(&solution, system, &explicit_units);
                let values: BTreeMap<String, f64> = rows
                    .iter()
                    .map(|row| (row.name.clone(), row.value))
                    .collect();
                *slot = Some(RowSide {
                    success: true,
                    values,
                    error: None,
                    variables: variable_entries(&rows, &uncertainties),
                    iterations: solution.stats.iterations,
                    max_residual: solution.stats.max_residual,
                    equations: solution.residuals.len(),
                    unknowns: solution.values.len(),
                });
                RowOutcome::solved(solution.values)
            }
            Err(failure) => {
                let message = failure.to_string_message();
                *slot = Some(RowSide {
                    success: false,
                    values: BTreeMap::new(),
                    error: Some(message.clone()),
                    variables: Vec::new(),
                    iterations: 0,
                    max_residual: 0.0,
                    equations: 0,
                    unknowns: 0,
                });
                RowOutcome::failed(message)
            }
        }
    });

    if deadline_hit {
        return Err(deadline_message());
    }

    // `SolveTableResponse`: per-row results in display units, the aggregated
    // stats (iterations summed, residual maxed, equations/unknowns assigned
    // from each successful row — last one wins, as the Java loop does), and
    // the last successful row's variable DTOs.
    let mut iterations = 0usize;
    let mut max_residual = 0.0f64;
    let mut equations = 0usize;
    let mut unknowns = 0usize;
    let mut solved = 0usize;
    let mut last_variables: Vec<Value> = Vec::new();
    let results: Vec<Value> = sides
        .iter()
        .map(|side| match side {
            Some(side) => {
                iterations += side.iterations;
                if side.success {
                    solved += 1;
                    max_residual = max_residual.max(side.max_residual);
                    equations = side.equations;
                    unknowns = side.unknowns;
                    if !side.variables.is_empty() {
                        last_variables = side.variables.clone();
                    }
                }
                json!({
                    "success": side.success,
                    "values": side.values,
                    "error": side.error,
                })
            }
            None => json!({
                "success": false,
                "values": {},
                "error": "The row was not solved.",
            }),
        })
        .collect();
    let _ = &sweep; // the sweep's columns fed the accessors; results are the answer

    Ok(json!({
        "results": results,
        "stats": {
            "runs": run_count,
            "solved": solved,
            "failed": run_count - solved,
            "equations": equations,
            "unknowns": unknowns,
            "iterations": iterations,
            "elapsedMillis": (now_ms() - started).round().max(0.0) as u64,
            "maxResidual": if max_residual.is_finite() { max_residual } else { 0.0 },
        },
        "variables": last_variables,
    }))
}
