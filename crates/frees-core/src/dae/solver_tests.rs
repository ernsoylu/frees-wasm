//! Tests for [`crate::dae::solver`], graded against the **real** SUNDIALS IDA.
//!
//! Every `ORACLE_*` constant below was produced by `tools/dae-probe/run.sh`,
//! which drives the Java `IdaDaeSolver` (JNA → libsundials_ida 6.4.1) over these
//! exact problems with these exact tolerances and writes
//! `fixtures/dae-oracle.json`. Re-run it after touching the integrator; do not
//! adjust a constant to make a test pass.

use super::*;
use crate::dae::assembly::{ClosureResidual, ClosureRootFn};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Largest relative difference, falling back to absolute near zero.
fn rel(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    let scale = a.abs().max(b.abs());
    if scale < 1e-12 {
        d
    } else {
        d / scale
    }
}

fn assert_close(got: &[f64], want: &[f64], tol: f64, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for i in 0..got.len() {
        let r = rel(got[i], want[i]);
        assert!(
            r <= tol,
            "{what}[{i}]: got {} want {} (rel {:.3e} > {:.3e})",
            got[i],
            want[i],
            r,
            tol
        );
    }
}

/// Runs a problem the way `DynamicSolver.solveWithIda` does and returns the
/// sampled `(t, y)` rows plus every root that fired.
#[allow(clippy::type_complexity)]
fn drive(
    n: usize,
    res: &dyn DaeResidual,
    id: &[f64],
    y0: &[f64],
    yp0: &[f64],
    times: &[f64],
    rtol: f64,
    atol: f64,
    root: Option<(usize, &dyn DaeRootFn)>,
) -> (
    Vec<(f64, Vec<f64>)>,
    Vec<(f64, Vec<i32>, Vec<f64>)>,
    Vec<f64>,
) {
    let mut s = IdaDaeSolver::new(n, res).unwrap();
    s.set_tolerances(rtol, atol);
    s.set_variable_id(id).unwrap();
    if let Some((nroots, rf)) = root {
        s.set_roots(nroots, rf);
    }
    s.init(times[0], y0, yp0).unwrap();
    let span = times[times.len() - 1] - times[0];
    if s.calc_consistent_ic(IDA_YA_YDP_INIT, times[0] + span * 1e-3)
        .is_err()
    {
        s.reinit(times[0], y0, yp0).unwrap();
    }
    let ic = s.current_state();
    let mut rows = vec![(times[0], s.current_state())];
    let mut roots = Vec::new();
    for &tout in &times[1..] {
        let mut step = s.step(tout).unwrap();
        while step.root_return() {
            roots.push((step.t, step.roots_found.clone(), step.y.clone()));
            step = s.step(tout).unwrap();
        }
        rows.push((step.t, step.y));
    }
    (rows, roots, ic)
}

// ---------------------------------------------------------------------------
// dense and sparse linear algebra
// ---------------------------------------------------------------------------

#[test]
fn dense_lu_solves_and_reports_singularity() {
    let a = vec![
        vec![4.0, 1.0, 0.0],
        vec![1.0, 3.0, 1.0],
        vec![0.0, 1.0, 2.0],
    ];
    let lu = dense_lu_factor(a).unwrap();
    let mut b = vec![1.0, 2.0, 3.0];
    dense_lu_solve(&lu, &mut b);
    // The same system the KLU oracle solved.
    assert_close(&b, &ORACLE_SPARSE_X, 1e-14, "dense LU");

    let singular = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
    assert!(dense_lu_factor(singular).is_none());
}

/// Ground truth: `sparse_solve.x` from the probe (KLU, through SparseSteadyKlu).
const ORACLE_SPARSE_X: [f64; 3] = [0.22222222222222224, 0.11111111111111101, 1.4444444444444446];

