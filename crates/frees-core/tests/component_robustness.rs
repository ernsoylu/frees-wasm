//! Adversarial robustness of the **component/connect surface** — Phase 6's half
//! of `robustness.rs` and `props_robustness.rs`.
//!
//! The rule is the one line those two files already enforce: **`parse_document`,
//! `check` and `solve` may return `Ok` or `Err` for any byte string whatsoever,
//! and must do nothing else** — not panic, not abort, not hang, not overflow the
//! stack, and not quietly answer a question that was never asked.
//!
//! Phase 6 is the first layer in this port that is *structurally* recursive. A
//! hierarchical `COMPONENT` flattens by calling itself
//! ([`components::expander::flatten_instance`]); a `connect(...)` node is an
//! n-ary junction over a union-find whose size is user-controlled; a `model$`
//! selector reaches a `VARIANT` chosen by a string the user types. Every one of
//! those is a new way to break the rule, and three of them (self-instantiation,
//! deep nesting, a 200-instance chain) can break it by *running out of machine*
//! rather than by returning the wrong thing.
//!
//! # What "not quietly answer" means for a component network
//!
//! Stricter than "no panic", and different from the property surface's
//! finite-value rule:
//!
//! 1. **An `Ok` must be structurally right, not merely produced.** A 50-endpoint
//!    signal node that solves but broadcasts the wrong value to endpoint 37 is a
//!    worse outcome than an error, so the wide/long cases assert the physics
//!    (`all endpoints equal`, `the chain's gain is 2^n`), not just `is_ok()`.
//! 2. **An `Err` must name the component or the instance, never a mangled
//!    scalar.** That is the parent engine's stated diagnostic invariant
//!    (`../frEES/CLAUDE.md`), and a port that reports `pu1$in$mdot` instead of
//!    `PU1` has broken it even though it correctly refused.
//! 3. **A refusal must be the *right* refusal.** `assert!(result.is_err())` is
//!    satisfied by a stack overflow caught as a panic in another thread, so
//!    every rejection here is matched against a substring of its message.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use frees_core::{check, parse_document, solve, SolverSettings};

// ── helpers ─────────────────────────────────────────────────────────────────

fn settings() -> SolverSettings {
    SolverSettings::default()
}

/// Run the three public entry points on `src` and report only *how* each
/// answered, with the offending document in any failure message.
///
/// `catch_unwind` is what makes a stack overflow visible as a test failure
/// rather than as a process abort — on the common case where the recursion is
/// guarded, this costs nothing.
fn answered(src: &str) -> Result<(), String> {
    fn run(
        name: &str,
        src: &str,
        f: impl Fn(&str) + std::panic::RefUnwindSafe,
    ) -> Result<(), String> {
        std::panic::catch_unwind(|| f(src)).map_err(|_| format!("{name} panicked on {src:?}"))
    }
    run("parse_document", src, |s| {
        let _ = parse_document(s);
    })?;
    run("check", src, |s| {
        let _ = check(s);
    })?;
    run("solve", src, |s| {
        let _ = solve(s, &settings());
    })
}

/// The standing contract, applied to one document: answered by all three entry
/// points, bounded, and — if it solved — every value finite.
fn survives(src: &str) -> Result<Duration, String> {
    let started = Instant::now();
    answered(src)?;
    let elapsed = started.elapsed();
    if let Ok(solution) = solve(src, &settings()) {
        let bad: Vec<(&String, &f64)> = solution
            .values
            .iter()
            .filter(|(_, v)| !v.is_finite())
            .collect();
        if !bad.is_empty() {
            return Err(format!(
                "{src:?} solved with non-finite values {bad:?} — the expansion produced \
                 equations the solver could not honestly satisfy"
            ));
        }
    }
    Ok(elapsed)
}

/// Assert the contract over a whole corpus, reporting every violation at once.
fn all_survive(corpus: &[String]) -> Duration {
    let mut worst = Duration::ZERO;
    let mut slowest = String::new();
    let mut failures = Vec::new();
    for src in corpus {
        match survives(src) {
            Ok(d) => {
                if d > worst {
                    worst = d;
                    slowest.clone_from(src);
                }
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} documents broke the contract:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n")
    );
    assert!(
        worst < Duration::from_secs(20),
        "slowest document took {worst:?}, which is a hang in all but name: {slowest:?}"
    );
    // See the sibling helper in `props_robustness.rs`: the margin under the
    // hang ceiling is worth reading, not just asserting.
    println!(
        "all_survive: {} documents, worst {worst:?} on {slowest:?}",
        corpus.len()
    );
    worst
}

/// How a document was answered, as a single string — `"Ok"` or the error text.
fn outcome(src: &str) -> String {
    match solve(src, &settings()) {
        Ok(_) => "Ok".to_string(),
        Err(e) => e.to_string(),
    }
}

/// Assert that `src` is refused, that the refusal is bounded and panic-free,
/// and that the message says `needle` (case-insensitively).
///
/// The needle is the whole point: `is_err()` alone is also satisfied by a
/// caught stack overflow or by an unrelated downstream failure.
fn rejected(src: &str, needle: &str) {
    let elapsed = survives(src).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        elapsed < Duration::from_secs(20),
        "refusal took {elapsed:?}: {src:?}"
    );
    let message = outcome(src);
    assert_ne!(
        message, "Ok",
        "expected a refusal mentioning {needle:?}, but the document solved:\n{src}"
    );
    assert!(
        message
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase()),
        "refused, but not for the reason under test.\n  wanted: {needle:?}\n  got:    \
         {message}\n  document:\n{src}"
    );
}

/// Assert that `src` solves, and hand back its values for a physics check.
fn solved(src: &str) -> BTreeMap<String, f64> {
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    match solve(src, &settings()) {
        Ok(s) => s.values,
        Err(e) => panic!("expected a solve, got: {e}\n  document:\n{src}"),
    }
}

/// `a ≈ b` at the tolerance the parity harness uses for exactly-computed
/// quantities.
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

// ── 1. recursion: a component that instantiates itself ───────────────────────

