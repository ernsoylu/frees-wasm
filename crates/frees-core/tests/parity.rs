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
//!
//!   The fold is the Java's own: `EquationSystemSolver.buildResult` keys
//!   `Result.variables()` with `displayNames.getOrDefault(name, name)`, so the
//!   golden's key is a *display* name, not a canonical one. For a scalar
//!   document the two coincide once lowercased (`T_out` → `t_out`), which is
//!   why folding the golden alone worked for the whole pre-component corpus.
//!   It stops coinciding the moment components expand: the canonical name is
//!   `s2$p` and its display name is `s2.P`. So this replay routes the **Rust**
//!   side through the same `display_names` map before lowercasing — identical
//!   to the previous behaviour on every non-component fixture, and the only
//!   thing that makes a component fixture comparable at all.
//! * `display_names` — **exact**: keys and values must match the Java
//!   `ParseResult.displayNames` map the dumper recorded, spelling included.
//! * `block_count` — exact. A different blocking is a real divergence.
//! * `error` — the *classification* must agree (both solve, or both fail with
//!   the equivalent error type). Messages are not compared verbatim.
//! * `ode_tables` — one entry per `DYNAMIC` block, compared in declaration
//!   order. `name`/`method`/`columns`/`stopped` and the event `name`s are
//!   **exact**; `end_time`, every row cell and every event `time` go through
//!   the same numeric tolerance as `variables`.
//!
//! # Why the `ode_tables` comparison is not optional
//!
//! **A solved `DYNAMIC` block puts nothing in `variables`.** The trajectory is a
//! first-class ODE Table, so a transient document's `variables` map holds only
//! its analytic parameters — `dyn_plain_ode` has exactly `{k, Tinf}` in it. A
//! fixture that compared `variables` alone would therefore pass *vacuously* on
//! every transient document in the corpus: the whole integration could be wrong,
//! or absent, and the gate would stay green.
//!
//! The comparison was validated the way the harness itself was in Phase 1 — by
//! perturbing a golden and watching the gate go red. Perturbing
//! `dyn_plain_ode`'s row `[20, 47.59095803046333]` to `47.6` produces
//!
//! ```text
//!   [dyn_plain_ode] ode_tables[0] `cooling` row 1 col `temp` = 47.59095803046333
//!   but Java got 47.6 (rel 1.9e-4, tolerance 1e-9)
//! ```
//!
//! and dropping the table entirely produces "Java recorded 1 ODE table(s), Rust
//! produced 0". Both were observed, then the golden was restored.
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

/// Which tolerance file grades this build — one per property backend.
///
/// Decision D9 pins the gate to **one** backend, but "one" cannot mean "one
/// file": the entries in `fixtures/tolerances.json` exist because of the (P,h)
/// tables' own interpolation error, and under rustprop most of them are dead
/// (the file's own rule then makes them failures) while the survivors have a
/// completely different cause — the *golden* side, where the Java answered
/// `(P,Hmass) → T/Dmass/Smass` from its own run-time 256/96/48 table. So each
/// backend is graded by the file that describes it, selected by the same `cfg`
/// that decides which backend `install_builtin_once` installs. There is no
/// configuration in which both files are read, and none in which neither is.
#[cfg(feature = "rustprop-backend")]
const TOLERANCE_FILE: &str = "tolerances-rustprop.json";
#[cfg(not(feature = "rustprop-backend"))]
const TOLERANCE_FILE: &str = "tolerances.json";

fn tolerance_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(TOLERANCE_FILE)
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