#[test]
fn sparse_steady_matches_the_klu_oracle() {
    // A = [[4,1,0],[1,3,1],[0,1,2]] declared column-wise, exactly as the Java
    // probe declares it.
    let pattern = vec![vec![0, 1], vec![0, 1, 2], vec![1, 2]];
    let mut klu = SparseSteady::create(&pattern).unwrap();
    assert_eq!(klu.nonzeros(), 7, "nnz matches the oracle");
    let values = [4.0, 1.0, 1.0, 3.0, 1.0, 1.0, 2.0];
    let x = klu.solve(&values, &[1.0, 2.0, 3.0]).unwrap();
    assert_close(&x, &ORACLE_SPARSE_X, 1e-14, "sparse steady solve");
}

#[test]
fn sparse_steady_refuses_an_empty_pattern_like_the_java() {
    assert!(SparseSteady::create(&[]).is_none());
    assert!(SparseSteady::create(&[vec![], vec![]]).is_none());
}

#[test]
fn sparse_steady_returns_none_on_a_singular_matrix() {
    let pattern = vec![vec![0, 1], vec![0, 1]];
    let mut klu = SparseSteady::create(&pattern).unwrap();
    // [[1,2],[2,4]] in CSC order.
    assert!(klu.solve(&[1.0, 2.0, 2.0, 4.0], &[1.0, 1.0]).is_none());
}

#[test]
fn sparse_lu_agrees_with_dense_lu_on_a_pivoting_case() {
    // A matrix whose natural diagonal is a poor pivot, so the row permutation
    // is actually exercised.
    let dense = vec![
        vec![0.0, 2.0, 1.0, 0.0],
        vec![1.0, 0.0, 0.0, 3.0],
        vec![0.0, 1.0, 4.0, 0.0],
        vec![2.0, 0.0, 0.0, 1.0],
    ];
    let cols: Vec<Vec<usize>> = (0..4)
        .map(|c| (0..4).filter(|&r| dense[r][c] != 0.0).collect())
        .collect();
    let mut csc = SparseCsc::from_columns(&cols).unwrap();
    let mut values = Vec::new();
    for (c, rows) in cols.iter().enumerate() {
        for &r in rows {
            values.push(dense[r][c]);
        }
    }
    csc.set_values(&values).unwrap();
    let b = [1.0, -2.0, 3.0, 0.5];
    let x_sparse = SparseLu::factor(&csc).unwrap().solve(&b);

    let lu = dense_lu_factor(dense.clone()).unwrap();
    let mut x_dense = b.to_vec();
    dense_lu_solve(&lu, &mut x_dense);
    assert_close(&x_sparse, &x_dense, 1e-12, "sparse vs dense LU");

    // And it really solves A x = b.
    for r in 0..4 {
        let ax: f64 = (0..4).map(|c| dense[r][c] * x_sparse[c]).sum();
        assert!((ax - b[r]).abs() < 1e-12, "row {r}: {ax} vs {}", b[r]);
    }
}

#[test]
fn sparse_lu_handles_a_dense_ish_fill_pattern() {
    // Tridiagonal + a full last row/column: the worst fill this ordering sees.
    let n = 12;
    let mut dense = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        dense[i][i] = 4.0 + i as f64;
        if i > 0 {
            dense[i][i - 1] = -1.0;
        }
        if i + 1 < n {
            dense[i][i + 1] = -1.5;
        }
        dense[n - 1][i] += 0.25;
        dense[i][n - 1] += 0.125;
    }
    let cols: Vec<Vec<usize>> = (0..n)
        .map(|c| (0..n).filter(|&r| dense[r][c] != 0.0).collect())
        .collect();
    let mut csc = SparseCsc::from_columns(&cols).unwrap();
    let mut values = Vec::new();
    for (c, rows) in cols.iter().enumerate() {
        for &r in rows {
            values.push(dense[r][c]);
        }
    }
    csc.set_values(&values).unwrap();
    let b: Vec<f64> = (0..n).map(|i| (i as f64 * 0.7).sin()).collect();
    let x = SparseLu::factor(&csc).unwrap().solve(&b);
    for r in 0..n {
        let ax: f64 = (0..n).map(|c| dense[r][c] * x[c]).sum();
        assert!((ax - b[r]).abs() < 1e-10, "row {r}");
    }
}

