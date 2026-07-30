//! Integration tests for the matrix expansion pass (`parser::expand`).
//!
//! Three layers:
//!
//! 1. **Scalar byte-identity** — every document in the golden corpus, plus a
//!    property test over pseudo-randomly generated scalar documents, must pass
//!    `expand_document` through *equal* to `Document::equations()` (the golden
//!    corpus freezes the scalar pipeline's behaviour).
//! 2. **Oracle mirroring** — the `solvesMatlab*` documents from the Java
//!    `EquationSystemSolverTest` expand into systems that are exactly
//!    satisfied by the oracle's published solutions.
//! 3. **Structure** — element naming, internal-temp prefixes, and equation
//!    counts for the array-language surface.
//!
//! (`frees_core::eval` used below is the engine's numeric evaluator over the
//! parsed, typed `Expr` AST — a safe expression evaluator, no code execution.)

use std::path::{Path, PathBuf};

use frees_core::ast::Equation;
use frees_core::eval;
use frees_core::parser::expand::{expand_document, is_internal_temp};
use frees_core::parser::parse_document;
use frees_core::procedures::flatten_calls;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus")
}

fn expand(source: &str) -> Vec<Equation> {
    expand_document(&parse_document(source).expect("parse")).expect("expand")
}

/// Assert `solution` (plus every `Var = Num` literal the expansion emitted)
/// zeroes the residual of every expanded equation.
fn assert_satisfied(eqs: &[Equation], solution: &[(&str, f64)]) {
    let mut scope: eval::Scope = eqs
        .iter()
        .filter_map(|eq| match (&eq.lhs, &eq.rhs) {
            (frees_core::ast::Expr::Var(name), frees_core::ast::Expr::Num { value, .. }) => {
                Some((name.clone(), *value))
            }
            _ => None,
        })
        .collect();
    for (name, value) in solution {
        scope.insert((*name).to_string(), *value);
    }
    for eq in eqs {
        let l = eval::eval(&eq.lhs, &scope)
            .unwrap_or_else(|e| panic!("lhs of `{}` did not evaluate: {e:?}", eq.source_text));
        let r = eval::eval(&eq.rhs, &scope)
            .unwrap_or_else(|e| panic!("rhs of `{}` did not evaluate: {e:?}", eq.source_text));
        assert!(
            (l - r).abs() < 1e-9,
            "`{}` not satisfied: {l} vs {r}",
            eq.source_text
        );
    }
}

// ---------------------------------------------------------------------------
// 1. Scalar byte-identity
// ---------------------------------------------------------------------------

/// True when `e` mentions any array/matrix syntax — the expander's whole
/// remit. A document free of these must come out of `expand_document`
/// untouched; a document containing one is expected to be rewritten and is
/// covered by the oracle-mirroring tests below instead.
fn mentions_array_syntax(e: &frees_core::ast::Expr) -> bool {
    use frees_core::ast::Expr;
    match e {
        Expr::ArrayLiteral(_) | Expr::ArrayAccess { .. } | Expr::Range { .. } => true,
        Expr::Num { .. } | Expr::Str(_) | Expr::Var(_) => false,
        Expr::Neg(inner) | Expr::Not(inner) => mentions_array_syntax(inner),
        // The elementwise operators are matrix-only spellings, so the expander
        // rewrites them even when neither side looks array-valued.
        Expr::BinOp { op, left, right } => {
            op.is_element_wise() || mentions_array_syntax(left) || mentions_array_syntax(right)
        }
        Expr::Compare { left, right, .. } => {
            mentions_array_syntax(left) || mentions_array_syntax(right)
        }
        Expr::Logical { left, right, .. } => {
            mentions_array_syntax(left) || mentions_array_syntax(right)
        }
        Expr::Call { function, args } => {
            MATRIX_VALUED_CALLS.contains(&function.as_str())
                || args.iter().any(mentions_array_syntax)
        }
    }
}

