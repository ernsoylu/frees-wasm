//! Adversarial DTO-parity tests for the wasm boundary.
//!
//! `web/src/api.ts` declares `SolveResponse` and `CheckResponse` structurally
//! and the UI dereferences them without runtime guards, so every field the
//! frontend consumes must arrive with the exact camelCase key and JSON type —
//! and *every* input, however hostile, must come back as one of the two
//! envelopes. A thrown JS exception from `solve()`/`check()` is a boundary
//! bug by contract (the worker shim treats exceptions as infrastructure
//! failures, not document problems).
//!
//! The reference wire shapes are the Java DTOs:
//! `SolveController.SolveResponse` / `CheckController.CheckResponse` over
//! `SolveDtos` (`../frEES/backend`), which `api.ts` was written against.

use frees_wasm::{check, solve};
use serde_json::Value;

fn parsed(payload: &str) -> Value {
    serde_json::from_str(payload).unwrap_or_else(|e| {
        panic!("boundary output must be valid JSON ({e}): {payload}");
    })
}

/// Asserts `value[key]` exists and satisfies `pred` — api.ts types are
/// structural, so presence and type are both load-bearing.
fn assert_key(value: &Value, key: &str, pred: impl Fn(&Value) -> bool) {
    let field = value
        .get(key)
        .unwrap_or_else(|| panic!("missing key {key:?} in {value}"));
    assert!(pred(field), "key {key:?} has the wrong type: {field}");
}

/// The full `SolveResponse` key set `api.ts` consumes, with the JSON types it
/// expects. Valid for the success AND failure envelopes: `mapSolveData` reads
/// every one of these keys off the parsed payload.
fn assert_solve_envelope(v: &Value) {
    assert_key(v, "success", Value::is_boolean);
    assert_key(v, "variables", Value::is_array);
    assert_key(v, "blocks", Value::is_array);
    assert_key(v, "residuals", Value::is_array);
    assert!(
        v["stats"].is_object() || v["stats"].is_null(),
        "stats must be object|null: {v}"
    );
    assert_key(v, "solutions", Value::is_array);
    assert_key(v, "unitWarnings", Value::is_array);
    assert!(
        v["error"].is_string() || v["error"].is_null(),
        "error must be string|null: {v}"
    );
    assert!(
        v["errorLine"].is_u64() || v["errorLine"].is_null(),
        "errorLine must be number|null: {v}"
    );
    assert!(
        v["failedBlockIndex"].is_u64() || v["failedBlockIndex"].is_null(),
        "failedBlockIndex must be number|null: {v}"
    );

    // Success and failure are mutually exclusive with the error field.
    if v["success"] == true {
        assert!(v["error"].is_null(), "success with an error: {v}");
        assert!(v["stats"].is_object(), "success without stats: {v}");
    } else {
        assert!(v["error"].is_string(), "failure without an error: {v}");
        // Two failure shapes, both Java (`SolveController`):
        // * pre-block failures (syntax, structural) — the empty
        //   `SolveResponse.failure` envelope with `stats: null`;
        // * a block-loop stall — the enriched 422 envelope carrying the block
        //   structure, the finite residuals at the stalled iterate, populated
        //   stats and `failedBlockIndex` (`SolverException.partialResult`).
        if v["failedBlockIndex"].is_u64() {
            assert!(
                v["stats"].is_object(),
                "block failure without partial stats: {v}"
            );
        } else {
            assert!(v["stats"].is_null(), "pre-block failure with stats: {v}");
            assert!(
                v["blocks"].as_array().is_some_and(Vec::is_empty),
                "pre-block failure with blocks: {v}"
            );
        }
        // Both shapes ship no solved values (the Java partial `Result` carries
        // `Map.of()` for variables).
        assert!(
            v["variables"].as_array().is_some_and(Vec::is_empty),
            "failure with variables: {v}"
        );
    }

    // Every VariableResult row: {name: string, value: number, units: string}.
    // `value` must be a JSON number — a NaN/Infinity would serialize as null
    // and the UI types `value: number` (required, no guard).
    for var in v["variables"].as_array().unwrap() {
        assert_key(var, "name", Value::is_string);
        assert_key(var, "value", Value::is_f64);
        assert_key(var, "units", Value::is_string);
    }
    // Every BlockResult row: {index: number, equations: string[], variables: string[]}.
    for block in v["blocks"].as_array().unwrap() {
        assert_key(block, "index", Value::is_u64);
        assert_key(block, "equations", |e| {
            e.as_array().is_some_and(|a| a.iter().all(Value::is_string))
        });
        assert_key(block, "variables", |e| {
            e.as_array().is_some_and(|a| a.iter().all(Value::is_string))
        });
    }
    // Every ResidualResult row: {equation: string, value: number}.
    for r in v["residuals"].as_array().unwrap() {
        assert_key(r, "equation", Value::is_string);
        assert_key(r, "value", Value::is_f64);
    }
    // unitWarnings are plain strings (the banner join()s them).
    for w in v["unitWarnings"].as_array().unwrap() {
        assert!(w.is_string(), "unit warning must be a string: {w}");
    }
    // SolutionResult rows: {variables: [...], maxResidual: number}.
    for s in v["solutions"].as_array().unwrap() {
        assert_key(s, "variables", Value::is_array);
        assert_key(s, "maxResidual", Value::is_f64);
    }
    if let Some(stats) = v["stats"].as_object() {
        for key in [
            "equations",
            "unknowns",
            "blocks",
            "iterations",
            "elapsedMillis",
        ] {
            assert!(
                stats.get(key).is_some_and(Value::is_u64),
                "stats.{key} must be a number: {v}"
            );
        }
        assert!(
            stats.get("maxResidual").is_some_and(Value::is_f64),
            "stats.maxResidual must be a number: {v}"
        );
    }
}