// ---------------------------------------------------------------------------
// trajectories — graded against SUNDIALS IDA 6.4.1
// ---------------------------------------------------------------------------

/// `dT/dt = k (Tinf - T)`, k = 0.05, Tinf = 20, T(0) = 95, rtol 1e-6/atol 1e-8.
const ORACLE_COOLING: [(f64, f64); 4] = [
    (0.0, 95.0),
    (20.0, 47.59091037331522),
    (40.0, 30.15013670134929),
    (60.0, 23.734015089281844),
];

#[test]
fn newton_cooling_matches_ida() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let times: Vec<f64> = ORACLE_COOLING.iter().map(|(t, _)| *t).collect();
    let (rows, _, ic) = drive(1, &res, &[1.0], &[95.0], &[0.0], &times, 1e-6, 1e-8, None);
    // IDACalcIC recovers y' = k(Tinf - T) = -3.75 from a zero guess.
    assert_eq!(ic, vec![95.0]);
    for (i, (t, y)) in rows.iter().enumerate() {
        assert_eq!(*t, ORACLE_COOLING[i].0);
        let r = rel(y[0], ORACLE_COOLING[i].1);
        assert!(
            r <= 1e-6,
            "t={t}: got {} want {} (rel {r:.3e})",
            y[0],
            ORACLE_COOLING[i].1
        );
    }
    // And it is the physics: T(t) = 20 + 75 e^{-kt}.
    for (t, y) in &rows {
        let exact = 20.0 + 75.0 * (-0.05 * t).exp();
        assert!(rel(y[0], exact) < 1e-6, "t={t}: {} vs exact {exact}", y[0]);
    }
}

/// Semi-explicit index-1: `y0' + y0 - y1 = 0`, `y1 - 2 y0 = 0` → `y0 = e^t`.
/// rtol 1e-8 / atol 1e-10.
const ORACLE_INDEX1: [(f64, [f64; 2]); 5] = [
    (0.0, [1.0, 2.0]),
    (0.5, [1.6487213535798277, 3.2974427071596546]),
    (1.0, [2.718282007918873, 5.436564015837748]),
    (1.5, [4.48168944384321, 8.96337888768642]),
    (2.0, [7.3890568406675845, 14.778113681335173]),
];

#[test]
fn index1_semi_explicit_matches_ida_including_the_consistent_ic() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0] - y[1];
        r[1] = y[1] - 2.0 * y[0];
        Ok(())
    });
    let times: Vec<f64> = ORACLE_INDEX1.iter().map(|(t, _)| *t).collect();
    let (rows, _, ic) = drive(
        2,
        &res,
        &[1.0, 0.0],
        &[1.0, 0.0],
        &[0.0, 0.0],
        &times,
        1e-8,
        1e-10,
        None,
    );
    // The algebraic component starts at a deliberately wrong 0; IDACalcIC must
    // find y1 = 2 y0 = 2 before the first step. This is the test that would
    // fail if consistent initialization were skipped.
    assert_close(&ic, &[1.0, 2.0], 1e-12, "consistent IC");
    for (i, (_t, y)) in rows.iter().enumerate() {
        assert_close(y, &ORACLE_INDEX1[i].1, 5e-7, "index1");
    }
    // Exact solution.
    for (t, y) in &rows {
        assert!(rel(y[0], t.exp()) < 1e-7, "t={t}: {} vs e^t", y[0]);
    }
}