/// Direct self-instantiation. `flatten_instance` recurses through
/// `sub_instances`, so without the ancestor stack this is an unbounded
/// recursion that dies as a stack overflow — an *abort*, not an `Err`, and one
/// `catch_unwind` cannot always convert.
#[test]
fn a_component_that_instantiates_itself_directly_is_refused_by_name() {
    rejected(
        "\
COMPONENT SelfLoop(a, b)
  SigGain inner(a, b, k = 2)
  SelfLoop again(a, b)
END
SelfLoop L(s1, s2)
s1.sig = 1
y = s2.sig
",
        "instantiates itself",
    );
}

/// The same cycle reached through one intermediate — the case a naive
/// "is this my own name?" guard misses.
#[test]
fn two_mutually_recursive_components_are_refused_not_recursed() {
    rejected(
        "\
COMPONENT Alpha(a, b)
  Beta inner(a, b)
END
COMPONENT Beta(a, b)
  Alpha inner(a, b)
END
Alpha L(s1, s2)
s1.sig = 1
y = s2.sig
",
        "instantiates itself",
    );
}

/// Three-cycle, to prove the guard is an ancestor *stack* and not a
/// parent-pointer comparison.
#[test]
fn a_three_component_instantiation_cycle_is_refused() {
    rejected(
        "\
COMPONENT Ay(a, b)
  Bee inner(a, b)
END
COMPONENT Bee(a, b)
  Cee inner(a, b)
END
COMPONENT Cee(a, b)
  Ay inner(a, b)
END
Ay L(s1, s2)
s1.sig = 1
y = s2.sig
",
        "instantiates itself",
    );
}

/// A legal hierarchy 50 subsystems deep. Nothing here is a cycle, so the
/// ancestor guard must *not* fire — but `flatten_instance` still recurses once
/// per level, and this is the case that finds a missing depth budget by
/// overflowing the stack on a machine with a smaller one.
///
/// It also pins the physics, which is the part `is_ok()` would miss: level `n`
/// wraps level `n-1` **and adds its own gain of 2**, so the whole tower is
/// `2^50` — a number a hierarchy that silently dropped or duplicated one level
/// cannot land on.
#[test]
fn a_fifty_level_deep_hierarchy_flattens_without_overflowing_the_stack() {
    const DEPTH: usize = 50;
    let mut src = String::from("COMPONENT Level0(a, b)\n  SigGain g(a, b, k = 2)\nEND\n");
    for level in 1..DEPTH {
        src.push_str(&format!(
            "COMPONENT Level{level}(a, b)\n  Level{} inner(a, mid)\n  \
             SigGain g(mid, b, k = 2)\nEND\n",
            level - 1
        ));
    }
    src.push_str(&format!("Level{} TOP(s1, s2)\n", DEPTH - 1));
    src.push_str("s1.sig = 1\ny = s2.sig\n");

    let values = solved(&src);
    let y = values["y"];
    assert!(
        close(y, 2f64.powi(DEPTH as i32)),
        "a {DEPTH}-deep hierarchy of gain-2 blocks gave y = {y}, expected 2^{DEPTH}"
    );
}

// ── 2. width: a connect node with many endpoints ─────────────────────────────

/// Fifty readers on one signal wire. The signal domain carries **no** flow
/// member, so the node is a pure broadcast: every endpoint must hold the same
/// value, and the union-find must emit exactly `n-1` spanning-tree equalities
/// rather than the `n(n-1)/2` all-pairs set (which would be structurally
/// singular).
///
/// Asserting `is_ok()` alone would pass on a network that solved 50 disconnected
/// variables to whatever the initial guess was, so this checks all 50 readers.
#[test]
fn a_connect_node_with_fifty_endpoints_broadcasts_to_every_one_of_them() {
    const N: usize = 50;
    let mut src = String::from("SigConstant SRC(k = 7)\n");
    for i in 0..N {
        src.push_str(&format!("SigGain G{i}(k = 1)\n"));
    }
    let mut endpoints = vec!["SRC.out".to_string()];
    for i in 0..N {
        endpoints.push(format!("G{i}.in"));
    }
    src.push_str(&format!("connect({})\n", endpoints.join(", ")));
    for i in 0..N {
        src.push_str(&format!("y{i} = G{i}.out.sig\n"));
    }

    let values = solved(&src);
    for i in 0..N {
        let y = values
            .get(&format!("y{i}"))
            .copied()
            .unwrap_or_else(|| panic!("y{i} missing from a {N}-endpoint broadcast"));
        assert!(
            close(y, 7.0),
            "endpoint {i} of a {N}-way signal node read {y}, not the broadcast 7"
        );
    }
}

/// The same width on a *bond-graph* node, where the flow rule matters: one
/// source into twenty-four parallel resistors and one sink, on the electrical
/// bond. `ΣI = 0` at the node, so the currents must add up rather than each
/// endpoint getting the whole current.
#[test]
fn a_wide_electrical_node_conserves_current_rather_than_duplicating_it() {
    const N: usize = 24;
    let mut src = String::from("VoltageSource VS(E = 10)\nGround GND()\n");
    for i in 0..N {
        src.push_str(&format!("Resistor R{i}(R = 100)\n"));
    }
    let mut hot = vec!["VS.p".to_string()];
    let mut cold = vec!["VS.n".to_string(), "GND.port".to_string()];
    for i in 0..N {
        hot.push(format!("R{i}.a"));
        cold.push(format!("R{i}.b"));
    }
    src.push_str(&format!("connect({})\n", hot.join(", ")));
    src.push_str(&format!("connect({})\n", cold.join(", ")));
    src.push_str("i_src = VS.p.I\n");

    let values = solved(&src);
    let i_src = values["i_src"];
    // N resistors of 100 Ω across 10 V: 0.1 A each, N × 0.1 A total. The sign
    // convention is the library's; magnitude is what this test is about.
    assert!(
        close(i_src.abs(), N as f64 * 0.1),
        "a {N}-way parallel node drew |I| = {} A, expected {} A",
        i_src.abs(),
        N as f64 * 0.1
    );
}

/// A `connect` needs two endpoints to mean anything; one is a typo, and the
/// expander says so rather than emitting a vacuous node.
#[test]
fn a_connect_with_a_single_endpoint_is_refused() {
    rejected(
        "\
SigConstant SRC(k = 1)
connect(SRC.out)
y = SRC.out.sig
",
        "at least two endpoints",
    );
}