/// The full `CheckResponse` key set `api.ts` consumes, with its JSON types.
fn assert_check_envelope(v: &Value) {
    assert_key(v, "solvable", Value::is_boolean);
    assert_key(v, "equations", Value::is_u64);
    assert_key(v, "unknowns", Value::is_u64);
    assert_key(v, "variables", |e| {
        e.as_array().is_some_and(|a| a.iter().all(Value::is_string))
    });
    assert_key(v, "unitWarnings", |e| {
        e.as_array().is_some_and(|a| a.iter().all(Value::is_string))
    });
    assert_key(v, "inferredUnits", Value::is_object);
    assert_key(v, "message", Value::is_string);
    assert!(
        v["errorLine"].is_u64() || v["errorLine"].is_null(),
        "errorLine must be number|null: {v}"
    );
    assert_key(v, "errors", Value::is_array);
    for (name, unit) in v["inferredUnits"].as_object().unwrap() {
        assert!(unit.is_string(), "inferredUnits[{name:?}] must be a string");
    }
    // EditorSyntaxError rows: 1-based {line, column} + message — the editor
    // filters `line >= 1` and squiggles from `column - 1`, so 0-based
    // positions would silently drop or shift every mark.
    for e in v["errors"].as_array().unwrap() {
        assert!(
            e["line"].as_u64().is_some_and(|l| l >= 1),
            "line must be 1-based: {e}"
        );
        assert!(
            e["column"].as_u64().is_some_and(|c| c >= 1),
            "column must be 1-based: {e}"
        );
        assert_key(e, "message", Value::is_string);
    }
}

fn solved(source: &str) -> Value {
    let v = parsed(&solve(source, "{}"));
    assert_solve_envelope(&v);
    v
}

fn checked(source: &str) -> Value {
    let v = parsed(&check(source, "{}"));
    assert_check_envelope(&v);
    v
}

// ─────────────────────────────────────────────────────────────────────────────
// The exact request bodies api.ts sends today
// ─────────────────────────────────────────────────────────────────────────────