/// Robertson's stiff kinetics DAE — SUNDIALS' own `idaRoberts` problem.
/// rtol 1e-8 / atol 1e-10, y0 = [1,0,0], id = [1,1,0].
const ORACLE_ROBERTSON: [(f64, [f64; 3]); 7] = [
    (0.0, [1.0, 0.0, 0.0]),
    (
        0.4,
        [
            0.9851721143767498,
            3.3863962556530464e-5,
            0.014794021655778885,
        ],
    ),
    (
        4.0,
        [
            0.9055186822333788,
            2.2404757211174165e-5,
            0.09445891300942291,
        ],
    ),
    (
        40.0,
        [
            0.7158270791419529,
            9.185535184615667e-6,
            0.28416373532286265,
        ],
    ),
    (
        400.0,
        [
            0.45051868386928773,
            3.222901628629768e-6,
            0.5494780932291937,
        ],
    ),
    (
        4000.0,
        [0.1832022790939602, 8.942362024302864e-7, 0.816796826669819],
    ),
    (
        40000.0,
        [
            0.03898337771535254,
            1.621760293984465e-7,
            0.9610164601086613,
        ],
    ),
];

fn robertson() -> ClosureResidual<'static> {
    ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + 0.04 * y[0] - 1.0e4 * y[1] * y[2];
        r[1] = yp[1] - 0.04 * y[0] + 1.0e4 * y[1] * y[2] + 3.0e7 * y[1] * y[1];
        r[2] = y[0] + y[1] + y[2] - 1.0;
        Ok(())
    })
}

#[test]
fn robertson_matches_ida_over_six_decades() {
    let res = robertson();
    let times: Vec<f64> = ORACLE_ROBERTSON.iter().map(|(t, _)| *t).collect();
    let (rows, _, _) = drive(
        3,
        &res,
        &[1.0, 1.0, 0.0],
        &[1.0, 0.0, 0.0],
        &[-0.04, 0.04, 0.0],
        &times,
        1e-8,
        1e-10,
        None,
    );
    for (i, (t, y)) in rows.iter().enumerate() {
        // y[1] is ~1e-5 and rides on cancellation between two 1e4/3e7 terms;
        // it is the component the tolerance is loosest on in any BDF code.
        assert_close(y, &ORACLE_ROBERTSON[i].1, 2e-4, &format!("robertson t={t}"));
        // The algebraic constraint is exact by construction, at every sample.
        let sum: f64 = y.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "mass balance at t={t}: {sum}");
    }
}

/// The two switching functions the IDA example roots on: `y0 - 0.8` and
/// `y2 - 1e-4`. Oracle root times and directions.
const ORACLE_ROOT_TIMES: [(f64, [i32; 2]); 2] = [
    (0.0034128414806206815, [0, 1]),
    (16.422179941819973, [-1, 0]),
];

#[test]
fn robertson_root_finding_matches_ida() {
    let res = robertson();
    let root = ClosureRootFn::new(|_t, y, _yp, g: &mut [f64]| {
        g[0] = y[0] - 0.8;
        g[1] = y[2] - 1.0e-4;
        Ok(())
    });
    let times = [0.0, 0.4, 4.0, 40.0, 400.0];
    let (rows, roots, _) = drive(
        3,
        &res,
        &[1.0, 1.0, 0.0],
        &[1.0, 0.0, 0.0],
        &[-0.04, 0.04, 0.0],
        &times,
        1e-8,
        1e-10,
        Some((2, &root)),
    );
    assert_eq!(roots.len(), 2, "two roots fire, as in the IDA example");
    for (i, (t, found, _y)) in roots.iter().enumerate() {
        let (want_t, want_dir) = ORACLE_ROOT_TIMES[i];
        assert_eq!(found.as_slice(), &want_dir, "root {i} direction");
        assert!(
            rel(*t, want_t) < 1e-6,
            "root {i} time: got {t} want {want_t}"
        );
    }
    // Integration continues past a non-stop root and still lands on the oracle.
    for (i, (_t, y)) in rows.iter().enumerate() {
        assert_close(y, &ORACLE_ROBERTSON[i].1, 2e-4, "robertson with roots");
    }
}