/// Calls whose *result* is matrix-valued, so `x = f(…)` is a bare matrix
/// creation even though no bracket appears in the source.
const MATRIX_VALUED_CALLS: &[&str] = &[
    "eye",
    "zeros",
    "ones",
    "diag",
    "linspace",
    "inverse",
    "inv",
    "transpose",
    "solvelinear",
    "cross",
    "det",
    "determinant",
    "dot",
    "norm",
    "nrm2",
    "asum",
    "trace",
    "matrixnorm",
    "fronorm",
];

fn document_is_scalar(doc: &frees_core::parser::Document) -> bool {
    doc.equations()
        .iter()
        .all(|eq| !mentions_array_syntax(&eq.lhs) && !mentions_array_syntax(&eq.rhs))
}

#[test]
fn every_corpus_document_passes_through_byte_identical() {
    let dir = corpus_dir();
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("frees") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read corpus doc");
        let mut doc = match parse_document(&source) {
            Ok(doc) => doc,
            // Corpus entries that intentionally exercise unsupported grammar
            // are outside this pass's contract.
            Err(_) => continue,
        };
        // Pipeline stage 2 runs before expansion: `expand_document` documents
        // that PROCEDURE/MODULE calls must already be flattened, so a corpus
        // document containing a CALL has to come through `flatten_calls`
        // first — otherwise this test would assert against an order the engine
        // never uses. Byte-identity is then asserted against the *flattened*
        // statements, which is the input the expander actually receives.
        let statements = std::mem::take(&mut doc.statements);
        doc.statements = match flatten_calls(statements, &doc.defs) {
            Ok(flat) => flat,
            // CALLs of intrinsics this port has not reached (control suite,
            // FFT, …) are refused at stage 2; not this pass's contract.
            Err(_) => continue,
        };
        // Documents that genuinely carry matrix content are the expander's job;
        // this pass only freezes the *scalar* pipeline.
        if !document_is_scalar(&doc) {
            continue;
        }
        let expanded = expand_document(&doc)
            .unwrap_or_else(|e| panic!("{} failed to expand: {e:?}", path.display()));
        let original: Vec<Equation> = doc.equations().into_iter().cloned().collect();
        assert_eq!(
            expanded,
            original,
            "{} did not pass through byte-identical",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 10, "only {checked} corpus documents checked");
}

/// A tiny deterministic LCG so the property test needs no dependencies.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        // Numerical Recipes constants.
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}

/// Generate a random scalar document: plain variables, arithmetic, elementary
/// intrinsics — nothing matrix-valued.
fn random_scalar_document(rng: &mut Lcg) -> String {
    let vars = ["x", "y", "z", "alpha", "t_out"];
    let funcs = ["sqrt", "abs", "sin", "exp"];
    let ops = ["+", "-", "*", "/", "^"];
    let mut lines = Vec::new();
    let count = 2 + (rng.next() % 4) as usize;
    for i in 0..count {
        let lhs = vars[i % vars.len()];
        let a = *rng.pick(&vars);
        let op = *rng.pick(&ops);
        let n = (rng.next() % 90) + 1;
        let rhs = match rng.next() % 3 {
            0 => format!("{n} {op} {a}"),
            1 => format!("{}({a}) {op} {n}", rng.pick(&funcs)),
            _ => format!("{a} {op} {}", *rng.pick(&vars)),
        };
        lines.push(format!("{lhs} = {rhs}"));
    }
    lines.join("\n")
}

#[test]
fn random_scalar_documents_pass_through_byte_identical() {
    let mut rng = Lcg(0x5eed_cafe_f00d_beef);
    for round in 0..300 {
        let source = random_scalar_document(&mut rng);
        let doc = parse_document(&source)
            .unwrap_or_else(|e| panic!("round {round}: {source:?} failed to parse: {e:?}"));
        let expanded = expand_document(&doc)
            .unwrap_or_else(|e| panic!("round {round}: {source:?} failed to expand: {e:?}"));
        let original: Vec<Equation> = doc.equations().into_iter().cloned().collect();
        assert_eq!(expanded, original, "round {round}: {source:?}");
    }
}

