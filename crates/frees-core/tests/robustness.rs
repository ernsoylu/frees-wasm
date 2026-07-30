//! Adversarial robustness: every failure mode must be a `Result`, never a panic.
//!
//! These tests are the regression net for a hostile-input audit of the whole
//! crate. The rule they encode is one line long: **`parse_document`, `check`
//! and `solve` may return `Ok` or `Err` for any byte string whatsoever, and
//! must do nothing else.** Not panic, not abort, not hang, and not quietly
//! answer a question that was never asked.
//!
//! Three of them are regressions for defects this audit found and fixed:
//!
//! * [`a_range_whose_element_count_overflows_is_refused`] — `x = 0:1e-320:1`
//!   made the element count overflow `i64`, which panics in a debug build and
//!   *wraps to a negative count* in a release one, sailing past the ceiling
//!   check. (`parser/toplevel.rs`)
//! * [`nesting_past_the_ceiling_is_a_parse_error_not_a_stack_overflow`] and its
//!   neighbours — a deeply nested or very long expression overflowed the stack
//!   while parsing, and, once parsed, overflowed it again in every recursive
//!   consumer (`eval`, `Expr::variables`, and `Drop` itself). A stack overflow
//!   aborts the process; no caller can catch it, so the only defence is to
//!   refuse to build the tree. (`parser/expr.rs`, `parser/toplevel.rs`,
//!   `units/registry.rs`)
//! * [`a_nan_percentile_is_refused_rather_than_indexing_off_the_front`] —
//!   `NaN` propagates through `Math.min`/`Math.max`, defeated every range check
//!   in `percentile`, and reached `sorted[index - 1]` with `index == 0`.
//!   (`eval.rs`)
//!
//! The rest are the audit's standing corpus: unterminated and truncated
//! constructs, non-ASCII input, enormous and subnormal numbers, degenerate
//! systems, and division by zero.

use std::collections::HashMap;

use frees_core::diag::Severity;
use frees_core::parser::expr::MAX_EXPR_DEPTH;
use frees_core::units::registry::UnitRegistry;
use frees_core::{check, parse_document, solve, FreesError, SolverSettings};

// ── helpers ─────────────────────────────────────────────────────────────────

fn settings() -> SolverSettings {
    SolverSettings::default()
}

/// Run the three public entry points and report only *how* each answered.
///
/// A panic escaping any of them fails the calling test with the offending
/// source in the message, which is what makes the corpus sweep useful rather
/// than merely red.
fn survives(src: &str) -> Result<(), String> {
    fn answered(
        name: &str,
        src: &str,
        run: impl Fn(&str) + std::panic::RefUnwindSafe,
    ) -> Result<(), String> {
        std::panic::catch_unwind(|| run(src)).map_err(|_| format!("{name} panicked on {src:?}"))
    }
    answered("parse_document", src, |s| {
        let _ = parse_document(s);
    })?;
    answered("check", src, |s| {
        let _ = check(s);
    })?;
    answered("solve", src, |s| {
        let _ = solve(s, &settings());
    })
}

