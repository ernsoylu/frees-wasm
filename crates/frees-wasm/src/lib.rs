//! wasm-bindgen boundary for the frees engine.
//!
//! Deliberately thin: it owns the JS type conversions and nothing else. All
//! engine logic lives in `frees-core`, which knows nothing about wasm.
//!
//! Results cross the boundary as JSON strings rather than structured
//! `JsValue`s: the frontend's `api.ts` already parses JSON responses from the
//! REST backend, so a string is the drop-in shape — the worker shim feeds it to
//! the same `JSON.parse` path the fetch layer used.

use frees_core::{FreesError, SolverSettings};
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

fn error_json(err: &FreesError) -> String {
    let kind = match err {
        FreesError::Parse { .. } => "parse",
        FreesError::Solver { .. } => "solver",
        FreesError::Property { .. } => "property",
        FreesError::Evaluation { .. } => "evaluation",
        FreesError::UnknownUnit { .. } => "unknown_unit",
    };
    let mut obj = serde_json::Map::new();
    obj.insert("ok".into(), serde_json::Value::Bool(false));
    obj.insert("error_kind".into(), kind.into());
    obj.insert("message".into(), err.to_string().into());
    if let Some(span) = err.span() {
        obj.insert("span_start".into(), span.start.into());
        obj.insert("span_end".into(), span.end.into());
    }
    serde_json::Value::Object(obj).to_string()
}

fn diagnostics_json(diags: &[frees_core::Diagnostic]) -> serde_json::Value {
    diags
        .iter()
        .map(|d| {
            serde_json::json!({
                "severity": match d.severity {
                    frees_core::Severity::Warning => "warning",
                    frees_core::Severity::Error => "error",
                },
                "message": d.message,
                "span_start": d.span.map(|s| s.start),
                "span_end": d.span.map(|s| s.end),
            })
        })
        .collect()
}

/// Solve a frees document. Returns a JSON string:
/// `{ok, variables, block_count, blocks, iterations, diagnostics}` on success,
/// `{ok: false, error_kind, message, span_start?, span_end?}` on failure.
#[wasm_bindgen]
pub fn solve(source: &str) -> String {
    match frees_core::solve(source, &SolverSettings::default()) {
        Ok(solution) => serde_json::json!({
            "ok": true,
            "variables": solution.values,
            "block_count": solution.blocks.len(),
            "blocks": solution.blocks.iter().map(|b| serde_json::json!({
                "equations": b.equations,
                "variables": b.variables,
            })).collect::<Vec<_>>(),
            "iterations": solution.iterations,
            "diagnostics": diagnostics_json(&solution.diagnostics),
        })
        .to_string(),
        Err(err) => error_json(&err),
    }
}

/// Check a document without solving — the `POST /api/check` equivalent.
/// Returns `{ok, solvable, equations, unknowns, variables, message, diagnostics}`.
#[wasm_bindgen]
pub fn check(source: &str) -> String {
    match frees_core::check(source) {
        Ok(report) => serde_json::json!({
            "ok": true,
            "solvable": report.solvable,
            "equations": report.equation_count,
            "unknowns": report.unknown_count,
            "variables": report.variables,
            "message": report.message,
            "diagnostics": diagnostics_json(&report.diagnostics),
        })
        .to_string(),
        Err(err) => error_json(&err),
    }
}

#[cfg(test)]
mod tests {
    use super::{check, solve};

    #[test]
    fn solve_produces_parseable_json_with_values() {
        let out = solve("x^2 + y^3 = 77\nx/y = 1.23456");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true);
        let x = v["variables"]["x"].as_f64().unwrap();
        assert!((x - 4.694012391660914).abs() < 1e-9);
    }

    #[test]
    fn solve_reports_errors_as_json_not_panics() {
        let out = solve("z = 1\nz = 2");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error_kind"], "solver");
    }

    #[test]
    fn check_reports_structure_without_solving() {
        let out = check("m + n = 5");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["solvable"], false);
        assert_eq!(v["unknowns"], 2);
    }
}