/// A stiff algebraic loop: one state driving two coupled algebraic auxiliaries.
/// rtol 1e-9 / atol 1e-11.
const ORACLE_LOOP_IC: [f64; 3] = [0.5, 0.6819028775674488, 0.7537221785481572];
const ORACLE_LOOP: [(f64, [f64; 3]); 5] = [
    (0.0, ORACLE_LOOP_IC),
    (
        0.25,
        [0.36099719291425447, 0.7743958215986404, 0.7741486666195643],
    ),
    (
        1.0,
        [-0.1824899381414632, 1.263979499550616, 0.6377742493217211],
    ),
    (
        3.0,
        [0.11272125688124719, 0.8228511723831367, -0.7054848202779337],
    ),
    (
        8.0,
        [1.4305600850946225, 0.11137410406075501, -1.2780082234712296],
    ),
];

#[test]
fn stiff_algebraic_loop_matches_ida() {
    let res = ClosureResidual::new(|t: f64, y: &[f64], yp: &[f64], r: &mut [f64]| {
        r[0] = yp[0] + y[1] * y[2];
        r[1] = y[1] - (-y[0]).exp() - 0.1 * y[2];
        r[2] = y[2] * y[2] + y[1] - 1.25 - 0.5 * t.sin();
        Ok(())
    });
    let times: Vec<f64> = ORACLE_LOOP.iter().map(|(t, _)| *t).collect();
    let (rows, _, ic) = drive(
        3,
        &res,
        &[1.0, 0.0, 0.0],
        &[0.5, 0.6, 0.8],
        &[0.0, 0.0, 0.0],
        &times,
        1e-9,
        1e-11,
        None,
    );
    // The line-search Newton inside IDACalcIC has to move both auxiliaries;
    // this is the constant that pins it.
    assert_close(&ic, &ORACLE_LOOP_IC, 1e-9, "consistent IC");
    for (i, (t, y)) in rows.iter().enumerate() {
        assert_close(y, &ORACLE_LOOP[i].1, 1e-5, &format!("loop t={t}"));
    }
}

/// A 16-node / 15-flux heat chain in C-R-C form: n = 31, above
/// [`SPARSE_THRESHOLD`], so the sparse CSC path and the coloured Jacobian run.
fn heat_chain_residual() -> ClosureResidual<'static> {
    const NODES: usize = 16;
    const CAP: f64 = 500.0;
    const COND: f64 = 12.0;
    ClosureResidual::new(|_t, y: &[f64], yp: &[f64], r: &mut [f64]| {
        for i in 0..NODES {
            let inflow = if i > 0 { y[NODES + i - 1] } else { 0.0 };
            let outflow = if i < NODES - 1 { y[NODES + i] } else { 0.0 };
            r[i] = CAP * yp[i] - (inflow - outflow);
        }
        for k in 0..(NODES - 1) {
            r[NODES + k] = y[NODES + k] - COND * (y[k] - y[k + 1]);
        }
        Ok(())
    })
}

/// Node temperatures at t = 1000 from the oracle (the 16 differential
/// components; the fluxes follow from them).
const ORACLE_CHAIN_T1000: [f64; 16] = [
    311.48712616224066,
    311.24937865049543,
    310.7887708395351,
    310.1335630656282,
    309.322584906329,
    308.40154836380674,
    307.41899345792916,
    306.42239986575623,
    305.4549290650125,
    304.55312424637,
    303.7457204043032,
    303.05353806210167,
    302.49028051046525,
    302.0639465346431,
    301.7785175029174,
    301.6355783624667,
];
const ORACLE_CHAIN_T50_NODE0: f64 = 348.51034118722083;
const ORACLE_CHAIN_T200_NODE0: f64 = 325.4092717702401;

