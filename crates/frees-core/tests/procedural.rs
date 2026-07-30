//! Document-level procedural tests — the `ProceduralFeaturesTest.java` oracle
//! (302 lines) ported over the public API: `parse_document` fills
//! `Document::defs`, `procedures::flatten_calls` rewrites CALL statements, and
//! `procedures::call_function` / `call_proc_output` execute bodies exactly as
//! `ProcedureEvaluator` does.
//!
//! The full solver round trips (FUNCTION calls inside solved equations) belong
//! to the engine wiring; here each oracle case is checked at the layer this
//! phase owns, with the oracle's exact numbers.

use frees_core::ast::{Expr, Statement};
use frees_core::eval::Scope;
use frees_core::parser::parse_document;
use frees_core::procedures::{call_function, call_proc_output, flatten_calls};

fn function_value(source: &str, name: &str, args: &[f64]) -> f64 {
    let doc = parse_document(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let def = doc
        .defs
        .function(name)
        .unwrap_or_else(|| panic!("no FUNCTION {name}"));
    call_function(def, args, &doc.defs, &Scope::new())
        .unwrap_or_else(|e| panic!("call failed: {e}"))
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

// ── FUNCTION tests (oracle §1) ──────────────────────────────────────────────

#[test]
fn function_factorial() {
    let source = "FUNCTION Factorial(n)
  IF n <= 1 THEN
    Factorial := 1
  ELSE
    Factorial := n * Factorial(n-1)
  END
END

y = Factorial(5)
";
    // 5! should be 120.
    assert_close(function_value(source, "factorial", &[5.0]), 120.0);
    // The document half: the equation is untouched by flattening.
    let doc = parse_document(source).unwrap();
    let flat = flatten_calls(doc.statements.clone(), &doc.defs).unwrap();
    assert_eq!(flat, doc.statements);
}

#[test]
fn function_simple_conditional() {
    let source = "FUNCTION AbsVal(x)
  IF x >= 0 THEN
    AbsVal := x
  ELSE
    AbsVal := -x
  END
END
";
    assert_close(function_value(source, "absval", &[-7.0]), 7.0);
    assert_close(function_value(source, "absval", &[3.0]), 3.0);
}

#[test]
fn function_repeat_until() {
    let source = "FUNCTION SumN(n)
  i := 1
  s := 0
  REPEAT
    s := s + i
    i := i + 1
  UNTIL i > n
  SumN := s
END
";
    assert_close(function_value(source, "sumn", &[10.0]), 55.0);
}

#[test]
fn function_while_loop() {
    let source = "FUNCTION SumWhile(n)
  i := 1
  s := 0
  WHILE i <= n DO
    s := s + i
    i := i + 1
  END
  SumWhile := s
END
";
    assert_close(function_value(source, "sumwhile", &[10.0]), 55.0);
}

#[test]
fn function_used_in_expression() {
    // z = Square(4) + Square(3) = 25; each call evaluated independently.
    let source = "FUNCTION Square(x)
  Square := x * x
END
";
    let a = function_value(source, "square", &[4.0]);
    let b = function_value(source, "square", &[3.0]);
    assert_close(a + b, 25.0);
}

// ── PROCEDURE tests (oracle §2) ─────────────────────────────────────────────

#[test]
fn procedure_basic_outputs() {
    let source = "PROCEDURE Swap(a, b : c, d)
  c := b
  d := a
END

CALL Swap(3, 7 : x, y)
";
    let doc = parse_document(source).unwrap();
    let flat = flatten_calls(doc.statements, &doc.defs).unwrap();
    // The CALL flattens to x = proc$swap$0(3, 7), y = proc$swap$1(3, 7).
    assert_eq!(flat.len(), 2);
    let scope = Scope::new();
    let (mut x, mut y) = (f64::NAN, f64::NAN);
    for statement in &flat {
        let Statement::Eq(eq) = statement else {
            panic!("expected an equation, got {statement:?}")
        };
        let Expr::Call { function, args } = &eq.rhs else {
            panic!("expected a proc$ call, got {:?}", eq.rhs)
        };
        let arg_values: Vec<f64> = args
            .iter()
            .map(|a| match a {
                Expr::Num { value, .. } => *value,
                other => panic!("literal inputs expected, got {other:?}"),
            })
            .collect();
        let value = call_proc_output(function, &arg_values, &doc.defs, &scope).unwrap();
        match &eq.lhs {
            Expr::Var(name) if name == "x" => x = value,
            Expr::Var(name) if name == "y" => y = value,
            other => panic!("{other:?}"),
        }
    }
    assert_close(x, 7.0);
    assert_close(y, 3.0);
}

#[test]
fn procedure_with_conditional() {
    let source = "PROCEDURE MinMax(a, b : lo, hi)
  IF a < b THEN
    lo := a
    hi := b
  ELSE
    lo := b
    hi := a
  END
END
";
    let doc = parse_document(source).unwrap();
    let scope = Scope::new();
    let small = call_proc_output("proc$minmax$0", &[8.0, 3.0], &doc.defs, &scope).unwrap();
    let large = call_proc_output("proc$minmax$1", &[8.0, 3.0], &doc.defs, &scope).unwrap();
    assert_close(small, 3.0);
    assert_close(large, 8.0);
}

// ── array-language-style multi-output FUNCTION tests (oracle §3) ────────────

#[test]
fn multi_output_function_basic() {
    // FUNCTION [outs] = name(...) is a procedure consumed with [a, b] = name(...).
    let source = "FUNCTION [q, r] = DivMod(a, b)
  q := trunc(a / b)
  r := a - q * b
END

[whole, rem] = DivMod(17, 5)
";
    let doc = parse_document(source).unwrap();
    assert!(
        doc.defs.function("divmod").is_none(),
        "lowered to PROCEDURE"
    );
    let flat = flatten_calls(doc.statements, &doc.defs).unwrap();
    assert_eq!(flat.len(), 2);
    let scope = Scope::new();
    let whole = call_proc_output("proc$divmod$0", &[17.0, 5.0], &doc.defs, &scope).unwrap();
    let rem = call_proc_output("proc$divmod$1", &[17.0, 5.0], &doc.defs, &scope).unwrap();
    assert_close(whole, 3.0);
    assert_close(rem, 2.0);
}

#[test]
fn multi_output_function_with_conditional() {
    let source = "FUNCTION [lo, hi] = Order(a, b)
  IF a < b THEN
    lo := a
    hi := b
  ELSE
    lo := b
    hi := a
  END
END
";
    let doc = parse_document(source).unwrap();
    let scope = Scope::new();
    let small = call_proc_output("proc$order$0", &[8.0, 3.0], &doc.defs, &scope).unwrap();
    let large = call_proc_output("proc$order$1", &[8.0, 3.0], &doc.defs, &scope).unwrap();
    assert_close(small, 3.0);
    assert_close(large, 8.0);
}

// ── MODULE tests (oracle §4) ────────────────────────────────────────────────

#[test]
fn module_basic_grafting() {
    // Calling a MODULE twice creates two namespaced copies of its equations.
    let source = "MODULE Doubler(x : y)
  y = 2 * x
END

CALL Doubler(5 : a)
CALL Doubler(10 : b)
";
    let doc = parse_document(source).unwrap();
    let flat = flatten_calls(doc.statements, &doc.defs).unwrap();
    // Per call: input binding + one body equation + output binding.
    assert_eq!(flat.len(), 6);
    // Solve the grafted equations by simple forward substitution — the chain
    // ns$x = in, ns$y = 2 * ns$x, out = ns$y is sequential.
    // (`frees_core::eval::eval` is the crate's numeric AST interpreter —
    // f64 arithmetic over a scope map, no code execution.)
    let mut values = Scope::new();
    for statement in &flat {
        let Statement::Eq(eq) = statement else {
            panic!("{statement:?}")
        };
        let value = frees_core::eval::eval(&eq.rhs, &values).unwrap();
        let Expr::Var(name) = &eq.lhs else {
            panic!("{:?}", eq.lhs)
        };
        values.insert(name.clone(), value);
    }
    assert_close(values["a"], 10.0);
    assert_close(values["b"], 20.0);
}

#[test]
fn module_solves_internal_equations() {
    // y = m * x_int + b with x_int = 3 → CALL Linear(2, 1 : result) = 7.
    let source = "MODULE Linear(m, b : y)
  y = m * x_int + b
  x_int = 3
END

CALL Linear(2, 1 : result)
";
    let doc = parse_document(source).unwrap();
    let flat = flatten_calls(doc.statements, &doc.defs).unwrap();
    let texts: Vec<&str> = flat
        .iter()
        .map(|s| match s {
            Statement::Eq(eq) => eq.source_text.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        texts,
        [
            "MODULE linear input m",
            "MODULE linear input b",
            "y = m * x_int + b",
            "x_int = 3",
            "MODULE linear output y",
        ]
    );
    // Order-independent check: evaluate with the namespaced fixture values.
    let mut values = Scope::new();
    values.insert("linear$1$m".into(), 2.0);
    values.insert("linear$1$b".into(), 1.0);
    values.insert("linear$1$x_int".into(), 3.0);
    let Statement::Eq(body_eq) = &flat[2] else {
        panic!()
    };
    assert_close(frees_core::eval::eval(&body_eq.rhs, &values).unwrap(), 7.0);
    let Statement::Eq(out_eq) = &flat[4] else {
        panic!()
    };
    assert_eq!(out_eq.lhs, Expr::var("result"));
    assert_eq!(out_eq.rhs, Expr::Var("linear$1$y".into()));
}

#[test]
fn module_namespaces_function_calls_and_negation() {
    // buf = -x + sin(0); y = buf + 1 → CALL Shift(4 : out) = -3.
    let source = "MODULE Shift(x : y)
  buf = -x + sin(0)
  y = buf + 1
END

CALL Shift(4 : out)
";
    let doc = parse_document(source).unwrap();
    let flat = flatten_calls(doc.statements, &doc.defs).unwrap();
    let mut values = Scope::new();
    for statement in &flat {
        let Statement::Eq(eq) = statement else {
            panic!("{statement:?}")
        };
        let value = frees_core::eval::eval(&eq.rhs, &values).unwrap();
        let Expr::Var(name) = &eq.lhs else {
            panic!("{:?}", eq.lhs)
        };
        values.insert(name.clone(), value);
    }
    assert_close(values["out"], -3.0);
}

// ── Milestone 3 verification (oracle §5) ────────────────────────────────────

#[test]
fn milestone3_factorial_for_all_loop_indices() {
    // The full oracle drives `FOR i = 1 TO 5 … f[i] = Factorial(i)` through the
    // solver; the array unroll belongs to the expansion pass. The procedural
    // half — Factorial at each index — must match the oracle values exactly.
    let source = "FUNCTION Factorial(n)
  IF n <= 1 THEN
    Factorial := 1
  ELSE
    Factorial := n * Factorial(n-1)
  END
END
";
    for (n, expected) in [
        (1.0, 1.0),
        (2.0, 2.0),
        (3.0, 6.0),
        (4.0, 24.0),
        (5.0, 120.0),
    ] {
        assert_close(function_value(source, "factorial", &[n]), expected);
    }
}

#[test]
fn nested_for_inside_function_accumulates_correctly() {
    // DoubleSum(3) = sum_{i,j=1..3} i*j = 36 (the inner loop must execute).
    let source = "FUNCTION DoubleSum(n)
  s := 0
  FOR i = 1 TO n
    FOR j = 1 TO n
      s = s + i * j
    END
  END
  DoubleSum := s
END
";
    assert_close(function_value(source, "doublesum", &[3.0]), 36.0);
}

#[test]
fn call_inside_procedure_for_is_rejected_not_silently_dropped() {
    // A CALL inside a FOR within a FUNCTION has no procedural meaning; it must
    // raise a clear error rather than being silently skipped.
    let source = "FUNCTION Bad(n)
  FOR i = 1 TO n
    CALL pole(i, i : a, b)
  END
  Bad := 1
END

y = Bad(2)
";
    let err = parse_document(source).expect_err("must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("CALL"),
        "expected a clear 'CALL not supported' error, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// Ignored-output sinks (`~ignored~N`)
//
// Omitting a trailing CALL output mints a hidden sink variable. The solver must
// still determine it — it backs a real equation — but the Java engine never
// surfaces it: `EquationSystemSolver` drops it from the result map
// (`if (isIgnoredSink(name)) return;`), from `check`'s variable list, and from
// both reported counts (`surfacedVarCount` / `surfacedEqs`). Verified against
// the oracle with `fixtures/corpus/call_linfit_omitted_r2.frees`.
// ---------------------------------------------------------------------------

#[test]
fn omitted_trailing_call_output_never_surfaces_in_the_solution() {
    // LinFit's third output (r2) is omitted, so its slot becomes a sink.
    let source = "x = [1, 2, 3]
y = [2.1, 3.9, 6.2]
CALL LinFit(x, y : m, b)
";
    let solution = frees_core::solve(source, &frees_core::SolverSettings::default())
        .expect("LinFit with an omitted trailing output solves");

    for name in solution.values.keys() {
        assert!(
            !frees_core::parser::toplevel::is_ignored_sink(name),
            "sink `{name}` leaked into the result map: {:?}",
            solution.values
        );
    }
    for name in solution.display_names.keys() {
        assert!(
            !frees_core::parser::toplevel::is_ignored_sink(name),
            "sink `{name}` leaked into display_names"
        );
    }
    // The visible outputs are still there and still correct (Java: m = 2.05,
    // b = -0.033333333333333215).
    assert!((solution.values["m"] - 2.05).abs() < 1e-12);
    assert!((solution.values["b"] + 0.033_333_333_333_333_215).abs() < 1e-12);
}

#[test]
fn check_reports_the_surfaced_equation_variable_balance() {
    // 6 element equations + 3 from the LinFit flattening = 9 equations in 9
    // unknowns, one of which is the r2 sink. Java hides the sink *and* the
    // equation that determines it, so `check` reports 8 and 8 — never the
    // unbalanced-looking 9 and 8.
    let source = "x = [1, 2, 3]
y = [2.1, 3.9, 6.2]
CALL LinFit(x, y : m, b)
";
    let report = frees_core::check(source).expect("check succeeds");
    assert!(report.solvable, "{}", report.message);
    assert_eq!(report.equation_count, report.unknown_count);
    assert_eq!(report.unknown_count, 8);
    assert!(
        report
            .variables
            .iter()
            .all(|v| !frees_core::parser::toplevel::is_ignored_sink(v)),
        "check listed a sink: {:?}",
        report.variables
    );
    assert!(
        report.message.contains("8 equations and 8 variables"),
        "unexpected check message: {}",
        report.message
    );
}