/// Declared Newton **stop criterion** per fixture stem, from the same file's
/// optional `solver_floor` object.
///
/// This is a different knob from `fixtures`, and it exists for a mechanism only
/// the accuracy path has. A `(P,h)` table is a bilinear surface, so a residual
/// like `T_out = Temperature(fluid, P, h)` is smooth in `h` and Newton drives it
/// to the `1e-12` default. rustprop answers the same call with an *iterative*
/// flash, whose output has a floor: it is the exact value to within its own
/// convergence, and stepping `h` by less than that moves `T` by a jump instead of
/// a slope. A block that carries such a residual therefore cannot be driven
/// below that floor by any line search — the engine reports "no full, halved or
/// damped step reduces the residual", which is the truth.
///
/// Relaxing the stop criterion for the named fixture is the honest response:
/// the *values* are still compared against the Java oracle at the ordinary
/// tolerance, so the assertion is intact — only the point at which the solver
/// stops chasing arithmetic noise moves. The guards mirror `fixtures`': an entry
/// whose fixture converges at the default is dead and fails, and an entry with
/// no fixture fails.
fn declared_solver_floors() -> BTreeMap<String, f64> {
    let path = tolerance_path();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => return BTreeMap::new(),
    };
    let doc: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
    let Some(entries) = doc["solver_floor"].as_object() else {
        // Absent is legitimate: it means every fixture solves at the default.
        return BTreeMap::new();
    };
    entries
        .iter()
        .map(|(name, entry)| {
            let rel = entry["rel_tolerance"].as_f64().unwrap_or_else(|| {
                panic!(
                    "{}: solver_floor `{name}` needs a numeric `rel_tolerance`",
                    path.display()
                )
            });
            assert!(
                entry["reason"].as_str().is_some_and(|r| r.len() > 40),
                "{}: solver_floor `{name}` needs a `reason` naming the residual whose \
                 property call has the floor, not a placeholder",
                path.display()
            );
            let default = SolverSettings::default().rel_tolerance;
            assert!(
                rel > default && rel < 1e-6,
                "{}: solver_floor `{name}` declares {rel:e}; the engine default is \
                 {default:e} and anything at or above 1e-6 stops the solver before the \
                 physics, not before the noise",
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

/// Compare the golden's `ode_tables` array against what the engine integrated.
///
/// A golden dumped before the dumper grew this section has `ode_tables` absent
/// (`Value::Null`), which is **not** the same as an empty array: absent means
/// "this fixture predates the section", empty means "the Java engine produced no
/// tables". Only the second is a claim, so only the second is checked against a
/// Rust engine that produced tables — otherwise every pre-Phase-7 fixture in the
/// corpus would fail the moment a `DYNAMIC` block started working.
fn compare_ode_tables(
    golden: &serde_json::Value,
    actual: &[frees_core::ode::problem::OdeTableResult],
    rel_tol: f64,
    fail: &mut impl FnMut(String),
) {
    let Some(expected) = golden.as_array() else {
        if !actual.is_empty() {
            fail(format!(
                "Rust produced {} ODE table(s) but the golden has no `ode_tables` section — \
                 re-dump this fixture with tools/golden-dumper so the trajectory is compared \
                 instead of ignored",
                actual.len()
            ));
        }
        return;
    };

    if expected.len() != actual.len() {
        fail(format!(
            "Java recorded {} ODE table(s), Rust produced {}",
            expected.len(),
            actual.len()
        ));
        return;
    }

    for (i, (want, got)) in expected.iter().zip(actual).enumerate() {
        let at = |what: &str| format!("ode_tables[{i}] `{}` {what}", got.name);

        // Identity and shape are exact: a renamed block, a different solver or a
        // reordered column set is a real divergence, not a rounding difference.
        for (field, expected_str, actual_str) in [
            (
                "name",
                want["name"].as_str().unwrap_or("?"),
                got.name.as_str(),
            ),
            (
                "method",
                want["method"].as_str().unwrap_or("?"),
                got.method.as_str(),
            ),
        ] {
            if expected_str != actual_str {
                fail(format!(
                    "{} = {actual_str:?} but Java got {expected_str:?}",
                    at(field)
                ));
            }
        }
        let want_columns: Vec<&str> = want["columns"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c.as_str()).collect())
            .unwrap_or_default();
        if want_columns != got.columns {
            fail(format!(
                "{} = {:?} but Java got {want_columns:?}",
                at("columns"),
                got.columns
            ));
            // Every row comparison below indexes by column, so a shape mismatch
            // would only produce noise.
            continue;
        }
        if want["stopped"].as_bool().unwrap_or(false) != got.stopped {
            fail(format!(
                "{} = {} but Java got {}",
                at("stopped"),
                got.stopped,
                want["stopped"]
            ));
        }
        let want_end = as_f64(&want["end_time"]);
        if !close(got.end_time, want_end, rel_tol) {
            fail(format!(
                "{} = {} but Java got {want_end} (rel {:e}, tolerance {rel_tol:e})",
                at("end_time"),
                got.end_time,
                rel_diff(got.end_time, want_end)
            ));
        }

        let want_rows = want["rows"].as_array().cloned().unwrap_or_default();
        if want_rows.len() != got.rows.len() {
            fail(format!(
                "{} — Java sampled {} row(s), Rust produced {}",
                at("rows"),
                want_rows.len(),
                got.rows.len()
            ));
            continue;
        }
        for (r, (want_row, got_row)) in want_rows.iter().zip(&got.rows).enumerate() {
            let cells = want_row.as_array().cloned().unwrap_or_default();
            if cells.len() != got_row.len() {
                fail(format!(
                    "{} row {r} has {} cell(s), Java had {}",
                    at("rows"),
                    got_row.len(),
                    cells.len()
                ));
                continue;
            }
            for (c, (want_cell, &got_cell)) in cells.iter().zip(got_row).enumerate() {
                let want_value = as_f64(want_cell);
                if !close(got_cell, want_value, rel_tol) {
                    fail(format!(
                        "{} row {r} col `{}` = {got_cell} but Java got {want_value} \
                         (rel {:e}, tolerance {rel_tol:e})",
                        at("rows"),
                        got.columns.get(c).map(String::as_str).unwrap_or("?"),
                        rel_diff(got_cell, want_value)
                    ));
                }
            }
        }

        // Events: the recorded name keeps its *source* case (the Java reads
        // `ctx.IDENT(0).getText()` raw), so it is compared exactly; the crossing
        // time is a solve output and takes the numeric tolerance.
        let want_events = want["events"].as_array().cloned().unwrap_or_default();
        if want_events.len() != got.events.len() {
            fail(format!(
                "{} — Java recorded {} event hit(s) ({:?}), Rust recorded {} ({:?})",
                at("events"),
                want_events.len(),
                want_events
                    .iter()
                    .map(|e| e["name"].as_str().unwrap_or("?"))
                    .collect::<Vec<_>>(),
                got.events.len(),
                got.events
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect::<Vec<_>>()
            ));
            continue;
        }
        for (e, (want_hit, got_hit)) in want_events.iter().zip(&got.events).enumerate() {
            let want_name = want_hit["name"].as_str().unwrap_or("?");
            if want_name != got_hit.name {
                fail(format!(
                    "{} hit {e} named `{}` but Java recorded `{want_name}`",
                    at("events"),
                    got_hit.name
                ));
            }
            let want_time = as_f64(&want_hit["time"]);
            if !close(got_hit.time, want_time, rel_tol) {
                fail(format!(
                    "{} hit {e} (`{}`) fired at {} but Java got {want_time} \
                     (rel {:e}, tolerance {rel_tol:e})",
                    at("events"),
                    got_hit.name,
                    got_hit.time,
                    rel_diff(got_hit.time, want_time)
                ));
            }
        }
    }
}

fn replay(
    path: &Path,
    tolerances: &BTreeMap<String, f64>,
    floors: &BTreeMap<String, f64>,
    used: &mut BTreeSet<String>,
    used_floors: &mut BTreeSet<String>,
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

    // A declared stop-criterion floor is a claim that the default cannot be
    // reached, so it is verified before it is used — exactly as a declared
    // numeric tolerance is. A fixture that solves at the default has a dead
    // entry; one the relaxation does not rescue has the wrong entry.
    let settings = match floors.get(&name) {
        None => SolverSettings::default(),
        Some(&rel_tolerance) => {
            if solve(source, &SolverSettings::default()).is_ok() {
                failures.push(Failure {
                    fixture: name.clone(),
                    detail: format!(
                        "{TOLERANCE_FILE} relaxes this fixture's stop criterion to \
                         {rel_tolerance:e}, but it solves at the engine default. Delete the \
                         solver_floor entry rather than leaving a dead relaxation in the file."
                    ),
                });
            } else {
                used_floors.insert(name.clone());
            }
            SolverSettings {
                rel_tolerance,
                ..SolverSettings::default()
            }
        }
    };

    let mut fail = |detail: String| {
        failures.push(Failure {
            fixture: name.clone(),
            detail,
        });
    };

    match solve(source, &settings) {
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

            // The Rust side keyed the way `Result.variables()` is keyed: through
            // `displayNames`, then lowercased. See the module docs.
            let actual_vars: BTreeMap<String, f64> = solution
                .values
                .iter()
                .map(|(name, value)| {
                    let display = solution.display_names.get(name).unwrap_or(name);
                    (display.to_ascii_lowercase(), *value)
                })
                .collect();

            let mut worst = 0.0f64;
            for (var, &expected) in &golden_vars {
                match actual_vars.get(var) {
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
                        "fixtures/{TOLERANCE_FILE} relaxes this fixture to {rel_tol:e}, but it \
                         matches the oracle to {worst:e} — at or under the {REL_TOL:e} default. \
                         Delete the entry rather than leaving a dead tolerance in the file."
                    ));
                } else {
                    used.insert(name.clone());
                }
            }
            for var in actual_vars.keys() {
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

            compare_ode_tables(
                &expect["ode_tables"],
                &solution.ode_tables,
                rel_tol,
                &mut fail,
            );
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
    let floors = declared_solver_floors();
    let mut used = BTreeSet::new();
    let mut used_floors = BTreeSet::new();
    let mut failures = Vec::new();
    for path in &paths {
        replay(
            path,
            &tolerances,
            &floors,
            &mut used,
            &mut used_floors,
            &mut failures,
        );
    }

    // A declaration for a fixture that is not in the corpus is a stale entry, and
    // the "dead entry" guards above cannot see it — nothing replays it.
    for (section, name) in tolerances
        .keys()
        .map(|n| ("fixtures", n))
        .chain(floors.keys().map(|n| ("solver_floor", n)))
    {
        if !paths
            .iter()
            .any(|p| p.file_stem().and_then(|s| s.to_str()) == Some(name.as_str()))
        {
            failures.push(Failure {
                fixture: name.clone(),
                detail: format!(
                    "declared in fixtures/{TOLERANCE_FILE} ({section}) but has no fixture in \
                     fixtures/golden/"
                ),
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
        "parity: {} fixtures match the Java oracle through {} \
         ({} at a declared tolerance from fixtures/{TOLERANCE_FILE}: {}) \
         ({} at a declared stop-criterion floor: {})",
        paths.len(),
        frees_core::props::propfun::backend_description(),
        used.len(),
        used.iter().cloned().collect::<Vec<_>>().join(", "),
        used_floors.len(),
        used_floors.iter().cloned().collect::<Vec<_>>().join(", ")
    );
}
