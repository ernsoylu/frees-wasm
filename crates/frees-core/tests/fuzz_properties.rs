//! Phase 12: property-based fuzzing over the hostile-input surfaces.
//!
//! The hand-written robustness suites (`robustness.rs` and friends, 565 tests)
//! encode *chosen* adversarial cases; this file generates them. The contract
//! being enforced is the same one line: **every public entry point answers
//! `Ok` or `Err` for any input whatsoever** — no panic, no abort, no hang.
//! `catch_unwind` works here because tests build with unwinding; the shipped
//! wasm is `panic = "abort"`, which is exactly why a panic found by this file
//! is a session-ending defect there and must be fixed at the source.
//!
//! Two kinds of generator, deliberately:
//!
//! * **Unstructured** — arbitrary unicode and arbitrary bytes. Finds lexer
//!   and early-parse failures; almost never reaches the solver.
//! * **Structure-aware** — a small grammar that emits documents which *parse*
//!   (identifiers, unit annotations, FOR/GUESS blocks, function calls,
//!   matrices). These reach the depths the unstructured ones cannot. (The
//!   MDF4 byte-splicing properties were removed with the format reader,
//!   decision D6.)
//!
//! Case counts are tuned so the whole file stays inside ~a minute in release
//! CI (`PROPTEST_CASES` overrides for a longer local soak). Shrinking is
//! proptest's; any minimized counterexample that survives triage belongs in
//! the matching hand-written suite as a named regression, with this file's
//! seed line quoted.

// Native-only: proptest's rand stack does not build on wasm32, and clippy
// compiles this target for wasm32 in CI. The properties themselves are
// target-independent — the same engine code ships in the wasm.
#![cfg(not(target_arch = "wasm32"))]

use proptest::prelude::*;

use frees_core::{check, parse_document, solve, SolverSettings};

// ── the survival oracle ─────────────────────────────────────────────────────

fn survives(src: &str) -> Result<(), TestCaseError> {
    let run = |name: &str, f: &(dyn Fn() + std::panic::RefUnwindSafe)| {
        std::panic::catch_unwind(f)
            .map_err(|_| TestCaseError::fail(format!("{name} panicked on {src:?}")))
    };
    run("parse_document", &|| {
        let _ = parse_document(src);
    })?;
    run("check", &|| {
        let _ = check(src);
    })?;
    run("solve", &|| {
        let _ = solve(src, &SolverSettings::default());
    })?;
    Ok(())
}

// ── unstructured input ──────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Any unicode string at all is answered, not survived.
    #[test]
    fn arbitrary_unicode_is_answered(src in "\\PC*") {
        survives(&src)?;
    }

    /// Arbitrary bytes, lossily decoded — exercises the replacement-character
    /// and truncated-multibyte paths the unicode strategy cannot produce.
    #[test]
    fn arbitrary_bytes_are_answered(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let src = String::from_utf8_lossy(&bytes);
        survives(&src)?;
    }
}

// ── structure-aware documents ───────────────────────────────────────────────

/// Identifiers biased toward collisions with builtins and unit names, plus
/// the sigil forms (`$` strings, `#` constants) the type system keys off.
fn ident() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9_]{0,6}",
        Just("x".to_string()),
        Just("sin".to_string()),
        Just("pi#".to_string()),
        Just("name$".to_string()),
        Just("T_in".to_string()),
        Just("m".to_string()), // also a unit
    ]
}

fn number() -> impl Strategy<Value = String> {
    prop_oneof![
        (-1.0e6..1.0e6f64).prop_map(|v| format!("{v}")),
        Just("0".to_string()),
        Just("1e308".to_string()),
        Just("1e-320".to_string()),
        Just("-0.0".to_string()),
        Just("1e309".to_string()), // parses as inf in Java; must be answered
    ]
}