/// The degenerate sibling: a `connect` with *zero* endpoints. The grammar may
/// reject it outright or the expander may — either is a clean refusal, and the
/// point of the test is that neither panics.
#[test]
fn a_connect_with_no_endpoints_at_all_is_answered_cleanly() {
    let src = "\
SigConstant SRC(k = 1)
connect()
y = SRC.out.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    let message = outcome(src);
    assert_ne!(
        message, "Ok",
        "connect() with no endpoints should not silently succeed"
    );
}

/// A node that ties a component's own two ports together — a cycle of length
/// one. This is a legal bond-graph short circuit (`in` and `out` of a pipe tied
/// to each other), so the interesting property is not that it is refused, but
/// that the union-find does not spin and the answer, if any, is finite.
#[test]
fn a_self_loop_connect_of_length_one_terminates() {
    // `GasPipe GP()` — the free-port form. The empty parameter list is not
    // optional: `GasPipe GP` is a *syntax* error (the statement grammar reads it
    // as an assignment missing its `=`), and writing it that way here would test
    // the lexer instead of the union-find.
    let src = "\
GasPipe GP()
connect(GP.in, GP.out)
p = GP.in.P
";
    let elapsed = survives(src).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        elapsed < Duration::from_secs(5),
        "a length-1 connect cycle took {elapsed:?} — the union-find is spinning"
    );
    // Whichever way it lands, it must land: `survives` already proved it is not
    // a panic and not a non-finite Ok. Record the outcome for the reader.
    println!("length-1 connect cycle → {}", outcome(src));
}

/// The same shape one level up: two components wired into a closed two-node
/// loop with nothing else. Union-find loop closure has to drop the second
/// across-equality or the system is over-determined.
#[test]
fn a_closed_two_node_signal_loop_terminates() {
    let src = "\
SigGain A(k = 1)
SigGain B(k = 1)
connect(A.out, B.in)
connect(B.out, A.in)
y = A.out.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    println!("closed 2-node signal loop → {}", outcome(src));
}

// ── 3. naming and arity ──────────────────────────────────────────────────────

#[test]
fn two_instances_sharing_a_name_are_refused_by_that_name() {
    rejected(
        "\
SigConstant SRC(out, k = 1)
SigConstant SRC(out2, k = 2)
y = out.sig
",
        "'src' is declared more than once",
    );
}

/// Two `COMPONENT` templates of one name — the definition-side twin of the
/// above. The Java raises this from the expander's constructor, so it must not
/// be silently resolved to "the first one wins".
#[test]
fn two_component_definitions_sharing_a_name_are_refused() {
    rejected(
        "\
COMPONENT Twin(a, b)
  b.sig = a.sig
END
COMPONENT Twin(a, b)
  b.sig = 2 * a.sig
END
Twin T(s1, s2)
s1.sig = 1
y = s2.sig
",
        "defined more than once",
    );
}

/// A user component may not shadow a shipped built-in either — that would make
/// the meaning of `Pump` depend on which file the reader is looking at.
#[test]
fn a_user_component_redefining_a_builtin_is_answered_cleanly() {
    let src = "\
COMPONENT Pump(in, out)
  out.P = in.P
END
Pump PU(s1, s2)
p = s2.P
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    println!("user redefinition of a built-in → {}", outcome(src));
}

#[test]
fn an_instance_bound_to_too_few_ports_is_refused_with_the_declared_list() {
    rejected(
        "\
Splitter SP(f1, f2)
f1.P = 1e5
",
        "binds 2 port(s) but COMPONENT splitter declares 3",
    );
}

#[test]
fn an_instance_bound_to_too_many_ports_is_refused_with_the_declared_list() {
    rejected(
        "\
Sink SK(f1, f2, f3)
f1.P = 1e5
",
        "binds 3 port(s) but COMPONENT sink declares 1",
    );
}

/// The hierarchical arity path is a **deliberate divergence** from the Java,
/// which indexes `portArgs` positionally with no guard and dies with
/// `IndexOutOfBoundsException` (`expander.rs` documents the check against the
/// oracle). A panic is not an acceptable port of a panic, so this pins the
/// diagnostic the port raises instead.
#[test]
fn a_hierarchical_instance_bound_to_too_few_ports_is_refused_not_indexed() {
    rejected(
        "\
COMPONENT Pair(a, b, c)
  SigGain g1(a, b, k = 2)
  SigGain g2(b, c, k = 3)
END
Pair P(s1)
s1.sig = 1
",
        "binds 1 port(s) but COMPONENT pair declares 3",
    );
}

#[test]
fn an_unknown_component_type_is_refused_by_name() {
    rejected(
        "\
Nonexistotron NX(s1, s2)
y = s2.sig
",
        "unknown component type 'nonexistotron'",
    );
}

/// Casing must not rescue it: the language is case-insensitive, so a misspelled
/// type is misspelled at every casing.
#[test]
fn an_unknown_component_type_is_refused_at_every_casing() {
    for spelling in ["SIGGAINN", "siggainn", "SigGainn"] {
        rejected(
            &format!("{spelling} X(s1, s2, k = 1)\ns1.sig = 1\ny = s2.sig\n"),
            "unknown component type",
        );
    }
}

#[test]
fn an_unknown_port_name_in_a_connect_is_refused_with_the_reference() {
    rejected(
        "\
SigConstant SRC(k = 1)
SigGain G(k = 2)
connect(SRC.nosuchport, G.in)
y = G.out.sig
",
        "src.nosuchport",
    );
}

/// The body-side twin: a `COMPONENT` whose equations name a port it does not
/// declare. The diagnostic must list the ports that *do* exist — a component
/// author reading `references unknown port 'inn'` needs to see `in, out`.
#[test]
fn an_unknown_port_name_in_a_component_body_is_refused_with_the_port_list() {
    rejected(
        "\
COMPONENT Typo(in, out)
  out.sig = inn.sig
END
Typo T(s1, s2)
s1.sig = 1
y = s2.sig
",
        "references unknown port 'inn'. ports: in, out",
    );
}