// ---------------------------------------------------------------------------
// 2. The solvesMatlab* oracle documents
// ---------------------------------------------------------------------------

#[test]
fn oracle_bare_name_matrix_solvelinear() {
    // solvesMatlabStyleBareNameMatrix: x = [3, 2].
    let eqs = expand("A = [2 0; 0 4]\nb = [6; 8]\nx = SolveLinear(A, b)");
    assert_satisfied(&eqs, &[("x[1]", 3.0), ("x[2]", 2.0)]);
}

#[test]
fn oracle_inverse_and_matvec() {
    // solvesMatlabStyleBareNameInverseAndMatVec.
    let inv = expand("A = [4 0; 0 5]\nC = Inverse(A)");
    assert_satisfied(
        &inv,
        &[
            ("c[1,1]", 0.25),
            ("c[1,2]", 0.0),
            ("c[2,1]", 0.0),
            ("c[2,2]", 0.2),
        ],
    );

    let mv = expand("A = [1 2; 3 4]\nx = [5; 6]\ny = A * x");
    assert_satisfied(&mv, &[("y[1]", 17.0), ("y[2]", 39.0)]);
}

#[test]
fn oracle_matrix_generators() {
    // solvesMatlabMatrixGenerators: literal values keyed by flattened names.
    let eqs = expand(
        "I = eye(3)\nZ = zeros(2,2)\nu = ones(3,1)\nD = diag([2; 5; 7])\ng = linspace(0, 10, 5)",
    );
    assert_satisfied(
        &eqs,
        &[
            ("i[1,1]", 1.0),
            ("i[1,2]", 0.0),
            ("z[2,1]", 0.0),
            ("u[3]", 1.0),
            ("d[2,2]", 5.0),
            ("d[1,2]", 0.0),
            ("g[2]", 2.5),
            ("g[5]", 10.0),
        ],
    );
}

#[test]
fn oracle_inv_det_aliases() {
    // solvesMatlabInvDetAliases: C = inv(A), d = det(A) = 20.
    let eqs = expand("A = [4 0; 0 5]\nC = inv(A)\nd = det(A)");
    assert_satisfied(
        &eqs,
        &[
            ("c[1,1]", 0.25),
            ("c[1,2]", 0.0),
            ("c[2,1]", 0.0),
            ("c[2,2]", 0.2),
            ("d", 20.0),
        ],
    );
}

#[test]
fn oracle_range_assign_slice_with_unit() {
    // unitOnMatrixLiteralAndRangeAssign (solve half): c[1:3] = [2, 3, 4] [kg].
    let eqs = expand("c[1:3] = [2, 3, 4] [kg]");
    assert_satisfied(&eqs, &[("c[1]", 2.0), ("c[2]", 3.0), ("c[3]", 4.0)]);
}

// ---------------------------------------------------------------------------
// 3. Structure
// ---------------------------------------------------------------------------

#[test]
fn library_temporaries_use_the_java_prefixes_and_are_filterable() {
    let eqs = expand("A = [4 0; 0 5]\nb = [8; 10]\nx = Inverse(A) * b");
    let temps: std::collections::BTreeSet<String> = eqs
        .iter()
        .flat_map(|eq| eq.variables())
        .filter(|name| is_internal_temp(name))
        .collect();
    assert!(
        temps.iter().all(|name| name.starts_with("inverse_temp_6[")),
        "unexpected temp names: {temps:?}"
    );
    assert_eq!(temps.len(), 4);
    // The user-facing unknowns remain x[1], x[2] — the filter never touches them.
    assert!(!is_internal_temp("x[1]"));
}

