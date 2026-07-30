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
        let doc = match parse_document(&source) {
            Ok(doc) => doc,
            // Corpus entries that intentionally exercise unsupported grammar
            // are outside this pass's contract.
            Err(_) => continue,
        };
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