/// A port reference with no member (`in` rather than `in.sig`) is the other
/// half of the same mistake — but a *different* outcome, and the difference is
/// load-bearing.
///
/// `rewriteBodyVar` only treats a name as a port reference when it contains a
/// dot, so a bare `in` is an ordinary component-local with nothing to define it.
/// The Java does exactly the same, so the port must not "improve" on it by
/// raising an unknown-port error the reference engine never raises. What it
/// *must* do is surface the dangling local under a readable name.
#[test]
fn a_port_reference_with_no_member_falls_through_to_a_readable_dangling_local() {
    let src = "\
COMPONENT NoMember(in, out)
  out.sig = in
END
NoMember N(s1, s2)
s1.sig = 1
y = s2.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    let message = outcome(src);
    assert_ne!(
        message, "Ok",
        "a body naming a port with no member left the system underspecified, but it \
         solved anyway:\n{src}"
    );
    assert!(
        message.contains("n.in"),
        "the dangling local must be reported as the component's `n.in`, got: {message}"
    );
    assert!(
        !message.contains("n$in"),
        "the diagnostic leaked the mangled scalar name: {message}"
    );
}

#[test]
fn an_unknown_parameter_at_instantiation_is_refused_by_name() {
    rejected(
        "\
SigGain G(s1, s2, k = 2, kk = 3)
s1.sig = 1
y = s2.sig
",
        "unknown parameter 'kk'",
    );
}

#[test]
fn a_parameter_with_no_value_and_no_default_is_refused_by_name() {
    rejected(
        "\
SigGain G(s1, s2)
s1.sig = 1
y = s2.sig
",
        "parameter 'k' has no value",
    );
}

// ── 4. variants ──────────────────────────────────────────────────────────────

/// `REQUIRE` names a parameter that is declared nowhere. `ComponentDef::new`
/// promotes such a name to a parameter with no default (that is the Java's own
/// post-processing), so the honest outcome is not "unknown name" but "you now
/// owe me a value for it" — reported against the *component and instance*, not
/// against a mangled scalar.
#[test]
fn a_variant_requiring_an_undeclared_parameter_asks_for_it_by_name() {
    rejected(
        "\
COMPONENT Choosy(in, out)
  PARAM model$ = linear
  VARIANT linear REQUIRE ghost
    out.sig = ghost * in.sig
  END
END
Choosy C(s1, s2)
s1.sig = 1
y = s2.sig
",
        "'ghost' has no value",
    );
}

/// The same document with the promoted parameter supplied must then solve —
/// otherwise the previous test is passing for the wrong reason.
#[test]
fn the_same_variant_solves_once_the_promoted_parameter_is_supplied() {
    let values = solved(
        "\
COMPONENT Choosy(in, out)
  PARAM model$ = linear
  VARIANT linear REQUIRE ghost
    out.sig = ghost * in.sig
  END
END
Choosy C(s1, s2, ghost = 5)
s1.sig = 3
y = s2.sig
",
    );
    assert!(close(values["y"], 15.0), "y = {}", values["y"]);
}

/// A parameter required only by an *unselected* variant stays optional — the
/// scoping rule `VariantScope::is_optional` implements. Getting this wrong the
/// other way (demanding every variant's parameters) would refuse most of the
/// shipped library.
#[test]
fn a_parameter_required_only_by_an_unselected_variant_is_not_demanded() {
    let values = solved(
        "\
COMPONENT Choosy(in, out)
  PARAM model$ = linear, g
  VARIANT linear REQUIRE g
    out.sig = g * in.sig
  END
  VARIANT quadratic REQUIRE q
    out.sig = q * in.sig^2
  END
END
Choosy C(s1, s2, g = 4)
s1.sig = 2
y = s2.sig
",
    );
    assert!(close(values["y"], 8.0), "y = {}", values["y"]);
}

#[test]
fn a_model_selector_naming_no_variant_is_refused_and_lists_the_variants() {
    rejected(
        "\
Valve VA(s1, s2, Cv = 0.004, rho = 990, u = 1, model$ = teleport)
s1.P = 3e5
",
        "unknown model$ 'teleport'",
    );
}

/// The same on a *user* component, so the check is not a property of the
/// shipped library's spellings.
#[test]
fn a_user_component_model_selector_naming_no_variant_is_refused() {
    rejected(
        "\
COMPONENT Choosy(in, out)
  PARAM model$ = linear
  VARIANT linear
    out.sig = in.sig
  END
END
Choosy C(s1, s2, model$ = nonlinear)
s1.sig = 1
y = s2.sig
",
        "unknown model$ 'nonlinear'",
    );
}

/// `VARIANT` blocks with no `model$` selector at all — the component cannot say
/// which physics it means, and the expander must say so rather than picking
/// the first.
#[test]
fn variants_without_a_selector_parameter_are_refused() {
    rejected(
        "\
COMPONENT Choosy(in, out)
  VARIANT linear
    out.sig = in.sig
  END
  VARIANT quadratic
    out.sig = in.sig^2
  END
END
Choosy C(s1, s2)
s1.sig = 1
y = s2.sig
",
        "no 'param model$' selector",
    );
}

/// A `model$` given an arithmetic expression rather than a name. `string_token`
/// accepts a bare name or a quoted string and nothing else.
#[test]
fn a_model_selector_given_a_number_is_refused() {
    rejected(
        "\
Valve VA(s1, s2, Cv = 0.004, rho = 990, u = 1, model$ = 1 + 2)
s1.P = 3e5
",
        "must be a name or quoted string",
    );
}

/// `model$` is matched case-insensitively like every other frees identifier, so
/// `model$ = M` must select `VARIANT m`. A port that lowercased only one side
/// would refuse half the documents a user writes.
#[test]
fn a_model_selector_matches_its_variant_case_insensitively() {
    let values = solved(
        "\
COMPONENT C(a)
  PARAM model$ = m
  VARIANT m
    a.sig = 7
  END
END
C X(s1, model$ = M)
y = s1.sig
",
    );
    assert!(close(values["y"], 7.0), "y = {}", values["y"]);
}

