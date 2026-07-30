//! Combustion-product chemical equilibrium with dissociation.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/props/Equilibrium.java`
//! (288 LOC).
//!
//! For a fuel CxHyOz burned in air at fuel/air equivalence ratio `phi`, the
//! product pool is {CO2, CO, H2O, H2, OH, H, O, O2} with N2 carried inert. The
//! composition satisfies the C/H/O element balances and five independent
//! dissociation equilibria:
//!
//! ```text
//!   CO2 = CO + 1/2 O2      H2O = H2 + 1/2 O2     H2O = OH + 1/2 H2
//!   1/2 H2 = H             1/2 O2 = O
//! ```
//!
//! Each `Kp(T) = exp(-ΔG°/RT)` is built from the Gibbs energy `g° = h - T s°`
//! supplied by [`crate::props::nasa`], so no equilibrium-constant tables are
//! needed. The 8×8 nonlinear system is solved by a damped Newton iteration in
//! log-moles. Unlike [`crate::props::thermochem::adiabatic_flame_temp`] this
//! admits rich mixtures (`phi > 1`) and predicts the dissociation that lowers
//! real flame temperatures.
//!
//! # Parity notes
//!
//! * The Java solves the Newton step with Apache Commons Math
//!   `LUDecomposition`. [`lu_solve`] below reproduces that decomposition and
//!   its `Solver.solve` back-substitution move for move, including the default
//!   `1e-11` singularity threshold and the whole-row pivot swap — the same
//!   conventions `crate::linalg::det_lu` already follows.
//! * `!(phi > 0.0)` stays negated so a NaN equivalence ratio is rejected.
//! * [`State::total`] sums N2 **first**, then the eight products in index
//!   order. Mole fractions are ratios of floating-point sums, so the order is
//!   part of the answer.
//! * [`adiabatic_flame_temp`]'s secant loop stops at
//!   `|f| < 1e-3·|h_react| + 1`, which is a *loose* criterion — roughly 0.1 K
//!   of slack in the returned temperature. See the test module for what that
//!   costs in reproducibility against the oracle.

// The Newton residual, Jacobian and LU solve index parallel arrays (and
// `a[i][j]` rows) by the same loop variable, mirroring the Java and Commons
// Math sources being transcribed. Iterator rewrites obscure that
// correspondence, so the indexed form stays.
#![allow(clippy::needless_range_loop)]
// The equivalence-ratio guard is written `!(phi > 0.0)` on purpose: the
// negation makes NaN take the reject branch, which `phi <= 0.0` would not.
// Clippy's `neg_cmp_op_on_partial_ord` exists to catch the *accidental* form;
// here the NaN behaviour is the point, and it is the Java guard being ported.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::diag::{FreesError, Result};
use crate::props::formula;
use crate::props::nasa;
use crate::props::thermochem;

const R: f64 = 8.314462618;
const P_REF: f64 = 101_325.0;

/// Product species, fixed index order used throughout.
const SP: [&str; 8] = ["CO2", "CO", "H2O", "H2", "OH", "H", "O", "O2"];
const CO2: usize = 0;
const CO: usize = 1;
const H2O: usize = 2;
const H2: usize = 3;
const OH: usize = 4;
const H: usize = 5;
const O: usize = 6;
const O2: usize = 7;

const N: usize = 8;

/// Commons Math `LUDecomposition.DEFAULT_TOO_SMALL`.
const LU_SINGULARITY_THRESHOLD: f64 = 1e-11;

/// Gibbs energy g°(T) [J/mol] at the reference pressure.
fn gibbs(sp: &str, t: f64) -> Result<f64> {
    Ok(nasa::molar_enthalpy(sp, t)? - t * nasa::molar_entropy(sp, t, P_REF)?)
}

/// Solved equilibrium state: product moles per mole of fuel, plus inert N2.
#[derive(Debug, Clone, Copy)]
pub struct State {
    /// Moles of each product in [`SP`] order, per mole of fuel.
    pub n: [f64; N],
    /// Moles of inert N2 per mole of fuel.
    pub n_n2: f64,
}

