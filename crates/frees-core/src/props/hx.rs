//! Heat-exchanger effectiveness-NTU relations, LMTD and fin efficiency.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/HeatExchanger.java`
//! (215 LOC), in full.
//!
//! Conventions: `NTU = UA/Cmin`, capacity ratio `Cr = Cmin/Cmax in [0, 1]`,
//! effectiveness `eps in [0, 1]`. `Cr = 0` is the boiling/condensing limit (one
//! stream isothermal), where every arrangement collapses to `1 − exp(−NTU)`.
//! All quantities are dimensionless except [`lmtd`], which carries the
//! temperature-difference units of its arguments.
//!
//! # Guard polarity is deliberate
//!
//! `requireNtu` / `requireCr` / `requireEps` are written `!(ntu >= 0.0)` in the
//! Java: the negation makes NaN take the *reject* branch, which `ntu < 0.0`
//! would not. [`lmtd`] and [`fin_efficiency`], by contrast, guard positively
//! (`dt1 <= 0.0`, `m_l < 0.0`) and therefore let NaN through. Both polarities
//! are transcribed as written — they are engine behaviour, not style.

// See the module docs: the NaN-rejecting `!(x >= y)` guards are the point, so
// clippy's "did you mean `x < y`?" suggestion must not be taken here.
#![allow(clippy::neg_cmp_op_on_partial_ord)]
// `requireCr` is `!(cr >= 0.0 && cr <= 1.0)`. `(0.0..=1.0).contains()` is the
// same comparison, but keeping the Java's literal shape makes the transcription
// checkable line by line against the source.
#![allow(clippy::manual_range_contains)]

use crate::diag::{FreesError, Result};

/// Canonical flow arrangements understood by the effectiveness/NTU functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    Counterflow,
    Parallelflow,
    CrossflowBothUnmixed,
    CrossflowCmaxMixed,
    CrossflowCminMixed,
    ShellAndTube,
}

/// Resolves a user-supplied arrangement spelling, ignoring case, spaces and
/// punctuation so `counter-flow`, `CounterFlow` and `counter flow` all match.
pub fn arrangement(name: &str) -> Result<Arrangement> {
    // Java: `name.toLowerCase().replaceAll("[^a-z0-9]", "")`.
    let key: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    match key.as_str() {
        "counterflow" | "counter" | "countercurrent" => Ok(Arrangement::Counterflow),
        "parallelflow" | "parallel" | "cocurrent" | "coflow" => Ok(Arrangement::Parallelflow),
        "crossflow"
        | "crossflowbothunmixed"
        | "crossbothunmixed"
        | "crossflowunmixed"
        | "bothunmixed" => Ok(Arrangement::CrossflowBothUnmixed),
        "crossflowcmaxmixed" | "cmaxmixed" | "crossflowcminunmixed" => {
            Ok(Arrangement::CrossflowCmaxMixed)
        }
        "crossflowcminmixed" | "cminmixed" | "crossflowcmaxunmixed" => {
            Ok(Arrangement::CrossflowCminMixed)
        }
        "shelltube" | "shellandtube" | "shellandtube1" | "shell" | "shelltube1" => {
            Ok(Arrangement::ShellAndTube)
        }
        _ => Err(FreesError::property(format!(
            "Heat exchanger: unknown flow arrangement '{name}'. Use one of counterflow, \
             parallelflow, crossflow_both_unmixed, crossflow_cmax_mixed, crossflow_cmin_mixed, \
             shell&tube."
        ))),
    }
}

fn require_ntu(ntu: f64) -> Result<()> {
    if !(ntu >= 0.0) {
        return Err(FreesError::property(format!(
            "Heat exchanger: NTU must be >= 0, got {}.",
            java_double_to_string(ntu)
        )));
    }
    Ok(())
}

fn require_cr(cr: f64) -> Result<()> {
    if !(cr >= 0.0 && cr <= 1.0) {
        return Err(FreesError::property(format!(
            "Heat exchanger: capacity ratio Cr = Cmin/Cmax must be in [0, 1], got {}.",
            java_double_to_string(cr)
        )));
    }
    Ok(())
}