/// An *empty* quoted `model$` is a name no variant has, and the refusal must
/// still list what is on offer.
#[test]
fn an_empty_model_selector_is_refused_and_still_lists_the_variants() {
    rejected(
        "\
COMPONENT C(a)
  PARAM model$ = m
  VARIANT m
    a.sig = 1
  END
END
C X(s1, model$ = '')
y = s1.sig
",
        "variants: m",
    );
}

// ── 5. length: a long chain ──────────────────────────────────────────────────

/// Two hundred components in series. This is the Tarjan/blocking path's width
/// test as much as the expander's: 200 instances, 201 streams, 200 blocks.
///
/// The physics is chosen so a wrong answer cannot hide: each stage halves, so
/// the terminal value is `2^-200 × 2^200 = 1`. A network that silently dropped
/// or duplicated a stage lands orders of magnitude away.
#[test]
fn a_two_hundred_component_chain_expands_solves_and_stays_exact() {
    const N: usize = 200;
    let mut src = String::new();
    for i in 0..N {
        src.push_str(&format!("SigGain G{i}(s{i}, s{}, k = 0.5)\n", i + 1));
    }
    src.push_str(&format!("s0.sig = 2^{N}\n"));
    src.push_str(&format!("y = s{N}.sig\n"));

    let started = Instant::now();
    let solution = solve(&src, &settings()).expect("a 200-stage signal chain solves");
    let elapsed = started.elapsed();

    let y = solution.values["y"];
    assert!(
        close(y, 1.0),
        "a {N}-stage halving chain fed 2^{N} produced y = {y}, expected 1"
    );
    assert!(
        solution.blocks.len() >= N,
        "a {N}-stage chain produced only {} blocks — stages were merged or lost",
        solution.blocks.len()
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "a {N}-stage chain took {elapsed:?}"
    );
    println!(
        "200-component chain: {} variables, {} blocks, {elapsed:?}",
        solution.values.len(),
        solution.blocks.len()
    );
}

/// The same length, wired with `connect(...)` instead of shared stream names,
/// so the union-find sees 200 nodes rather than none.
#[test]
fn a_two_hundred_component_connect_chain_terminates() {
    const N: usize = 200;
    let mut src = String::from("SigConstant SRC(k = 1)\n");
    for i in 0..N {
        src.push_str(&format!("SigGain G{i}(k = 1)\n"));
    }
    src.push_str("connect(SRC.out, G0.in)\n");
    for i in 1..N {
        src.push_str(&format!("connect(G{}.out, G{i}.in)\n", i - 1));
    }
    src.push_str(&format!("y = G{}.out.sig\n", N - 1));

    let started = Instant::now();
    let values = solved(&src);
    let elapsed = started.elapsed();
    assert!(
        close(values["y"], 1.0),
        "a {N}-stage connect chain produced y = {}",
        values["y"]
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "a {N}-stage connect chain took {elapsed:?}"
    );
}

/// A wrapper tower deeper than the hierarchy ceiling.
///
/// **This is the defect this file was written to find.** `flatten_instance`
/// recurses once per subsystem level, and the self-instantiation guard cannot
/// stop a *finite* tower because every level is a different name. Before
/// `MAX_HIERARCHY_DEPTH` existed, a 600-level document aborted a debug build
/// with `fatal runtime error: stack overflow` — a `SIGABRT`, not an `Err`, that
/// `catch_unwind` cannot convert and that would take the whole wasm module down
/// in a browser tab. Measured: 400 levels survived, 600 did not, on a 2 MiB
/// test-thread stack; the browser's is smaller.
///
/// The ceiling is 64 (the shipped library's deepest subsystem is 1), so this
/// asks for 1,600 and requires a named refusal.
#[test]
fn a_hierarchy_past_the_ceiling_is_an_error_not_a_stack_overflow() {
    const DEPTH: usize = 1600;
    let mut src = String::from("COMPONENT L0(a, b)\n  SigGain g(a, b, k = 2)\nEND\n");
    for level in 1..DEPTH {
        src.push_str(&format!(
            "COMPONENT L{level}(a, b)\n  L{} inner(a, b)\nEND\n",
            level - 1
        ));
    }
    src.push_str(&format!("L{} TOP(s1, s2)\n", DEPTH - 1));
    src.push_str("s1.sig = 1\ny = s2.sig\n");

    rejected(&src, "nested more than 64 subsystems deep");
}

/// The boundary, from both sides: 63 levels of wrapper flatten and solve, 65 do
/// not. A ceiling nobody tests at its edge is a ceiling that drifts.
#[test]
fn the_hierarchy_ceiling_holds_exactly_where_it_says() {
    fn tower(depth: usize) -> String {
        let mut src = String::from("COMPONENT L0(a, b)\n  SigGain g(a, b, k = 2)\nEND\n");
        for level in 1..depth {
            src.push_str(&format!(
                "COMPONENT L{level}(a, b)\n  L{} inner(a, b)\nEND\n",
                level - 1
            ));
        }
        src.push_str(&format!("L{} TOP(s1, s2)\n", depth - 1));
        src.push_str("s1.sig = 1\ny = s2.sig\n");
        src
    }
    // 63 wrappers over one leaf: 63 nested `flatten_instance` frames, one below
    // the ceiling.
    let values = solved(&tower(64));
    assert!(close(values["y"], 2.0), "y = {}", values["y"]);
    rejected(&tower(66), "nested more than 64 subsystems deep");
}