impl State {
    /// Total moles. N2 is added first, then the products in index order — the
    /// Java accumulation order, which the mole fractions inherit.
    pub fn total(&self) -> f64 {
        let mut s = self.n_n2;
        for v in self.n {
            s += v;
        }
        s
    }
}

/// Reaction log-equilibrium constants `ln Kp(T)` for the five dissociations
/// (index 0 = `CO2 = CO + 1/2 O2`).
///
/// Public so the values can be checked against standard JANAF thermochemical
/// tables; the Java keeps it package-visible for the same reason.
pub fn ln_kp(t: f64) -> Result<[f64; 5]> {
    let gco2 = gibbs("CO2", t)?;
    let gco = gibbs("CO", t)?;
    let go2 = gibbs("O2", t)?;
    let gh2o = gibbs("H2O", t)?;
    let gh2 = gibbs("H2", t)?;
    let goh = gibbs("OH", t)?;
    let gh = gibbs("H", t)?;
    let go = gibbs("O", t)?;
    let d_g = [
        gco + 0.5 * go2 - gco2, // CO2 = CO + 1/2 O2
        gh2 + 0.5 * go2 - gh2o, // H2O = H2 + 1/2 O2
        goh + 0.5 * gh2 - gh2o, // H2O = OH + 1/2 H2
        gh - 0.5 * gh2,         // 1/2 H2 = H
        go - 0.5 * go2,         // 1/2 O2 = O
    ];
    let mut k = [0.0; 5];
    for i in 0..5 {
        k[i] = -d_g[i] / (R * t);
    }
    Ok(k)
}

/// Solves the equilibrium product composition at temperature `t` and pressure
/// `p` for fuel CxHyOz with O2 supply `a` (mol per mol fuel) and inert N2
/// `n_n2`.
fn solve(x: f64, y: f64, z: f64, a: f64, n_n2: f64, t: f64, p: f64) -> Result<State> {
    let c_tot = x;
    let h_tot = y;
    let o_tot = z + 2.0 * a;
    let kp = ln_kp(t)?;
    let ln_pp0 = libm::log(p / P_REF);

    let mut u = [0.0; N]; // log-moles
    initial_guess(&mut u, x, y, o_tot);

    for _ in 0..200 {
        let f = residual(&u, c_tot, h_tot, o_tot, n_n2, &kp, ln_pp0);
        let mut norm = 0.0;
        for v in f {
            norm += v * v;
        }
        if norm.sqrt() < 1e-11 {
            break;
        }
        let j = jacobian(&mut u, c_tot, h_tot, o_tot, n_n2, &kp, ln_pp0);
        let du = lu_solve(&j, &negate(&f))?;
        let mut damp: f64 = 1.0;
        for d in du {
            if d.abs() > 2.0 {
                damp = damp.min(2.0 / d.abs());
            }
        }
        for i in 0..N {
            u[i] = java_clamp(u[i] + damp * du[i], -80.0, 80.0);
        }
    }
    let mut n = [0.0; N];
    for i in 0..N {
        n[i] = libm::exp(u[i]);
    }
    Ok(State { n, n_n2 })
}

fn initial_guess(u: &mut [f64; N], x: f64, y: f64, o_tot: f64) {
    let mut n = [0.0; N];
    let o_need = 2.0 * x + y / 2.0; // O atoms for full CO2 + H2O
    if o_tot >= o_need {
        n[CO2] = x;
        n[H2O] = y / 2.0;
        n[O2] = ((o_tot - o_need) / 2.0).max(1e-8);
    } else {
        let def = o_need - o_tot; // O atoms to free by making CO / H2
        let nco = def.min(x);
        n[CO] = nco;
        n[CO2] = x - nco;
        let def2 = def - nco;
        let nh2 = def2.min(y / 2.0);
        n[H2] = nh2;
        n[H2O] = y / 2.0 - nh2;
        n[O2] = 1e-8;
    }
    n[OH] = 1e-6;
    n[H] = 1e-6;
    n[O] = 1e-6;
    for i in 0..N {
        u[i] = libm::log(n[i].max(1e-30));
    }
}