fn heat_chain_setup() -> (Vec<f64>, Vec<f64>, Vec<Vec<usize>>) {
    const NODES: usize = 16;
    let n = 31;
    let mut y0 = vec![0.0; n];
    let mut id = vec![0.0; n];
    for i in 0..NODES {
        y0[i] = if i == 0 { 400.0 } else { 300.0 };
        id[i] = 1.0;
    }
    // The structural pattern of the residual above, per row.
    let mut sparsity: Vec<Vec<usize>> = Vec::with_capacity(n);
    for i in 0..NODES {
        let mut cols = vec![i];
        if i > 0 {
            cols.push(NODES + i - 1);
        }
        if i < NODES - 1 {
            cols.push(NODES + i);
        }
        cols.sort_unstable();
        sparsity.push(cols);
    }
    for k in 0..(NODES - 1) {
        sparsity.push(vec![k, k + 1, NODES + k]);
    }
    (y0, id, sparsity)
}

#[test]
fn heat_chain_matches_ida_on_the_dense_path() {
    let res = heat_chain_residual();
    let (y0, id, _) = heat_chain_setup();
    let times = [0.0, 50.0, 200.0, 1000.0];
    let (rows, _, ic) = drive(
        31,
        &res,
        &id,
        &y0,
        &vec![0.0; 31],
        &times,
        1e-8,
        1e-10,
        None,
    );
    // IDACalcIC must find the initial flux 12*(400-300) = 1200 in the first
    // auxiliary; the rest start at zero temperature difference.
    assert!(
        rel(ic[16], 1200.0) < 1e-9,
        "consistent IC flux: {} vs 1200",
        ic[16]
    );
    assert!(rel(rows[1].1[0], ORACLE_CHAIN_T50_NODE0) < 1e-6);
    assert!(rel(rows[2].1[0], ORACLE_CHAIN_T200_NODE0) < 1e-6);
    assert_close(&rows[3].1[..16], &ORACLE_CHAIN_T1000, 1e-6, "chain t=1000");
    // Energy is conserved: the chain is closed, so the mean stays put.
    let mean: f64 = rows[3].1[..16].iter().sum::<f64>() / 16.0;
    let mean0: f64 = y0[..16].iter().sum::<f64>() / 16.0;
    assert!((mean - mean0).abs() < 1e-6, "{mean} vs {mean0}");
}

#[test]
fn heat_chain_matches_ida_on_the_sparse_path_too() {
    let res = heat_chain_residual();
    let (y0, id, sparsity) = heat_chain_setup();
    let mut s = IdaDaeSolver::new(31, &res).unwrap();
    s.set_tolerances(1e-8, 1e-10);
    s.set_variable_id(&id).unwrap();
    s.set_sparsity(&sparsity).unwrap();
    s.init(0.0, &y0, &vec![0.0; 31]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0).unwrap();
    let step = s.step(1000.0).unwrap();
    assert_close(&step.y[..16], &ORACLE_CHAIN_T1000, 1e-6, "sparse chain");
    // n = 31 > SPARSE_THRESHOLD, which is what selects this path in
    // `for_assembly`.
    assert!(31 > SPARSE_THRESHOLD);
}

// ---------------------------------------------------------------------------
// behaviour that is not a trajectory
// ---------------------------------------------------------------------------

#[test]
fn a_zero_dimension_system_is_refused() {
    let res = ClosureResidual::new(|_t, _y, _yp, _r: &mut [f64]| Ok(()));
    assert!(IdaDaeSolver::new(0, &res).is_err());
}

#[test]
fn stepping_before_init_is_refused() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0];
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    let err = s.step(1.0).unwrap_err().to_string();
    assert!(err.contains("init"), "{err}");
}

#[test]
fn an_id_vector_of_the_wrong_length_is_refused() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0];
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    assert!(s.set_variable_id(&[1.0, 0.0]).is_err());
    assert!(s.set_sparsity(&[vec![0], vec![0]]).is_err());
}