/// **Parameter substitution is exponential in hierarchy depth**, and this is the
/// regression test that says so out loud.
///
/// `ComponentExpander` substitutes a parameter's *expression* into every place
/// the body names it. A subsystem that passes `k = k + k` down to a child which
/// uses `k` twice therefore doubles the expression tree at every level: at depth
/// `n` the expanded equation has `Θ(2^n)` nodes. The Rust `Expr` is an owned
/// tree, so the substitution deep-clones and the cost is paid in full.
///
/// **Checked against the oracle** (`tools/golden-dumper/run.sh`, 2026-07-31), so
/// this is not a guess about what the reference does:
///
/// | depth | Java oracle | this port |
/// |---|---|---|
/// | 12 / 16 / 20 | solves, `y = 2^n` | solves, `y = 2^n`, 17 ms / 246 ms / 4.2 s |
/// | 24 | solves, `y = 16777216` | solves, same value, **65 s** |
/// | 28 | solves, `y = 268435456` | not attempted (projected ~17 min) |
/// | 32 | **`OutOfMemoryError: Java heap space`**, killing the process | not attempted |
///
/// So the port is *more* robust at the top end (Java dies; this returns an
/// answer eventually) and roughly an order of magnitude slower in the middle,
/// because Java's immutable AST nodes are shared by reference and become a DAG
/// where the Rust tree is materialised. Neither engine is usable past ~depth 24.
///
/// The test itself runs at depth 16 — deep enough that a regression to a worse
/// exponent (or to a copy in the *solver* rather than only the expander) blows
/// the budget, shallow enough to belong in a test suite.
#[test]
fn parameter_substitution_stays_within_its_measured_exponential() {
    const DEPTH: usize = 16;
    let mut src = String::from("COMPONENT L0(a, b)\n  PARAM k\n  b.sig = a.sig * (k + k)\nEND\n");
    for level in 1..DEPTH {
        src.push_str(&format!(
            "COMPONENT L{level}(a, b)\n  PARAM k\n  L{} inner(a, b, k = k + k)\nEND\n",
            level - 1
        ));
    }
    src.push_str(&format!("L{} TOP(s1, s2, k = 1)\n", DEPTH - 1));
    src.push_str("s1.sig = 1\ny = s2.sig\n");

    let started = Instant::now();
    let values = solved(&src);
    let elapsed = started.elapsed();
    let y = values["y"];
    assert!(
        close(y, 2f64.powi(DEPTH as i32)),
        "a depth-{DEPTH} doubling substitution gave y = {y}, expected 2^{DEPTH} \
         (the oracle says 65536)"
    );
    // Measured at ~250 ms in release. The budget is deliberately loose against a
    // slow machine but far below the next term of the series.
    assert!(
        elapsed < Duration::from_secs(10),
        "depth-{DEPTH} parameter substitution took {elapsed:?}; it was measured at \
         ~250 ms, and 10 s means the exponent got worse"
    );
    println!("parameter substitution at depth {DEPTH}: {elapsed:?}");
}

// ── 6. domain separation ─────────────────────────────────────────────────────

/// The hard rule from `../frEES/CLAUDE.md`: connector-domain separation is a
/// **parse error by design, not a warning**. A pneumatic (`gas`) line tied to a
/// hydraulic (`oil`) one shares the `(P, ṁ, h)` bond algebra, so nothing
/// downstream would notice — which is exactly why it is caught here.
#[test]
fn a_gas_line_connected_to_an_oil_line_is_a_hard_error() {
    rejected(
        "\
PneumaticAtmosphere PA(P = 1e5)
HydraulicTank HT(P = 2e7)
connect(PA.port, HT.port)
p = PA.port.P
",
        "cannot connect a 'gas' line",
    );
}

/// The same two connector types forced onto **one stream** by shared naming
/// rather than by a `connect` — the check that runs at instance-tagging time
/// instead of at node time.
#[test]
fn a_gas_and_an_oil_component_sharing_one_stream_are_refused() {
    rejected(
        "\
PneumaticAtmosphere PA(s1, P = 1e5)
HydraulicTank HT(s1, P = 2e7)
p = s1.P
",
        "incompatible fluid connector types on stream",
    );
}

/// The same contradiction expressed through a component's own reserved
/// `domain$` parameter rather than through two different library families.
#[test]
fn contradictory_domain_parameters_on_one_stream_are_refused() {
    rejected(
        "\
COMPONENT GasBit(port)
  PARAM domain$ = gas
  port.mdot = 0.01
END
COMPONENT OilBit(port)
  PARAM domain$ = oil
  port.P = 2e7
END
GasBit GB(s1)
OilBit OB(s1)
p = s1.P
",
        "incompatible fluid connector types",
    );
}

/// Cross-*domain* (not merely cross-connector-type): a signal wire tied to a
/// thermofluid port. The two have no members in common, so this is the
/// coarser of the two separations.
#[test]
fn a_signal_wire_connected_to_a_fluid_port_is_a_hard_error() {
    rejected(
        "\
Source SRC(fluid$ = Water, mdot = 1, P = 1e5, T = 300)
SigConstant SC(k = 1)
connect(SRC.out, SC.out)
p = SRC.out.P
",
        "different physical domains",
    );
}

/// An electrical pin tied to a mechanical flange — the classic modelling
/// mistake a transducer exists to prevent.
#[test]
fn an_electrical_pin_connected_to_a_mechanical_shaft_is_a_hard_error() {
    rejected(
        "\
VoltageSource VS(E = 10)
RotationalDamper RD(c = 0.1)
connect(VS.p, RD.a)
v = VS.p.V
",
        "different physical domains",
    );
}

/// A `domain$` naming a connector type that does not exist at all.
#[test]
fn an_unknown_domain_value_is_answered_cleanly() {
    let src = "\
COMPONENT Weird(port)
  PARAM domain$ = plasma
  port.mdot = 0.01
  port.P = 1e5
  port.h = 1e5
END
Weird W(s1)
p = s1.P
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    println!("domain$ = plasma → {}", outcome(src));
}

// ── 7. undefined locals and other body mistakes ──────────────────────────────

/// A component body that references a name which is neither a parameter, nor a
/// port member, nor anything it defines. The name becomes a component-local
/// unknown with no equation to fix it, so the system is underspecified — and
/// the diagnostic must reach the user as the *component's* local, not as the
/// mangled scalar `c$undefined_thing`.
#[test]
fn a_body_referencing_an_undefined_local_is_refused_and_names_it_readably() {
    let src = "\
COMPONENT Leaky(in, out)
  PARAM k
  out.sig = k * in.sig + undefined_thing
END
Leaky C(s1, s2, k = 1)
s1.sig = 1
y = s2.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    let message = outcome(src);
    assert_ne!(
        message, "Ok",
        "an undefined local left the system underspecified, but it solved anyway:\n{src}"
    );
    assert!(
        message.to_ascii_lowercase().contains("undefined_thing"),
        "the diagnostic must name the undefined local, got: {message}"
    );
    assert!(
        !message.contains("c$undefined_thing"),
        "the diagnostic leaked the mangled scalar name — the engine's contract is \
         component-named diagnostics: {message}"
    );
}