fn residual(
    u: &[f64; N],
    c_tot: f64,
    h_tot: f64,
    o_tot: f64,
    n_n2: f64,
    kp: &[f64; 5],
    ln_pp0: f64,
) -> [f64; N] {
    let mut n = [0.0; N];
    let mut ntot = n_n2;
    for i in 0..N {
        n[i] = libm::exp(u[i]);
        ntot += n[i];
    }
    let ln_tot = libm::log(ntot);
    let mut f = [0.0; N];
    // Element balances, scaled by the element totals to stay O(1).
    f[0] = ((n[CO2] + n[CO]) - c_tot) / c_tot.max(1.0);
    f[1] = ((2.0 * n[H2O] + 2.0 * n[H2] + n[OH] + n[H]) - h_tot) / h_tot.max(1.0);
    f[2] = ((2.0 * n[CO2] + n[CO] + n[H2O] + n[OH] + n[O] + 2.0 * n[O2]) - o_tot) / o_tot.max(1.0);
    // Dissociation equilibria (all have net moles change +1/2).
    f[3] = (u[CO] + 0.5 * u[O2] - u[CO2]) - 0.5 * ln_tot + 0.5 * ln_pp0 - kp[0];
    f[4] = (u[H2] + 0.5 * u[O2] - u[H2O]) - 0.5 * ln_tot + 0.5 * ln_pp0 - kp[1];
    f[5] = (u[OH] + 0.5 * u[H2] - u[H2O]) - 0.5 * ln_tot + 0.5 * ln_pp0 - kp[2];
    f[6] = (u[H] - 0.5 * u[H2]) - 0.5 * ln_tot + 0.5 * ln_pp0 - kp[3];
    f[7] = (u[O] - 0.5 * u[O2]) - 0.5 * ln_tot + 0.5 * ln_pp0 - kp[4];
    f
}

/// Forward-difference Jacobian. `u` is perturbed in place and restored
/// exactly, as the Java does.
fn jacobian(
    u: &mut [f64; N],
    c_tot: f64,
    h_tot: f64,
    o_tot: f64,
    n_n2: f64,
    kp: &[f64; 5],
    ln_pp0: f64,
) -> [[f64; N]; N] {
    let f0 = residual(u, c_tot, h_tot, o_tot, n_n2, kp, ln_pp0);
    let mut jac = [[0.0; N]; N];
    for col in 0..N {
        let save = u[col];
        let h = 1e-6 * save.abs().max(1.0);
        u[col] = save + h;
        let f1 = residual(u, c_tot, h_tot, o_tot, n_n2, kp, ln_pp0);
        u[col] = save;
        for row in 0..N {
            jac[row][col] = (f1[row] - f0[row]) / h;
        }
    }
    jac
}

fn negate(v: &[f64; N]) -> [f64; N] {
    let mut r = [0.0; N];
    for i in 0..N {
        r[i] = -v[i];
    }
    r
}

