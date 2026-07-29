//! Adversarial grammar-parity tests: real lexer + real parser, end to end.
//!
//! Every assertion here is derived from
//! `../frEES/backend/core/src/main/antlr/Frees.g4` and
//! `../frEES/backend/core/src/main/java/com/frees/backend/parser/AstBuilder.java`,
//! and is checked against `parse_document` — the *production* pipeline.
//!
//! That last point is the reason this file exists rather than more unit tests
//! inside `parser::expr`. The `#[cfg(test)]` module in `parser/expr.rs` drives
//! the grammar with a stand-in lexer of its own, and that stand-in resolves a
//! single quote MATLAB-style (a `'` right after an operand is a transpose).
//! `crate::lexer` deliberately does *not* — it reproduces ANTLR's maximal munch,
//! where `STRING_LITERAL` outbids `TRANSPOSE` whenever another quote follows.
//! Several expression tests therefore assert shapes the real pipeline never
//! produces (`a''`, `[a' b']`). The cases below pin down what actually happens.
//!
//! Where the port deliberately differs from the JVM oracle the test says so and
//! locks in the *chosen* behaviour, so a later "fix" cannot drift silently.

use frees_core::ast::{BinOp, Equation, Expr, Statement};
use frees_core::lexer::tokenize;
use frees_core::parser::{parse_document, Document};
use frees_core::token::TokenKind;

// ── helpers ─────────────────────────────────────────────────────────────────

fn doc(src: &str) -> Document {
    parse_document(src).unwrap_or_else(|e| panic!("expected `{src}` to parse, got {e}"))
}

fn fails(src: &str) -> String {
    match parse_document(src) {
        Ok(d) => panic!("expected `{src}` to fail, got {d:?}"),
        Err(e) => e.to_string(),
    }
}

/// The single statement of a one-statement document.
fn only(src: &str) -> Statement {
    let d = doc(src);
    assert_eq!(d.statements.len(), 1, "`{src}` should be one statement");
    d.statements.into_iter().next().unwrap()
}

fn equation(src: &str) -> Equation {
    match only(src) {
        Statement::Eq(e) => e,
        other => panic!("`{src}`: expected an equation, got {other:?}"),
    }
}

/// The right-hand side of `x = <expr>`, i.e. a way to parse a bare expression
/// through the production entry point.
fn rhs(expr_src: &str) -> Expr {
    equation(&format!("x = {expr_src}")).rhs
}

fn kinds(src: &str) -> Vec<TokenKind> {
    let mut t = tokenize(src).unwrap_or_else(|e| panic!("lexing `{src}` failed: {e}"));
    t.pop();
    t.into_iter().map(|t| t.kind).collect()
}

fn num(v: f64) -> Expr {
    Expr::num(v)
}
fn var(n: &str) -> Expr {
    Expr::var(n)
}
fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    Expr::bin(op, l, r)
}
fn neg(e: Expr) -> Expr {
    Expr::Neg(Box::new(e))
}
fn call(f: &str, a: Vec<Expr>) -> Expr {
    Expr::call(f, a)
}
fn arr(e: Vec<Expr>) -> Expr {
    Expr::ArrayLiteral(e)
}