fn require_eps(eps: f64) -> Result<()> {
    if !(eps > 0.0 && eps < 1.0) {
        return Err(FreesError::property(format!(
            "Heat exchanger: effectiveness must be in (0, 1), got {}.",
            java_double_to_string(eps)
        )));
    }
    Ok(())
}

/// Effectiveness `eps(NTU, Cr)` for the given flow arrangement.
pub fn effectiveness(ty: Arrangement, ntu: f64, cr: f64) -> Result<f64> {
    require_ntu(ntu)?;
    require_cr(cr)?;
    // Boiling/condensing limit: one stream isothermal, identical for all types.
    if cr == 0.0 {
        return Ok(1.0 - libm::exp(-ntu));
    }
    Ok(match ty {
        Arrangement::Counterflow => {
            if libm::fabs(cr - 1.0) < 1e-10 {
                ntu / (1.0 + ntu)
            } else {
                let e = libm::exp(-ntu * (1.0 - cr));
                (1.0 - e) / (1.0 - cr * e)
            }
        }
        Arrangement::Parallelflow => (1.0 - libm::exp(-ntu * (1.0 + cr))) / (1.0 + cr),
        Arrangement::CrossflowBothUnmixed => {
            // Standard approximate correlation (no simple closed exact form):
            // eps = 1 - exp[ (1/Cr) NTU^0.22 (exp(-Cr NTU^0.78) - 1) ].
            let n022 = libm::pow(ntu, 0.22);
            let n078 = libm::pow(ntu, 0.78);
            1.0 - libm::exp((1.0 / cr) * n022 * (libm::exp(-cr * n078) - 1.0))
        }
        // Cmax mixed, Cmin unmixed.
        Arrangement::CrossflowCmaxMixed => {
            (1.0 / cr) * (1.0 - libm::exp(-cr * (1.0 - libm::exp(-ntu))))
        }
        // Cmin mixed, Cmax unmixed.
        Arrangement::CrossflowCminMixed => {
            1.0 - libm::exp(-(1.0 / cr) * (1.0 - libm::exp(-cr * ntu)))
        }
        Arrangement::ShellAndTube => {
            // One shell pass, 2,4,... tube passes.
            let root = libm::sqrt(1.0 + cr * cr);
            let e = libm::exp(-ntu * root);
            2.0 / (1.0 + cr + root * (1.0 + e) / (1.0 - e))
        }
    })
}

/// Inverse relation `NTU(eps, Cr)`: the number of transfer units needed to
/// reach effectiveness `eps`.
///
/// Closed form where one exists, otherwise a monotone bisection on the forward
/// correlation (crossflow both unmixed).
pub fn ntu(ty: Arrangement, eps: f64, cr: f64) -> Result<f64> {
    require_eps(eps)?;
    require_cr(cr)?;
    if cr == 0.0 {
        return Ok(-libm::log(1.0 - eps));
    }
    let eps_max = max_effectiveness(ty, cr);
    if eps >= eps_max {
        return Err(FreesError::property(format!(
            "Heat exchanger: effectiveness {} is unreachable for this arrangement at Cr={} \
             (limit {} as NTU->inf).",
            format_4(eps),
            format_4(cr),
            format_4(eps_max)
        )));
    }
    Ok(match ty {
        Arrangement::Counterflow => {
            if libm::fabs(cr - 1.0) < 1e-10 {
                eps / (1.0 - eps)
            } else {
                (1.0 / (cr - 1.0)) * libm::log((eps - 1.0) / (eps * cr - 1.0))
            }
        }
        Arrangement::Parallelflow => -libm::log(1.0 - eps * (1.0 + cr)) / (1.0 + cr),
        Arrangement::CrossflowCmaxMixed => -libm::log(1.0 + libm::log(1.0 - cr * eps) / cr),
        Arrangement::CrossflowCminMixed => -(1.0 / cr) * libm::log(1.0 + cr * libm::log(1.0 - eps)),
        Arrangement::ShellAndTube => {
            let root = libm::sqrt(1.0 + cr * cr);
            let e = (2.0 / eps - (1.0 + cr)) / root;
            -libm::log((e - 1.0) / (e + 1.0)) / root
        }
        Arrangement::CrossflowBothUnmixed => {
            bisect_ntu(|n| effectiveness(ty, n, cr).map(|value| value - eps))?
        }
    })
}

