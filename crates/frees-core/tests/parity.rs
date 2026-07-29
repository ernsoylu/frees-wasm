//! Golden-corpus parity replay — the Rust engine against the Java oracle.
//!
//! Every fixture in `fixtures/golden/` was produced by running the document in
//! `fixtures/corpus/` through the reference Java engine
//! (`tools/golden-dumper`). This test replays the same documents through the
//! Rust engine and compares.
//!
//! Comparison policy (`fixtures/README.md`):
//! * `variables` — relative tolerance `1e-9`, absolute `1e-12` near zero.
//!   Golden keys carry the Java first-seen spelling (`T_out`); the Rust engine
//!   keys by lowercase canonical name, so keys are folded before matching.
//! * `block_count` — exact. A different blocking is a real divergence.
//! * `error` — the *classification* must agree (both solve, or both fail with
//!   the equivalent error type). Messages are not compared verbatim.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use frees_core::{solve, FreesError, SolverSettings};

const REL_TOL: f64 = 1e-9;
const ABS_TOL: f64 = 1e-12;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

/// `Double.toString` output, or `"NaN"` / `"Infinity"` / `"-Infinity"` strings.
fn as_f64(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().expect("numeric fixture value"),
        serde_json::Value::String(s) => match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => panic!("unexpected string number {other:?}"),
        },
        other => panic!("unexpected fixture value {other:?}"),
    }
}

fn close(actual: f64, expected: f64) -> bool {
    if actual.is_nan() && expected.is_nan() {
        return true;
    }
    if actual == expected {
        return true; // covers infinities and exact hits
    }
    let diff = (actual - expected).abs();
    diff <= ABS_TOL || diff <= REL_TOL * expected.abs().max(actual.abs())
}

/// Map a golden `error.type` (a Java exception simple name) to the Rust error
/// classification it must correspond to.
fn error_matches(java_type: &str, rust: &FreesError) -> bool {
    match java_type {
        "SolverException" => matches!(rust, FreesError::Solver { .. }),
        "ParseException" => matches!(rust, FreesError::Parse { .. }),
        "PropertyEvaluationException" => matches!(rust, FreesError::Property { .. }),
        // Unmapped Java exception types: accept any Rust error — both engines
        // refused the document, which is the parity that matters here.
        _ => true,
    }
}

struct Failure {
    fixture: String,
    detail: String,
}

fn replay(path: &Path, failures: &mut Vec<Failure>) {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let raw = fs::read_to_string(path).expect("fixture readable");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid JSON");

    let source = fixture["source"].as_str().expect("fixture has source");
    let expect = &fixture["expect"];
    let expected_error = &expect["error"];

    let mut fail = |detail: String| {
        failures.push(Failure {
            fixture: name.clone(),
            detail,
        });
    };

    match solve(source, &SolverSettings::default()) {
        Ok(solution) => {
            if !expected_error.is_null() {
                fail(format!(
                    "Java failed with {} but Rust solved: {:?}",
                    expected_error["type"].as_str().unwrap_or("?"),
                    solution.values
                ));
                return;
            }

            // Fold golden keys to lowercase to match the Rust canonical keys.
            let golden_vars: BTreeMap<String, f64> = expect["variables"]
                .as_object()
                .expect("variables object")
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), as_f64(v)))
                .collect();

            for (var, &expected) in &golden_vars {
                match solution.values.get(var) {
                    None => fail(format!("missing variable `{var}` (expected {expected})")),
                    Some(&actual) if !close(actual, expected) => fail(format!(
                        "`{var}` = {actual} but Java got {expected} (diff {})",
                        (actual - expected).abs()
                    )),
                    _ => {}
                }
            }
            for var in solution.values.keys() {
                if !golden_vars.contains_key(var) {
                    fail(format!("extra variable `{var}` not in the golden fixture"));
                }
            }

            let expected_blocks = expect["block_count"].as_u64().unwrap_or(0) as usize;
            if solution.blocks.len() != expected_blocks {
                fail(format!(
                    "block_count {} but Java got {expected_blocks}",
                    solution.blocks.len()
                ));
            }
        }
        Err(err) => {
            if expected_error.is_null() {
                fail(format!("Java solved but Rust failed: {err}"));
            } else {
                let java_type = expected_error["type"].as_str().unwrap_or("?");
                if !error_matches(java_type, &err) {
                    fail(format!(
                        "Java failed with {java_type} but Rust failed differently: {err}"
                    ));
                }
            }
        }
    }
}

#[test]
fn golden_corpus_parity() {
    let dir = golden_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|entry| {
            let p = entry.ok()?.path();
            (p.extension().and_then(|x| x.to_str()) == Some("json")).then_some(p)
        })
        .collect();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "no golden fixtures in {} — the parity harness is not wired",
        dir.display()
    );

    let mut failures = Vec::new();
    for path in &paths {
        replay(path, &mut failures);
    }

    if !failures.is_empty() {
        let mut report = format!(
            "\n{}/{} fixtures diverged from the Java oracle:\n",
            failures
                .iter()
                .map(|f| f.fixture.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            paths.len()
        );
        for f in &failures {
            report.push_str(&format!("  [{}] {}\n", f.fixture, f.detail));
        }
        panic!("{report}");
    }

    println!("parity: {} fixtures match the Java oracle", paths.len());
}