#[test]
fn consistent_ic_needs_the_variable_id() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0];
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.init(0.0, &[1.0], &[-1.0]).unwrap();
    let err = s
        .calc_consistent_ic(IDA_YA_YDP_INIT, 1.0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("marker"), "{err}");
}

#[test]
fn a_residual_that_always_fails_surfaces_as_a_solver_error() {
    let res = ClosureResidual::new(|_t, _y, _yp, _r: &mut [f64]| {
        Err(crate::diag::FreesError::property("outside the fluid table"))
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[1.0], &[0.0]).unwrap();
    // Consistent initialization is where it is noticed first.
    assert!(s.calc_consistent_ic(IDA_YA_YDP_INIT, 1.0).is_err());
}

#[test]
fn a_residual_that_fails_only_off_the_solution_is_recovered_from() {
    // Recoverable failure: the residual refuses states above 200, which a bold
    // first step would probe. IDA cuts h and carries on.
    let res = ClosureResidual::new(|_t, y: &[f64], yp: &[f64], r: &mut [f64]| {
        if y[0] > 200.0 {
            return Err(crate::diag::FreesError::property("above the table"));
        }
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-6, 1e-8);
    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[95.0], &[0.0]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 0.06).unwrap();
    let step = s.step(60.0).unwrap();
    assert!(rel(step.y[0], 23.734015089281844) < 1e-6);
}

#[test]
fn an_empty_interval_is_refused_with_a_model_level_message() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] + y[0];
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[1.0], &[-1.0]).unwrap();
    let err = s.step(0.0).unwrap_err().to_string();
    assert!(err.contains("end time"), "{err}");
}

#[test]
fn the_step_budget_is_enforced() {
    // A pure oscillator integrated far past what two steps can cover.
    let res = ClosureResidual::new(|_t, y: &[f64], yp: &[f64], r: &mut [f64]| {
        r[0] = yp[0] - y[1];
        r[1] = yp[1] + y[0];
        Ok(())
    });
    let mut s = IdaDaeSolver::new(2, &res).unwrap();
    s.set_tolerances(1e-10, 1e-12);
    s.set_max_steps(3);
    s.set_variable_id(&[1.0, 1.0]).unwrap();
    s.init(0.0, &[1.0, 0.0], &[0.0, -1.0]).unwrap();
    let err = s.step(100.0).unwrap_err().to_string();
    assert!(err.contains("steps"), "{err}");
}

#[test]
fn interpolation_returns_intermediate_samples_without_re_integrating() {
    // Sampling densely must give the same trajectory as sampling coarsely:
    // the dense output comes from the same `phi` history either way.
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-8, 1e-10);
    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[95.0], &[0.0]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 0.06).unwrap();
    for k in 1..=60 {
        let t = k as f64;
        let step = s.step(t).unwrap();
        assert_eq!(step.t, t);
        let exact = 20.0 + 75.0 * (-0.05 * t).exp();
        assert!(
            rel(step.y[0], exact) < 1e-7,
            "t={t}: {} vs {exact}",
            step.y[0]
        );
    }
}

#[test]
fn integrating_backwards_in_time_works() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-9, 1e-11);
    s.set_variable_id(&[1.0]).unwrap();
    // Start from the t = 60 value and integrate back to 0.
    let y60 = 20.0 + 75.0 * (-3.0f64).exp();
    s.init(60.0, &[y60], &[0.0]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 59.94).unwrap();
    let step = s.step(0.0).unwrap();
    assert!(rel(step.y[0], 95.0) < 1e-7, "got {}", step.y[0]);
}

#[test]
fn reinit_restarts_the_history_at_a_new_point() {
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-8, 1e-10);
    s.set_variable_id(&[1.0]).unwrap();
    s.init(0.0, &[95.0], &[0.0]).unwrap();
    s.step(30.0).unwrap();
    s.reinit(0.0, &[95.0], &[0.0]).unwrap();
    let step = s.step(60.0).unwrap();
    let exact = 20.0 + 75.0 * (-3.0f64).exp();
    assert!(rel(step.y[0], exact) < 1e-6, "got {}", step.y[0]);
}