/// Numeric value of a literal, for unit-conversion assertions.
fn literal(e: &Expr) -> (f64, Option<String>) {
    match e {
        Expr::Num { value, unit, .. } => (*value, unit.clone()),
        other => panic!("expected a numeric literal, got {other:?}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// 1. Lexer — ANTLR maximal munch
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn dot_family_resolves_by_longest_match() {
    // `..` DOTDOT, `.5` NUMBER, `.*` DOTSTAR, lone `.` DOT — Frees.g4 513-529.
    assert!(matches!(kinds("0..600")[1], TokenKind::DotDot));
    assert!(matches!(kinds("x = .5")[2], TokenKind::Number { .. }));
    assert!(matches!(kinds("a .* b")[1], TokenKind::DotStar));
    assert!(matches!(kinds("a.b")[1], TokenKind::Dot));
    // `..5` is DOTDOT + NUMBER, never DOT + `.5`: both start at the same
    // offset, and DOTDOT is the longer match there.
    assert!(matches!(kinds("..5")[0], TokenKind::DotDot));
    assert!(matches!(kinds("..5")[1], TokenKind::Number { .. }));
}

#[test]
fn a_fraction_needs_digits_after_the_dot() {
    // NUMBER : DIGIT+ ('.' DIGIT+)? — so `1.` is NUMBER + DOT, which is what
    // lets `1.x` be a member access on a numeric-looking token.
    assert_eq!(kinds("1.").len(), 2);
    assert!(matches!(kinds("1.e5")[1], TokenKind::Dot));
    assert!(matches!(kinds("1.e5")[2], TokenKind::Ident(_)));
}

#[test]
fn an_incomplete_exponent_backtracks_to_a_shorter_number() {
    // `3e` has no viable EXPONENT, so the shorter NUMBER match wins and `e`
    // becomes an identifier — exactly ANTLR's behaviour, and a parse error one
    // level up.
    assert!(matches!(kinds("3e")[0], TokenKind::Number { .. }));
    assert!(matches!(kinds("3e")[1], TokenKind::Ident(_)));
    assert!(fails("x = 3e").contains("expected end of statement"));
}

#[test]
fn keywords_are_case_insensitive_but_never_prefix_matched() {
    assert_eq!(kinds("FOR")[0], TokenKind::For);
    assert_eq!(kinds("for")[0], TokenKind::For);
    assert_eq!(kinds("FoR")[0], TokenKind::For);
    for word in ["format", "endpoint", "iffy", "notation", "orifice", "toe"] {
        assert!(
            matches!(kinds(word)[0], TokenKind::Ident(_)),
            "`{word}` must lex as an identifier"
        );
    }
}

#[test]
fn a_trailing_sigil_disqualifies_a_keyword() {
    // IDENT : [a-zA-Z][a-zA-Z0-9_]* ('$'|'#')? — the sigil is part of the token,
    // so `end$` can never be END.
    assert_eq!(kinds("end$"), vec![TokenKind::Ident("end$".into())]);
    assert_eq!(kinds("IF#"), vec![TokenKind::Ident("IF#".into())]);
    assert_eq!(equation("R$ = 'Water'").lhs, var("r$"));
}

#[test]
fn state_table_is_one_token_and_a_lone_state_is_not() {
    assert_eq!(kinds("STATE TABLE")[0], TokenKind::StateTable);
    assert_eq!(kinds("state\ttable")[0], TokenKind::StateTable);
    assert_eq!(kinds("state")[0], TokenKind::Ident("state".into()));
    // A line break does not join the two halves: STATETABLE separates on
    // `[ \t]+` only.
    assert_eq!(kinds("state\nTABLE")[0], TokenKind::Ident("state".into()));
    // ... so `state` stays usable as an ordinary variable.
    assert_eq!(equation("state = 1").lhs, var("state"));
}

#[test]
fn a_reserved_word_cannot_be_a_variable_in_either_engine() {
    // INPUT/OUTPUT/DO are lexer keywords in Frees.g4, so `output = 1` is a
    // syntax error for ANTLR too. The port reproduces the wart rather than
    // quietly widening the language.
    for src in ["output = 1", "input = 2", "do = 3", "then = 4"] {
        assert!(
            fails(src).contains("expected an expression"),
            "`{src}` should be rejected"
        );
    }
}

#[test]
fn the_three_comment_forms_are_skipped() {
    // BRACE_COMMENT, QUOTE_COMMENT and LINE_COMMENT, Frees.g4 610-620. Note
    // that `"` opens a *comment*, not a string.
    assert_eq!(equation("a {note} = 1 {more}").lhs, var("a"));
    assert_eq!(equation("a \"note\" = 1").lhs, var("a"));
    assert_eq!(doc("// whole line\nx = 1").statements.len(), 1);
    // A brace comment is non-greedy and does not nest: `{a {b}` is a *complete*
    // comment (were it nested it would be unterminated), and the `}` that would
    // have closed the outer one is then a stray character.
    assert_eq!(equation("x = 1 {a {b}").rhs, num(1.0));
    assert!(fails("x = 1 {a {b} }").contains("no `{` opened a comment here"));
    // A line comment leaves its newline behind, so it still separates.
    assert_eq!(doc("x = 1 // c\ny = 2").statements.len(), 2);
    // A multi-line brace comment swallows the break with it.
    assert!(fails("x = 1 {a\nb} y = 2").contains("expected end of statement"));
}

#[test]
fn quotes_inside_the_other_delimiter_are_content() {
    assert_eq!(rhs("'say \"hi\"'"), Expr::Str("say \"hi\"".into()));
    assert_eq!(equation("x \"it's fine\" = 2").lhs, var("x"));
}

#[test]
fn an_unterminated_comment_is_a_parse_error_not_a_silent_skip() {
    assert!(fails("x = 1 { never closed").contains("unterminated"));
    assert!(fails("x = 1 \" never closed").contains("unterminated"));
}

// ════════════════════════════════════════════════════════════════════════════
// 2. Operator precedence and associativity (Frees.g4 431-451)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn additive_and_multiplicative_levels_are_left_associative() {
    assert_eq!(
        rhs("a - b - c"),
        bin(BinOp::Sub, bin(BinOp::Sub, var("a"), var("b")), var("c"))
    );
    assert_eq!(
        rhs("a / b / c"),
        bin(BinOp::Div, bin(BinOp::Div, var("a"), var("b")), var("c"))
    );
    assert_eq!(
        rhs("a + b * c"),
        bin(BinOp::Add, var("a"), bin(BinOp::Mul, var("b"), var("c")))
    );
}

#[test]
fn backslash_is_a_multiplicative_left_division() {
    assert_eq!(
        rhs("A \\ b * c"),
        bin(
            BinOp::Mul,
            bin(BinOp::LeftDiv, var("a"), var("b")),
            var("c")
        )
    );
}

#[test]
fn element_wise_multiplicatives_share_the_mul_level_but_dotcaret_does_not() {
    // mulExpr lists .* ./ .\ ; DOTCARET lives in powExpr (Frees.g4 440/450).
    // That asymmetry is the grammar as written and must survive the port.
    assert_eq!(
        rhs("a .* b ./ c"),
        bin(
            BinOp::ElemDiv,
            bin(BinOp::ElemMul, var("a"), var("b")),
            var("c")
        )
    );
    assert_eq!(
        rhs("a .^ b * c"),
        bin(
            BinOp::Mul,
            bin(BinOp::ElemPow, var("a"), var("b")),
            var("c")
        )
    );
    assert_eq!(rhs("a .\\ b"), bin(BinOp::ElemLeftDiv, var("a"), var("b")));
}

#[test]
fn power_is_right_associative_and_outranks_unary_minus() {
    assert_eq!(
        rhs("2 ^ 3 ^ 2"),
        bin(BinOp::Pow, num(2.0), bin(BinOp::Pow, num(3.0), num(2.0)))
    );
    // powExpr's exponent is a unaryExpr, so `-2^2` is -(2^2) and `2^-3` needs
    // no parentheses.
    assert_eq!(rhs("-2 ^ 2"), neg(bin(BinOp::Pow, num(2.0), num(2.0))));
    assert_eq!(rhs("2 ^ -3"), bin(BinOp::Pow, num(2.0), neg(num(3.0))));
    assert_eq!(rhs("-1 .^ 2"), neg(bin(BinOp::ElemPow, num(1.0), num(2.0))));
}

#[test]
fn unary_plus_produces_no_node_and_unary_minus_nests() {
    assert_eq!(rhs("+a"), var("a"));
    assert_eq!(rhs("--a"), neg(neg(var("a"))));
    // unaryExpr is a mulExpr operand, so `-a * b` is (-a) * b.
    assert_eq!(rhs("-a * b"), bin(BinOp::Mul, neg(var("a")), var("b")));
}

// ════════════════════════════════════════════════════════════════════════════
// 3. The `-10 [C]` unary-sign fold (AstBuilder.bareUnitLiteral, 1205-1214)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn a_sign_folds_into_a_bare_offset_unit_literal() {
    // -10 °C is 263.15 K, never -(283.15 K).
    let (v, u) = literal(&rhs("-10 [C]"));
    assert!((v - 263.15).abs() < 1e-9, "{v}");
    assert_eq!(u.as_deref(), Some("K"));
    // No space needed between the number and the annotation.
    assert!((literal(&rhs("-10[C]")).0 - 263.15).abs() < 1e-9);
}

#[test]
fn the_fold_needs_a_bare_literal_exactly() {
    // An exponent, a transpose or a nested sign all take the ordinary Neg path
    // (bareUnitLiteral returns null), so the literal converts normally first.
    match rhs("-10 [C]^2") {
        Expr::Neg(inner) => match *inner {
            Expr::BinOp { op, left, .. } => {
                assert_eq!(op, BinOp::Pow);
                assert!((literal(&left).0 - 283.15).abs() < 1e-9);
            }
            other => panic!("expected a power under the Neg, got {other:?}"),
        },
        other => panic!("expected a Neg, got {other:?}"),
    }
    match rhs("--10 [C]") {
        // The inner sign still folds; the outer one is an ordinary negation.
        Expr::Neg(inner) => assert!((literal(&inner).0 - 263.15).abs() < 1e-9),
        other => panic!("expected a Neg, got {other:?}"),
    }
}

#[test]
fn only_offset_scales_fold_and_only_for_real_literals() {
    // A pure factor commutes with negation, so it keeps the Neg node.
    assert_eq!(
        rhs("-10 [kPa]"),
        neg(Expr::Num {
            value: 10_000.0,
            unit: Some("Pa".into()),
            is_imaginary: false
        })
    );
    // An unknown unit falls through to Neg as well (a bad unit never fails a
    // parse — it becomes a warning downstream).
    assert_eq!(
        rhs("-10 [flurbles]"),
        neg(Expr::Num {
            value: 10.0,
            unit: Some("flurbles".into()),
            is_imaginary: false
        })
    );
    // An empty annotation is no annotation at all (AstBuilder.unitText).
    assert_eq!(rhs("-10 []"), neg(num(10.0)));
    assert_eq!(rhs("-10"), neg(num(10.0)));
    // bareUnitLiteral matches NumberAtomContext only — never IMAG_NUMBER.
    match rhs("-2i [C]") {
        Expr::Neg(_) => {}
        other => panic!("an imaginary literal must keep the Neg path, got {other:?}"),
    }
}

#[test]
fn the_fold_reaches_the_operand_of_a_binary_minus_too() {
    // `a - -10 [C]` : the second operand is a unaryExpr, so the inner sign
    // folds and the subtraction sees 263.15 K. Faithful to visitUnaryExpr.
    match rhs("a - -10 [C]") {
        Expr::BinOp { op, right, .. } => {
            assert_eq!(op, BinOp::Sub);
            assert!((literal(&right).0 - 263.15).abs() < 1e-9);
        }
        other => panic!("expected a subtraction, got {other:?}"),
    }
    // A *binary* minus in front of a unit literal is never a fold.
    match rhs("3 [C] - 1 [C]") {
        Expr::BinOp { left, right, .. } => {
            assert!((literal(&left).0 - 276.15).abs() < 1e-9);
            assert!((literal(&right).0 - 274.15).abs() < 1e-9);
        }
        other => panic!("expected a subtraction, got {other:?}"),
    }
}

#[test]
fn the_fold_covers_the_literal_and_nothing_more() {
    match rhs("-10 [C] * 2") {
        Expr::BinOp { op, left, right } => {
            assert_eq!(op, BinOp::Mul);
            assert!((literal(&left).0 - 263.15).abs() < 1e-9);
            assert_eq!(*right, num(2.0));
        }
        other => panic!("expected a product, got {other:?}"),
    }
}

#[test]
fn a_bare_name_before_a_bracket_is_an_array_access_not_a_unit() {
    // `-b [C]` is ArrayAtom `b[c]` under a Neg: `unit?` only follows NUMBER /
    // IMAG_NUMBER / a matrix literal, and ArrayAtom outranks VarAtom.
    assert_eq!(
        rhs("-b [C]"),
        neg(Expr::ArrayAccess {
            name: "b".into(),
            indices: vec![var("c")]
        })
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 4. Transpose — and the STRING_LITERAL / TRANSPOSE collision
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn a_lone_quote_is_a_transpose_call() {
    assert_eq!(rhs("A'"), call("transpose", vec![var("a")]));
    assert_eq!(
        rhs("(A*x)'"),
        call("transpose", vec![bin(BinOp::Mul, var("a"), var("x"))])
    );
    assert_eq!(
        rhs("[1;2]'"),
        call(
            "transpose",
            vec![arr(vec![arr(vec![num(1.0)]), arr(vec![num(2.0)])])]
        )
    );
}

#[test]
fn transpose_binds_after_the_exponent_within_one_powexpr() {
    // powExpr : atom ((CARET|DOTCARET) unaryExpr)? TRANSPOSE* — the exponent is
    // itself a unaryExpr and grabs the ticks first, so `a ^ b'` transposes `b`.
    assert_eq!(
        rhs("a ^ b'"),
        bin(BinOp::Pow, var("a"), call("transpose", vec![var("b")]))
    );
    // Once TRANSPOSE* has run the rule is finished, so a following `^` has
    // nowhere to attach.
    assert!(fails("x = a' ^ b").contains("expected end of statement"));
}

#[test]
fn a_second_quote_on_the_same_line_lexes_as_a_string_like_antlr() {
    // THE WART, pinned down. STRING_LITERAL : '\'' (~'\'')* '\'' outbids
    // TRANSPOSE by maximal munch whenever another quote follows anywhere later
    // in the document, so `A' + B'` is IDENT + STRING(" + B"). The port
    // reproduces the JVM oracle instead of silently improving on it.
    assert_eq!(
        kinds("A' + B'"),
        vec![
            TokenKind::Ident("A".into()),
            TokenKind::StringLiteral(" + B".into())
        ]
    );
    // Consequences at the parser level: a doubled tick is an empty string, not
    // two transposes (unlike the stand-in lexer in `parser::expr`'s own tests).
    assert_eq!(
        kinds("a''"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::StringLiteral("".into())
        ]
    );
    assert!(fails("b = a''").contains("expected end of statement"));
    // A single transpose per line, with no other quote, is unambiguous — which
    // is why `C = A' * B` still works.
    assert_eq!(
        equation("C = A' * B").rhs,
        bin(BinOp::Mul, call("transpose", vec![var("a")]), var("b"))
    );
}

#[test]
fn a_string_literal_has_no_escape_for_its_own_quote() {
    // `'it''s'` is two adjacent literals, because `~'\''` cannot cover a quote.
    assert_eq!(
        kinds("'it''s'"),
        vec![
            TokenKind::StringLiteral("it".into()),
            TokenKind::StringLiteral("s".into())
        ]
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 5. Matrix literals — row and element separation (Frees.g4 462-468)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn rows_are_semicolon_separated_and_always_wrapped() {
    assert_eq!(
        rhs("[1, 2; 3, 4]"),
        arr(vec![
            arr(vec![num(1.0), num(2.0)]),
            arr(vec![num(3.0), num(4.0)])
        ])
    );
    // visitMatrixLiteralAtom wraps even a single row.
    assert_eq!(rhs("[1, 2]"), arr(vec![arr(vec![num(1.0), num(2.0)])]));
}

#[test]
fn commas_between_elements_are_optional() {
    // matrixRow : expr (COMMA? expr)*
    assert_eq!(rhs("[1 2 3]"), rhs("[1, 2, 3]"));
    assert_eq!(rhs("[1 2; 3 4]"), rhs("[1, 2; 3, 4]"));
}

#[test]
fn juxtaposition_never_splits_a_greedy_expression() {
    // `[1 -2]` is ONE element: addExpr's `((PLUS|MINUS) mulExpr)*` loop is
    // greedy, and ANTLR resolves the ambiguity in favour of staying in it.
    assert_eq!(
        rhs("[1 -2]"),
        arr(vec![arr(vec![bin(BinOp::Sub, num(1.0), num(2.0))])])
    );
    // With the comma it is two elements.
    assert_eq!(
        rhs("[1, -2]"),
        arr(vec![arr(vec![num(1.0), neg(num(2.0))])])
    );
}

#[test]
fn a_newline_inside_a_matrix_literal_is_not_a_row_separator() {
    // Frees.g4 has no `sep` inside matrixRow and NEWLINE is a real token, so
    // this is a syntax error for ANTLR too — `;` is the only row separator.
    assert!(fails("x = [1 2\n3 4]").contains("expected `]`"));
    assert!(fails("x = [1,\n 2]").contains("expected an expression"));
}

#[test]
fn a_trailing_unit_reaches_every_numeric_leaf_without_rescaling() {
    // applyUnitToElements attaches the *raw* unit text and does not convert —
    // deliberately unlike `NUMBER unit?`. Reproduced so the two engines agree.
    assert_eq!(
        rhs("[1, 2] [kg]"),
        arr(vec![arr(vec![
            Expr::Num {
                value: 1.0,
                unit: Some("kg".into()),
                is_imaginary: false
            },
            Expr::Num {
                value: 2.0,
                unit: Some("kg".into()),
                is_imaginary: false
            },
        ])])
    );
    // Even an offset scale stays unconverted here (1 [C], not 274.15 K).
    let (v, u) = match rhs("[1] [C]") {
        Expr::ArrayLiteral(rows) => match &rows[0] {
            Expr::ArrayLiteral(cells) => literal(&cells[0]),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    assert_eq!((v, u.as_deref()), (1.0, Some("C")));
    // A symbolic element cannot carry one.
    assert!(fails("x = [a, b] [kg]").contains("requires numeric elements"));
}

#[test]
fn matrix_literals_nest_and_the_grammar_has_no_empty_literal() {
    assert_eq!(
        rhs("[[1, 2]]"),
        arr(vec![arr(vec![arr(vec![arr(vec![num(1.0), num(2.0)])])])])
    );
    // matrixRow : expr (...)*  — at least one expr, so `[]` in value position
    // is a syntax error (it is only ever a *unit* annotation).
    assert!(fails("x = []").contains("expected an expression"));
    assert!(fails("x = [1,2,]").contains("expected an expression"));
}

// ════════════════════════════════════════════════════════════════════════════
// 6. Named arguments, property calls, chemistry tokens (AstBuilder 1409-1517)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn named_arguments_encode_a_property_call() {
    assert_eq!(
        rhs("Enthalpy(R134a, T=T1, x=1)"),
        call("prop$enthalpy$r134a$t$x", vec![var("t1"), num(1.0)])
    );
    // The fluid and the indicator labels must not become solver unknowns.
    assert_eq!(
        rhs("Enthalpy(R134a, T=T1, x=1)")
            .variables()
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["t1"]
    );
    // Bare name, quoted literal and string variable are all accepted spellings.
    assert_eq!(
        rhs("Enthalpy('R134a', T=300)"),
        call("prop$enthalpy$r134a$t", vec![num(300.0)])
    );
    assert_eq!(
        rhs("Enthalpy(R$, T=300)"),
        call("prop$enthalpy$r$$t", vec![num(300.0)])
    );
}

#[test]
fn the_fluid_comes_first_and_the_rest_must_be_named() {
    assert!(fails("x = Enthalpy(T=300, x=1)").contains("take the fluid name first"));
    assert!(fails("x = Enthalpy(1+2, T=300)").contains("Invalid fluid name '1+2'"));
    assert!(fails("x = Enthalpy(R134a, T=300, 5)").contains("Property indicators must be named"));
    // Named arguments are a fluid-property feature only.
    assert!(fails("x = If(a=1, 2, 3)").contains("only valid in fluid property functions"));
}

#[test]
fn a_named_argument_needs_ident_then_eq_and_nothing_else() {
    // `arg : IDENT EQ expr` — a dotted name is not an argument name, so this
    // falls through to PositionalArg (the MemberAtom `a.b`) and the `=` then
    // has nowhere to go.
    assert!(fails("x = f(a.b = c)").contains("expected `)`"));
}

#[test]
fn chemistry_calls_preserve_token_case_outside_the_function_name() {
    assert_eq!(
        rhs("MolarMass(C8H18)"),
        call("prop$molarmass", vec![Expr::Str("C8H18".into())])
    );
    assert_eq!(
        rhs("HeatingValue(CH4, 'LHV')"),
        call(
            "prop$heatingvalue",
            vec![Expr::Str("CH4".into()), Expr::Str("LHV".into())]
        )
    );
    assert!(rhs("MolarMass(C8H18)").variables().is_empty());
    // A formula with parentheses has to be quoted.
    assert_eq!(
        rhs("MolarMass('Ca(OH)2')"),
        call("prop$molarmass", vec![Expr::Str("Ca(OH)2".into())])
    );
    assert!(fails("x = MolarMass('2H2O')").contains("Invalid token '2H2O'"));
    // A quoted token keeps its interior spacing, so it is rejected rather than
    // silently glued into `StainlessSteel`.
    assert!(fails("x = k_('Stainless Steel')").contains("Invalid token 'Stainless Steel'"));
}

#[test]
fn a_comment_inside_an_argument_is_invisible_to_the_argument_text() {
    // REGRESSION. ANTLR's `ctx.getText()` walks the parse *tree*, so every
    // `-> skip` rule — whitespace and all three comment forms — is absent from
    // it. Slicing the raw source instead glued the comment into the token and
    // rejected documents the JVM oracle accepts.
    assert_eq!(
        rhs("Enthalpy(R134a {refrigerant}, T=300)"),
        call("prop$enthalpy$r134a$t", vec![num(300.0)])
    );
    assert_eq!(
        rhs("MolarMass(C8H18 {octane})"),
        call("prop$molarmass", vec![Expr::Str("C8H18".into())])
    );
    assert_eq!(
        rhs("Enthalpy(R134a \"a note\", T=300)"),
        call("prop$enthalpy$r134a$t", vec![num(300.0)])
    );
    // ... and the unit-name arguments of Convert take the same path.
    assert_eq!(literal(&rhs("Convert(kJ {energy}, J)")).0, 1000.0);
}

#[test]
fn convert_and_converttemp_fold_at_parse_time() {
    assert_eq!(literal(&rhs("Convert('ft^2','in^2')")).0, 144.0);
    assert_eq!(
        rhs("ConvertTemp(C,K,25)"),
        Expr::Num {
            value: 298.15,
            unit: Some("K".into()),
            is_imaginary: false
        }
    );
    // A non-literal argument becomes an affine expression; a unit scale of 1
    // emits no multiply node.
    assert_eq!(
        rhs("ConvertTemp(C, K, T)"),
        bin(BinOp::Add, var("t"), num(273.15))
    );
    assert_eq!(rhs("ConvertTemp(K, K, T)"), var("t"));
    assert!(fails("x = ConvertTemp(X, K, 25)").contains("unknown temperature scale 'X'"));
    assert!(fails("x = Convert(kJ, m)").contains("different dimensions"));
}

#[test]
fn the_if_intrinsic_is_the_five_argument_call_form() {
    assert_eq!(
        rhs("If(time, t_burn, F0, F0, 0)"),
        call(
            "if",
            vec![var("time"), var("t_burn"), var("f0"), var("f0"), num(0.0)]
        )
    );
}

#[test]
fn arglist_has_no_empty_alternative() {
    // `argList : arg (COMMA arg)*` — unlike `callArgList`, which does.
    assert!(fails("x = f()").contains("expected an expression"));
}

// ════════════════════════════════════════════════════════════════════════════
// 7. Array indices, ranges, member access (Frees.g4 481-489, AstBuilder 1320)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn an_index_colon_becomes_a_range_node() {
    assert_eq!(
        rhs("A[1:3]"),
        Expr::ArrayAccess {
            name: "a".into(),
            indices: vec![Expr::Range {
                start: Box::new(num(1.0)),
                end: Box::new(num(3.0))
            }]
        }
    );
    // arrayIndex : expr (COLON expr)? — one colon per index, and each index in
    // the list is independent.
    match rhs("A[1:3, 2:4]") {
        Expr::ArrayAccess { indices, .. } => assert_eq!(indices.len(), 2),
        other => panic!("{other:?}"),
    }
    assert!(fails("x = A[1:3:5]").contains("expected `]`"));
    // Bounds may be arbitrary expressions.
    assert_eq!(
        rhs("A[i+1 : 2*n]"),
        Expr::ArrayAccess {
            name: "a".into(),
            indices: vec![Expr::Range {
                start: Box::new(bin(BinOp::Add, var("i"), num(1.0))),
                end: Box::new(bin(BinOp::Mul, num(2.0), var("n")))
            }]
        }
    );
}

#[test]
fn an_array_access_cannot_be_chained_or_carry_a_unit() {
    // ArrayAtom is a leaf: `A[1][2]` and `A[1] [kg]` are both syntax errors,
    // because `unit?` follows only NUMBER / IMAG_NUMBER / a matrix literal.
    assert!(fails("y = A[1:3][2]").contains("expected end of statement"));
    assert!(fails("x = a[1] [kg]").contains("expected end of statement"));
    assert!(fails("x = 1 [kg] [m]").contains("expected end of statement"));
}

#[test]
fn a_dotted_path_is_one_variable_carried_whole() {
    // visitMemberAtom joins the segments and Expr.Var lowercases the result.
    assert_eq!(rhs("in.P"), Expr::Var("in.p".into()));
    assert_eq!(rhs("HP.out.h"), Expr::Var("hp.out.h".into()));
    assert_eq!(
        rhs("HP.out.h").variables().into_iter().collect::<Vec<_>>(),
        vec!["hp.out.h"]
    );
    assert!(fails("x = in.").contains("expected an identifier"));
}

#[test]
fn identifiers_and_call_names_are_lowercased_but_array_names_too() {
    assert_eq!(rhs("T_in"), Expr::Var("t_in".into()));
    assert_eq!(rhs("Sin(X)"), call("sin", vec![var("x")]));
    assert_eq!(
        rhs("Speed[N]"),
        Expr::ArrayAccess {
            name: "speed".into(),
            indices: vec![var("n")]
        }
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 8. Unit annotations (Frees.g4 491-497, AstBuilder.visitNumberAtom)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn a_unit_annotated_literal_converts_to_si_at_parse_time() {
    assert_eq!(
        rhs("140 [kPa]"),
        Expr::Num {
            value: 140_000.0,
            unit: Some("Pa".into()),
            is_imaginary: false
        }
    );
    let (v, u) = literal(&rhs("25 [C]"));
    assert!((v - 298.15).abs() < 1e-9, "{v}");
    assert_eq!(u.as_deref(), Some("K"));
    // An imaginary literal converts the same way.
    match rhs("2i [kPa]") {
        Expr::Num {
            value,
            unit,
            is_imaginary,
        } => {
            assert!((value - 2000.0).abs() < 1e-9);
            assert_eq!(unit.as_deref(), Some("Pa"));
            assert!(is_imaginary);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unitcontent_admits_exactly_the_tokens_the_grammar_lists() {
    // IDENT NUMBER * / ^ - + , ( ) — and nothing else.
    for src in [
        "1 [kJ/kg-K]",
        "1 [m^2]",
        "1 [1/s]",
        "1 [kg/(m-s)]",
        "1 [kg,m]",
    ] {
        let _ = rhs(src);
    }
    // A `;` is not unitContent, so it terminates the annotation as an error.
    assert!(fails("x = 1 [kPa;]").contains("unit expression"));
    // An unterminated annotation reports the same way.
    assert!(fails("x = 1 [kPa").contains("unit expression"));
}

#[test]
fn an_unknown_unit_never_fails_a_parse() {
    assert_eq!(
        rhs("5 [flurbles]"),
        Expr::Num {
            value: 5.0,
            unit: Some("flurbles".into()),
            is_imaginary: false
        }
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 9. Statement-level disambiguation (Frees.g4 348-397)
// ════════════════════════════════════════════════════════════════════════════

fn call_proc(s: &Statement) -> (&str, &Vec<Expr>, &Vec<Expr>) {
    match s {
        Statement::CallProc {
            name,
            inputs,
            outputs,
            ..
        } => (name, inputs, outputs),
        other => panic!("expected a CallProc, got {other:?}"),
    }
}

#[test]
fn multiassign_wins_over_a_matrix_literal_equation() {
    let s = only("[a, b] = f(x)");
    let (name, inputs, outputs) = call_proc(&s);
    assert_eq!(name, "f");
    assert_eq!(*inputs, vec![var("x")]);
    assert_eq!(*outputs, vec![var("a"), var("b")]);
    // `callArgList` may be empty, unlike an expression `argList`.
    assert!(call_proc(&only("[a, b] = f()")).1.is_empty());
}

#[test]
fn a_tilde_output_binds_an_unforgeable_sink() {
    let s = only("[~, ~, V] = svd(A)");
    let (_, _, outputs) = call_proc(&s);
    assert_eq!(outputs.len(), 3);
    for slot in &outputs[..2] {
        match slot {
            // A leading `~` cannot occur in a user identifier (IDENT starts
            // [a-zA-Z]), so these names are unforgeable.
            Expr::Var(n) => assert!(n.starts_with('~'), "{n}"),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(outputs[2], var("v"));
    // Distinct slots get distinct sinks.
    assert_ne!(outputs[0], outputs[1]);
}

#[test]
fn a_multiassign_shape_that_is_not_a_whole_statement_is_an_equation() {
    // `[a, b] = f(x) + 1` cannot be a multiAssign — the call is not the whole
    // statement — so ANTLR falls through to `equation`, and so must the port.
    let eq = equation("[a, b] = f(x) + 1");
    assert_eq!(eq.lhs, arr(vec![arr(vec![var("a"), var("b")])]));
    // Juxtaposed outputs are not `callOutputs` (which needs commas) either.
    assert_eq!(
        equation("[a b] = f(x)").lhs,
        arr(vec![arr(vec![var("a"), var("b")])])
    );
    // ... nor is a semicolon-separated bracket.
    assert_eq!(
        equation("[a; b] = [1; 2]").lhs,
        arr(vec![arr(vec![var("a")]), arr(vec![var("b")])])
    );
}

#[test]
fn a_multiassign_may_sit_directly_in_front_of_end() {
    // REGRESSION. `forBlock : … statementList sep? END` — the separator before
    // END is optional, so `[a, b] = f(x) END` is a well-formed multiAssign for
    // ANTLR. Treating only `;`/newline/EOF as a statement terminator demoted it
    // to the matrix-literal equation `[[a, b]] = f(x)`, which binds nothing.
    let body = match only("FOR i = 1 TO 2\n[a, b] = f(x) END") {
        Statement::For { body, .. } => body,
        other => panic!("{other:?}"),
    };
    assert_eq!(body.len(), 1);
    let (name, _, outputs) = call_proc(&body[0]);
    assert_eq!(name, "f");
    assert_eq!(*outputs, vec![var("a"), var("b")]);
    // The same statement with an explicit separator was always fine; both
    // spellings must now agree.
    let with_sep = match only("FOR i = 1 TO 2\n[a, b] = f(x)\nEND") {
        Statement::For { body, .. } => body,
        other => panic!("{other:?}"),
    };
    assert_eq!(body, with_sep);
}

#[test]
fn rangeassign_wins_over_an_equation_on_ident_eq_number_colon() {
    // DEVIATION: the Java builder materialises the elements at parse time into
    // `x[1:N] = [v1, …]`; the port keeps the range symbolic as a `range`
    // intrinsic and leaves expansion to the array layer. The *validation* below
    // is the Java validation, verbatim.
    let eq = equation("x = 0:10:100");
    assert_eq!(eq.lhs, var("x"));
    assert_eq!(
        eq.rhs,
        call(
            "range",
            vec![num(0.0), num(10.0), num(100.0), Expr::Str("linear".into())]
        )
    );
    // The two-number form implies step 1 and spacing Linear.
    assert_eq!(
        equation("x = 1:5").rhs,
        call(
            "range",
            vec![num(1.0), num(1.0), num(5.0), Expr::Str("linear".into())]
        )
    );
    // `signedNumber` allows a sign on any of the three.
    assert_eq!(
        equation("x = -1:1:1").rhs,
        call(
            "range",
            vec![num(-1.0), num(1.0), num(1.0), Expr::Str("linear".into())]
        )
    );
    // A `| flag` selects the spacing, case-insensitively.
    assert_eq!(
        equation("x = 1:3:100 | Log").rhs,
        call(
            "range",
            vec![num(1.0), num(3.0), num(100.0), Expr::Str("log".into())]
        )
    );
}

#[test]
fn range_validation_matches_astbuilder() {
    assert!(fails("x = 1:0:5").contains("Range step is zero"));
    assert!(fails("x = 5:1:1").contains("points the wrong way"));
    assert!(fails("x = 0:1e-9:1").contains("Use a larger step"));
    assert!(fails("x = 1:2 | Foo").contains("Unknown range spacing 'Foo'"));
    assert!(fails("x = 1:5 | Log").contains("needs start:count:stop"));
    assert!(fails("x = 0:10:100 | Log").contains("needs positive bounds"));
    assert!(fails("x = 1:1:100 | Log").contains("at least 2"));
    // `rangeAssign` needs literal numbers, so `1:N` is a syntax error for both
    // engines (it is not an equation either — the colon has nowhere to go).
    assert!(fails("x = 1:N").contains("expected a number"));
}

#[test]
fn a_range_bound_that_cannot_be_counted_is_refused_not_a_panic() {
    // REGRESSION. `(stop - start) / step` overflows `i64` for a plausible typo
    // (`x = 0:1:1e30`) and is `inf` when the step underflows. Saturating the
    // cast and then adding 1 panics in a debug build and wraps to a *negative*
    // element count in a release one — sailing straight past the ceiling check.
    for src in [
        "x = 0:1:1e30",
        "x = 0:1:1e400",
        "x = 0:1e-320:1",
        "x = -1e308:1:1e308",
        "x = 1:1e400:1e400",
    ] {
        let message = fails(src);
        assert!(
            message.contains("elements"),
            "`{src}` should be refused by the element ceiling, got: {message}"
        );
    }
}

#[test]
fn a_range_shape_that_is_not_ident_eq_number_colon_stays_an_equation() {
    assert!(fails("x[1] = 1:3").contains("expected end of statement"));
    assert_eq!(equation("x = 5").rhs, num(5.0));
}

#[test]
fn call_statements_split_inputs_from_outputs_on_the_colon() {
    let s = only("CALL split(x, y : p, q)");
    let (name, inputs, outputs) = call_proc(&s);
    assert_eq!(name, "split");
    assert_eq!(*inputs, vec![var("x"), var("y")]);
    assert_eq!(*outputs, vec![var("p"), var("q")]);
    // `callArgList` is optional on both sides.
    assert!(call_proc(&only("CALL f( : y)")).1.is_empty());
    assert!(call_proc(&only("CALL f(x : )")).2.is_empty());
}

#[test]
fn symbolic_declares_a_list_of_lowercased_names() {
    assert_eq!(
        only("SYMBOLIC S, t"),
        Statement::Symbolic(vec!["s".into(), "t".into()])
    );
}

#[test]
fn for_blocks_nest_and_tolerate_an_omitted_separator_before_end() {
    let d = doc("FOR i = 1 TO 2\n  FOR j = 1 TO 3\n    a[i] = j\n  END\n  b[i] = i\nEND");
    assert_eq!(d.equations().len(), 2, "equations() flattens both levels");
    // `sep?` before END.
    match only("FOR i = 1 TO 2\nq = 1 END") {
        Statement::For { body, .. } => assert_eq!(body.len(), 1),
        other => panic!("{other:?}"),
    }
    assert!(fails("FOR i = 1 TO 2\nq = 1").contains("unterminated FOR"));
    assert!(fails("END").contains("unexpected `END`"));
}

#[test]
fn guess_directives_leave_the_statement_list_alone() {
    let d = doc("x = 1\nGUESS x = 2 [0, 10]\ny = 2");
    assert_eq!(d.statements.len(), 2);
    assert_eq!(d.guesses.len(), 1);
    assert_eq!(d.guesses[0].name, "x");
    assert_eq!(d.guesses[0].guess, Some(2.0));
    assert_eq!(
        (d.guesses[0].lower, d.guesses[0].upper),
        (Some(0.0), Some(10.0))
    );
    // Bounds-only and guess-only forms.
    assert_eq!(doc("GUESS x [0, 10]").guesses[0].guess, None);
    assert_eq!(doc("GUESS x = 2").guesses[0].lower, None);
    // The three AstBuilder rejections.
    assert!(fails("GUESS x").contains("declare a guess"));
    assert!(fails("GUESS x [10, 0]").contains("lower bound must be below"));
    assert!(fails("GUESS x = 20 [0, 10]").contains("lies outside"));
}

#[test]
fn component_instantiation_is_detected_rather_than_mis_reported() {
    // `componentInst : IDENT IDENT LPAREN` — nothing in the expression grammar
    // makes that shape, so it is named instead of producing a confusing
    // expression error.
    assert!(fails("Pump P1(s3, s4)").contains("COMPONENT instantiation"));
    for (src, construct) in [
        ("FUNCTION f(x)\nEND", "FUNCTION"),
        ("TABLE cp(T)\nEND", "TABLE"),
        ("STATE TABLE C(P)\nEND", "STATE TABLE"),
        ("DYNAMIC d()\nEND", "DYNAMIC"),
        ("connect(a.b, c)", "CONNECT"),
    ] {
        assert!(fails(src).contains(construct), "`{src}`");
    }
}

// ════════════════════════════════════════════════════════════════════════════
// 10. Program structure and separators (Frees.g4 3-9)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn separators_are_newlines_and_semicolons_and_runs_collapse() {
    assert_eq!(doc("a = 1\nb = 2; c = 3\n\n;\nd = 4").statements.len(), 4);
    assert_eq!(doc("\n\n;  x = 1  ;\n\n").statements.len(), 1);
    assert_eq!(doc("x=1;;y=2").statements.len(), 2);
    assert!(fails("a = 1 b = 2").contains("expected end of statement"));
}

#[test]
fn trivia_only_documents_are_empty_not_errors() {
    for src in [
        "", "   \t  ", "\n\n\n", "{ c }\n", "// c\n", "\"c\"\n", ";;;",
    ] {
        let d = doc(src);
        assert!(d.statements.is_empty() && d.guesses.is_empty(), "`{src}`");
    }
}

#[test]
fn an_equation_source_text_is_verbatim_not_re_rendered() {
    // DEVIATION: the Java builder stores `ctx.getText()`, which drops the
    // user's spacing. Keeping the slice verbatim is what lets a diagnostic
    // quote the line the user actually wrote.
    assert_eq!(
        equation("x   =  1 +    2 { why }").source_text,
        "x   =  1 +    2"
    );
}

#[test]
fn errors_are_source_mapped() {
    let e = parse_document("a = 1\nb = 2\nc = @").unwrap_err();
    let span = e.span().expect("parse errors carry a span");
    assert_eq!(span.line_col("a = 1\nb = 2\nc = @"), (3, 5));
}

// ════════════════════════════════════════════════════════════════════════════
// 11. Known, deliberate departures from the JVM oracle
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn deviation_newlines_are_tolerated_inside_bracketed_argument_lists() {
    // NEWLINE is a real token in Frees.g4 (not `-> skip`), and neither
    // `callOutputs` nor `callArgList` admits a `sep`, so ANTLR *rejects* both
    // of these. The port accepts them — a strict superset, never a different
    // answer. Pinned so the leniency stays deliberate and visible.
    assert_eq!(call_proc(&only("[a,\n b] = f(x)")).2.len(), 2);
    assert_eq!(call_proc(&only("CALL f(x,\n y : z)")).1.len(), 2);
    // The leniency is NOT extended to expression argument lists, which stay
    // strict — an inconsistency worth knowing about.
    assert!(fails("x = f(a,\n b)").contains("expected an expression"));
}

#[test]
fn deviation_builtin_constants_are_not_folded_at_parse_time() {
    // AstBuilder.visitVarAtom substitutes `pi#`/`R#`/`g#` for numeric literals
    // while parsing. The port keeps them as variables and binds them as knowns
    // in the engine instead (see `engine::builtin_constants`), so the parsed
    // tree lists them where Java's would not.
    assert_eq!(rhs("pi#"), Expr::Var("pi#".into()));
    assert_eq!(
        rhs("a + pi#").variables().into_iter().collect::<Vec<_>>(),
        vec!["a", "pi#"]
    );
}

#[test]
fn deviation_a_byte_order_mark_is_tolerated() {
    // U+FEFF matches no lexer rule in Frees.g4; Windows-authored documents
    // carry one anyway.
    assert_eq!(doc("\u{feff}x = 1").statements.len(), 1);
}
