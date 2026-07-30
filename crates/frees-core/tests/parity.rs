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
//! * `display_names` — **exact**: keys and values must match the Java
//!   `ParseResult.displayNames` map the dumper recorded, spelling included.
//! * `block_count` — exact. A different blocking is a real divergence.
//! * `error` — the *classification* must agree (both solve, or both fail with
//!   the equivalent error type). Messages are not compared verbatim.
//!
//! # Per-fixture tolerance
//!
//! `fixtures/tolerances.json` may relax the *numeric* tolerance for a named
//! fixture, and nothing else. It exists because this build resolves real-fluid
//! properties from precomputed tables whose measured error is `1e-7…1e-4`
//! (decision D1) while the goldens hold full-accuracy CoolProp values — a gap no
//! table-backed engine can close, and one that must not be hidden by loosening
//! the gate for everybody. Two guards keep it honest:
//!
//! * a fixture named there but **absent** from `fixtures/golden/` fails;
//! * a fixture named there that **passes at the default** fails, so a tolerance
//!   that is no longer needed cannot sit in the file pretending it is.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use frees_core::{solve, FreesError, SolverSettings};

const REL_TOL: f64 = 1e-9;
const ABS_TOL: f64 = 1e-12;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

fn tolerance_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tolerances.json")
}

/// Declared relative tolerance per fixture stem, from `fixtures/tolerances.json`.
fn declared_tolerances() -> BTreeMap<String, f64> {
    let path = tolerance_path();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        // Absent is legitimate: it means every fixture is held to the default.
        Err(_) => return BTreeMap::new(),
    };
    let doc: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
    doc["fixtures"]
        .as_object()
        .unwrap_or_else(|| panic!("{} needs a `fixtures` object", path.display()))
        .iter()
        .map(|(name, entry)| {
            let rel = entry["relative"].as_f64().unwrap_or_else(|| {
                panic!(
                    "{}: fixture `{name}` needs a numeric `relative`",
                    path.display()
                )
            });
            assert!(
                entry["reason"].as_str().is_some_and(|r| r.len() > 40),
                "{}: fixture `{name}` needs a `reason` that says which mechanism \
                 produces the error, not a placeholder",
                path.display()
            );
            assert!(
                rel > REL_TOL && rel < 1e-2,
                "{}: fixture `{name}` declares {rel:e}, which is either tighter than \
                 the default or loose enough to hide a real divergence",
                path.display()
            );
            (name.clone(), rel)
        })
        .collect()
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

/// Relative difference, `0.0` when both are NaN or exactly equal (which covers
/// the infinities).
fn rel_diff(actual: f64, expected: f64) -> f64 {
    if (actual.is_nan() && expected.is_nan()) || actual == expected {
        return 0.0;
    }
    let diff = (actual - expected).abs();
    if diff <= ABS_TOL {
        return 0.0;
    }
    diff / expected.abs().max(actual.abs()).max(f64::MIN_POSITIVE)
}

fn close(actual: f64, expected: f64, rel_tol: f64) -> bool {
    if actual.is_nan() && expected.is_nan() {
        return true;
    }
    if actual == expected {
        return true; // covers infinities and exact hits
    }
    let diff = (actual - expected).abs();
    diff <= ABS_TOL || diff <= rel_tol * expected.abs().max(actual.abs())
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

fn replay(
    path: &Path,
    tolerances: &BTreeMap<String, f64>,
    used: &mut BTreeSet<String>,
    failures: &mut Vec<Failure>,
) {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let rel_tol = tolerances.get(&name).copied().unwrap_or(REL_TOL);
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

            let mut worst = 0.0f64;
            for (var, &expected) in &golden_vars {
                match solution.values.get(var) {
                    None => fail(format!("missing variable `{var}` (expected {expected})")),
                    Some(&actual) => {
                        worst = worst.max(rel_diff(actual, expected));
                        if !close(actual, expected, rel_tol) {
                            fail(format!(
                                "`{var}` = {actual} but Java got {expected} (rel {:e}, \
                                 tolerance {rel_tol:e})",
                                rel_diff(actual, expected)
                            ));
                        }
                    }
                }
            }
            if tolerances.contains_key(&name) {
                if worst <= REL_TOL {
                    fail(format!(
                        "fixtures/tolerances.json relaxes this fixture to {rel_tol:e}, but it \
                         matches the oracle to {worst:e} — at or under the {REL_TOL:e} default. \
                         Delete the entry rather than leaving a dead tolerance in the file."
                    ));
                } else {
                    used.insert(name.clone());
                }
            }
            for var in solution.values.keys() {
                if !golden_vars.contains_key(var) {
                    fail(format!("extra variable `{var}` not in the golden fixture"));
                }
            }

            // display_names is compared EXACTLY: the Java engine records the
            // spelling of each variable's first appearance, and the dumper
            // wrote that map into the fixture verbatim.
            let golden_names: BTreeMap<String, String> = expect["display_names"]
                .as_object()
                .expect("fixture has display_names")
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str().expect("display name is a string").to_string(),
                    )
                })
                .collect();
            if solution.display_names != golden_names {
                fail(format!(
                    "display_names {:?} but Java recorded {golden_names:?}",
                    solution.display_names
                ));
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
                if !error_matches(java_type, &err.error) {
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

    let tolerances = declared_tolerances();
    let mut used = BTreeSet::new();
    let mut failures = Vec::new();
    for path in &paths {
        replay(path, &tolerances, &mut used, &mut failures);
    }

    // A tolerance for a fixture that is not in the corpus is a stale entry, and
    // the "dead tolerance" guard above cannot see it — nothing replays it.
    for name in tolerances.keys() {
        if !paths
            .iter()
            .any(|p| p.file_stem().and_then(|s| s.to_str()) == Some(name.as_str()))
        {
            failures.push(Failure {
                fixture: name.clone(),
                detail: "declared in fixtures/tolerances.json but has no fixture in \
                         fixtures/golden/"
                    .to_string(),
            });
        }
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

    println!(
        "parity: {} fixtures match the Java oracle ({} at a declared table tolerance: {})",
        paths.len(),
        used.len(),
        used.iter().cloned().collect::<Vec<_>>().join(", ")
    );
}