/// The same shape one layer in: an undefined local inside a *hierarchical*
/// subsystem, where the mangling is two levels deep.
#[test]
fn an_undefined_local_inside_a_subsystem_is_still_named_readably() {
    let src = "\
COMPONENT Inner(a, b)
  b.sig = a.sig + mystery
END
COMPONENT Outer(a, b)
  Inner i(a, b)
END
Outer O(s1, s2)
s1.sig = 1
y = s2.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    let message = outcome(src);
    assert_ne!(message, "Ok", "underspecified subsystem solved anyway");
    assert!(
        message.to_ascii_lowercase().contains("mystery"),
        "the diagnostic must name the undefined local, got: {message}"
    );
}

/// A `connect` naming an instance that was never declared.
#[test]
fn a_connect_naming_an_unknown_instance_is_refused_with_the_reference() {
    rejected(
        "\
SigConstant SRC(k = 1)
connect(SRC.out, GHOST.in)
y = SRC.out.sig
",
        "ghost.in",
    );
}

/// A `COMPONENT` that declares no ports at all, instantiated with one.
#[test]
fn a_portless_component_bound_to_a_stream_is_refused() {
    rejected(
        "\
COMPONENT Portless()
  PARAM k
  z = k
END
Portless P(s1, k = 1)
y = s1.sig
",
        "binds 1 port(s) but COMPONENT portless declares 0",
    );
}

/// Duplicate port names inside one `COMPONENT` declaration — `(a, a)`. The
/// second binding would silently shadow the first.
#[test]
fn a_component_declaring_the_same_port_twice_is_answered_cleanly() {
    let src = "\
COMPONENT Dup(a, a)
  a.sig = 1
END
Dup D(s1, s2)
y = s1.sig
";
    survives(src).unwrap_or_else(|e| panic!("{e}"));
    println!("duplicate port names → {}", outcome(src));
}

/// Two instances writing the same equation onto one shared stream. The
/// expansion has to *keep both*, so the result is over-determined rather than
/// quietly de-duplicated — the difference between "your model is wrong" and
/// "the engine hid your mistake".
#[test]
fn two_sources_on_one_stream_are_overspecified_not_silently_merged() {
    rejected(
        "\
SigConstant A(s1, k = 1)
SigConstant B(s1, k = 2)
y = s1.sig
",
        "overspecified",
    );
}

/// A component parameter whose value names another instance's output that is
/// itself unbound. The substitution must not resolve it to something plausible.
#[test]
fn a_parameter_referencing_a_dangling_instance_output_is_refused() {
    rejected(
        "\
SigGain G(s1, s2, k = G.out.sig)
s1.sig = 1
y = s2.sig
",
        "underspecified",
    );
}

/// A self-referential parameter default (`PARAM k = k`) must terminate. Naive
/// substitute-to-fixed-point would not.
#[test]
fn a_self_referential_parameter_default_terminates() {
    let src = "\
COMPONENT S(a)
  PARAM k = k
  a.sig = k
END
S X(s1)
y = s1.sig
";
    let elapsed = survives(src).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        elapsed < Duration::from_secs(5),
        "PARAM k = k took {elapsed:?} — substitution is chasing its own tail"
    );
    println!("PARAM k = k → {}", outcome(src));
}

/// Two parameters whose defaults name each other. Same property, two hops.
#[test]
fn mutually_referential_parameter_defaults_terminate() {
    let src = "\
COMPONENT S(a)
  PARAM k = j, j = k
  a.sig = k
END
S X(s1)
y = s1.sig
";
    let elapsed = survives(src).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        elapsed < Duration::from_secs(5),
        "mutually referential defaults took {elapsed:?}"
    );
}

/// A `COMPONENT` with 500 ports, all bound. Wide declarations are linear
/// elsewhere in the engine; the port-to-stream binding is an association list,
/// which is the one place a quadratic could hide.
#[test]
fn a_five_hundred_port_component_binds_in_bounded_time() {
    const N: usize = 500;
    let ports: Vec<String> = (0..N).map(|i| format!("p{i}")).collect();
    let mut src = format!("COMPONENT Wide({})\n", ports.join(", "));
    for i in 0..N {
        src.push_str(&format!("  p{i}.sig = {i}\n"));
    }
    src.push_str("END\n");
    let args: Vec<String> = (0..N).map(|i| format!("s{i}")).collect();
    src.push_str(&format!("Wide W({})\n", args.join(", ")));
    src.push_str(&format!("y = s{}.sig\n", N - 1));

    let started = Instant::now();
    let values = solved(&src);
    let elapsed = started.elapsed();
    assert!(
        close(values["y"], (N - 1) as f64),
        "port {} of a {N}-port component read {}",
        N - 1,
        values["y"]
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "a {N}-port component took {elapsed:?}"
    );
}

/// A non-ASCII component name. The lexer's alphabet is ASCII, so this is a
/// lexical error — the point is that it is *that*, with a quoted character, and
/// not a panic on a multi-byte boundary.
#[test]
fn a_non_ascii_component_name_is_a_lexical_error_not_a_byte_panic() {
    rejected(
        "\
COMPONENT Ünïcødé(a)
  a.sig = 1
END
Ünïcødé X(s1)
y = s1.sig
",
        "unexpected character",
    );
}

/// Four-thousand-character component and stream names. Nothing in the expander
/// may assume a name fits anywhere in particular.
#[test]
fn four_thousand_character_names_are_handled() {
    let long = "a".repeat(4000);
    let values = solved(&format!(
        "COMPONENT {long}(a)\n  a.sig = 1\nEND\n{long} X(s1)\ny = s1.sig\n"
    ));
    assert!(close(values["y"], 1.0));

    let stream = "s".repeat(4000);
    let values = solved(&format!(
        "SigConstant S({stream}, k = 1)\ny = {stream}.sig\n"
    ));
    assert!(close(values["y"], 1.0));
}

// ── 8. the standing sweep ────────────────────────────────────────────────────

