//! Native tests for the `solve_table` export — the first end-to-end drive of
//! `analysis::parametric::run_sweep` with a real `engine::solve_with` (the
//! module's own tests inject a stubbed `solve_row`; this is the integration
//! the Phase-8 status doc recorded as missing).

use serde_json::Value;

fn call(source: &str, request: &str) -> Value {
    serde_json::from_str(&frees_wasm::solve_table(source, request)).expect("valid JSON out")
}

#[test]
fn a_plain_sweep_solves_every_row_in_display_units() {
    let out = call(
        "y = 2 * x\n",
        r#"{"table": {"variables": ["x", "y"], "rows": [{"x": 1}, {"x": 2}, {"x": 3}]}}"#,
    );
    assert!(out.get("error").is_none(), "{out}");
    let results = out["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    for (i, expect_y) in [(0, 2.0), (1, 4.0), (2, 6.0)] {
        let row = &results[i];
        assert_eq!(row["success"], true, "{row}");
        assert_eq!(row["error"], Value::Null);
        assert_eq!(row["values"]["y"].as_f64().unwrap(), expect_y);
        assert_eq!(row["values"]["x"].as_f64().unwrap(), (i + 1) as f64);
    }
    let stats = &out["stats"];
    assert_eq!(stats["runs"], 3);
    assert_eq!(stats["solved"], 3);
    assert_eq!(stats["failed"], 0);
    // Each row is base + one pin: 2 equations, 2 unknowns.
    assert_eq!(stats["equations"], 2);
    assert_eq!(stats["unknowns"], 2);
    assert!(stats["iterations"].as_u64().unwrap() >= 3, "{stats}");
    // `variables` is the last successful row's DTO list.
    let vars = out["variables"].as_array().unwrap();
    assert!(
        vars.iter()
            .any(|v| v["name"] == "y" && v["value"].as_f64() == Some(6.0)),
        "{vars:?}"
    );
}

#[test]
fn an_accessor_table_iterates_to_the_fixed_point() {
    // `avg` reads the whole `y` column, so pass 1 (no accessors installed)
    // sees the empty-context default and pass 2 must land on mean(2, 4, 6).
    // This is the first test anywhere that drives `RowJob::accessors` into a
    // real solve — it proves `solve_with_parametric` actually installs the
    // channel `eval.rs` reads.
    let out = call(
        "avg = TableAvg('y')\ny = 2 * x\n",
        r#"{"table": {"variables": ["x", "y"], "rows": [{"x": 1}, {"x": 2}, {"x": 3}]}}"#,
    );
    assert!(out.get("error").is_none(), "{out}");
    let results = out["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    for row in results {
        assert_eq!(row["success"], true, "{row}");
        assert_eq!(row["values"]["avg"].as_f64().unwrap(), 4.0, "{row}");
    }
}

#[test]
fn a_failing_row_reports_its_error_and_the_rest_still_solve() {
    // Row 2 pins x = 0 and the document divides by x.
    let out = call(
        "y = 1 / x\n",
        r#"{"table": {"variables": ["x", "y"], "rows": [{"x": 2}, {"x": 0}]}}"#,
    );
    let results = out["results"].as_array().unwrap();
    assert_eq!(results[0]["success"], true, "{results:?}");
    assert_eq!(results[0]["values"]["y"].as_f64().unwrap(), 0.5);
    assert_eq!(results[1]["success"], false, "{results:?}");
    assert!(
        results[1]["error"]
            .as_str()
            .unwrap()
            .contains("division by zero"),
        "{results:?}"
    );
    assert_eq!(out["stats"]["solved"], 1);
    assert_eq!(out["stats"]["failed"], 1);
}

#[test]
fn the_row_cap_refuses_with_the_java_message() {
    let rows: Vec<String> = (0..5001).map(|i| format!("{{\"x\": {i}}}")).collect();
    let request = format!(
        "{{\"table\": {{\"variables\": [\"x\", \"y\"], \"rows\": [{}]}}}}",
        rows.join(",")
    );
    let out = call("y = 2 * x\n", &request);
    assert_eq!(
        out["error"].as_str().unwrap(),
        "The parametric table has too many rows (5001; limit 5000). Reduce the run count.",
        "{out}"
    );
    assert_eq!(out["results"].as_array().unwrap().len(), 0);
    assert_eq!(out["stats"], Value::Null);
}

#[test]
fn a_syntax_error_answers_once_not_per_row() {
    let out = call(
        "y = = x\n",
        r#"{"table": {"variables": ["x"], "rows": [{"x": 1}, {"x": 2}]}}"#,
    );
    let error = out["error"].as_str().unwrap();
    assert!(error.starts_with("Syntax error:"), "{error}");
    assert_eq!(out["results"].as_array().unwrap().len(), 0);
}

#[test]
fn a_blank_document_and_a_missing_table_are_data_not_exceptions() {
    let out = call("", r#"{"table": {"variables": [], "rows": []}}"#);
    assert_eq!(out["error"], "The document is empty.");
    let out = call("y = 2\n", "{}");
    assert_eq!(out["error"], "The request carries no table.");
}