/// Maximum reachable effectiveness as `NTU -> infinity`, used to guard
/// inversion.
pub fn max_effectiveness(ty: Arrangement, cr: f64) -> f64 {
    match ty {
        Arrangement::Counterflow | Arrangement::CrossflowBothUnmixed => 1.0,
        Arrangement::Parallelflow => 1.0 / (1.0 + cr),
        Arrangement::CrossflowCmaxMixed => (1.0 / cr) * (1.0 - libm::exp(-cr)),
        Arrangement::CrossflowCminMixed => 1.0 - libm::exp(-1.0 / cr),
        Arrangement::ShellAndTube => 2.0 / (1.0 + cr + libm::sqrt(1.0 + cr * cr)),
    }
}

/// Log-mean temperature difference from the two terminal temperature
/// differences (same units in, same units out).
///
/// For equal differences it returns that common value — the removable
/// singularity of the log mean.
pub fn lmtd(dt1: f64, dt2: f64) -> Result<f64> {
    if dt1 <= 0.0 || dt2 <= 0.0 {
        return Err(FreesError::property(format!(
            "Heat exchanger: LMTD terminal differences must be positive (a temperature cross \
             or pinch gives a non-physical LMTD); got {}, {}.",
            java_double_to_string(dt1),
            java_double_to_string(dt2)
        )));
    }
    if libm::fabs(dt1 - dt2) < 1e-12 * java_max(dt1, dt2) {
        return Ok(0.5 * (dt1 + dt2));
    }
    Ok((dt1 - dt2) / libm::log(dt1 / dt2))
}

/// Efficiency of a straight fin with an adiabatic tip, `eta = tanh(mL)/(mL)`,
/// where `mL = L·sqrt(2h/(k·t))` is the dimensionless fin parameter (use the
/// corrected length for a convective tip). Approaches 1 as `mL -> 0`.
pub fn fin_efficiency(m_l: f64) -> Result<f64> {
    if m_l < 0.0 {
        return Err(FreesError::property(format!(
            "Heat exchanger: fin parameter mL must be >= 0, got {}.",
            java_double_to_string(m_l)
        )));
    }
    if m_l < 1e-8 {
        return Ok(1.0);
    }
    Ok(libm::tanh(m_l) / m_l)
}

/// Bisection for a monotone-increasing-in-NTU residual on `[0, 200]`.
fn bisect_ntu(residual: impl Fn(f64) -> Result<f64>) -> Result<f64> {
    let mut lo = 0.0;
    let mut hi = 200.0;
    let mut flo = residual(lo)?;
    let fhi = residual(hi)?;
    if flo * fhi > 0.0 {
        return Err(FreesError::property(
            "Heat exchanger: requested effectiveness is out of the solvable NTU range.",
        ));
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let fm = residual(mid)?;
        if libm::fabs(fm) < 1e-12 || (hi - lo) < 1e-12 {
            return Ok(mid);
        }
        if (fm > 0.0) == (flo > 0.0) {
            lo = mid;
            flo = fm;
        } else {
            hi = mid;
        }
    }
    Ok(0.5 * (lo + hi))
}

/// `Math.max` (NaN-propagating, `0.0 > -0.0`).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