/// Every hostile component document in one corpus, run through all three entry
/// points, with the "bounded and never non-finite" contract asserted over the
/// whole set.
///
/// The individual tests above assert *which* refusal each case gets. This one
/// asserts the property that has to hold for every one of them at once, and
/// adds the shapes that are only interesting in bulk: empty bodies, empty
/// parameter lists, deeply nested parentheses in a port argument, a component
/// name that is a keyword, and the whole matrix of arity mistakes against a
/// three-port built-in.
#[test]
fn the_whole_hostile_component_corpus_is_answered_bounded_and_finite() {
    let mut corpus: Vec<String> = Vec::new();

    // Arity, every count from 0 to 6 against a 3-port built-in.
    for n in 0..=6 {
        let args: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
        corpus.push(format!("Splitter SP({})\nf0.P = 1e5\n", args.join(", ")));
    }

    // Structural degeneracies.
    corpus.push("COMPONENT Empty()\nEND\nEmpty E\n".to_string());
    corpus.push("COMPONENT Empty(a)\nEND\nEmpty E(s1)\ny = s1.sig\n".to_string());
    corpus.push("COMPONENT P()\n  PARAM\nEND\nP X\n".to_string());
    corpus.push("COMPONENT V(a)\n  VARIANT\n  END\nEND\nV X(s1)\n".to_string());
    corpus.push(
        "COMPONENT R(a)\n  VARIANT m REQUIRE\n    a.sig = 1\n  END\nEND\nR X(s1)\n".to_string(),
    );

    // Names that collide with the language.
    for name in ["END", "PARAM", "VARIANT", "COMPONENT", "connect", "if"] {
        corpus.push(format!(
            "COMPONENT {name}(a)\n  a.sig = 1\nEND\n{name} X(s1)\ny = s1.sig\n"
        ));
    }

    // Connect arity, 0..4 endpoints, and repeated endpoints.
    corpus.push("SigConstant S(k = 1)\nconnect()\n".to_string());
    corpus.push("SigConstant S(k = 1)\nconnect(S.out)\n".to_string());
    corpus.push("SigConstant S(k = 1)\nconnect(S.out, S.out)\ny = S.out.sig\n".to_string());
    corpus.push(
        "SigConstant S(k = 1)\nSigGain G(k = 1)\nconnect(S.out, G.in, S.out, G.in)\n\
         y = G.out.sig\n"
            .to_string(),
    );

    // Malformed references inside a connect.
    for reference in [".", "..", "a.", ".b", "a.b.c.d.e", "", "S.", "S..out"] {
        corpus.push(format!(
            "SigConstant S(k = 1)\nSigGain G(k = 1)\nconnect(S.out, {reference})\n"
        ));
    }

    // A port argument that is not a plain name.
    for arg in ["1", "1 + 2", "(((s1)))", "'s1'", "s1.sig"] {
        corpus.push(format!("SigGain G({arg}, s2, k = 1)\ny = s2.sig\n"));
    }

    // Parameter values of every hostile shape.
    for value in ["0", "-1", "1e300", "1e-300", "0/0", "log(-1)", "sqrt(-1)"] {
        corpus.push(format!(
            "SigGain G(s1, s2, k = {value})\ns1.sig = 1\ny = s2.sig\n"
        ));
    }

    // String parameters given the wrong kind of thing.
    for value in ["1", "1 + 2", "'unquotedish", "\"\"", "''"] {
        corpus.push(format!(
            "Source SRC(s1, fluid$ = {value}, mdot = 1, P = 1e5, T = 300)\np = s1.P\n"
        ));
    }

    // Recursion and depth, in bulk.
    for depth in [1usize, 2, 3, 8, 32] {
        let mut src = String::from("COMPONENT L0(a, b)\n  SigGain g(a, b, k = 1)\nEND\n");
        for level in 1..=depth {
            src.push_str(&format!(
                "COMPONENT L{level}(a, b)\n  L{} inner(a, b)\nEND\n",
                level - 1
            ));
        }
        // Close the cycle at the top: the deepest level instantiates the outermost.
        src.push_str(&format!(
            "COMPONENT Lx(a, b)\n  L{depth} inner(a, b)\n  Lx again(a, b)\nEND\nLx T(s1, s2)\n\
             s1.sig = 1\ny = s2.sig\n"
        ));
        corpus.push(src);
    }

    let worst = all_survive(&corpus);
    println!(
        "hostile component corpus: {} documents, slowest {worst:?}",
        corpus.len()
    );
}

/// Every shipped built-in, instantiated with **no** parameters and **no** port
/// arguments at all.
///
/// This is the widest single sweep in the file: 295 documents, each one asking
/// the expander to resolve a definition whose every requirement is unmet. Most
/// must refuse (the library declares no defaults for physical inputs on
/// purpose); the ones that do not must still be finite and bounded. Either way
/// the failure mode under test is a panic on an unresolved parameter, an empty
/// port list, or an unselected variant.
#[test]
fn every_builtin_instantiated_with_nothing_supplied_is_answered_cleanly() {
    let library = frees_core::components::library::builtins().expect("library parses");
    let mut corpus: Vec<String> = Vec::new();
    for def in library.defs() {
        corpus.push(format!("{} X\np = 1\n", def.name));
        corpus.push(format!("{} X()\np = 1\n", def.name));
    }
    let worst = all_survive(&corpus);
    println!(
        "bare built-in instantiation: {} documents, slowest {worst:?}",
        corpus.len()
    );
}

/// Every shipped built-in against a `model$` that no component declares, and a
/// `fluid$` that no backend serves. Both are strings the user types, and both
/// reach the library by name.
#[test]
fn every_builtin_given_a_nonsense_string_parameter_is_answered_cleanly() {
    let library = frees_core::components::library::builtins().expect("library parses");
    let mut corpus: Vec<String> = Vec::new();
    for def in library.defs() {
        corpus.push(format!("{} X(model$ = no_such_variant)\np = 1\n", def.name));
        corpus.push(format!("{} X(fluid$ = Unobtainium)\np = 1\n", def.name));
    }
    let worst = all_survive(&corpus);
    println!(
        "nonsense string parameters: {} documents, slowest {worst:?}",
        corpus.len()
    );
}