/// Assert that every document in `corpus` is answered rather than survived.
fn none_panic(corpus: &[&str]) {
    let failures: Vec<String> = corpus
        .iter()
        .filter_map(|src| survives(src).err())
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

fn parse_err(src: &str) -> String {
    match parse_document(src) {
        Ok(doc) => panic!("expected {src:?} to be refused, got {doc:?}"),
        Err(err) => {
            assert!(
                matches!(err, FreesError::Parse { .. }),
                "{src:?} should be a parse error, got {err:?}"
            );
            err.to_string_message()
        }
    }
}

fn solve_err(src: &str) -> String {
    solve(src, &settings())
        .map(|s| format!("unexpectedly solved to {:?}", s.values))
        .unwrap_err()
        .to_string_message()
}

fn solved(src: &str) -> std::collections::BTreeMap<String, f64> {
    solve(src, &settings())
        .unwrap_or_else(|err| panic!("{src:?} should solve, got {err}"))
        .values
}

fn nested(open: &str, close: &str, depth: usize) -> String {
    format!("x = {}1{}", open.repeat(depth), close.repeat(depth))
}

/// The deepest nesting the parser accepts, established by bisection rather than
/// by arithmetic on the budget constants — the point is to pin the *observable*
/// ceiling, so a change to how levels are charged shows up here.
fn deepest_accepted(shape: impl Fn(usize) -> String) -> usize {
    let (mut lo, mut hi) = (1usize, 4096usize);
    assert!(parse_document(&shape(lo)).is_ok(), "depth 1 must parse");
    assert!(parse_document(&shape(hi)).is_err(), "depth 4096 must not");
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if parse_document(&shape(mid)).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

// ── ranges: the i64 overflow ────────────────────────────────────────────────

#[test]
fn a_range_whose_element_count_overflows_is_refused() {
    // `(stop - start) / step` is `inf` here, and `(long) inf + 1` wraps to
    // `Long.MIN_VALUE` in Java — past the ceiling check with a negative count.
    // Rust panicked on the same overflow. Both are wrong; a refusal is right.
    for src in [
        "x = 0:1e-320:1",     // step underflows relative to the span
        "x = 0:1e-300:1e300", // and the span overflows too
        "x = 0:1:1e300",      // enormous span, unit step
        "x = 1e300:-1:0",     // the same, descending
    ] {
        let message = parse_err(src);
        assert!(
            message.contains("more than 100000 elements"),
            "{src:?}: {message}"
        );
    }
}

#[test]
fn the_element_count_ceiling_still_lands_on_the_documented_boundary() {
    // The overflow screen must not move the ordinary ceiling: 100 000 elements
    // is the last accepted range and 100 001 the first refused one.
    assert!(parse_document("x = 0:1:99999").is_ok());
    let message = parse_err("x = 0:1:100000");
    assert!(
        message.contains("would generate 100001 elements"),
        "{message}"
    );
}

#[test]
fn ordinary_ranges_are_untouched_by_the_overflow_screen() {
    for src in [
        "x = 0:1:10",
        "x = 1:5",
        "x = 0:0.5:1",
        "x = 10:-1:0",
        "x = 1:10:100 | Log",
    ] {
        assert!(parse_document(src).is_ok(), "{src:?} should still parse");
    }
}

#[test]
fn a_degenerate_range_keeps_its_own_diagnosis() {
    assert!(parse_err("x = 0:0:1").contains("step is zero"));
    assert!(parse_err("x = 0:-1:10").contains("wrong way"));
    assert!(parse_err("x = 1:2:3 | Weird").contains("Unknown range spacing"));
}

// ── nesting depth: the stack overflow ───────────────────────────────────────

#[test]
fn nesting_past_the_ceiling_is_a_parse_error_not_a_stack_overflow() {
    // Every one of these aborted the process before the depth guard existed.
    // 100 000 levels is far past any stack; the point is that the answer is a
    // `Result`, and arrives promptly.
    for (label, src) in [
        ("parentheses", nested("(", ")", 100_000)),
        ("calls", nested("sin(", ")", 100_000)),
        ("matrix literals", nested("[", "]", 100_000)),
        ("unary minus", format!("x = {}1", "-".repeat(100_000))),
        ("exponentiation", format!("x = {}1", "2 ^ ".repeat(100_000))),
        ("addition", format!("x = {}1", "1 + ".repeat(100_000))),
        (
            "unclosed parentheses",
            format!("x = {}1", "(".repeat(100_000)),
        ),
    ] {
        let message = parse_err(&src);
        assert!(message.contains("too deeply nested"), "{label}: {message}");
        // and the same refusal reaches the two callers that matter: solve as
        // an Err, check as the not-solvable syntax-failure report it returns
        // for every parse problem (the Java 400-with-body shape).
        let report = check(&src).unwrap_or_else(|e| panic!("{label}: check errored: {e}"));
        assert!(!report.solvable, "{label}: check");
        assert!(
            report.message.contains("too deeply nested"),
            "{label}: {}",
            report.message
        );
        assert!(solve(&src, &settings()).is_err(), "{label}: solve");
    }
}

#[test]
fn nested_for_blocks_are_bounded_too() {
    // `for_block` → `statement` → `for_block` is its own recursion, separate
    // from the expression grammar's.
    let src = format!(
        "{}a = 1\n{}",
        "FOR i = 1 TO 2\n".repeat(10_000),
        "END\n".repeat(10_000)
    );
    let message = parse_err(&src);
    assert!(message.contains("nested more than"), "{message}");

    // A realistic amount of nesting still works. (The body must use every
    // loop index: FOR unrolls one equation per iteration, so a constant body
    // would be the same equation stated eight times — correctly refused.)
    let ok = "FOR i = 1 TO 2\nFOR j = 1 TO 2\nFOR k = 1 TO 2\na[i, j, k] = i * j * k\n\
              END\nEND\nEND\nb = a[1, 1, 1] + a[2, 2, 2]";
    assert_eq!(solved(ok).get("b"), Some(&9.0));
}

#[test]
fn every_nesting_shape_shares_one_ceiling() {
    // The budget is charged per *level of tree*, so which construct did the
    // nesting must not change how much of it is allowed. A shape that slipped
    // through unguarded would show up here as an outlier.
    type Shape = (&'static str, fn(usize) -> String);
    let shapes: [Shape; 5] = [
        ("parentheses", |d| nested("(", ")", d)),
        ("calls", |d| nested("sin(", ")", d)),
        ("matrix literals", |d| nested("[", "]", d)),
        ("unary minus", |d| format!("x = {}1", "-".repeat(d))),
        ("exponentiation", |d| format!("x = {}1", "2 ^ ".repeat(d))),
    ];
    let ceilings: Vec<(&str, usize)> = shapes
        .iter()
        .map(|(name, shape)| (*name, deepest_accepted(shape)))
        .collect();
    let first = ceilings[0].1;
    assert!(
        ceilings.iter().all(|(_, d)| *d == first),
        "shapes disagree on the nesting ceiling: {ceilings:?}"
    );
    // Deep enough for any hand-written equation, shallow enough to be safe on
    // the 1 MiB stack the wasm build ships with.
    assert!(
        (16..=128).contains(&first),
        "nesting ceiling {first} is outside the intended band"
    );
}

#[test]
fn a_long_flat_chain_is_allowed_where_deep_nesting_is_not() {
    // `a + b + c + …` is parsed by a loop but builds a tree as deep as it is
    // long, so it is charged too — just far more cheaply than nesting, because
    // it costs no parser recursion. A 200-term sum is a legitimate document.
    let chain = format!("x = {}1", "1 + ".repeat(200));
    assert_eq!(solved(&chain).get("x"), Some(&201.0));

    // Past the budget it is refused rather than overflowing a consumer's stack.
    let absurd = format!("x = {}1", "1 + ".repeat(MAX_EXPR_DEPTH as usize + 10));
    assert!(parse_err(&absurd).contains("too deeply nested"));
}

#[test]
fn breadth_is_not_charged_as_depth() {
    // Guards are released as the parser returns, so a wide argument list or
    // matrix row must not accumulate budget. 20 000 arguments is far more than
    // the depth ceiling and must still parse.
    let wide = format!("x = max({})", vec!["1"; 20_000].join(", "));
    assert!(parse_document(&wide).is_ok(), "wide call was refused");
    let row = format!("x = [{}]", vec!["1"; 20_000].join(", "));
    assert!(parse_document(&row).is_ok(), "wide matrix row was refused");
}

#[test]
fn the_depth_budget_is_released_between_statements_and_after_a_failure() {
    // The counter lives on the thread, so a leak would make the *second*
    // document fail — the nastiest possible symptom, since it depends on what
    // was parsed before.
    let one = format!("{}1", "1 + ".repeat(200));
    let many: String = (0..500).map(|i| format!("v{i} = {one}\n")).collect();
    assert!(
        parse_document(&many).is_ok(),
        "500 statements each near the limit should parse"
    );

    let rejected = format!("x = {}1", "1 + ".repeat(100_000));
    assert!(parse_document(&rejected).is_err());
    assert!(
        parse_document("a = 1 + 2").is_ok(),
        "a rejected document must not poison the next one"
    );

    // A failed *speculative* parse (the bool-paren lookahead) also has to
    // hand its levels back.
    for _ in 0..100 {
        assert!(parse_document("x = (a + b) * 2").is_ok());
    }
    assert!(parse_document(&many).is_ok());
}

// ── unit expressions ────────────────────────────────────────────────────────

#[test]
fn a_deeply_nested_unit_expression_is_refused_not_fatal() {
    // `parse_factor` recurses into `parse_expr` for `( … )`, so the annotation
    // has a stack hazard of its own — reached from ordinary source text.
    let deep = format!("{}m{}", "(".repeat(100_000), ")".repeat(100_000));
    assert!(
        UnitRegistry::parse(&deep).is_err(),
        "a 100k-deep unit must be refused"
    );

    // Reached through a document, an unparseable unit is a *warning*: the
    // engine's invariant is that a unit problem never blocks a solve.
    let src = format!("P = 1 [{deep}]");
    let solution = solve(&src, &settings()).expect("unit trouble must not fail the solve");
    assert_eq!(solution.values.get("p"), Some(&1.0));
    assert!(solution
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains("unknown unit")));
}

#[test]
fn realistic_unit_nesting_still_parses() {
    for unit in ["kJ/(kg-K)", "W/m^2-K", "m^3/s", "lbm/ft^3", "N-m", "kPa"] {
        assert!(
            UnitRegistry::parse(unit).is_ok(),
            "{unit} should still resolve"
        );
    }
}

#[test]
fn malformed_and_unknown_units_warn_but_never_panic() {
    let corpus = [
        "P = 140 [zorp]",
        "P = 140 []",
        "P = 140 [[kPa]]",
        "P = 1 [(m]",
        "P = 1 [m)]",
        "P = 1 [/]",
        "P = 1 [---]",
        "P = 1 [2]",
        "P = 1 [m^999999999999]",
        "P = 1 [m/s/s/s/s/s]",
        "P = 1 [µm]",
    ];
    none_panic(&corpus);
    // and the archetype resolves to the unconverted value plus a warning
    let solution = solve("P = 140 [zorp]", &settings()).unwrap();
    assert_eq!(solution.values.get("p"), Some(&140.0));
    assert!(solution
        .diagnostics
        .iter()
        .any(|d| d.message.contains("zorp")));
}

// ── percentile: the NaN index underflow ─────────────────────────────────────

#[test]
fn a_nan_percentile_is_refused_rather_than_indexing_off_the_front() {
    // `Math.min`/`Math.max` propagate NaN, so the clamp left it alone and every
    // subsequent comparison answered `false`, landing on `sorted[0 - 1]`.
    // NaN is reachable from plain arithmetic, so this was a live path.
    let message = solve_err("x = percentile(1e999 - 1e999, 1, 2, 3)");
    assert!(message.contains("got NaN"), "{message}");
    assert!(message.contains("percentile"), "{message}");
}

#[test]
fn percentile_still_computes_the_apache_legacy_estimate() {
    assert_eq!(
        solved("x = percentile(50, 1, 2, 3, 4, 5)").get("x"),
        Some(&3.0)
    );
    assert_eq!(solved("x = percentile(0, 5, 1, 3)").get("x"), Some(&1.0));
    assert_eq!(solved("x = percentile(100, 5, 1, 3)").get("x"), Some(&5.0));
    assert_eq!(solved("x = percentile(50, 7)").get("x"), Some(&7.0));
}

#[test]
fn statistics_intrinsics_absorb_infinities_without_panicking() {
    none_panic(&[
        "x = percentile(1e999, 1, 2, 3)",
        "x = percentile(-1e999, 1, 2, 3)",
        "x = percentile(1e999 - 1e999, 1)",
        "x = median(1e999 - 1e999, 1, 2)",
        "x = variance(1)",
        "x = stdev(1)",
        "x = rms(1e999)",
        "x = mean(1e999, -1e999)",
        "x = average()",
    ]);
}

// ── empty, trivial and comment-only documents ───────────────────────────────

#[test]
fn documents_with_nothing_to_solve_are_reported_not_crashed() {
    for src in [
        "",
        "   \t  ",
        "\n\n\n",
        ";;;;",
        "{ only a comment }",
        "// only a comment",
        "\" only a comment \"",
        "\u{feff}",
    ] {
        let report = check(src).unwrap_or_else(|e| panic!("check({src:?}) failed: {e}"));
        assert!(!report.solvable, "{src:?}");
        assert_eq!(report.equation_count, 0, "{src:?}");
        assert_eq!(
            solve(src, &settings()).unwrap_err(),
            FreesError::solver("No equations to solve.")
        );
    }
}

#[test]
fn a_byte_order_mark_does_not_hide_the_first_equation() {
    assert_eq!(solved("\u{feff}x = 1").get("x"), Some(&1.0));
}

// ── unterminated and truncated constructs ───────────────────────────────────

#[test]
fn an_unterminated_comment_names_the_delimiter_that_opened_it() {
    for (src, delimiter) in [("{ never closed", '}'), ("\" never closed", '"')] {
        let message = parse_err(src);
        assert!(message.contains("unterminated comment"), "{message}");
        assert!(message.contains(delimiter), "{message}");
    }
    // A bare opener at end of input is the same story, not an index panic.
    assert!(parse_err("{").contains("unterminated"));
    assert!(parse_err("\"").contains("unterminated"));
}

#[test]
fn truncated_documents_all_answer_with_a_parse_error() {
    // Every one of these is a prefix of something valid; the parser must reach
    // end-of-input and say so rather than reading past it.
    let corpus = [
        "x =",
        "x =\n",
        "= 1",
        "x = (1",
        "x = ((((1",
        "x = [1",
        "x = sin(",
        "x = max(1,",
        "FOR i = 1 TO 3",
        "FOR i = 1 TO 3\n  a = 1",
        "GUESS",
        "GUESS x =",
        "GUESS x [1,",
        "CALL f(",
        "CALL f(1 :",
        "SYMBOLIC",
        "x = 1:",
        "x = 1:2:",
        "x = 1:2 |",
        "x = 1 +",
        "x = = 2",
        "s$ = 'abc",
    ];
    none_panic(&corpus);
    for src in corpus {
        assert!(
            matches!(parse_document(src), Err(FreesError::Parse { .. })),
            "{src:?} should be a parse error"
        );
    }
}

#[test]
fn a_stray_operator_or_delimiter_is_a_parse_error_with_a_span() {
    for src in [
        "END", "THEN", "ELSE", "UNTIL", "DO", "TO", ",", ")", "]", ":", "|", "->", "~", "..", ":=",
        "\\",
    ] {
        match parse_document(src) {
            Err(FreesError::Parse { span, .. }) => {
                assert!(span.is_some(), "{src:?} should carry a span");
            }
            other => panic!("{src:?} should be a parse error, got {other:?}"),
        }
    }
}

// ── unicode and other non-ASCII bytes ───────────────────────────────────────

#[test]
fn non_ascii_input_is_rejected_at_a_character_boundary() {
    // The lexer walks bytes; quoting the offending character in the diagnostic
    // is the one place it decodes UTF-8, and it must not split a code point.
    for src in ["λ = 1", "x = 1 ± 2", "x = 🎉", "变量 = 1", "x\u{a0}= 1"] {
        let message = parse_err(src);
        assert!(
            message.contains("unexpected character"),
            "{src:?}: {message}"
        );
    }
}

#[test]
fn non_ascii_inside_strings_and_comments_is_content() {
    none_panic(&[
        "s$ = 'héllo wörld'",
        "{ héllo }\nx = 1",
        "// héllo ✓\nx = 1",
        "\" é \"\nx = 1",
        "{ \u{e9}\u{e9}\u{e9}",
        "s$ = 'é",
        "x = 1 \u{202e} 2",
        "x\u{0301} = 1",
        "x = 1\u{0}",
        "x = \u{feff}1",
    ]);
    assert_eq!(solved("{ héllo }\nx = 1").get("x"), Some(&1.0));
    assert_eq!(solved("// héllo ✓\nx = 1").get("x"), Some(&1.0));
}

// ── numbers: enormous, subnormal, non-finite ────────────────────────────────

#[test]
fn overflowing_and_subnormal_literals_are_answered_not_crashed() {
    none_panic(&[
        "x = 1e999",
        "x = -1e999",
        "x = 1e-999",
        "x = 5e-324",
        "x = 1e999 - 1e999",
        "x = 1e999 * 0",
        "x = 1e999 / 1e999",
        &format!("x = {}", "9".repeat(5_000)),
        &format!("x = 0.{}", "1".repeat(5_000)),
        &format!("x = 1e{}", "9".repeat(400)),
    ]);
    // A subnormal survives *lexing* as itself — it is only the solver's
    // absolute residual tolerance (1e-10) that cannot resolve it afterwards,
    // which is arithmetic, not a defect.
    let doc = parse_document("x = 5e-324").unwrap();
    assert_eq!(
        doc.equations()[0].rhs,
        frees_core::Expr::num(5e-324),
        "the smallest subnormal must not be flattened at parse time"
    );
    assert!(solved("x = 5e-324").get("x").unwrap().abs() < 1e-300);
    // A literal that overflows to infinity cannot be solved for, and says so
    // rather than reporting a confident `inf`.
    let message = solve_err("x = 1e999");
    assert!(message.contains("not finite"), "{message}");
}

#[test]
fn division_by_zero_is_an_error_that_quotes_the_equation() {
    for src in ["x = 1 / 0", "x = 0 / 0", "x = mod(1, 0)"] {
        let message = solve_err(src);
        assert!(message.contains("division by zero"), "{src:?}: {message}");
        assert!(message.contains(src), "{src:?} should be quoted: {message}");
    }
}

#[test]
fn domain_errors_name_the_function_and_the_argument() {
    for (src, needle) in [
        ("x = sqrt(-1)", "square root of a negative"),
        ("x = ln(0)", "logarithm of zero"),
        ("x = arcsin(2)", "[-1, 1]"),
        ("x = gamma(-3)", "pole"),
        ("x = nosuchfn(2)", "nosuchfn"),
        ("x = sin(1, 2, 3)", "1 argument"),
    ] {
        let message = solve_err(src);
        assert!(message.contains(needle), "{src:?}: {message}");
    }
}

// ── degenerate and hostile equation systems ─────────────────────────────────

#[test]
fn duplicate_and_conflicting_equations_are_reported_as_overspecified() {
    for src in [
        "x = 1\nx = 1",
        "x = 1\nx = 2",
        "a = 1\na = 2\na = 3",
        "0 = 0",
        "1 = 2",
    ] {
        let message = solve_err(src);
        assert!(message.contains("overspecified"), "{src:?}: {message}");
        // and `check` answers with data rather than an error
        let report = check(src).unwrap();
        assert!(!report.solvable, "{src:?}");
    }
}

#[test]
fn a_self_referential_equation_does_not_hang_or_crash() {
    // `x = x` is satisfied by anything; `x = x + 1` by nothing. Both must
    // terminate with an answer of some kind.
    none_panic(&[
        "x = x",
        "x = x + 1",
        "x = x * 2",
        "x = sin(x)",
        "a = b\nb = a",
    ]);
    assert!(solve("x = x", &settings()).is_ok());
    assert!(solve("x = x + 1", &settings()).is_err());
}

#[test]
fn under_and_over_determined_systems_name_the_quantity_at_fault() {
    assert!(solve_err("m + n = 5").contains("underspecified"));
    assert!(solve_err("z = 1\nz = 2").contains("overspecified"));
}

#[test]
fn a_block_that_cannot_converge_fails_with_its_equation_quoted() {
    let message = solve_err("exp(x) = -1");
    assert!(message.starts_with("Block 1"), "{message}");
    assert!(message.contains("exp(x) = -1"), "{message}");
}

// ── sizes: long names, many equations, wide expressions ─────────────────────

#[test]
fn very_long_identifiers_and_very_large_documents_are_handled() {
    let long = "a".repeat(100_000);
    assert_eq!(solved(&format!("{long} = 1")).get(&long), Some(&1.0));

    let many: String = (0..2_000).map(|i| format!("v{i} = {i}\n")).collect();
    let solution = solved(&many);
    assert_eq!(solution.len(), 2_000);
    assert_eq!(solution.get("v1999"), Some(&1999.0));

    // A dependency chain forces the blocker to order 1 000 blocks.
    let chain: String = std::iter::once("v0 = 1\n".to_string())
        .chain((1..1_000).map(|i| format!("v{i} = v{} + 1\n", i - 1)))
        .collect();
    assert_eq!(solved(&chain).get("v999"), Some(&1000.0));
}

// ── the standing corpus ─────────────────────────────────────────────────────

#[test]
fn no_document_in_the_hostile_corpus_panics() {
    // Everything the audit tried that is not already pinned by a test above.
    none_panic(&[
        // grammar shapes
        "A' + B'",
        "x = a''''''",
        "x = [[1,2],[3,4]]",
        "x = [1:3]",
        "x = 1(2)",
        "x = 1[2]",
        "x := 1",
        "x = 1;;;y = 2",
        "x = [1;2;3]",
        "x = [ ( [ 1 ] ) ]",
        "x = []",
        "[] = 1",
        "[~] = f(x)",
        "[a, b] = f(x)",
        "x = a[]",
        "x = a[0]",
        "x = a[-1]",
        "x = a[1e300]",
        "x = 1 < 2 < 3",
        "x = 'water' + 1",
        "s$ = 'water'",
        "x = 1\r\ny = 2\r\n",
        "x = 1\ry = 2",
        "x\t=\t1",
        // unported constructs, refused by name
        "COMPONENT pump\nEND",
        "Pump p1(x)",
        "connect a, b",
        "STATE TABLE t(x)",
        "PARAMETRIC p",
        "CALL f(1 : y)\nx = 1",
        "SYMBOLIC s\nx = 1",
        // guess directives
        "GUESS x = 1",
        "GUESS x = 1\nGUESS x = 2\nx = 1",
        "GUESS zzz = 3\nx = 1",
        "GUESS x = 1 [1e999, 1e999]\nx = 1",
        "GUESS x [0, 10]\nx = 5",
        // reductions and intrinsic edges
        "x = sum(i, 1, 1e18, i)",
        "x = sum(i, 1, 1e999, i)",
        "x = sum(i, -1e999, 1, i)",
        "x = sum(i, 10, 1, i)",
        "x = sum(1, 1, 3, 1)",
        "x = sum(i, 1, 3)",
        "x = product(i, 1, 1e18, i)",
        "x = sum(i, 1, 3, sum(j, 1, 3, i*j))",
        "x = gaussintegral(t, t, 0, 1)",
        "x = if(1)",
        "x = legendrep(1e18, 0.5)",
        "x = legendrep(-1e18, 0.5)",
        "x = chebyshevt(1e9, 0.5)",
        "x = gcd(1e300, 1e300)",
        "x = bitshiftl(1, 1e300)",
        "x = bitshiftl(1, -1e300)",
        "x = round(1e300, 1e300)",
        "x = normalcdf(1, 0, 0)",
        "x = factorial(1e300)",
        "x = concat$('a')",
        "x = stringval('nope')",
        // loops
        "FOR i = 1 TO 0\n a = 1\nEND\nb = a",
        "FOR i = 1 TO 1e300\n a = 1\nEND",
        "FOR i = 1 TO 2\n  a = 5\nEND\nb = a + 1",
    ]);
}

// ── direct evaluator surface ────────────────────────────────────────────────

#[test]
fn evaluating_an_expression_the_parser_produced_never_panics() {
    // Anything `parse_document` accepts is shallow enough for the recursive
    // evaluator, whatever its arithmetic does.
    let scope = HashMap::new();
    for src in [
        "x = 1 / 0",
        "x = 1e999 - 1e999",
        "x = sqrt(-1)",
        "x = 0 ^ (-1)",
        "x = (-8) ^ 0.5",
        "x = ln(0)",
    ] {
        let doc = parse_document(src).unwrap();
        for equation in doc.equations() {
            // Ok or Err, but never a panic.
            let _ = frees_core::eval::eval(&equation.rhs, &scope);
        }
    }
}

// ── Phase 4: the procedural / matrix / quadrature surface ───────────────────
//
// Everything below is an adversarial sweep of the constructs Phase 4 added.
// The rule is the one at the top of this file — `Ok` or `Err`, promptly, and
// nothing else — and two of these are regressions for aborts it caught:
//
// * [`a_flat_chain_at_the_ceiling_does_not_abort_a_consumer`] — the budget in
//   `parser/expr.rs` admitted a 519-link chain while `check`/`solve` could
//   only walk 304, so `x + x + … + x` with 400 terms parsed and then killed
//   the process. Both constants were halved. (`parser/expr.rs`)
// * [`differentiating_a_long_chain_stays_inside_the_depth_budget`] — a rule
//   builds a tree *deeper than the one it consumed* (measured: ×2 for `*`,
//   ×3 for `/`), so a legal expression differentiated into an illegal one and
//   overflowed the stack building, evaluating and dropping it.
//   (`differentiator.rs`)

/// The depth of an expression tree, for asserting on what a pass *built*.
fn expr_depth(e: &frees_core::Expr) -> u32 {
    use frees_core::Expr;
    let kids: Vec<&Expr> = match e {
        Expr::Num { .. } | Expr::Var(_) | Expr::Str(_) => vec![],
        Expr::Neg(a) | Expr::Not(a) => vec![a.as_ref()],
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
        } => vec![left.as_ref(), right.as_ref()],
        Expr::Call { args, .. } | Expr::ArrayLiteral(args) => args.iter().collect(),
        Expr::ArrayAccess { indices, .. } => indices.iter().collect(),
    };
    1 + kids.iter().map(|k| expr_depth(k)).max().unwrap_or(0)
}

fn chain(op: &str, links: usize) -> String {
    format!("{} = 1\n", vec!["x"; links].join(&format!(" {op} ")))
}

#[test]
fn a_flat_chain_at_the_ceiling_does_not_abort_a_consumer() {
    // A chain costs one level per link, so these span the ceiling and land
    // well past it. Every one aborted the process at 400+ links before the
    // budget was recalibrated — for `+` too, which rules out the
    // differentiator and pins it on the plain recursive walk.
    for op in ["+", "-", "*", "/"] {
        for links in [200, 250, 260, 300, 400, 520] {
            let src = chain(op, links);
            survives(&src).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}

#[test]
fn the_flat_chain_ceiling_is_a_refusal_and_still_clears_a_long_sum() {
    // The capability the budget must not cost us: a 200-term sum is a
    // legitimate document (matrix expansion emits one per matvec row).
    assert_eq!(
        solved(&format!("x = {}1", "1 + ".repeat(200))).get("x"),
        Some(&201.0)
    );
    // Past the budget the answer is the parser's diagnostic, not a signal.
    assert!(parse_err(&chain("+", 520)).contains("too deeply nested"));
}

#[test]
fn differentiating_a_long_chain_stays_inside_the_depth_budget() {
    // The invariant: whatever `differentiate` hands back is a tree the rest of
    // the crate is already calibrated to walk. `None` (fall back to finite
    // differences) is always an acceptable answer; an over-deep tree is not.
    for op in ["+", "-", "*", "/"] {
        for links in [2, 10, 50, 100, 200, 260] {
            let src = chain(op, links);
            let Ok(doc) = parse_document(&src) else {
                continue; // past the parser's ceiling; nothing to differentiate
            };
            for equation in doc.equations() {
                if let Some(d) = frees_core::differentiator::differentiate(&equation.lhs, "x") {
                    let depth = expr_depth(&d);
                    assert!(
                        depth <= MAX_EXPR_DEPTH,
                        "d/dx of a {links}-link `{op}` chain is {depth} deep, \
                         past the {MAX_EXPR_DEPTH} every consumer is calibrated for"
                    );
                }
            }
        }
    }
}

#[test]
fn a_long_sum_is_still_differentiated_analytically() {
    // Charging per *rule* rather than per node is what keeps this true: the
    // shape matrix expansion generates for every matvec row must not be
    // pushed onto the finite-difference path by the depth guard.
    let doc = parse_document(&chain("+", 200)).expect("a 200-term sum parses");
    let lhs = &doc.equations()[0].lhs;
    assert!(
        frees_core::differentiator::differentiate(lhs, "x").is_some(),
        "a 200-term sum must still differentiate symbolically"
    );
}

// ── huge matrix literals ────────────────────────────────────────────────────

fn square_matrix_literal(n: usize) -> String {
    let mut m = String::with_capacity(n * n * 2 + 8);
    m.push_str("A = [");
    for i in 0..n {
        if i > 0 {
            m.push_str("; ");
        }
        for j in 0..n {
            if j > 0 {
                m.push(' ');
            }
            m.push_str(if i == j { "2" } else { "1" });
        }
    }
    m.push_str("]\n");
    m
}

#[test]
fn an_enormous_matrix_literal_is_refused_rather_than_expanded() {
    // 200x200 is 40 000 elements, past `MAX_GENERATED_EQUATIONS`. The refusal
    // must name the ceiling rather than arrive as an out-of-memory abort.
    let message = solve_err(&square_matrix_literal(200));
    assert!(message.contains("Too many equations"), "{message}");
    // A matrix small enough to expand still does.
    assert!(solve(&square_matrix_literal(20), &settings()).is_ok());
}

#[test]
fn matrix_kernels_on_a_large_system_answer_rather_than_hang() {
    let mut src = square_matrix_literal(60);
    src.push_str("b = [");
    for i in 0..60 {
        if i > 0 {
            src.push_str("; ");
        }
        src.push('1');
    }
    src.push_str("]\nx = SolveLinear(A, b)\n");
    survives(&src).unwrap_or_else(|e| panic!("{e}"));

    let mut inv = square_matrix_literal(40);
    inv.push_str("B = Inverse(A)\n");
    survives(&inv).unwrap_or_else(|e| panic!("{e}"));
}

// ── recursive and non-terminating procedural bodies ─────────────────────────

#[test]
fn runaway_recursion_hits_the_call_depth_guard() {
    for (label, src) in [
        (
            "mutual",
            "FUNCTION F(n)\n  F := G(n-1)\nEND\nFUNCTION G(n)\n  G := F(n-1)\nEND\ny = F(10)\n",
        ),
        ("self", "FUNCTION F(n)\n  F := F(n-1)\nEND\ny = F(10)\n"),
        (
            "no base case reached",
            "FUNCTION Fact(n)\n  IF n <= 1 THEN\n    Fact := 1\n  ELSE\n    Fact := n * Fact(n-1)\n  END\nEND\ny = Fact(1e9)\n",
        ),
    ] {
        let message = solve_err(src);
        assert!(
            message.contains("nested more than"),
            "{label}: {message}"
        );
    }
}

#[test]
fn a_deep_chain_of_distinct_functions_is_bounded_too() {
    // Not recursion — 200 distinct functions each calling the next. The guard
    // counts nesting, so it must catch this the same way.
    let mut src = String::new();
    for i in 0..200 {
        src.push_str(&format!(
            "FUNCTION F{i}(n)\n  F{i} := F{}(n) + 1\nEND\n",
            i + 1
        ));
    }
    src.push_str("FUNCTION F200(n)\n  F200 := n\nEND\ny = F0(1)\n");
    assert!(solve_err(&src).contains("nested more than"));
}

#[test]
fn a_loop_that_never_terminates_hits_the_iteration_ceiling() {
    for (label, src, expected) in [
        (
            "WHILE",
            "FUNCTION Spin(n)\n  s := 0\n  WHILE 1 > 0 DO\n    s := s + 1\n  END\n  Spin := s\nEND\ny = Spin(1)\n",
            "WHILE loop exceeded",
        ),
        (
            "REPEAT",
            "FUNCTION Spin(n)\n  s := 0\n  REPEAT\n    s := s + 1\n  UNTIL 1 < 0\n  Spin := s\nEND\ny = Spin(1)\n",
            "REPEAT-UNTIL exceeded",
        ),
        (
            "nested WHILE",
            "FUNCTION Spin(n)\n  s := 0\n  WHILE 1 > 0 DO\n    WHILE 1 > 0 DO\n      s := s + 1\n    END\n  END\n  Spin := s\nEND\ny = Spin(1)\n",
            "WHILE loop exceeded",
        ),
        (
            "WHILE in a PROCEDURE",
            "PROCEDURE P(a : b)\n  b := 0\n  WHILE 1 > 0 DO\n    b := b + 1\n  END\nEND\nCALL P(1 : z)\n",
            "WHILE loop exceeded",
        ),
    ] {
        let message = solve_err(src);
        assert!(message.contains(expected), "{label}: {message}");
    }
}

#[test]
fn a_for_loop_with_absurd_bounds_is_bounded_by_the_same_ceiling() {
    let message = solve_err(
        "FUNCTION Big(n)\n  s := 0\n  FOR i = 1 TO 1e18\n    s = s + 1\n  END\n  Big := s\nEND\ny = Big(1)\n",
    );
    assert!(message.contains("FOR loop exceeded"), "{message}");

    // A NaN bound must be diagnosed, not silently treated as an empty loop.
    let nan = solve_err(
        "FUNCTION Big(n)\n  s := 0\n  FOR i = 1 TO 0/0\n    s = s + 1\n  END\n  Big := s\nEND\ny = Big(1)\n",
    );
    assert!(nan.contains("division by zero"), "{nan}");
}

// ── procedural output and arity contracts ───────────────────────────────────

#[test]
fn an_output_a_procedure_never_assigns_is_diagnosed() {
    for (label, src) in [
        (
            "assigns a different name",
            "PROCEDURE P(a : b)\n  c := a\nEND\nCALL P(1 : z)\n",
        ),
        ("empty body", "PROCEDURE P(a : b)\nEND\nCALL P(1 : z)\n"),
        (
            "assigned only on a branch not taken",
            "PROCEDURE P(a : b)\n  IF a > 100 THEN\n    b := 1\n  END\nEND\nCALL P(1 : z)\n",
        ),
    ] {
        let message = solve_err(src);
        assert!(
            message.contains("never assigned output variable"),
            "{label}: {message}"
        );
    }

    // The FUNCTION counterpart: a body that never assigns its own name.
    assert!(solve_err("FUNCTION F(n)\n  x := n\nEND\ny = F(1)\n")
        .contains("never assigned a return value"));
}

#[test]
fn an_arity_mismatch_is_refused_in_both_directions() {
    const DIVMOD: &str =
        "FUNCTION [q, r] = DivMod(a, b)\n  q := trunc(a / b)\n  r := a - q * b\nEND\n";
    for (label, src, expected) in [
        (
            "too many destructuring targets",
            format!("{DIVMOD}[x, y, z] = DivMod(17, 5)\n"),
            "provides 3 output variable(s) but PROCEDURE declares 2",
        ),
        (
            "too few destructuring targets",
            format!("{DIVMOD}[x] = DivMod(17, 5)\n"),
            "provides 1 output variable(s) but PROCEDURE declares 2",
        ),
        (
            "too many CALL outputs",
            "PROCEDURE P(a : b, c)\n  b := a\n  c := a\nEND\nCALL P(1 : x, y, z)\n".to_string(),
            "provides 3 output variable(s) but PROCEDURE declares 2",
        ),
        (
            "too many CALL inputs",
            "PROCEDURE P(a : b)\n  b := a\nEND\nCALL P(1, 2, 3 : x)\n".to_string(),
            "expects 1 input(s), got 3",
        ),
        (
            "too few CALL inputs",
            "PROCEDURE P(a, b : c)\n  c := a + b\nEND\nCALL P(1 : x)\n".to_string(),
            "expects 2 input(s), got 1",
        ),
    ] {
        let message = solve_err(&src);
        assert!(message.contains(expected), "{label}: {message}");
    }
}

#[test]
fn a_module_body_that_calls_itself_is_refused_not_expanded_forever() {
    for (label, src) in [
        ("self", "MODULE M(x : y)\n  CALL M(x : y)\nEND\nCALL M(1 : z)\n"),
        (
            "mutual",
            "MODULE A(x : y)\n  CALL B(x : y)\nEND\nMODULE B(x : y)\n  CALL A(x : y)\nEND\nCALL A(1 : z)\n",
        ),
    ] {
        let message = solve_err(src);
        assert!(message.contains("contains a CALL"), "{label}: {message}");
    }
    // An empty MODULE grafts nothing, so its output stays free.
    assert!(solve("MODULE M(x : y)\nEND\nCALL M(1 : z)\n", &settings()).is_err());
}

// ── degenerate TABLEs ───────────────────────────────────────────────────────

#[test]
fn a_degenerate_table_answers_rather_than_dividing_by_its_own_span() {
    none_panic(&[
        // One point: every interpolation is a zero-width span.
        "TABLE t(x)\n  1   10\nEND\ny = t(5)\n",
        // Duplicate abscissa: the span between rows 1 and 2 is zero.
        "TABLE t(x)\n  1   10\n  1   20\n  2   30\nEND\ny = t(1)\n",
        "TABLE t(x)\n  1   10\n  1   20\nEND\ny = t(1)\n",
        // Descending x, which the interpolator's bracket search assumes away.
        "TABLE t(x)\n  3   30\n  2   20\n  1   10\nEND\ny = t(2.5)\n",
        // Log axes over zero and negative abscissae.
        "TABLE t(x) XLOG YLOG\n  0   0\n  -1  -10\n  10  10\nEND\ny = t(5)\n",
        // Overflowing spans.
        "TABLE t(x)\n  -1e308   1e308\n  1e308    -1e308\nEND\ny = t(0)\n",
        // A curve family with a short row.
        "TABLE t(re : p = 1, 2)\n  0    0    0\n  10   10\nEND\ny = t(5, 1.5)\n",
    ]);
    // An empty body is a parse error, not an empty table.
    assert!(parse_document("TABLE t(x)\nEND\ny = t(5)\n").is_err());
}

// ── rangeAssign ─────────────────────────────────────────────────────────────

#[test]
fn a_pathological_range_answers_rather_than_materialising() {
    none_panic(&[
        "x = 1:0:10",               // step zero
        "x = 10:1:1",               // bounds the wrong way round
        "x = -1e308:1:1e308",       // span overflows the element count
        "x = 0:5e-324:1",           // subnormal step
        "x = 1e308:-5e-324:-1e308", // both at once, descending
        "x = 0:1:1e308 | Log",      // log spacing over an overflowing span
        "x = 0:10:100 | Log",       // log spacing across zero
        "x = -100:10:-1 | Log",     // log spacing over negatives
    ]);
}

#[test]
fn an_out_of_range_array_index_is_diagnosed() {
    none_panic(&[
        "speed = 0:10:100\na = speed[0]", // 1-based, so 0 is off the front
        "speed = 0:10:100\na = speed[-1]",
        "speed = 0:10:100\na = speed[1e18]",
        "speed = 0:10:100\na = speed[0/0]",
        "speed = 0:10:100\na = speed[12]", // one past the end
    ]);
}

// ── Integral / GaussIntegral ────────────────────────────────────────────────

#[test]
fn a_pathological_integral_answers_rather_than_sweeping_forever() {
    none_panic(&[
        "F = Integral(t^2, t, 1, 0)",                  // bounds reversed
        "F = Integral(0/0, t, 0, 1)",                  // integrand is NaN everywhere
        "F = Integral(F, t, 0, 1)",                    // integrand is its own result
        "F = Integral(1/t, t, 0, 1)",                  // singular at the lower limit
        "F = Integral(Integral(u, u, 0, t), t, 0, 1)", // nested
    ]);
}

/// The two quadrature inputs that are *bounded but not quick*.
///
/// Both terminate with the right answer, so they are not defects — but they
/// are the slowest inputs found on this surface and the numbers should not be
/// lost. Measured in a debug build:
///
/// | document | wall clock | outcome |
/// |---|---|---|
/// | `Integral(t, t, -1e308, 1e308)` | ~200 s | `Ok` |
/// | `GaussIntegral(1/t, t, 0, 1)` | ~170 s | `Err`, did not converge |
///
/// `Integral` is bounded only by `integral::MAX_STEPS` (200 000), a faithful
/// port of the Java sweep, and an interval that wide uses every one of them.
/// Lowering it would be a parity change, not a fix, so the cost is recorded
/// here instead. Ignored because 6 minutes is not a unit test.
#[test]
#[ignore = "bounded but slow (~6 min); documents the quadrature step budget"]
fn the_slowest_quadrature_inputs_still_terminate() {
    none_panic(&[
        "F = Integral(t, t, -1e308, 1e308)",
        "F = GaussIntegral(1/t, t, 0, 1)",
    ]);
}

#[test]
fn a_pathological_gauss_integral_answers_rather_than_refining_forever() {
    none_panic(&[
        "F = GaussIntegral(t^2, t, 1, 0)",       // bounds reversed
        "F = GaussIntegral(t^2, t, 0, 1, 0/0)",  // NaN point count
        "F = GaussIntegral(t^2, t, 0, 1, 1e18)", // point count past the clamp
        "F = GaussIntegral(t^2, t, 0, 1, -5)",   // negative point count
        "F = GaussIntegral(F, t, 0, 1)",         // integrand is its own result
    ]);
    // Reversed limits are named rather than silently negated.
    assert!(solve_err("F = GaussIntegral(t^2, t, 1, 0)").contains("do not specify an interval"));
}

#[test]
fn an_integrand_that_recurses_without_a_base_case_is_bounded() {
    let message = solve_err("FUNCTION F(n)\n  F := F(n-1)\nEND\nG = Integral(F(t), t, 0, 1)\n");
    assert!(message.contains("nested more than"), "{message}");
}

// ── complex mode ────────────────────────────────────────────────────────────

#[test]
fn complex_mode_on_a_large_real_document_terminates() {
    let complex = SolverSettings {
        complex_mode: true,
        ..SolverSettings::default()
    };
    // Every real equation doubles into a real and an imaginary part, so this
    // is 800 equations by the time it reaches the blocker.
    let mut src = String::from("x0 = 1\n");
    for i in 1..400 {
        src.push_str(&format!("x{i} = x{} * 1.0001 + sin(x{})\n", i - 1, i - 1));
    }
    assert!(
        solve(&src, &complex).is_ok(),
        "large real document in complex mode"
    );

    // Mixed with the rest of the Phase-4 surface.
    let mixed = "FUNCTION F(n)\n  F := n * 2\nEND\n\
                 TABLE t(x)\n  1   10\n  2   20\nEND\n\
                 a = F(3)\nb = t(1.5)\nc_r = 1\nc_i = 2\nd = c_r * c_i + a + b\n";
    let _ = solve(mixed, &complex);

    // A chain of square roots of a negative, which is where complex expansion
    // has to seed `_i` components rather than inherit the real guess.
    let mut roots = String::from("x0 = -1\n");
    for i in 1..100 {
        roots.push_str(&format!("x{i} = sqrt(x{})\n", i - 1));
    }
    let _ = solve(&roots, &complex);
}