#[test]
fn element_variables_use_flattened_lowercase_names() {
    let eqs = expand("Speed = 0:50:100\nKE[1,1] = 3");
    let names: std::collections::BTreeSet<String> =
        eqs.iter().flat_map(|eq| eq.variables()).collect();
    assert!(names.contains("speed[1]"));
    assert!(names.contains("speed[3]"));
    assert!(names.contains("ke[1,1]"));
}

#[test]
fn matrix_products_chain_through_registered_shapes() {
    // C = A * B, then a scalar consumes an element of C — the whole chain is
    // scalar equations over flattened names.
    let eqs = expand("A = [1 2; 3 4]\nB = [5 6; 7 8]\nC = A * B\ntotal = C[1,1] + C[2,2]");
    assert_satisfied(
        &eqs,
        &[
            ("c[1,1]", 19.0),
            ("c[1,2]", 22.0),
            ("c[2,1]", 43.0),
            ("c[2,2]", 50.0),
            ("total", 69.0),
        ],
    );
}

#[test]
fn expansion_is_deterministic() {
    let source = "A = [4 0; 0 5]\nb = [8; 10]\nx = Inverse(A) * b\ny = A \\ b";
    let first = expand(source);
    let second = expand(source);
    assert_eq!(first, second);
}

// ---------------------------------------------------------------------------
// 4. The stage-2 / stage-3 CALL seam
// ---------------------------------------------------------------------------

/// The Java flattens PROCEDURE/MODULE calls and the matrix-intrinsic CALLs in
/// one pass (`EquationParser.flattenCallProc`); this port splits that across
/// `procedures::flatten_calls` (stage 2) and `parser::expand` (stage 3). A CALL
/// whose flattener lives in stage 3 therefore has to survive stage 2 untouched
/// — refusing it by name there left `flatten_lu_decompose` and
/// `flatten_interp2` unreachable from user text, which is what
/// `procedures::EXPANDED_CALL_TARGETS` now prevents.
fn pipeline(source: &str) -> Vec<Equation> {
    let mut doc = parse_document(source).expect("parse");
    let statements = std::mem::take(&mut doc.statements);
    doc.statements = flatten_calls(statements, &doc.defs).expect("stage 2: flatten CALLs");
    expand_document(&doc).expect("stage 3: expand")
}

#[test]
fn call_ludecompose_survives_stage_two_and_flattens_in_stage_three() {
    // Java: L = [1 0; 1.5 1], U = [4 3; 0 -1.5] for A = [4 3; 6 3].
    let eqs =
        pipeline("A[1:2,1:2] = [4 3; 6 3]\nCALL LUDecompose(A[1:2,1:2] : L[1:2,1:2], U[1:2,1:2])");
    assert_satisfied(
        &eqs,
        &[
            ("l[1,1]", 1.0),
            ("l[1,2]", 0.0),
            ("l[2,1]", 1.5),
            ("l[2,2]", 1.0),
            ("u[1,1]", 4.0),
            ("u[1,2]", 3.0),
            ("u[2,1]", 0.0),
            ("u[2,2]", -1.5),
        ],
    );
}

#[test]
fn call_interp2_lowers_to_the_interp2_synthetic() {
    let eqs =
        pipeline("x = [0, 1]\ny = [0, 1]\nZ = [0, 1; 2, 3]\nCALL Interp2(x, y, Z, 0.5, 0.5 : zc)");
    let call = eqs
        .iter()
        .find(|eq| matches!(&eq.lhs, frees_core::ast::Expr::Var(v) if v == "zc"))
        .expect("an equation binding zc");
    match &call.rhs {
        frees_core::ast::Expr::Call { function, args } => {
            assert_eq!(function, "interp2$2$2");
            // m x-nodes, n y-nodes, m*n grid entries row-major, xq, yq.
            assert_eq!(args.len(), 2 + 2 + 4 + 2);
        }
        other => panic!("expected an interp2$ call, got {other:?}"),
    }
    // Bilinear centre of the four corners.
    assert_satisfied(&eqs, &[("zc", 1.5)]);
}