fn unit_annotation() -> impl Strategy<Value = String> {
    let base = prop_oneof![
        Just("m"),
        Just("s"),
        Just("kg"),
        Just("K"),
        Just("C"),
        Just("kPa"),
        Just("W"),
        Just("kJ/kg"),
        Just("m^2"),
        Just("m/s^2"),
        Just("bogus"),
    ];
    (base, 0u8..3).prop_map(|(b, p)| match p {
        0 => format!(" [{b}]"),
        1 => format!(" [{b}*{b}]"),
        _ => format!(" [{b}/{b}^2]"),
    })
}

/// Expressions built to a bounded depth; every form the grammar admits at
/// expression level gets a branch, including the matrix literal.
fn expr(depth: u32) -> BoxedStrategy<String> {
    if depth == 0 {
        return prop_oneof![
            number(),
            ident(),
            (number(), unit_annotation()).prop_map(|(n, u)| format!("{n}{u}")),
        ]
        .boxed();
    }
    let sub = expr(depth - 1);
    prop_oneof![
        (
            sub.clone(),
            prop_oneof![Just("+"), Just("-"), Just("*"), Just("/"), Just("^")],
            sub.clone()
        )
            .prop_map(|(a, op, b)| format!("({a} {op} {b})")),
        sub.clone().prop_map(|a| format!("-({a})")),
        (
            prop_oneof![
                Just("sin"),
                Just("exp"),
                Just("abs"),
                Just("sqrt"),
                Just("ln")
            ],
            sub.clone()
        )
            .prop_map(|(f, a)| format!("{f}({a})")),
        (sub.clone(), sub.clone(), sub.clone())
            .prop_map(|(c, a, b)| format!("if({c} > 0, {a}, {b})")),
        (sub.clone(), sub.clone()).prop_map(|(a, b)| format!("[{a} {b}; {b} {a}]")),
        sub,
    ]
    .boxed()
}

/// One document line: an equation, occasionally a GUESS or a FOR wrapper.
fn line() -> impl Strategy<Value = String> {
    (ident(), expr(3)).prop_map(|(lhs, rhs)| format!("{lhs} = {rhs}"))
}

fn document() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(line(), 1..12),
        prop::option::of((ident(), number()).prop_map(|(v, n)| format!("GUESS {v} = {n} END"))),
        any::<bool>(),
    )
        .prop_map(|(lines, guess, wrap_for)| {
            let mut doc = String::new();
            if let Some(g) = guess {
                doc.push_str(&g);
                doc.push('\n');
            }
            if wrap_for {
                doc.push_str("FOR i = 1 TO 3\n");
            }
            for l in &lines {
                doc.push_str(l);
                doc.push('\n');
            }
            if wrap_for {
                doc.push_str("END\n");
            }
            doc
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Grammar-shaped documents reach the blocker and Newton; they too are
    /// answered, never survived.
    #[test]
    fn structured_documents_are_answered(doc in document()) {
        survives(&doc)?;
    }

    /// Solving the same document twice gives the identical answer — the
    /// engine has no hidden global state and no nondeterminism. (Bitwise:
    /// f64 results are compared through their exact debug form.)
    #[test]
    fn solve_is_deterministic(doc in document()) {
        let a = solve(&doc, &SolverSettings::default());
        let b = solve(&doc, &SolverSettings::default());
        let fmt = |r: &Result<frees_core::Solution, frees_core::SolveFailure>| match r {
            Ok(s) => {
                let mut vars: Vec<String> =
                    s.values.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                vars.sort();
                vars.join(";")
            }
            Err(e) => format!("err:{e:?}"),
        };
        prop_assert_eq!(fmt(&a), fmt(&b), "solve nondeterministic on {:?}", doc);
    }
}

// ── unit-annotation surface ─────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Arbitrary text inside a unit annotation — the registry's own parser,
    /// reached exactly as a document reaches it.
    #[test]
    fn arbitrary_unit_annotations_are_answered(u in "[ -~]{0,40}") {
        let doc = format!("x = 1 [{u}]\n");
        survives(&doc)?;
    }
}