/// Java `Math.clamp(value, lo, hi)`: `min(hi, max(value, lo))`, `NaN`
/// propagating.
fn java_clamp(value: f64, lo: f64, hi: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// `new LUDecomposition(A).getSolver().solve(b)` — Doolittle LU with partial
/// pivoting and Commons Math's `1e-11` singularity threshold, followed by the
/// permuted forward/backward substitution of `LUDecomposition.Solver.solve`.
fn lu_solve(matrix: &[[f64; N]; N], b: &[f64; N]) -> Result<[f64; N]> {
    let mut lu = *matrix;
    let mut pivot = [0usize; N];
    for (i, slot) in pivot.iter_mut().enumerate() {
        *slot = i;
    }

    for col in 0..N {
        // Upper part.
        for row in 0..col {
            let mut sum = lu[row][col];
            for i in 0..row {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
        }
        // Lower part, tracking the largest pivot candidate.
        let mut max = col;
        let mut largest = f64::NEG_INFINITY;
        for row in col..N {
            let mut sum = lu[row][col];
            for i in 0..col {
                sum -= lu[row][i] * lu[i][col];
            }
            lu[row][col] = sum;
            if sum.abs() > largest {
                largest = sum.abs();
                max = row;
            }
        }
        if lu[max][col].abs() < LU_SINGULARITY_THRESHOLD {
            // Commons Math raises SingularMatrixException from getSolver().
            return Err(FreesError::property(
                "Equilibrium: the Newton Jacobian is singular; \
                 the product composition could not be solved.",
            ));
        }
        if max != col {
            lu.swap(max, col);
            pivot.swap(max, col);
        }
        let diag = lu[col][col];
        for row in (col + 1)..N {
            lu[row][col] /= diag;
        }
    }

    // Apply the permutation to b.
    let mut bp = [0.0; N];
    for row in 0..N {
        bp[row] = b[pivot[row]];
    }
    // Solve LY = b.
    for col in 0..N {
        let bp_col = bp[col];
        for i in (col + 1)..N {
            bp[i] -= bp_col * lu[i][col];
        }
    }
    // Solve UX = Y.
    for col in (0..N).rev() {
        bp[col] /= lu[col][col];
        let bp_col = bp[col];
        for i in 0..col {
            bp[i] -= bp_col * lu[i][col];
        }
    }
    Ok(bp)
}

// ----- public API ------------------------------------------------------------

/// Reactant bookkeeping per mole of fuel.
#[derive(Debug, Clone, Copy)]
struct Reactants {
    x: f64,
    y: f64,
    z: f64,
    /// O2 supplied per mole of fuel.
    a: f64,
    /// Inert N2 carried with it (3.76 per O2).
    n_n2: f64,
}

fn reactants(fuel: &str, phi: f64) -> Result<Reactants> {
    let counts = formula::parse(fuel)?;
    let x = f64::from(formula::count_of(&counts, "C"));
    let y = f64::from(formula::count_of(&counts, "H"));
    let z = f64::from(formula::count_of(&counts, "O"));
    let a_st = x + y / 4.0 - z / 2.0;
    if a_st <= 0.0 {
        return Err(FreesError::property(format!(
            "Equilibrium: '{fuel}' has no oxygen demand (non-combustible)."
        )));
    }
    // Negated on purpose: a NaN equivalence ratio must be rejected.
    if !(phi > 0.0) {
        return Err(FreesError::property(format!(
            "Equilibrium: equivalence ratio phi must be > 0, got {phi}."
        )));
    }
    let a = a_st / phi;
    Ok(Reactants {
        x,
        y,
        z,
        a,
        n_n2: 3.76 * a,
    })
}

/// The full equilibrium product state for `fuel` at equivalence ratio `phi`,
/// temperature `t` [K] and pressure `p` [Pa].
pub fn products(fuel: &str, phi: f64, t: f64, p: f64) -> Result<State> {
    let r = reactants(fuel, phi)?;
    solve(r.x, r.y, r.z, r.a, r.n_n2, t, p)
}

/// Equilibrium mole fraction of one product species (CO2, CO, H2O, H2, OH, H,
/// O, O2 or N2).
pub fn mole_fraction(fuel: &str, phi: f64, t: f64, p: f64, species: &str) -> Result<f64> {
    let st = products(fuel, phi, t, p)?;
    let total = st.total();
    let key = species.trim().to_uppercase();
    if key == "N2" {
        return Ok(st.n_n2 / total);
    }
    for i in 0..SP.len() {
        if SP[i] == key {
            return Ok(st.n[i] / total);
        }
    }
    Err(FreesError::property(format!(
        "Equilibrium: species '{species}' is not in the product pool \
         (CO2, CO, H2O, H2, OH, H, O, O2, N2)."
    )))
}

/// Product mixture enthalpy per mole of fuel [J] at temperature `t`.
fn product_enthalpy(st: &State, t: f64) -> Result<f64> {
    let mut h = st.n_n2 * nasa::molar_enthalpy("N2", t)?;
    for i in 0..SP.len() {
        h += st.n[i] * nasa::molar_enthalpy(SP[i], t)?;
    }
    Ok(h)
}

/// Adiabatic flame temperature [K] **with** dissociation: a constant-pressure
/// energy balance whose products are re-equilibrated at every trial
/// temperature. Lower, and more realistic, than the frozen-product value from
/// [`crate::props::thermochem::adiabatic_flame_temp`].
pub fn adiabatic_flame_temp(fuel: &str, phi: f64, t_react: f64, p: f64) -> Result<f64> {
    let r = reactants(fuel, phi)?;
    let h_react = thermochem::h_mol(fuel, t_react)?
        + r.a * nasa::molar_enthalpy("O2", t_react)?
        + r.n_n2 * nasa::molar_enthalpy("N2", t_react)?;

    // Java reaches `Math.clamp(tMid, tReact, 3500.0)`, which throws when the
    // lower bound exceeds the upper one or is NaN.
    if t_react.is_nan() || t_react > 3500.0 {
        return Err(FreesError::evaluation(format!(
            "AdiabaticFlameTempEq: reactant temperature must be a number at most 3500 K, \
             got {t_react}."
        )));
    }

    let balance = |t: f64| -> Result<f64> {
        let st = solve(r.x, r.y, r.z, r.a, r.n_n2, t, p)?;
        Ok(product_enthalpy(&st, t)? - h_react)
    };

    // Secant iteration bracketed in a physical flame-temperature window.
    let mut t_lo = 1000.0;
    let mut t_hi = 3200.0;
    let mut f_lo = balance(t_lo)?;
    let mut f_hi = balance(t_hi)?;
    for _ in 0..100 {
        let mut t_mid = t_hi - f_hi * (t_hi - t_lo) / (f_hi - f_lo);
        t_mid = java_clamp(t_mid, t_react, 3500.0);
        let f_mid = balance(t_mid)?;
        if f_mid.abs() < 1e-3 * h_react.abs() + 1.0 {
            return Ok(t_mid);
        }
        if (f_mid > 0.0) == (f_lo > 0.0) {
            t_lo = t_mid;
            f_lo = f_mid;
        } else {
            t_hi = t_mid;
            f_hi = f_mid;
        }
    }
    Ok(0.5 * (t_lo + t_hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mole fractions are pinned by a Newton iteration driven to a residual
    /// below 1e-11 in log-mole space, so the solution — unlike the iteration
    /// path — is reproducible far tighter than the gate's 1e-9.
    fn close(actual: f64, expected: f64) {
        let tol = 1e-9 * expected.abs().max(1e-12);
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected}, got {actual} (relative Δ = {:e})",
            ((actual - expected) / expected).abs()
        );
    }

    // ---- the linear algebra ----------------------------------------------

    #[test]
    fn lu_solve_inverts_a_known_system() {
        let mut a = [[0.0; N]; N];
        let mut b = [0.0; N];
        // A well-conditioned, non-symmetric, pivot-exercising matrix.
        for i in 0..N {
            for j in 0..N {
                a[i][j] = 1.0 / ((i + j + 1) as f64) + if i == j { 3.0 } else { 0.0 };
            }
            b[i] = (i as f64) - 3.5;
        }
        let x = lu_solve(&a, &b).expect("non-singular");
        for i in 0..N {
            let mut row = 0.0;
            for j in 0..N {
                row += a[i][j] * x[j];
            }
            assert!((row - b[i]).abs() < 1e-10, "row {i}: {row} vs {}", b[i]);
        }
    }

    #[test]
    fn lu_solve_reports_a_singular_matrix() {
        let mut a = [[0.0; N]; N];
        for i in 0..N {
            a[i][0] = 1.0; // rank 1
        }
        assert!(lu_solve(&a, &[1.0; N]).is_err());
    }

    #[test]
    fn lu_solve_pivots_past_a_zero_leading_entry() {
        // Without row pivoting the first division would be by zero.
        let mut a = [[0.0; N]; N];
        for i in 0..N {
            a[i][(i + 1) % N] = 1.0 + i as f64;
        }
        let b: [f64; N] = std::array::from_fn(|i| i as f64 + 1.0);
        let x = lu_solve(&a, &b).expect("permutation matrices are non-singular");
        for i in 0..N {
            let mut row = 0.0;
            for j in 0..N {
                row += a[i][j] * x[j];
            }
            assert!((row - b[i]).abs() < 1e-12);
        }
    }

    // ---- equilibrium constants -------------------------------------------

    /// Every dissociation is unfavourable at flame temperatures and gets less
    /// so as T rises — the qualitative shape the whole model rests on.
    #[test]
    fn ln_kp_has_the_right_sign_and_slope() {
        let k2000 = ln_kp(2000.0).unwrap();
        let k3000 = ln_kp(3000.0).unwrap();
        for i in 0..5 {
            assert!(
                k2000[i] < 0.0,
                "reaction {i} should be unfavourable at 2000 K, got {}",
                k2000[i]
            );
            assert!(
                k3000[i] > k2000[i],
                "reaction {i}: dissociation should grow with T ({} vs {})",
                k3000[i],
                k2000[i]
            );
        }
        // Absolute anchors, derived by hand from h° and s° of the reactants:
        // ½H2 = H at 2000 K has ΔG° = 106.8 kJ/mol, so ln Kp = -6.42; CO2
        // dissociation at 2000 K is the textbook Kp ≈ 1.3e-3.
        assert!((k2000[3] - -6.42).abs() < 0.02, "1/2 H2 = H: {}", k2000[3]);
        assert!((k2000[0] - -6.63).abs() < 0.02, "CO2 dissoc: {}", k2000[0]);
    }

    /// Independent-dataset cross-check. [`crate::props::idealgas`] carries
    /// tabulated formation enthalpies and third-law entropies from the JANAF
    /// tables — a *different* source from the combustion mechanism these
    /// coefficients come from. Computing the same five ΔG° values both ways
    /// agrees to within 0.14 in `ln Kp` from 1000 K to 3000 K — worst case the
    /// OH equilibrium at 3000 K, where the two datasets genuinely differ by
    /// ~14% in Kp. That is the physics saying both transcriptions are right;
    /// a swapped coefficient would miss by orders of magnitude, not by 14%.
    #[test]
    fn ln_kp_agrees_with_the_independent_janaf_dataset() {
        use crate::props::idealgas;
        let g = |sp: &str, t: f64| {
            idealgas::molar_enthalpy(sp, t) - t * idealgas::molar_entropy(sp, t, P_REF)
        };
        for t in [1000.0, 1600.0, 2000.0, 2400.0, 3000.0] {
            let mechanism = ln_kp(t).unwrap();
            let janaf = [
                g("co", t) + 0.5 * g("o2", t) - g("co2", t),
                g("h2", t) + 0.5 * g("o2", t) - g("h2o", t),
                g("oh", t) + 0.5 * g("h2", t) - g("h2o", t),
                g("h", t) - 0.5 * g("h2", t),
                g("o", t) - 0.5 * g("o2", t),
            ];
            for i in 0..5 {
                let reference = -janaf[i] / (R * t);
                assert!(
                    (mechanism[i] - reference).abs() < 0.15,
                    "T={t}, reaction {i}: mechanism {} vs JANAF {reference}",
                    mechanism[i]
                );
            }
        }
    }

    // ---- product composition, oracle ground truth ------------------------

    /// Oracle values from `tools/golden-dumper`, fixture `chem_equilibrium`
    /// (CH4, phi = 1.0, T = 2300 K, P = 101325 Pa).
    #[test]
    fn mole_fractions_match_the_oracle() {
        let case = |sp: &str| mole_fraction("CH4", 1.0, 2300.0, 101_325.0, sp).unwrap();
        close(case("CO2"), 0.08260327980405986);
        close(case("CO"), 0.011525237150430006);
        close(case("H2O"), 0.18131047630958524);
        close(case("H2"), 0.004519297199981506);
        close(case("OH"), 0.004203362477945942);
        close(case("H"), 0.000651158320879988);
        close(case("O"), 0.0004130502068286622);
        close(case("O2"), 0.006927691032524978);
        close(case("N2"), 0.7078464474977637);
    }

    #[test]
    fn off_stoichiometric_and_high_pressure_cases_match_the_oracle() {
        close(
            mole_fraction("CH4", 1.2, 2200.0, 101_325.0, "CO").unwrap(),
            0.045722641312221375,
        );
        close(
            mole_fraction("CH4", 0.7, 1900.0, 101_325.0, "O2").unwrap(),
            0.05850465444233417,
        );
        close(
            mole_fraction("C3H8", 1.0, 2400.0, 2_000_000.0, "CO").unwrap(),
            0.007867232242811982,
        );
        close(
            mole_fraction("C8H18", 1.0, 2400.0, 101_325.0, "OH").unwrap(),
            0.005703051245840133,
        );
    }

    #[test]
    fn mole_fractions_sum_to_one_and_conserve_atoms() {
        let st = products("CH4", 1.0, 2300.0, 101_325.0).unwrap();
        let total = st.total();
        let mut sum = st.n_n2 / total;
        for i in 0..N {
            sum += st.n[i] / total;
        }
        assert!((sum - 1.0).abs() < 1e-12, "mole fractions sum to {sum}");

        // CH4 + 2 O2 + 7.52 N2: 1 C, 4 H, 4 O.
        let c = st.n[CO2] + st.n[CO];
        let h = 2.0 * st.n[H2O] + 2.0 * st.n[H2] + st.n[OH] + st.n[H];
        let o = 2.0 * st.n[CO2] + st.n[CO] + st.n[H2O] + st.n[OH] + st.n[O] + 2.0 * st.n[O2];
        assert!((c - 1.0).abs() < 1e-9, "C balance: {c}");
        assert!((h - 4.0).abs() < 1e-9, "H balance: {h}");
        assert!((o - 4.0).abs() < 1e-9, "O balance: {o}");
    }

    /// The whole point of the model: rich mixtures make CO and H2, lean ones
    /// leave O2. The frozen-product model cannot express either.
    #[test]
    fn dissociation_responds_to_equivalence_ratio() {
        let co_rich = mole_fraction("CH4", 1.3, 2200.0, 101_325.0, "CO").unwrap();
        let co_lean = mole_fraction("CH4", 0.7, 2200.0, 101_325.0, "CO").unwrap();
        assert!(co_rich > co_lean, "{co_rich} vs {co_lean}");
        let o2_lean = mole_fraction("CH4", 0.7, 2200.0, 101_325.0, "O2").unwrap();
        let o2_rich = mole_fraction("CH4", 1.3, 2200.0, 101_325.0, "O2").unwrap();
        assert!(o2_lean > o2_rich, "{o2_lean} vs {o2_rich}");
    }

    /// Le Chatelier: every dissociation here increases the mole count, so
    /// raising the pressure suppresses it.
    #[test]
    fn pressure_suppresses_dissociation() {
        let low = mole_fraction("CH4", 1.0, 2600.0, 101_325.0, "CO").unwrap();
        let high = mole_fraction("CH4", 1.0, 2600.0, 5_000_000.0, "CO").unwrap();
        assert!(low > high, "CO at 1 atm {low} should exceed 50 atm {high}");
    }

    #[test]
    fn unknown_species_and_bad_reactants_are_refused() {
        assert!(mole_fraction("CH4", 1.0, 2300.0, 101_325.0, "NO").is_err());
        assert!(mole_fraction("CO2", 1.0, 2300.0, 101_325.0, "CO").is_err());
        assert!(mole_fraction("CH4", 0.0, 2300.0, 101_325.0, "CO").is_err());
        assert!(mole_fraction("CH4", -1.0, 2300.0, 101_325.0, "CO").is_err());
        assert!(mole_fraction("CH4", f64::NAN, 2300.0, 101_325.0, "CO").is_err());
        assert!(mole_fraction("not a formula", 1.0, 2300.0, 101_325.0, "CO").is_err());
    }

    #[test]
    fn species_names_are_trimmed_and_case_folded() {
        let a = mole_fraction("CH4", 1.0, 2300.0, 101_325.0, "co2").unwrap();
        let b = mole_fraction("CH4", 1.0, 2300.0, 101_325.0, "  CO2 ").unwrap();
        let c = mole_fraction("CH4", 1.0, 2300.0, 101_325.0, "CO2").unwrap();
        assert_eq!(a, c);
        assert_eq!(b, c);
    }

    // ---- flame temperature with dissociation -----------------------------

    /// Held to the same 1e-9 relative tolerance as everything else — and it
    /// currently passes **bit-exactly**, all five cases, which is worth
    /// stating because it did not have to.
    ///
    /// The secant loop returns as soon as the energy residual falls below
    /// `1e-3·|h_react| + 1` J — about 75 J for CH4, which given the ~550 J/K
    /// product heat capacity is roughly 0.15 K of slack. The returned
    /// temperature is therefore a function of the *iteration path*, and that
    /// path runs through `exp` and `ln`, which IEEE-754 does not specify. If
    /// this assertion ever fails by less than ~0.2 K on some other libm, that
    /// is this slack showing, not a porting defect — but it is real
    /// information and the tolerance should not be loosened to hide it.
    #[test]
    fn equilibrium_flame_temperature_matches_the_oracle() {
        close(
            adiabatic_flame_temp("CH4", 1.0, 298.15, 101_325.0).unwrap(),
            2230.292091923241,
        );
        close(
            adiabatic_flame_temp("CH4", 0.8, 298.15, 101_325.0).unwrap(),
            2001.9976259686528,
        );
        close(
            adiabatic_flame_temp("CH4", 1.1, 298.15, 101_325.0).unwrap(),
            2212.6514623070216,
        );
        close(
            adiabatic_flame_temp("C3H8", 1.0, 298.15, 101_325.0).unwrap(),
            2272.4099682439996,
        );
        close(
            adiabatic_flame_temp("CH4", 1.0, 298.15, 500_000.0).unwrap(),
            2261.550039179287,
        );
    }

    /// Dissociation is endothermic, so the equilibrium flame temperature must
    /// sit **below** the frozen-product value, and raising the pressure (which
    /// suppresses dissociation) must push it back up.
    #[test]
    fn dissociation_lowers_the_flame_temperature() {
        let frozen = thermochem::adiabatic_flame_temp("CH4", 1.0, 298.15).unwrap();
        let equilibrium = adiabatic_flame_temp("CH4", 1.0, 298.15, 101_325.0).unwrap();
        assert!(
            equilibrium < frozen,
            "equilibrium {equilibrium} should be below frozen {frozen}"
        );
        assert!(frozen - equilibrium > 50.0, "the gap should be substantial");

        let high_p = adiabatic_flame_temp("CH4", 1.0, 298.15, 5_000_000.0).unwrap();
        assert!(high_p > equilibrium, "{high_p} vs {equilibrium}");
        assert!(high_p < frozen);
    }

    /// Rich combustion — which the frozen-product model refuses outright — is
    /// the case this solver exists for.
    #[test]
    fn rich_mixtures_are_admitted_here_but_not_by_the_frozen_model() {
        assert!(thermochem::adiabatic_flame_temp("CH4", 1.3, 298.15).is_err());
        let t = adiabatic_flame_temp("CH4", 1.3, 298.15, 101_325.0).unwrap();
        assert!((1500.0..3000.0).contains(&t), "rich flame temperature {t}");
    }

    #[test]
    fn flame_temperature_refuses_bad_reactants() {
        assert!(adiabatic_flame_temp("CO2", 1.0, 298.15, 101_325.0).is_err());
        assert!(adiabatic_flame_temp("CH4", 0.0, 298.15, 101_325.0).is_err());
        assert!(adiabatic_flame_temp("CH4", f64::NAN, 298.15, 101_325.0).is_err());
        // Java's Math.clamp(tMid, tReact, 3500.0) throws for tReact > 3500.
        assert!(adiabatic_flame_temp("CH4", 1.0, 4000.0, 101_325.0).is_err());
        assert!(adiabatic_flame_temp("CH4", 1.0, f64::NAN, 101_325.0).is_err());
    }
}