#[test]
fn a_stopping_event_reports_the_crossing_time_and_direction() {
    // Cooling with a root at T = 50: the analytic crossing is
    // t = -ln(30/75)/0.05.
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let root = ClosureRootFn::new(|_t, y: &[f64], _yp: &[f64], g: &mut [f64]| {
        g[0] = y[0] - 50.0;
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-9, 1e-11);
    s.set_variable_id(&[1.0]).unwrap();
    s.set_roots(1, &root);
    s.init(0.0, &[95.0], &[0.0]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 0.06).unwrap();
    let step = s.step(60.0).unwrap();
    assert!(step.root_return(), "flag {}", step.flag);
    assert_eq!(step.roots_found, vec![-1], "T is decreasing through 50");
    let exact = -(30.0f64 / 75.0).ln() / 0.05;
    assert!(rel(step.t, exact) < 1e-8, "got {} want {exact}", step.t);
    assert!((step.y[0] - 50.0).abs() < 1e-8);
}

#[test]
fn a_root_already_at_t0_does_not_fire_immediately() {
    // `IDARcheck1` deactivates a switching function that is exactly zero at t0
    // and only reactivates it once it moves off zero — without that, a
    // condition written at its trigger point would fire forever.
    let res = ClosureResidual::new(|_t, y, yp, r: &mut [f64]| {
        r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        Ok(())
    });
    let root = ClosureRootFn::new(|_t, y: &[f64], _yp: &[f64], g: &mut [f64]| {
        g[0] = y[0] - 95.0;
        Ok(())
    });
    let mut s = IdaDaeSolver::new(1, &res).unwrap();
    s.set_tolerances(1e-8, 1e-10);
    s.set_variable_id(&[1.0]).unwrap();
    s.set_roots(1, &root);
    s.init(0.0, &[95.0], &[0.0]).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 0.06).unwrap();
    let step = s.step(60.0).unwrap();
    assert!(!step.root_return(), "a root at t0 must not fire at t0");
    assert_eq!(step.t, 60.0);
}

#[test]
fn for_assembly_wires_tolerances_id_roots_and_the_sparse_threshold() {
    use crate::ast::{BinOp, Equation, Expr};
    use crate::dae::assembly::{assemble, AssemblySpec, EventSpec};
    use crate::eval::{EvalContext, Scope};

    let mut analytic = Scope::new();
    analytic.insert("k".into(), 0.05);
    analytic.insert("tinf".into(), 20.0);
    let spec = AssemblySpec {
        block_name: "cool".into(),
        time_var: "time".into(),
        states: vec!["temp".into()],
        aux: Vec::new(),
        template: vec![Equation::new(
            Expr::var("der$temp"),
            Expr::bin(
                BinOp::Mul,
                Expr::var("k"),
                Expr::bin(BinOp::Sub, Expr::var("tinf"), Expr::var("temp")),
            ),
            "der(Temp) = k*(Tinf - Temp)",
        )],
        analytic_values: analytic,
        state_initials: vec![95.0],
        seed: None,
        events: vec![EventSpec {
            name: "cold".into(),
            lhs: Expr::var("temp"),
            rhs: Expr::num(50.0),
            stops: true,
        }],
        ctx: EvalContext::default(),
    };
    let dae = assemble(spec).unwrap();
    let mut s = IdaDaeSolver::for_assembly(&dae, 1e-9, 1e-11).unwrap();
    s.init(0.0, &dae.y0, &dae.yp0).unwrap();
    s.calc_consistent_ic(IDA_YA_YDP_INIT, 0.06).unwrap();
    let step = s.step(60.0).unwrap();
    assert!(step.root_return());
    let exact = -(30.0f64 / 75.0).ln() / 0.05;
    assert!(rel(step.t, exact) < 1e-8, "got {}", step.t);
}