/// `solve()` in api.ts serializes this full body (minus `text`). Unknown
/// fields must be ignored, not rejected — the frontend keeps sending the
/// complete former POST body unchanged.
#[test]
fn the_full_api_ts_solve_request_body_is_accepted() {
    let request = r#"{
        "stopCriteria": {"maxIterations": 250, "relativeResiduals": 1e-12,
                         "changeInVariables": 1e-15, "elapsedTimeSeconds": 3600,
                         "complexMode": false},
        "variableInfo": [{"name": "x", "guess": 1, "lower": null, "upper": null,
                          "units": null, "uncertainty": null}],
        "findAllSolutions": false,
        "displayUnitSystem": "ENG_SI",
        "fillMissing": true,
        "functionTables": [{"name": "t", "argNames": ["x"], "xLog": false,
                            "yLog": false, "curves": []}],
        "overrides": ["eta = 0.75"]
    }"#;
    let v = parsed(&solve("x = 2\n", request));
    assert_solve_envelope(&v);
    assert_eq!(v["success"], true, "{v}");
}

/// `check()` in api.ts sends `stopCriteria: {complexMode}` plus
/// functionTables/overrides; same tolerance requirement.
#[test]
fn the_full_api_ts_check_request_body_is_accepted() {
    let request = r#"{
        "variableInfo": [],
        "stopCriteria": {"complexMode": false},
        "functionTables": [],
        "overrides": []
    }"#;
    let v = parsed(&check("x = 2\n", request));
    assert_check_envelope(&v);
    assert_eq!(v["solvable"], true, "{v}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Units column: the Java toVariableDto conventions
// ─────────────────────────────────────────────────────────────────────────────

/// Java's `UnitChecker` marks a *derived dimensionless* unit `"-"`, and that
/// marker reaches the wire (the UI special-cases it: `v.units !== '-'` in
/// sliders.ts / App.tsx). A dimensionless-derived variable must keep it.
#[test]
fn derived_dimensionless_units_are_the_dash_marker() {
    let v = solved("x = 1\n");
    assert_eq!(v["variables"][0]["units"], "-", "{v}");
}

/// A variable *absent* from the units map takes the Java fallback `""`
/// (`unitsByLowerName.getOrDefault(canonicalName, "")`) — never `"-"`, which
/// means "explicitly dimensionless", and never a missing key, which would
/// break the required `units: string` of api.ts.
#[test]
fn declared_and_derived_units_fill_the_units_column() {
    let v = solved("P = 140 [kPa]\nQ = P * 2\n");
    for var in v["variables"].as_array().unwrap() {
        assert!(var["units"].is_string(), "{v}");
        assert!(!var["units"].as_str().unwrap().is_empty(), "{v}");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The SolveDiagnostics join key: blocks[].equations ↔ residuals[].equation
// ─────────────────────────────────────────────────────────────────────────────

/// SolveDiagnostics.tsx builds `Map(block.equations[i] → block.index)` and
/// looks residuals up by `residual.equation` — the two sides must be the
/// *same source text strings* (never indices) or every residual loses its
/// block badge.
#[test]
fn residual_equations_join_onto_block_equations_by_source_text() {
    let v = solved("a = 2\nb = a + 3\nc = b * a\n");
    let mut block_equations = std::collections::BTreeSet::new();
    for block in v["blocks"].as_array().unwrap() {
        for eq in block["equations"].as_array().unwrap() {
            block_equations.insert(eq.as_str().unwrap().to_string());
        }
    }
    for r in v["residuals"].as_array().unwrap() {
        let eq = r["equation"].as_str().unwrap();
        assert!(
            block_equations.contains(eq),
            "residual equation {eq:?} not present in any block: {v}"
        );
    }
}

/// blocks[].index is 0-based (Java `Blocker` numbers from 0) and dense, and
/// failedBlockIndex — recovered from the "Block N (…) failed" prose — must be
/// in the same 0-based domain so the red badge matches a real block.
#[test]
fn block_indexes_are_zero_based_and_dense() {
    let v = solved("a = 2\nb = a + 3\n");
    let blocks = v["blocks"].as_array().unwrap();
    for (i, block) in blocks.iter().enumerate() {
        assert_eq!(block["index"], i as u64, "{v}");
    }
}

#[test]
fn a_failed_block_reports_its_zero_based_index() {
    // Block 0 assigns k; block 1 (exp(x) = -k) has no real root and fails.
    let v = parsed(&solve("k = 1\nexp(x) = -k\n", "{}"));
    assert_solve_envelope(&v);
    assert_eq!(v["success"], false);
    assert_eq!(v["failedBlockIndex"], 1, "{v}");

    // The enriched Java 422 envelope (`SolverException.partialResult`): the
    // full block structure with source-text equations, the finite residuals at
    // the stalled iterate (block 0 solved, so `k = 1` reads 0.0 there), and
    // populated stats — this is what the SolveDiagnostics tab renders.
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2, "{v}");
    assert_eq!(blocks[1]["equations"][0], "exp(x) = -k", "{v}");
    let residuals = v["residuals"].as_array().unwrap();
    assert!(
        residuals
            .iter()
            .any(|r| r["equation"] == "k = 1" && r["value"] == 0.0),
        "solved upstream residual missing: {v}"
    );
    assert_eq!(v["stats"]["blocks"], 2, "{v}");
    assert_eq!(v["stats"]["unknowns"], 2, "{v}");
}

// ─────────────────────────────────────────────────────────────────────────────
// errorLine / errors[]: 1-based, both endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn solve_error_line_is_one_based_for_a_first_line_error() {
    let v = parsed(&solve("x = = 1\n", "{}"));
    assert_solve_envelope(&v);
    // A 0-based encoding would say 0 here — the editor would drop the mark
    // (its filter is `line >= 1`).
    assert_eq!(v["errorLine"], 1, "{v}");
}

#[test]
fn check_error_line_and_column_are_one_based_for_a_first_line_error() {
    let v = checked("= 1\n");
    assert_eq!(v["solvable"], false);
    assert_eq!(v["errorLine"], 1, "{v}");
    assert_eq!(v["errors"][0]["line"], 1, "{v}");
    assert_eq!(v["errors"][0]["column"], 1, "{v}");
}

#[test]
fn check_error_line_tracks_later_lines() {
    let v = checked("a = 1\nb = 2\nc = = 3\n");
    assert_eq!(v["errorLine"], 3, "{v}");
    assert_eq!(v["errors"][0]["line"], 3, "{v}");
}

#[test]
fn crlf_documents_report_the_same_editor_line() {
    let lf = parsed(&solve("a = 1\nb = = 2\n", "{}"));
    let crlf = parsed(&solve("a = 1\r\nb = = 2\r\n", "{}"));
    assert_eq!(lf["errorLine"], 2, "{lf}");
    assert_eq!(crlf["errorLine"], 2, "{crlf}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Non-finite arithmetic: JSON has no NaN/Infinity
// ─────────────────────────────────────────────────────────────────────────────

/// Documents that force NaN/Infinity must fail as data — a success envelope
/// could not carry the value (serde_json would emit `null` where api.ts
/// requires `value: number`, and the Variable Explorer reads it unguarded).
#[test]
fn non_finite_arithmetic_is_a_failure_envelope_not_a_null_value() {
    for source in [
        "x = 1/0\n",
        "x = 0/0\n",
        "x = 1e400\n",
        "x = exp(9000)\n",
        "x = ln(0)\n",
        "big = 1e308\nx = big * 10\n",
    ] {
        let v = parsed(&solve(source, "{}"));
        assert_solve_envelope(&v);
        assert_eq!(v["success"], false, "expected failure for {source:?}: {v}");
        // And the raw payload must never smuggle a null value slot.
        assert!(
            !payload_has_null_variable_value(&v),
            "null variable value leaked for {source:?}: {v}"
        );
    }
}

fn payload_has_null_variable_value(v: &Value) -> bool {
    v["variables"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|var| var["value"].is_null())
}

// ─────────────────────────────────────────────────────────────────────────────
// Hostile documents: everything is data, nothing is an exception
// ─────────────────────────────────────────────────────────────────────────────

/// Every hostile document must produce a well-formed envelope from BOTH
/// endpoints. In wasm a panic traps and surfaces as a thrown JS exception —
/// the exact thing the boundary contract forbids.
#[test]
fn hostile_documents_always_return_envelopes() {
    let deep_parens = format!("x = {}1{}\n", "(".repeat(50_000), ")".repeat(50_000));
    let long_chain = format!("x = {}1\n", "1 + ".repeat(50_000));
    let many_errors = "= 1\n".repeat(500);
    let hostile: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("whitespace", "   \n\t\r\n  ".into()),
        ("nul byte", "x = 1\u{0000}\n".into()),
        (
            "unterminated brace comment",
            "{ never closed\nx = 2\n".into(),
        ),
        (
            "unterminated quote comment",
            "\" never closed\nx = 2\n".into(),
        ),
        ("unterminated string", "s$ = 'oops\n".into()),
        ("unicode identifier", "\u{03b1} = 2\n".into()),
        ("emoji", "x = 1 \u{1f4a5}\n".into()),
        (
            "unicode comment then error",
            "{ caf\u{e9} \u{1f600} } x = = 2\n".into(),
        ),
        ("only operators", "^*/+-=()[]\n".into()),
        ("deep parens", deep_parens),
        ("long flat chain", long_chain),
        ("error cascade", many_errors),
        ("lone backslash", "\\\n".into()),
        ("stray dollar", "$ = 1\n".into()),
    ];
    for (label, source) in &hostile {
        let s = parsed(&solve(source, "{}"));
        assert_solve_envelope(&s);
        assert_eq!(s["success"], false, "{label}: {s}");
        let c = parsed(&check(source, "{}"));
        assert_check_envelope(&c);
        assert_eq!(c["solvable"], false, "{label}: {c}");
    }
}

/// A UTF-8 byte-order mark is tolerated, not a syntax error (editors and
/// copy-paste sources prepend them silently).
#[test]
fn a_byte_order_mark_still_solves() {
    let v = solved("\u{feff}x = 1\n");
    assert_eq!(v["success"], true, "{v}");
}

/// A large but well-formed document must solve (bounded only by memory/time,
/// not by any hidden recursion limit in the response builder).
#[test]
fn a_huge_document_round_trips_through_the_envelope() {
    let mut source = String::from("v0 = 1\n");
    for i in 1..5_000 {
        source.push_str(&format!("v{i} = v{} + 1\n", i - 1));
    }
    let v = solved(&source);
    assert_eq!(v["success"], true);
    assert_eq!(v["variables"].as_array().unwrap().len(), 5_000);
    assert_eq!(v["stats"]["unknowns"], 5_000);
}

// ─────────────────────────────────────────────────────────────────────────────
// Hostile request JSON: rejected as data, never thrown
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn hostile_request_json_is_always_an_envelope() {
    let hostile = [
        "null",
        "true",
        "42",
        "\"a string\"",
        "{not json",
        "{\"variableInfo\": null}",
        "{\"variableInfo\": [null]}",
        "{\"variableInfo\": [{\"name\": 3}]}",
        "{\"variableInfo\": [{\"name\": \"x\", \"guess\": \"NaN\"}]}",
        "{\"variableInfo\": [{\"name\": \"x\", \"guess\": 1e999}]}",
        "{\"stopCriteria\": {\"maxIterations\": -1}}",
        "{\"stopCriteria\": {\"maxIterations\": 1e99}}",
        "{\"stopCriteria\": \"fast\"}",
        "{\"stopCriteria\": {\"relativeResiduals\": \"tight\"}}",
    ];
    for request in hostile {
        let s = parsed(&solve("x = 1\n", request));
        assert_solve_envelope(&s);
        let c = parsed(&check("x = 1\n", request));
        assert_check_envelope(&c);
    }
}

/// The two blank-request conventions of the worker shim: `""` and `"{}"` both
/// mean "no overrides, default settings".
#[test]
fn blank_requests_mean_defaults() {
    for request in ["", "{}", "   "] {
        let v = parsed(&solve("x = 2\n", request));
        assert_solve_envelope(&v);
        assert_eq!(v["success"], true, "request {request:?}: {v}");
    }
}