/// Java `Double.toString(double)` — what `"got " + ntu` produces.
///
/// Plain decimal for `1e-3 <= |d| < 1e7`, otherwise `D.DDDE±X`, always with at
/// least one fractional digit. (`parser::latex` carries a private twin; the two
/// are small enough that duplicating beats coupling a props module to the
/// LaTeX renderer.)
pub(crate) fn java_double_to_string(val: f64) -> String {
    if val.is_nan() {
        return "NaN".to_string();
    }
    if val.is_infinite() {
        return if val > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if val == 0.0 {
        return if val.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_string();
    }
    let abs = libm::fabs(val);
    if (1e-3..1e7).contains(&abs) {
        let s = format!("{val}");
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        let s = format!("{val:e}");
        let (mantissa, exponent) = s.split_once('e').unwrap_or((s.as_str(), "0"));
        if mantissa.contains('.') {
            format!("{mantissa}E{exponent}")
        } else {
            format!("{mantissa}.0E{exponent}")
        }
    }
}

/// Java `String.format("%.4f", d)` — four decimals, **HALF_UP**.
///
/// Rust's `{:.4}` rounds half-to-even, so the two disagree whenever the
/// double's exact decimal expansion terminates in a 5 at the fifth place
/// (every odd multiple of 1/32: 0.03125, 0.09375, …). Twenty decimals is past
/// any tie a binary double can produce there, so rounding that string HALF_UP
/// reproduces Java exactly.
fn format_4(val: f64) -> String {
    if val.is_nan() {
        return "NaN".to_string();
    }
    if val.is_infinite() {
        return if val > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let negative = val.is_sign_negative();
    let s = format!("{:.20}", libm::fabs(val));
    let (int_part, frac) = s.split_once('.').unwrap_or((s.as_str(), ""));
    let round_up = frac.as_bytes().get(4).is_some_and(|d| *d >= b'5');
    let mut digits: Vec<u8> = int_part.bytes().chain(frac.bytes().take(4)).collect();
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let text = String::from_utf8(digits).unwrap_or_default();
    let split = text.len() - 4;
    let body = format!("{}.{}", &text[..split], &text[split..]);
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::Arrangement::*;
    use super::*;

    /// Every expectation is the Java oracle's value (`tools/golden-dumper`,
    /// intrinsics `hx_effectiveness` / `hx_epsilon` / `hx_ntu` / `lmtd` /
    /// `fin_efficiency`).
    fn close(actual: f64, expected: f64) {
        let tol = 1e-14 * libm::fabs(expected).max(1.0);
        assert!(
            libm::fabs(actual - expected) <= tol,
            "expected {expected:.17e}, got {actual:.17e}"
        );
    }

    fn eff(name: &str, ntu_: f64, cr: f64) -> f64 {
        effectiveness(arrangement(name).unwrap(), ntu_, cr).unwrap()
    }

    fn inv(name: &str, eps: f64, cr: f64) -> f64 {
        ntu(arrangement(name).unwrap(), eps, cr).unwrap()
    }

    #[test]
    fn counterflow_matches_the_oracle() {
        close(eff("counterflow", 1.5, 0.6), 0.6726995772651676);
        close(eff("counterflow", 0.25, 0.95), 0.20100292082917287);
        close(eff("counterflow", 3.0, 1.0), 0.75);
        close(eff("counterflow", 2.0, 0.0), 0.8646647167633873);
        close(eff("counterflow", 0.0, 0.5), 0.0);
        close(eff("counterflow", 12.0, 0.3), 0.9998425822536426);
    }

    #[test]
    fn the_counterflow_cr_one_branch_covers_a_true_singularity() {
        // At Cr = 1 the general form is 0/0 — this is what it would return.
        let e = libm::exp(-3.0 * (1.0 - 1.0));
        assert!(((1.0 - e) / (1.0 - e)).is_nan());
        // The |Cr − 1| < 1e-10 window substitutes NTU/(1+NTU) instead, and the
        // two forms agree to the last bit across the seam (the oracle returns
        // 0.75 on both sides).
        close(eff("counterflow", 3.0, 1.0), 0.75);
        close(eff("counterflow", 3.0, 0.99999999995), 0.75);
        close(eff("counterflow", 3.0, 0.999999999), 0.75);
        // Same story for the inverse: eps/(1−eps) replaces a 1/(Cr−1) blow-up.
        close(inv("counterflow", 0.75, 1.0), 3.0);
        close(inv("counterflow", 0.75, 0.99999999995), 3.0);
    }

    #[test]
    fn parallelflow_matches_the_oracle() {
        close(eff("parallelflow", 1.5, 0.6), 0.5683012791941172);
        close(eff("parallel", 0.4, 0.2), 0.31768050682821597);
        close(eff("cocurrent", 5.0, 1.0), 0.49997730003511875);
        close(eff("coflow", 2.0, 0.0), 0.8646647167633873);
    }

    #[test]
    fn crossflow_both_unmixed_matches_the_oracle() {
        close(eff("crossflow", 1.5, 0.6), 0.6401932091181524);
        close(eff("crossflowbothunmixed", 0.3, 0.1), 0.2548807385372508);
        close(eff("crossbothunmixed", 4.0, 1.0), 0.723486657053717);
        close(eff("crossflowunmixed", 0.05, 0.75), 0.047086207256394674);
        close(eff("bothunmixed", 8.0, 0.45), 0.9572144952421648);
    }

    #[test]
    fn crossflow_mixed_variants_match_the_oracle() {
        close(eff("crossflowcmaxmixed", 1.5, 0.6), 0.6209486781372714);
        close(eff("cmaxmixed", 0.8, 0.25), 0.5144473817807254);
        close(eff("crossflowcminunmixed", 6.0, 1.0), 0.6312075457639212);
        close(eff("crossflowcminmixed", 1.5, 0.6), 0.6280703543153826);
        close(eff("cminmixed", 0.8, 0.25), 0.515712716638194);
        close(eff("crossflowcmaxunmixed", 6.0, 1.0), 0.6312075457639212);
    }

    #[test]
    fn shell_and_tube_matches_the_oracle() {
        close(eff("shelltube", 1.5, 0.6), 0.614030543569211);
        close(eff("shellandtube", 0.7, 0.35), 0.4611571704479791);
        close(eff("shell", 4.0, 1.0), 0.5840900955827815);
        close(eff("shellandtube1", 2.5, 0.5), 0.7237007381981052);
        close(eff("shelltube1", 0.2, 0.9), 0.16722043586340038);
    }

    #[test]
    fn arrangement_ignores_case_spaces_and_punctuation() {
        close(eff("Counter-Flow", 1.5, 0.6), 0.6726995772651676);
        close(eff("COUNTER FLOW", 1.5, 0.6), 0.6726995772651676);
        close(eff("countercurrent", 1.5, 0.6), 0.6726995772651676);
        close(eff("Shell & Tube", 1.5, 0.6), 0.614030543569211);
        close(eff("Cross Flow", 1.5, 0.6), 0.6401932091181524);
    }

    #[test]
    fn every_alias_maps_to_the_documented_arrangement() {
        for (name, want) in [
            ("counterflow", Counterflow),
            ("counter", Counterflow),
            ("countercurrent", Counterflow),
            ("parallelflow", Parallelflow),
            ("parallel", Parallelflow),
            ("cocurrent", Parallelflow),
            ("coflow", Parallelflow),
            ("crossflow", CrossflowBothUnmixed),
            ("crossflow_both_unmixed", CrossflowBothUnmixed),
            ("cross both unmixed", CrossflowBothUnmixed),
            ("crossflowunmixed", CrossflowBothUnmixed),
            ("bothunmixed", CrossflowBothUnmixed),
            ("crossflow_cmax_mixed", CrossflowCmaxMixed),
            ("cmaxmixed", CrossflowCmaxMixed),
            ("crossflow_cmin_unmixed", CrossflowCmaxMixed),
            ("crossflow_cmin_mixed", CrossflowCminMixed),
            ("cminmixed", CrossflowCminMixed),
            ("crossflow_cmax_unmixed", CrossflowCminMixed),
            ("shell&tube", ShellAndTube),
            ("shell and tube", ShellAndTube),
            ("shell and tube 1", ShellAndTube),
            ("shell", ShellAndTube),
            ("shelltube1", ShellAndTube),
        ] {
            assert_eq!(arrangement(name).unwrap(), want, "{name}");
        }
    }

    #[test]
    fn cr_zero_is_the_boiling_limit_for_every_arrangement() {
        for ty in [
            Counterflow,
            Parallelflow,
            CrossflowBothUnmixed,
            CrossflowCmaxMixed,
            CrossflowCminMixed,
            ShellAndTube,
        ] {
            close(
                effectiveness(ty, 2.0, 0.0).unwrap(),
                1.0 - libm::exp(-2.0f64),
            );
        }
    }

    // The Cr = 0 inverse is −ln(1 − eps), so eps = 0.5 lands on ln 2 and
    // clippy flags the oracle's digits as an "approximate constant". Keep the
    // oracle's number: it is an expectation copied from the Java run, not a
    // mathematical constant this code is trying to spell.
    #[allow(clippy::approx_constant)]
    #[test]
    fn ntu_inversion_matches_the_oracle() {
        close(
            inv("counterflow", 0.6726995772651676, 0.6),
            1.5000000000000013,
        );
        close(inv("counterflow", 0.75, 1.0), 3.0);
        close(inv("counterflow", 0.5, 0.0), 0.6931471805599453);
        close(inv("parallelflow", 0.5, 0.6), 1.0058986952713127);
        close(
            inv("crossflow", 0.6401932091181524, 0.6),
            1.4999999999986358,
        );
        close(inv("crossflow", 0.2, 0.9), 0.2601086320794366);
        close(inv("bothunmixed", 0.95, 0.25), 4.319434078843187);
        close(inv("cmaxmixed", 0.6, 0.6), 1.3618430955597902);
        close(inv("cminmixed", 0.6, 0.6), 1.3300109590162743);
        close(inv("shelltube", 0.6, 0.6), 1.3991629486280424);
        close(inv("shelltube", 0.4, 0.2), 0.5395270383645363);
    }

    #[test]
    fn ntu_round_trips_the_forward_correlation() {
        for name in [
            "counterflow",
            "parallelflow",
            "crossflow",
            "cmaxmixed",
            "cminmixed",
            "shelltube",
        ] {
            for cr in [0.15, 0.5, 0.85] {
                let target = 1.75_f64;
                let eps = eff(name, target, cr);
                let back = inv(name, eps, cr);
                assert!(
                    libm::fabs(back - target) < 1e-9,
                    "{name} Cr={cr}: NTU {target} -> eps {eps} -> NTU {back}"
                );
            }
        }
    }

    #[test]
    fn max_effectiveness_bounds_the_forward_correlation() {
        for (ty, cr) in [
            (Parallelflow, 0.6),
            (CrossflowCmaxMixed, 0.6),
            (CrossflowCminMixed, 0.6),
            (ShellAndTube, 0.6),
        ] {
            let limit = max_effectiveness(ty, cr);
            let huge = effectiveness(ty, 1e6, cr).unwrap();
            assert!(huge <= limit + 1e-12, "{ty:?}: {huge} > {limit}");
            assert!(huge > limit - 1e-6, "{ty:?}: {huge} never reaches {limit}");
        }
    }

    #[test]
    fn lmtd_matches_the_oracle() {
        close(lmtd(50.0, 20.0).unwrap(), 32.74070003811874);
        close(lmtd(30.0, 30.0).unwrap(), 30.0);
        close(lmtd(30.0, 30.00000000000001).unwrap(), 30.000000000000007);
        close(lmtd(1e-3, 500.0).unwrap(), 38.10281620923246);
    }

    #[test]
    fn lmtd_takes_the_removable_singularity_within_a_relative_1e_minus_12() {
        // |dt1 − dt2| < 1e-12·max(dt1, dt2) is the arithmetic-mean branch.
        assert_eq!(lmtd(30.0, 30.0).unwrap(), 30.0);
        let near = 30.0 * (1.0 + 1e-13);
        assert_eq!(lmtd(30.0, near).unwrap(), 0.5 * (30.0 + near));
    }

    #[test]
    fn fin_efficiency_matches_the_oracle() {
        close(fin_efficiency(0.0).unwrap(), 1.0);
        close(fin_efficiency(1e-9).unwrap(), 1.0);
        close(fin_efficiency(1e-8).unwrap(), 1.0);
        close(fin_efficiency(0.5).unwrap(), 0.9242343145200195);
        close(fin_efficiency(1.0).unwrap(), 0.7615941559557649);
        close(fin_efficiency(3.5).unwrap(), 0.28519368503177106);
        close(fin_efficiency(50.0).unwrap(), 0.02);
    }

    #[test]
    fn fin_efficiency_short_circuits_below_1e_minus_8() {
        // The Java returns exactly 1.0 there rather than tanh(mL)/mL.
        assert_eq!(fin_efficiency(9.9e-9).unwrap(), 1.0);
        assert!(fin_efficiency(1e-7).unwrap() < 1.0);
    }

    // ---- errors ------------------------------------------------------------

    #[test]
    fn unknown_arrangement_lists_the_canonical_spellings() {
        assert_eq!(
            arrangement("spiral").unwrap_err(),
            FreesError::property(
                "Heat exchanger: unknown flow arrangement 'spiral'. Use one of counterflow, \
                 parallelflow, crossflow_both_unmixed, crossflow_cmax_mixed, \
                 crossflow_cmin_mixed, shell&tube."
            )
        );
    }

    #[test]
    fn domain_guards_carry_the_java_text() {
        assert_eq!(
            effectiveness(Counterflow, -0.5, 0.5).unwrap_err(),
            FreesError::property("Heat exchanger: NTU must be >= 0, got -0.5.")
        );
        assert_eq!(
            effectiveness(Counterflow, 1.0, 1.5).unwrap_err(),
            FreesError::property(
                "Heat exchanger: capacity ratio Cr = Cmin/Cmax must be in [0, 1], got 1.5."
            )
        );
        assert_eq!(
            ntu(Counterflow, 1.5, 0.5).unwrap_err(),
            FreesError::property("Heat exchanger: effectiveness must be in (0, 1), got 1.5.")
        );
        assert_eq!(
            ntu(Parallelflow, 0.9, 0.6).unwrap_err(),
            FreesError::property(
                "Heat exchanger: effectiveness 0.9000 is unreachable for this arrangement at \
                 Cr=0.6000 (limit 0.6250 as NTU->inf)."
            )
        );
        assert_eq!(
            lmtd(10.0, -3.0).unwrap_err(),
            FreesError::property(
                "Heat exchanger: LMTD terminal differences must be positive (a temperature \
                 cross or pinch gives a non-physical LMTD); got 10.0, -3.0."
            )
        );
        assert_eq!(
            fin_efficiency(-0.25).unwrap_err(),
            FreesError::property("Heat exchanger: fin parameter mL must be >= 0, got -0.25.")
        );
        assert_eq!(
            ntu(CrossflowBothUnmixed, 0.999, 0.5).unwrap_err(),
            FreesError::property(
                "Heat exchanger: requested effectiveness is out of the solvable NTU range."
            )
        );
    }

    #[test]
    fn nan_is_rejected_by_the_negated_guards() {
        assert!(effectiveness(Counterflow, f64::NAN, 0.5).is_err());
        assert!(effectiveness(Counterflow, 1.0, f64::NAN).is_err());
        assert!(ntu(Counterflow, f64::NAN, 0.5).is_err());
        // …but survives the positively-guarded pair, exactly as in Java.
        assert!(lmtd(f64::NAN, 1.0).unwrap().is_nan());
        assert!(fin_efficiency(f64::NAN).unwrap().is_nan());
    }

    #[test]
    fn java_double_to_string_matches_the_jdk() {
        assert_eq!(java_double_to_string(1.5), "1.5");
        assert_eq!(java_double_to_string(-0.5), "-0.5");
        assert_eq!(java_double_to_string(10.0), "10.0");
        assert_eq!(java_double_to_string(-3.0), "-3.0");
        assert_eq!(java_double_to_string(0.0), "0.0");
        assert_eq!(java_double_to_string(-0.0), "-0.0");
        assert_eq!(java_double_to_string(1e7), "1.0E7");
        assert_eq!(java_double_to_string(1e-4), "1.0E-4");
        assert_eq!(java_double_to_string(f64::NAN), "NaN");
        assert_eq!(java_double_to_string(f64::INFINITY), "Infinity");
        assert_eq!(java_double_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn format_4_rounds_half_up_like_java_not_half_even_like_rust() {
        assert_eq!(format_4(0.9), "0.9000");
        assert_eq!(format_4(0.625), "0.6250");
        assert_eq!(format_4(1.0), "1.0000");
        // 0.03125 is exactly representable and ties at the fifth decimal:
        // Java HALF_UP gives 0.0313, Rust's `{:.4}` would give 0.0312.
        assert_eq!(format_4(0.03125), "0.0313");
        assert_eq!(format_4(0.09375), "0.0938");
        assert_eq!(format_4(-0.03125), "-0.0313");
        // Carry propagation out of the integer part.
        assert_eq!(format_4(9.99999), "10.0000");
        assert_eq!(format_4(0.99999), "1.0000");
        assert_eq!(format_4(-1e-9), "-0.0000");
        assert_eq!(format_4(f64::NAN), "NaN");
    }
}
