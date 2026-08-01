//! Rational functions over ℚ, and partial-fraction decomposition.
//!
//! Port note: like [`super::poly`], there is no Java file behind this — Symja
//! supplied `Together`, `Cancel`, `Numerator`, `Denominator` and `Apart`, and
//! Symja cannot ship (see [`crate::cas`]). What is here is the exact-arithmetic
//! replacement those five operations need, plus the partial-fraction
//! decomposition `InverseLaplaceTransform` is built on.
//!
//! # Two types, because there are two jobs
//!
//! * [`RatFun`] is **multivariate** (over [`MPoly`]) and is the IR
//!   [`crate::cas::ops`] lowers every expression into. `Expand`, `Simplify`,
//!   `Together`, `Cancel`, `Numerator`, `Denominator` and `Collect` all operate
//!   on whole expressions, which routinely carry several symbols
//!   (`Together(1/(a+b) + 1/c)`), so a univariate IR would not do.
//! * [`URatFun`] is **univariate** (over [`UPoly`]) and is where the exact
//!   theory lives: [`URatFun::partial_fractions`] needs a complete
//!   factorisation over ℚ, which [`super::poly`] provides for one variable and
//!   deliberately does not provide for several.
//!
//! [`RatFun::partial_fractions`] bridges them: it collapses to one variable and
//! delegates.
//!
//! # Canonical forms
//!
//! [`RatFun`] — the contract `cas::ops` is written against:
//!
//! * the denominator is a **primitive integer polynomial with a positive
//!   leading coefficient** (in the [`super::poly::Mono`] order), so every rational
//!   coefficient lives in the numerator;
//! * numerator and denominator share no common factor, via [`MPoly::gcd`].
//!   That GCD certifies its own answer by exact division, so the one way this
//!   can degrade — its PRS work budget running out on a large multivariate
//!   input — leaves the value **under-reduced but correct**, never wrong;
//! * the zero function is exactly `0/1`.
//!
//! Consequently `denom().as_constant() == Some(1)` **exactly** when the value
//! is a polynomial — the branch `Expand`, `Collect`, `Simplify` and `Integrate`
//! all take.
//!
//! [`URatFun`] — the denominator is **monic** instead. That is what makes
//! `Cancel((2x² + 4x)/(4x))` come back as the polynomial `x/2 + 1`, which is how
//! the oracle spells it (`1/2*(2+x)`), and it makes the denominator of a
//! partial-fraction base directly readable as a pole.
//!
//! # Exactness
//!
//! Everything is [`Rat`] arithmetic. Nothing here computes a root numerically:
//! partial fractions go through the **rational** factorisation of the
//! denominator, so an irreducible quadratic stays an irreducible quadratic
//! instead of being split into a conjugate pair of floating-point poles. That
//! is what lets `Apart(1/((s+1)(s²+1)), s)` reproduce the oracle's
//! `1/(2(1+s)) + (1-s)/(2(1+s²))` exactly.

// `cas::ops` is written against inherent `add`/`sub`/`mul`/`div`/`neg` methods
// on `RatFun` (its module header records the exact ten-item wire contract), and
// a `Result`-free `Option` shape for the fallible ones. The `std::ops` traits
// are implemented as well, so `&a + &b` works too — but the inherent methods
// have to keep their names, so this lint cannot be satisfied here.
#![allow(clippy::should_implement_trait)]

use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};

use num_traits::{One, Signed, Zero};

use super::poly::{MPoly, PolyError, PolyResult, Rat, UPoly};

// ---------------------------------------------------------------------------
// Multivariate rational functions — the `cas::ops` IR
// ---------------------------------------------------------------------------

/// A rational function over ℚ in any number of variables, held in the canonical
/// form described in the module documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatFun {
    num: MPoly,
    den: MPoly,
}

impl Default for RatFun {
    fn default() -> RatFun {
        RatFun::zero()
    }
}

impl RatFun {
    /// Build and normalise. `None` **iff** `den` is the zero polynomial.
    pub fn new(num: MPoly, den: MPoly) -> Option<RatFun> {
        if den.is_zero() {
            return None;
        }
        Some(RatFun::normalise(num, den))
    }

    /// A polynomial seen as a rational function.
    pub fn from_poly(p: MPoly) -> RatFun {
        RatFun::normalise(p, MPoly::one())
    }

    /// The constant `c`.
    pub fn constant(c: Rat) -> RatFun {
        RatFun::from_poly(MPoly::constant(c))
    }

    /// The single generator `name`.
    pub fn var(name: &str) -> RatFun {
        RatFun::from_poly(MPoly::var(name))
    }

    pub fn zero() -> RatFun {
        RatFun {
            num: MPoly::zero(),
            den: MPoly::one(),
        }
    }

    pub fn one() -> RatFun {
        RatFun {
            num: MPoly::one(),
            den: MPoly::one(),
        }
    }

    /// The numerator. `Numerator(...)` returns this.
    pub fn numer(&self) -> &MPoly {
        &self.num
    }

    /// The denominator: primitive, integer, positive leading coefficient.
    /// `Denominator(...)` returns this.
    pub fn denom(&self) -> &MPoly {
        &self.den
    }

    /// Consume, returning `(numerator, denominator)`.
    pub fn into_parts(self) -> (MPoly, MPoly) {
        (self.num, self.den)
    }

    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    /// Is the denominator `1`? By the canonical form that is exactly the test
    /// "is this value a polynomial".
    pub fn is_polynomial(&self) -> bool {
        self.den.as_constant().is_some_and(|c| c.is_one())
    }

    /// The numerator, when the denominator is `1`.
    pub fn as_polynomial(&self) -> Option<&MPoly> {
        self.is_polynomial().then_some(&self.num)
    }

    pub fn add(&self, rhs: &RatFun) -> RatFun {
        RatFun::normalise(
            &(&self.num * &rhs.den) + &(&rhs.num * &self.den),
            &self.den * &rhs.den,
        )
    }

    pub fn sub(&self, rhs: &RatFun) -> RatFun {
        RatFun::normalise(
            &(&self.num * &rhs.den) - &(&rhs.num * &self.den),
            &self.den * &rhs.den,
        )
    }

    pub fn mul(&self, rhs: &RatFun) -> RatFun {
        RatFun::normalise(&self.num * &rhs.num, &self.den * &rhs.den)
    }

    /// `self / rhs`. `None` **iff** `rhs` is zero.
    pub fn div(&self, rhs: &RatFun) -> Option<RatFun> {
        if rhs.num.is_zero() {
            return None;
        }
        Some(RatFun::normalise(
            &self.num * &rhs.den,
            &self.den * &rhs.num,
        ))
    }

    pub fn neg(&self) -> RatFun {
        RatFun {
            num: -&self.num,
            den: self.den.clone(),
        }
    }

    /// `1/self`. `None` **iff** `self` is zero.
    pub fn inv(&self) -> Option<RatFun> {
        RatFun::one().div(self)
    }

    /// `self^exp`. `None` **iff** `self` is zero and `exp` is negative.
    pub fn pow(&self, exp: i32) -> Option<RatFun> {
        let n = exp.unsigned_abs() as usize;
        if exp >= 0 {
            return Some(RatFun::normalise(self.num.pow(n), self.den.pow(n)));
        }
        if self.num.is_zero() {
            return None;
        }
        Some(RatFun::normalise(self.den.pow(n), self.num.pow(n)))
    }

    /// Collapse to the univariate world, when at most `var` occurs.
    pub fn to_univariate(&self, var: &str) -> Option<URatFun> {
        let num = self.num.to_upoly(var)?;
        let den = self.den.to_upoly(var)?;
        URatFun::new(num, den).ok()
    }

    /// `Apart(self, var)` — see [`URatFun::partial_fractions`].
    ///
    /// Fails with [`PolyError::Internal`] if the value is not univariate in
    /// `var`: partial fractions over several variables are not defined here,
    /// and guessing would be worse than refusing.
    pub fn partial_fractions(&self, var: &str) -> PolyResult<PartialFractions> {
        self.to_univariate(var)
            .ok_or(PolyError::Internal(
                "partial fractions need a value in a single variable",
            ))?
            .partial_fractions()
    }

    /// Reduce, then move all rational scaling out of the denominator.
    ///
    /// Total by construction: `den` is non-zero on every path in, and
    /// [`MPoly::gcd`] certifies that it divides both of its inputs. The `_` arm
    /// is unreachable; it keeps the un-reduced pair (still the same value,
    /// merely not in lowest terms) rather than panicking, because `cas`
    /// compiles into a browser tab where a panic aborts the session.
    fn normalise(num: MPoly, den: MPoly) -> RatFun {
        if num.is_zero() || den.is_zero() {
            return RatFun::zero();
        }
        // Plain polynomial arithmetic — `den` a non-zero constant — is the
        // overwhelmingly common case and is *already* in lowest terms once the
        // constant is divided out. Reaching [`MPoly::gcd`] for it is pure
        // waste: that runs a 20,000-step multivariate PRS and two exact
        // divisions, and a left-associated sum of `n` generators normalises
        // `n` times. Skipping it takes `Expand` over a 200-generator sum from
        // 256 s to 0.41 s. The value is unchanged — the general path
        // below reduces by `gcd(num, c) = 1` and then divides by exactly this
        // same constant. Regression:
        // `tests/cas_control_robustness.rs::a_wide_sum_of_distinct_generators_stays_fast`.
        if let Some(c) = den.as_constant() {
            let inv = c.recip();
            return RatFun {
                num: num.scale(&inv),
                den: MPoly::one(),
            };
        }
        let g = num.gcd(&den);
        let (num, den) = match (num.exact_div(&g), den.exact_div(&g)) {
            (Some(n), Some(d)) if !d.is_zero() => (n, d),
            _ => (num, den),
        };
        let content = den.content();
        let scale = if den.lc().is_negative() {
            -content
        } else {
            content
        };
        if scale.is_zero() {
            return RatFun::zero();
        }
        let inv = scale.recip();
        RatFun {
            num: num.scale(&inv),
            den: den.scale(&inv),
        }
    }
}

impl Neg for &RatFun {
    type Output = RatFun;
    fn neg(self) -> RatFun {
        RatFun::neg(self)
    }
}

impl Add for &RatFun {
    type Output = RatFun;
    fn add(self, rhs: &RatFun) -> RatFun {
        RatFun::add(self, rhs)
    }
}

impl Sub for &RatFun {
    type Output = RatFun;
    fn sub(self, rhs: &RatFun) -> RatFun {
        RatFun::sub(self, rhs)
    }
}

impl Mul for &RatFun {
    type Output = RatFun;
    fn mul(self, rhs: &RatFun) -> RatFun {
        RatFun::mul(self, rhs)
    }
}

// ---------------------------------------------------------------------------
// Univariate rational functions — where the exact theory lives
// ---------------------------------------------------------------------------

/// A rational function `numerator / denominator` over ℚ in **one** variable,
/// with a monic denominator and no common factor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct URatFun {
    num: UPoly,
    den: UPoly,
}

impl Default for URatFun {
    fn default() -> URatFun {
        URatFun::zero()
    }
}

impl URatFun {
    /// Build and normalise. The only failure is a zero denominator.
    pub fn new(num: UPoly, den: UPoly) -> PolyResult<URatFun> {
        if den.is_zero() {
            return Err(PolyError::DivisionByZero);
        }
        Ok(URatFun::normalise(num, den))
    }

    /// A polynomial seen as a rational function.
    pub fn from_poly(p: UPoly) -> URatFun {
        URatFun {
            num: p,
            den: UPoly::one(),
        }
    }

    /// The constant `c`.
    pub fn constant(c: Rat) -> URatFun {
        URatFun::from_poly(UPoly::constant(c))
    }

    pub fn zero() -> URatFun {
        URatFun {
            num: UPoly::zero(),
            den: UPoly::one(),
        }
    }

    pub fn one() -> URatFun {
        URatFun::from_poly(UPoly::one())
    }

    /// The reciprocal of a polynomial — `1/(s + a)` and friends.
    pub fn recip_of(p: UPoly) -> PolyResult<URatFun> {
        URatFun::new(UPoly::one(), p)
    }

    /// The numerator in canonical form.
    pub fn numer(&self) -> &UPoly {
        &self.num
    }

    /// The **monic** denominator in canonical form.
    pub fn denom(&self) -> &UPoly {
        &self.den
    }

    /// Consume, returning `(numerator, denominator)`.
    pub fn into_parts(self) -> (UPoly, UPoly) {
        (self.num, self.den)
    }

    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }

    /// Is the denominator a constant — i.e. is this really a polynomial?
    pub fn is_polynomial(&self) -> bool {
        self.den.is_constant()
    }

    /// The numerator, when the denominator is trivial.
    pub fn as_polynomial(&self) -> Option<&UPoly> {
        self.is_polynomial().then_some(&self.num)
    }

    /// Is this a proper rational function (`deg num < deg den`)?
    pub fn is_proper(&self) -> bool {
        match (self.num.degree(), self.den.degree()) {
            (None, _) => true,
            (Some(n), Some(d)) => n < d,
            (Some(_), None) => false,
        }
    }

    /// Evaluate at an exact rational point; `None` at a pole.
    pub fn eval(&self, at: &Rat) -> Option<Rat> {
        let d = self.den.eval(at);
        if d.is_zero() {
            return None;
        }
        Some(self.num.eval(at) / d)
    }

    /// `1/self`. Fails on the zero function.
    pub fn inv(&self) -> PolyResult<URatFun> {
        if self.num.is_zero() {
            return Err(PolyError::DivisionByZero);
        }
        Ok(URatFun::normalise(self.den.clone(), self.num.clone()))
    }

    /// `self / rhs`. Not the [`std::ops::Div`] trait, because division by zero
    /// is a real outcome here and a `Result` says so; `Div` would have to panic.
    pub fn checked_div(&self, rhs: &URatFun) -> PolyResult<URatFun> {
        if rhs.num.is_zero() {
            return Err(PolyError::DivisionByZero);
        }
        Ok(URatFun::normalise(
            &self.num * &rhs.den,
            &self.den * &rhs.num,
        ))
    }

    /// `self^exp`, with negative exponents inverting first.
    pub fn pow(&self, exp: i32) -> PolyResult<URatFun> {
        let n = exp.unsigned_abs() as usize;
        if exp >= 0 {
            return Ok(URatFun {
                num: self.num.pow(n),
                den: self.den.pow(n),
            });
        }
        let inverted = self.inv()?;
        Ok(URatFun {
            num: inverted.num.pow(n),
            den: inverted.den.pow(n),
        })
    }

    /// Lift into the multivariate world under the name `var`.
    pub fn to_multivariate(&self, var: &str) -> RatFun {
        RatFun::normalise(
            MPoly::from_upoly(&self.num, var),
            MPoly::from_upoly(&self.den, var),
        )
    }

    /// **Partial-fraction decomposition over ℚ.**
    ///
    /// Returns the polynomial part plus one term per `(irreducible factor,
    /// power)` of the denominator, so that
    ///
    /// ```text
    /// self = polynomial + Σ numeratorᵢ / baseᵢ^powerᵢ
    /// ```
    ///
    /// Each `base` is **monic and irreducible over ℚ**, and each `numerator`
    /// has degree strictly below its base — so a linear base gives a residue, a
    /// repeated base gives one term per power, and an irreducible quadratic
    /// keeps a linear numerator instead of being split over ℂ. All three shapes
    /// are checked against the Java oracle in this module's tests.
    ///
    /// The result is verified by recombination before it is returned.
    ///
    /// # Algorithm
    ///
    /// Divide out the polynomial part, factor the denominator into `Π qᵢ^eᵢ`,
    /// then split the proper remainder by CRT: with `dᵢ = qᵢ^eᵢ` and
    /// `mᵢ = den / dᵢ`, the unique `nᵢ` with `deg nᵢ < deg dᵢ` and
    /// `Σ nᵢ·mᵢ = r` is `nᵢ ≡ r·mᵢ⁻¹ (mod dᵢ)`. Finally expand each `nᵢ` in
    /// base `qᵢ`; digit `j` is the numerator over `qᵢ^(eᵢ-j)`.
    pub fn partial_fractions(&self) -> PolyResult<PartialFractions> {
        let (quotient, remainder) = self.num.div_rem(&self.den)?;
        if remainder.is_zero() {
            return Ok(PartialFractions {
                polynomial: quotient,
                terms: Vec::new(),
            });
        }

        let factors = self.den.monic_factors()?;
        let mut terms: Vec<PartialFractionTerm> = Vec::new();

        for (base, power) in &factors {
            let block = base.pow(*power);
            let cofactor = self.den.exact_div(&block).ok_or(PolyError::Internal(
                "partial fractions: a denominator factor does not divide the denominator",
            ))?;
            // s·cofactor ≡ 1 (mod block), because the blocks are pairwise coprime.
            let (g, s, _) = cofactor.ext_gcd(&block);
            if !g.is_constant() || g.is_zero() {
                return Err(PolyError::Internal(
                    "partial fractions: denominator blocks are not coprime",
                ));
            }
            let (_, mut digits) = (&remainder * &s).div_rem(&block)?;

            for j in 0..*power {
                let (quo, rem) = digits.div_rem(base)?;
                if !rem.is_zero() {
                    terms.push(PartialFractionTerm {
                        numerator: rem,
                        base: base.clone(),
                        power: power - j,
                    });
                }
                digits = quo;
            }
            if !digits.is_zero() {
                return Err(PolyError::Internal(
                    "partial fractions: base expansion left a remainder",
                ));
            }
        }

        let result = PartialFractions {
            polynomial: quotient,
            terms,
        };
        if result.recombine()? != *self {
            return Err(PolyError::Internal(
                "partial fractions: recombination does not reproduce the input",
            ));
        }
        Ok(result)
    }

    /// Render as `num/den` with an explicit variable name — a debugging and
    /// test aid, not the user-facing spelling (that belongs to `cas::ops`).
    pub fn to_string_in(&self, var: &str) -> String {
        if self.is_polynomial() && self.den.lc().is_one() {
            return self.num.to_string_in(var);
        }
        format!(
            "({})/({})",
            self.num.to_string_in(var),
            self.den.to_string_in(var)
        )
    }

    /// Reduce and make the denominator monic. Total, for the same reason
    /// [`RatFun::normalise`] is.
    fn normalise(num: UPoly, den: UPoly) -> URatFun {
        if num.is_zero() || den.is_zero() {
            return URatFun::zero();
        }
        let g = num.gcd(&den);
        let (num, den) = match (num.exact_div(&g), den.exact_div(&g)) {
            (Some(n), Some(d)) if !d.is_zero() => (n, d),
            _ => (num, den),
        };
        let inv_lc = den.lc().recip();
        URatFun {
            num: num.scale(&inv_lc),
            den: den.scale(&inv_lc),
        }
    }
}

impl fmt::Display for URatFun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_in("x"))
    }
}

impl Neg for &URatFun {
    type Output = URatFun;
    fn neg(self) -> URatFun {
        URatFun {
            num: -&self.num,
            den: self.den.clone(),
        }
    }
}

impl Neg for URatFun {
    type Output = URatFun;
    fn neg(self) -> URatFun {
        -&self
    }
}

impl Add for &URatFun {
    type Output = URatFun;
    fn add(self, rhs: &URatFun) -> URatFun {
        URatFun::normalise(
            &(&self.num * &rhs.den) + &(&rhs.num * &self.den),
            &self.den * &rhs.den,
        )
    }
}

impl Sub for &URatFun {
    type Output = URatFun;
    fn sub(self, rhs: &URatFun) -> URatFun {
        URatFun::normalise(
            &(&self.num * &rhs.den) - &(&rhs.num * &self.den),
            &self.den * &rhs.den,
        )
    }
}

impl Mul for &URatFun {
    type Output = URatFun;
    fn mul(self, rhs: &URatFun) -> URatFun {
        URatFun::normalise(&self.num * &rhs.num, &self.den * &rhs.den)
    }
}

impl Add for URatFun {
    type Output = URatFun;
    fn add(self, rhs: URatFun) -> URatFun {
        &self + &rhs
    }
}

impl Sub for URatFun {
    type Output = URatFun;
    fn sub(self, rhs: URatFun) -> URatFun {
        &self - &rhs
    }
}

impl Mul for URatFun {
    type Output = URatFun;
    fn mul(self, rhs: URatFun) -> URatFun {
        &self * &rhs
    }
}

// ---------------------------------------------------------------------------
// Partial fractions
// ---------------------------------------------------------------------------

/// One term of a partial-fraction decomposition: `numerator / base^power`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialFractionTerm {
    /// Degree strictly below `base`. Never the zero polynomial — terms that
    /// would vanish are dropped.
    pub numerator: UPoly,
    /// **Monic and irreducible over ℚ.** Degree 1 is a real pole; degree 2 is a
    /// complex-conjugate pair that stays exact by not being split.
    pub base: UPoly,
    /// At least 1.
    pub power: usize,
}

impl PartialFractionTerm {
    /// The pole `a` when `base = x - a`, i.e. for a real pole. `None` for an
    /// irreducible quadratic or higher.
    pub fn linear_pole(&self) -> Option<Rat> {
        (self.base.degree() == Some(1)).then(|| -self.base.coeff(0))
    }

    /// This term as a rational function in its own right.
    pub fn to_uratfun(&self) -> PolyResult<URatFun> {
        URatFun::new(self.numerator.clone(), self.base.pow(self.power))
    }
}

/// The result of [`URatFun::partial_fractions`].
///
/// `value = polynomial + Σ terms`. `polynomial` is the zero polynomial exactly
/// when the input was proper, which is the common case for a Laplace-domain
/// expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialFractions {
    /// The polynomial part from dividing numerator by denominator.
    pub polynomial: UPoly,
    /// Deterministic order: bases follow [`UPoly::factor`]'s ordering — by
    /// degree, then by the coefficients of the **primitive integer**
    /// representative, which is what fixes `(s+1)` before `(s+1/2)` — and
    /// within one base, powers descend. That is the order the Java oracle
    /// prints in every case checked in this module's tests.
    pub terms: Vec<PartialFractionTerm>,
}

impl PartialFractions {
    /// Sum the decomposition back into a single rational function.
    pub fn recombine(&self) -> PolyResult<URatFun> {
        let mut acc = URatFun::from_poly(self.polynomial.clone());
        for term in &self.terms {
            acc = &acc + &term.to_uratfun()?;
        }
        Ok(acc)
    }

    /// Is the decomposition trivial (no proper part at all)?
    pub fn is_polynomial(&self) -> bool {
        self.terms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::poly::{rat, rat_int};
    use super::*;

    fn p(coeffs: &[i64]) -> UPoly {
        UPoly::from_ints(coeffs)
    }

    fn rf(num: &[i64], den: &[i64]) -> URatFun {
        URatFun::new(p(num), p(den)).expect("non-zero denominator")
    }

    /// Reproducible generator, same shape as the one in `poly`.
    struct TestRng(u64);
    impl TestRng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 11
        }
        fn int(&mut self, lo: i64, hi: i64) -> i64 {
            let span = (hi - lo + 1) as u64;
            lo + (self.next() % span) as i64
        }
        fn poly(&mut self, max_deg: usize, mag: i64) -> UPoly {
            let d = (self.next() as usize) % (max_deg + 1);
            let mut c: Vec<Rat> = (0..=d).map(|_| rat_int(self.int(-mag, mag))).collect();
            if c.last().is_some_and(Zero::is_zero) {
                *c.last_mut().expect("non-empty") = rat_int(1);
            }
            UPoly::from_coeffs(c)
        }
    }

    // --- the multivariate `cas::ops` contract ------------------------------

    #[test]
    fn ratfun_denominator_is_primitive_integer_with_positive_lc() {
        // 1/(2x + 4): the rational scaling must end up in the numerator, so the
        // denominator stays a primitive integer polynomial.
        let f = RatFun::new(
            MPoly::one(),
            &MPoly::var("x").scale(&rat_int(2)) + &MPoly::constant(rat_int(4)),
        )
        .expect("non-zero denominator");
        assert_eq!(
            f.denom(),
            &(&MPoly::var("x") + &MPoly::constant(rat_int(2)))
        );
        assert_eq!(f.numer(), &MPoly::constant(rat(1, 2).expect("valid")));

        // A negative leading coefficient flips into the numerator.
        let g = RatFun::new(MPoly::one(), -&MPoly::var("x")).expect("non-zero");
        assert_eq!(g.denom(), &MPoly::var("x"));
        assert_eq!(g.numer(), &MPoly::constant(rat_int(-1)));
    }

    #[test]
    fn ratfun_is_polynomial_iff_denominator_is_one() {
        // This is the branch `Expand`/`Collect`/`Simplify`/`Integrate` take.
        let poly = RatFun::from_poly(&MPoly::var("x") * &MPoly::var("y"));
        assert!(poly.is_polynomial());
        assert_eq!(poly.denom().as_constant(), Some(rat_int(1)));

        let frac = RatFun::new(MPoly::one(), MPoly::var("x")).expect("non-zero");
        assert!(!frac.is_polynomial());
        assert_eq!(frac.as_polynomial(), None);

        // Zero counts as a polynomial.
        assert!(RatFun::zero().is_polynomial());
        assert_eq!(RatFun::zero().denom().as_constant(), Some(rat_int(1)));

        // A constant denominator is divided out entirely, not left as `2`.
        let halved = RatFun::new(MPoly::var("x"), MPoly::constant(rat_int(2))).expect("non-zero");
        assert!(halved.is_polynomial());
        assert_eq!(
            halved.numer(),
            &MPoly::var("x").scale(&rat(1, 2).expect("valid"))
        );
    }

    #[test]
    fn ratfun_zero_denominator_is_none_not_a_panic() {
        assert_eq!(RatFun::new(MPoly::one(), MPoly::zero()), None);
        assert_eq!(RatFun::one().div(&RatFun::zero()), None);
        assert_eq!(RatFun::zero().inv(), None);
        assert_eq!(RatFun::zero().pow(-1), None);
        // 0^0 and 0^positive are both fine.
        assert_eq!(RatFun::zero().pow(0), Some(RatFun::one()));
        assert_eq!(RatFun::zero().pow(3), Some(RatFun::zero()));
    }

    #[test]
    fn ratfun_arithmetic_cancels_what_it_can_see() {
        let x = RatFun::var("x");
        let y = RatFun::var("y");
        // 1/x + 1/y = (x + y)/(x·y)
        let sum = x.inv().expect("non-zero").add(&y.inv().expect("non-zero"));
        assert_eq!(sum.numer(), &(&MPoly::var("x") + &MPoly::var("y")));
        assert_eq!(sum.denom(), &(&MPoly::var("x") * &MPoly::var("y")));

        // (x·y)/x reduces by the monomial gcd, in several variables.
        let reduced = (&x * &y).div(&x).expect("non-zero divisor");
        assert!(reduced.is_polynomial());
        assert_eq!(reduced.numer(), &MPoly::var("y"));

        // a·(x² − 1) / (a·x + a) → x − 1: the monomial gcd removes `a`, and what
        // is left is univariate, so the full GCD applies.
        let a = MPoly::var("a");
        let num = &a * &(&(&MPoly::var("x") * &MPoly::var("x")) - &MPoly::one());
        let den = &(&a * &MPoly::var("x")) + &a;
        let f = RatFun::new(num, den).expect("non-zero");
        assert!(f.is_polynomial());
        assert_eq!(f.numer(), &(&MPoly::var("x") - &MPoly::one()));

        // Field laws hold on the IR.
        assert_eq!(
            sum.sub(&y.inv().expect("non-zero")),
            x.inv().expect("non-zero")
        );
        assert_eq!(x.neg().neg(), x);
        assert_eq!(x.pow(3).expect("ok"), x.mul(&x).mul(&x));
        assert_eq!(
            x.pow(-2).expect("ok").mul(&x.pow(2).expect("ok")),
            RatFun::one()
        );
    }

    #[test]
    fn ratfun_cancels_genuinely_multivariate_common_factors() {
        // (x² − y²)/(x + y) = x − y. Neither side is univariate, so this is the
        // recursive multivariate GCD doing the work.
        let x = MPoly::var("x");
        let y = MPoly::var("y");
        let f = RatFun::new(&(&x * &x) - &(&y * &y), &x + &y).expect("non-zero");
        assert!(f.is_polynomial());
        assert_eq!(f.numer(), &(&x - &y));

        // (x³ − y³)/(x² + x·y + y²) = x − y.
        let g = RatFun::new(
            &x.pow(3) - &y.pow(3),
            &(&(&x * &x) + &(&x * &y)) + &(&y * &y),
        )
        .expect("non-zero");
        assert!(g.is_polynomial());
        assert_eq!(g.numer(), &(&x - &y));

        // Three variables, with a repeated factor and a monomial part:
        // z·(x+y)²·(x−1)  over  z²·(x+y)  →  (x+y)(x−1)/z
        let z = MPoly::var("z");
        let num = &(&z * &(&x + &y).pow(2)) * &(&x - &MPoly::one());
        let den = &(&z * &z) * &(&x + &y);
        let h = RatFun::new(num, den).expect("non-zero");
        assert_eq!(h.denom(), &z);
        assert_eq!(h.numer(), &(&(&x + &y) * &(&x - &MPoly::one())));
    }

    #[test]
    fn ratfun_declines_rather_than_mis_reduces_when_the_gcd_gives_up() {
        // The GCD certifies its answer, so even if the budget runs out the
        // value is still correct — only its spelling degrades. Verified here by
        // the identity numer·den == num·denom for a hard multivariate pair.
        let x = MPoly::var("x");
        let y = MPoly::var("y");
        let num = &(&x.pow(4) - &y.pow(4)) * &(&x + &(&y * &MPoly::constant(rat_int(3))));
        let den = &(&x * &x) + &(&y * &y);
        let f = RatFun::new(num.clone(), den.clone()).expect("non-zero");
        assert_eq!(&(f.numer() * &den), &(&num * f.denom()));
        // This particular one does reduce: x⁴ − y⁴ = (x² + y²)(x² − y²).
        assert!(f.is_polynomial());
    }

    #[test]
    fn ratfun_bridges_to_the_univariate_world() {
        let f = RatFun::new(
            &MPoly::var("s") + &MPoly::constant(rat_int(3)),
            &(&(&MPoly::var("s") * &MPoly::var("s")) + &MPoly::var("s").scale(&rat_int(3)))
                + &MPoly::constant(rat_int(2)),
        )
        .expect("non-zero");
        let u = f.to_univariate("s").expect("univariate in s");
        assert_eq!(u, rf(&[3, 1], &[2, 3, 1]));
        assert_eq!(u.to_multivariate("s"), f);

        // Apart straight off the multivariate IR.
        let pf = f.partial_fractions("s").expect("ok");
        assert_eq!(pf.terms.len(), 2);
        assert_eq!(pf.recombine().expect("ok"), u);

        // A genuinely multivariate value refuses rather than guessing.
        let g = RatFun::new(MPoly::one(), &MPoly::var("a") + &MPoly::var("b")).expect("non-zero");
        assert_eq!(g.to_univariate("a"), None);
        assert!(g.partial_fractions("a").is_err());
    }

    #[test]
    fn ratfun_normal_form_holds_under_random_arithmetic() {
        let mut rng = TestRng(0x5A17);
        let names = ["x", "y"];
        let build = |rng: &mut TestRng| -> MPoly {
            let mut acc = MPoly::zero();
            for _ in 0..3 {
                let v = names[(rng.next() as usize) % names.len()];
                let e = (rng.next() as usize) % 3;
                acc = &acc + &MPoly::var(v).pow(e).scale(&rat_int(rng.int(-4, 4)));
            }
            acc
        };
        for _ in 0..200 {
            let a = RatFun::new(build(&mut rng), {
                let d = build(&mut rng);
                if d.is_zero() {
                    MPoly::one()
                } else {
                    d
                }
            })
            .expect("non-zero denominator");
            let b = RatFun::new(build(&mut rng), {
                let d = build(&mut rng);
                if d.is_zero() {
                    MPoly::one()
                } else {
                    d
                }
            })
            .expect("non-zero denominator");
            for f in [a.add(&b), a.sub(&b), a.mul(&b)] {
                let den = f.denom();
                assert!(!den.is_zero());
                assert_eq!(den.content(), rat_int(1), "{den:?} is not primitive");
                assert!(!den.lc().is_negative(), "{den:?} has a negative lc");
                if f.is_zero() {
                    assert_eq!(den.as_constant(), Some(rat_int(1)));
                }
            }
            assert_eq!(a.add(&b).sub(&b), a);
            if !b.is_zero() {
                assert_eq!(a.mul(&b).div(&b), Some(a.clone()));
            }
        }
    }

    // --- canonical form ----------------------------------------------------

    #[test]
    fn construction_reduces_and_monicises() {
        // Cancel((x^2-1)/(x-1)) ==> 1+x   (oracle)
        let f = rf(&[-1, 0, 1], &[-1, 1]);
        assert_eq!(f.numer(), &p(&[1, 1]));
        assert_eq!(f.denom(), &UPoly::one());
        assert!(f.is_polynomial());
        assert_eq!(f.as_polynomial(), Some(&p(&[1, 1])));

        // Cancel((2*x^2+4*x)/(4*x)) ==> 1/2*(2+x)  — i.e. the polynomial x/2 + 1
        let g = rf(&[0, 4, 2], &[0, 4]);
        assert!(g.is_polynomial());
        assert_eq!(
            g.numer(),
            &UPoly::from_ratios(&[(1, 1), (1, 2)]).expect("valid")
        );
    }

    #[test]
    fn denominator_is_always_monic() {
        let f = rf(&[1], &[4, 2]); // 1/(2x+4)
        assert!(f.denom().is_monic());
        assert_eq!(f.denom(), &p(&[2, 1]));
        assert_eq!(f.numer(), &UPoly::constant(rat(1, 2).expect("valid")));
    }

    #[test]
    fn zero_denominator_is_an_error_not_a_panic() {
        assert_eq!(
            URatFun::new(p(&[1]), UPoly::zero()),
            Err(PolyError::DivisionByZero)
        );
        assert_eq!(URatFun::zero().inv(), Err(PolyError::DivisionByZero));
        assert_eq!(
            URatFun::one().checked_div(&URatFun::zero()),
            Err(PolyError::DivisionByZero)
        );
    }

    #[test]
    fn zero_is_canonical() {
        let z = rf(&[0], &[3, 1]);
        assert!(z.is_zero());
        assert_eq!(z, URatFun::zero());
        assert_eq!(z.denom(), &UPoly::one());
    }

    // --- arithmetic --------------------------------------------------------

    #[test]
    fn together_matches_the_oracle() {
        // Together(1/(s+1)+1/(s+2)) ==> (3+2*s)/((1+s)*(2+s))
        let sum = &rf(&[1], &[1, 1]) + &rf(&[1], &[2, 1]);
        assert_eq!(sum.numer(), &p(&[3, 2]));
        // Numerator/Denominator of that sum, as the oracle reports them
        // (it prints the denominator factored; the value is s²+3s+2).
        assert_eq!(sum.denom(), &p(&[2, 3, 1]));
        let factored = sum.denom().factor().expect("factorable");
        assert_eq!(factored.factors, vec![(p(&[1, 1]), 1), (p(&[2, 1]), 1)]);
    }

    #[test]
    fn arithmetic_stays_reduced() {
        let a = rf(&[1], &[-1, 1]); // 1/(x-1)
        let b = rf(&[1], &[1, 1]); // 1/(x+1)
        let sum = &a + &b; // 2x/(x²-1)
        assert_eq!(sum.numer(), &p(&[0, 2]));
        assert_eq!(sum.denom(), &p(&[-1, 0, 1]));
        let diff = &a - &b; // 2/(x²-1)
        assert_eq!(diff.numer(), &p(&[2]));
        let product = &a * &b;
        assert_eq!(product.denom(), &p(&[-1, 0, 1]));
        // (1/(x-1)) / (1/(x+1)) = (x+1)/(x-1)
        let quotient = a.checked_div(&b).expect("non-zero divisor");
        assert_eq!(quotient.numer(), &p(&[1, 1]));
        assert_eq!(quotient.denom(), &p(&[-1, 1]));
        // Cancellation all the way to a polynomial.
        assert_eq!(&a - &a, URatFun::zero());
        assert_eq!(&a * &a.inv().expect("non-zero"), URatFun::one());
    }

    #[test]
    fn pow_handles_negative_exponents() {
        let f = rf(&[1, 1], &[2, 1]); // (x+1)/(x+2)
        assert_eq!(f.pow(0).expect("ok"), URatFun::one());
        assert_eq!(f.pow(1).expect("ok"), f);
        assert_eq!(f.pow(2).expect("ok"), &f * &f);
        assert_eq!(f.pow(-1).expect("ok"), f.inv().expect("ok"));
        assert_eq!(
            &f.pow(-2).expect("ok") * &f.pow(2).expect("ok"),
            URatFun::one()
        );
        assert_eq!(URatFun::zero().pow(-1), Err(PolyError::DivisionByZero));
    }

    #[test]
    fn eval_is_exact_and_reports_poles() {
        let f = rf(&[1], &[-2, 1]); // 1/(x-2)
        assert_eq!(f.eval(&rat_int(4)), Some(rat(1, 2).expect("valid")));
        assert_eq!(f.eval(&rat_int(2)), None);
        // A removable singularity really is removed by normalisation.
        let g = rf(&[-1, 0, 1], &[-1, 1]); // (x²-1)/(x-1) → x+1
        assert_eq!(g.eval(&rat_int(1)), Some(rat_int(2)));
    }

    #[test]
    fn is_proper_classifies_correctly() {
        assert!(rf(&[1], &[0, 1]).is_proper());
        assert!(!rf(&[0, 1], &[1]).is_proper());
        assert!(URatFun::zero().is_proper());
        assert!(!URatFun::one().is_proper());
    }

    // --- partial fractions, against the Java oracle ------------------------

    #[test]
    fn apart_distinct_linear_factors() {
        // Apart((s+3)/(s^2+3*s+2),s) ==> 2/(1+s)-1/(2+s)
        let f = rf(&[3, 1], &[2, 3, 1]);
        let pf = f.partial_fractions().expect("ok");
        assert!(pf.polynomial.is_zero());
        assert_eq!(pf.terms.len(), 2);
        assert_eq!(pf.terms[0].numerator, p(&[2]));
        assert_eq!(pf.terms[0].base, p(&[1, 1]));
        assert_eq!(pf.terms[0].power, 1);
        assert_eq!(pf.terms[0].linear_pole(), Some(rat_int(-1)));
        assert_eq!(pf.terms[1].numerator, p(&[-1]));
        assert_eq!(pf.terms[1].base, p(&[2, 1]));
        assert_eq!(pf.terms[1].linear_pole(), Some(rat_int(-2)));
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_repeated_factor_at_the_origin() {
        // Apart(1/(s^2*(s+1)),s) ==> 1/s^2-1/s+1/(1+s)
        let f = rf(&[1], &[0, 0, 1, 1]);
        let pf = f.partial_fractions().expect("ok");
        assert!(pf.polynomial.is_zero());
        let shape: Vec<(UPoly, UPoly, usize)> = pf
            .terms
            .iter()
            .map(|t| (t.numerator.clone(), t.base.clone(), t.power))
            .collect();
        assert_eq!(
            shape,
            vec![
                (p(&[1]), p(&[0, 1]), 2),  //  1/s²
                (p(&[-1]), p(&[0, 1]), 1), // −1/s
                (p(&[1]), p(&[1, 1]), 1),  //  1/(1+s)
            ]
        );
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_keeps_irreducible_quadratics_intact() {
        // Apart(1/((s+1)*(s^2+1)),s) ==> 1/(2*(1+s))+(1-s)/(2*(1+s^2))
        let f = URatFun::new(p(&[1]), &p(&[1, 1]) * &p(&[1, 0, 1])).expect("ok");
        let pf = f.partial_fractions().expect("ok");
        assert_eq!(pf.terms.len(), 2);
        // 1/(2(1+s))
        assert_eq!(pf.terms[0].base, p(&[1, 1]));
        assert_eq!(
            pf.terms[0].numerator,
            UPoly::constant(rat(1, 2).expect("valid"))
        );
        // (1-s)/(2(1+s²)) — a *linear* numerator over the quadratic, not two
        // complex poles.
        assert_eq!(pf.terms[1].base, p(&[1, 0, 1]));
        assert_eq!(
            pf.terms[1].numerator,
            UPoly::from_ratios(&[(1, 2), (-1, 2)]).expect("valid")
        );
        assert_eq!(pf.terms[1].linear_pole(), None);
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_splits_off_a_polynomial_part() {
        // Apart((s^3+1)/(s^2-1),s) ==> 1/(-1+s)+s
        // (normalisation first cancels the shared (s+1))
        let f = rf(&[1, 0, 0, 1], &[-1, 0, 1]);
        assert_eq!(f.numer(), &p(&[1, -1, 1]));
        assert_eq!(f.denom(), &p(&[-1, 1]));
        let pf = f.partial_fractions().expect("ok");
        assert_eq!(pf.polynomial, p(&[0, 1])); // s
        assert_eq!(pf.terms.len(), 1);
        assert_eq!(pf.terms[0].numerator, p(&[1]));
        assert_eq!(pf.terms[0].base, p(&[-1, 1]));
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_leaves_an_already_simple_term_alone() {
        // Apart((2*s+3)/(s^2+4),s) ==> (3+2*s)/(4+s^2)
        let f = rf(&[3, 2], &[4, 0, 1]);
        let pf = f.partial_fractions().expect("ok");
        assert!(pf.polynomial.is_zero());
        assert_eq!(pf.terms.len(), 1);
        assert_eq!(pf.terms[0].numerator, p(&[3, 2]));
        assert_eq!(pf.terms[0].base, p(&[4, 0, 1]));
        assert_eq!(pf.terms[0].power, 1);
    }

    #[test]
    fn apart_handles_a_repeated_shifted_pole() {
        // Apart(1/(s*(s+1)^2),s) ==> 1/s-1/(1+s)^2-1/(1+s)
        let f = URatFun::new(p(&[1]), &p(&[0, 1]) * &p(&[1, 1]).pow(2)).expect("ok");
        let pf = f.partial_fractions().expect("ok");
        let shape: Vec<(UPoly, UPoly, usize)> = pf
            .terms
            .iter()
            .map(|t| (t.numerator.clone(), t.base.clone(), t.power))
            .collect();
        assert_eq!(
            shape,
            vec![
                (p(&[1]), p(&[0, 1]), 1),  //  1/s
                (p(&[-1]), p(&[1, 1]), 2), // −1/(1+s)²
                (p(&[-1]), p(&[1, 1]), 1), // −1/(1+s)
            ]
        );
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_of_a_polynomial_is_the_polynomial() {
        let f = URatFun::from_poly(p(&[1, 2, 3]));
        let pf = f.partial_fractions().expect("ok");
        assert_eq!(pf.polynomial, p(&[1, 2, 3]));
        assert!(pf.is_polynomial());
        assert_eq!(pf.recombine().expect("ok"), f);
        // And of zero.
        let z = URatFun::zero().partial_fractions().expect("ok");
        assert!(z.polynomial.is_zero());
        assert!(z.terms.is_empty());
    }

    #[test]
    fn apart_with_a_repeated_irreducible_quadratic() {
        // (s+1)/(s²+1)² — a repeated complex-conjugate pair. Must stay exact.
        let f = URatFun::new(p(&[1, 1]), p(&[1, 0, 1]).pow(2)).expect("ok");
        let pf = f.partial_fractions().expect("ok");
        assert_eq!(pf.terms.len(), 1);
        assert_eq!(pf.terms[0].base, p(&[1, 0, 1]));
        assert_eq!(pf.terms[0].power, 2);
        assert_eq!(pf.terms[0].numerator, p(&[1, 1]));
        assert_eq!(pf.recombine().expect("ok"), f);
    }

    #[test]
    fn apart_with_rational_coefficients_and_content() {
        // (x/2 + 1/3) / (x² + 5x/6 + 1/6) — content in both parts.
        let num = UPoly::from_ratios(&[(1, 3), (1, 2)]).expect("valid");
        let den = UPoly::from_ratios(&[(1, 6), (5, 6), (1, 1)]).expect("valid");
        let f = URatFun::new(num, den).expect("ok");
        let pf = f.partial_fractions().expect("ok");
        assert_eq!(pf.recombine().expect("ok"), f);
        for term in &pf.terms {
            assert!(term.base.is_monic());
            assert!(
                term.numerator.degree_or_zero() < term.base.degree_or_zero(),
                "numerator degree must stay below the base degree"
            );
        }
    }

    /// A second oracle battery for `Apart`, read off the real Symja engine.
    /// Every expectation is the exact decomposition it returned; the term
    /// ordering here is also the ordering the oracle prints.
    #[test]
    fn apart_matches_the_oracle_on_hard_shapes() {
        let shape = |f: &URatFun| -> (UPoly, Vec<(UPoly, UPoly, usize)>) {
            let pf = f.partial_fractions().expect("ok");
            assert_eq!(pf.recombine().expect("ok"), *f);
            let terms = pf
                .terms
                .iter()
                .map(|t| (t.numerator.clone(), t.base.clone(), t.power))
                .collect();
            (pf.polynomial, terms)
        };
        let q = |n: i64, d: i64| UPoly::constant(rat(n, d).expect("valid"));

        // Apart(1/(s^3*(s+2)^2),s)
        //   ==> 1/(4*s^3) - 1/(4*s^2) + 3/16*1/s - 1/(8*(2+s)^2) - 3/16*1/(2+s)
        let f = URatFun::new(p(&[1]), &p(&[0, 1]).pow(3) * &p(&[2, 1]).pow(2)).expect("ok");
        let (poly, terms) = shape(&f);
        assert!(poly.is_zero());
        assert_eq!(
            terms,
            vec![
                (q(1, 4), p(&[0, 1]), 3),
                (q(-1, 4), p(&[0, 1]), 2),
                (q(3, 16), p(&[0, 1]), 1),
                (q(-1, 8), p(&[2, 1]), 2),
                (q(-3, 16), p(&[2, 1]), 1),
            ]
        );

        // Apart(1/(s^4-1),s) ==> 1/(4*(-1+s)) - 1/(4*(1+s)) - 1/(2*(1+s^2))
        let f = rf(&[-1, 0, 0, 0, 1], &[1]).inv().expect("non-zero");
        let (poly, terms) = shape(&f);
        assert!(poly.is_zero());
        assert_eq!(
            terms,
            vec![
                (q(1, 4), p(&[-1, 1]), 1),
                (q(-1, 4), p(&[1, 1]), 1),
                (q(-1, 2), p(&[1, 0, 1]), 1),
            ]
        );

        // Apart((s^4+1)/(s^3-s),s) ==> 1/(-1+s) - 1/s + s + 1/(1+s)
        let f = rf(&[1, 0, 0, 0, 1], &[0, -1, 0, 1]);
        let (poly, terms) = shape(&f);
        assert_eq!(poly, p(&[0, 1]));
        assert_eq!(
            terms,
            vec![
                (p(&[1]), p(&[-1, 1]), 1),
                (p(&[-1]), p(&[0, 1]), 1),
                (p(&[1]), p(&[1, 1]), 1),
            ]
        );

        // Apart((s^2+2*s+3)/((s+1)*(s^2+2*s+5)),s)
        //   ==> 1/(2*(1+s)) + (1+s)/(2*(5+2*s+s^2))
        let f = URatFun::new(p(&[3, 2, 1]), &p(&[1, 1]) * &p(&[5, 2, 1])).expect("ok");
        let (poly, terms) = shape(&f);
        assert!(poly.is_zero());
        assert_eq!(
            terms,
            vec![
                (q(1, 2), p(&[1, 1]), 1),
                (
                    UPoly::from_ratios(&[(1, 2), (1, 2)]).expect("valid"),
                    p(&[5, 2, 1]),
                    1
                ),
            ]
        );

        // Apart(1/(s*(s^2+s+1)),s) ==> 1/s - (1+s)/(1+s+s^2)
        let f = URatFun::new(p(&[1]), &p(&[0, 1]) * &p(&[1, 1, 1])).expect("ok");
        let (_, terms) = shape(&f);
        assert_eq!(
            terms,
            vec![(p(&[1]), p(&[0, 1]), 1), (p(&[-1, -1]), p(&[1, 1, 1]), 1),]
        );

        // Apart((3*s+5)/((s+2)^3),s) ==> -1/(2+s)^3 + 3/(2+s)^2
        let f = URatFun::new(p(&[5, 3]), p(&[2, 1]).pow(3)).expect("ok");
        let (_, terms) = shape(&f);
        assert_eq!(
            terms,
            vec![(p(&[-1]), p(&[2, 1]), 3), (p(&[3]), p(&[2, 1]), 2)]
        );

        // Apart(1/(2*s^2+3*s+1),s) ==> -1/(1+s) + 2/(1+2*s).
        // The oracle keeps a non-monic base; the canonical form here is
        // -1/(s+1) + 1/(s+1/2), the same function.
        let f = rf(&[1], &[1, 3, 2]);
        let (_, terms) = shape(&f);
        assert_eq!(
            terms,
            vec![
                (p(&[-1]), p(&[1, 1]), 1),
                (
                    UPoly::one(),
                    UPoly::from_ratios(&[(1, 2), (1, 1)]).expect("valid"),
                    1
                ),
            ]
        );

        // Apart((s^5)/((s+1)^2*(s^2+4)),s)
        //   ==> -2 + s - 1/(5*(1+s)^2) + 23/25*1/(1+s) + 16/25*(8-3*s)/(4+s^2)
        let f = URatFun::new(
            UPoly::monomial(5, rat_int(1)),
            &p(&[1, 1]).pow(2) * &p(&[4, 0, 1]),
        )
        .expect("ok");
        let (poly, terms) = shape(&f);
        assert_eq!(poly, p(&[-2, 1]));
        assert_eq!(
            terms,
            vec![
                (q(-1, 5), p(&[1, 1]), 2),
                (q(23, 25), p(&[1, 1]), 1),
                (
                    UPoly::from_ratios(&[(128, 25), (-48, 25)]).expect("valid"),
                    p(&[4, 0, 1]),
                    1
                ),
            ]
        );
    }

    #[test]
    fn together_cancel_and_parts_match_the_oracle() {
        // Together(1/s+1/(s+1)+1/(s+2)) ==> (2+6*s+3*s^2)/(s*(1+s)*(2+s))
        let sum = &(&rf(&[1], &[0, 1]) + &rf(&[1], &[1, 1])) + &rf(&[1], &[2, 1]);
        assert_eq!(sum.numer(), &p(&[2, 6, 3]));
        assert_eq!(sum.denom(), &p(&[0, 2, 3, 1]));

        // Cancel((x^3-1)/(x^2-1)) ==> (1+x+x^2)/(1+x)
        let c = rf(&[-1, 0, 0, 1], &[-1, 0, 1]);
        assert_eq!(c.numer(), &p(&[1, 1, 1]));
        assert_eq!(c.denom(), &p(&[1, 1]));

        // Cancel((6*x^2+12*x)/(9*x)) ==> 2/3*(2+x)
        let c = rf(&[0, 12, 6], &[0, 9]);
        assert!(c.is_polynomial());
        assert_eq!(
            c.numer(),
            &UPoly::from_ratios(&[(4, 3), (2, 3)]).expect("valid")
        );

        // Numerator/Denominator(Together(1/s+2/(s^2+1)))
        //   ==> 1+2*s+s^2  over  s*(1+s^2)
        let t = &rf(&[1], &[0, 1]) + &rf(&[2], &[1, 0, 1]);
        assert_eq!(t.numer(), &p(&[1, 2, 1]));
        assert_eq!(t.denom(), &p(&[0, 1, 0, 1]));
    }

    // --- properties --------------------------------------------------------

    #[test]
    fn apart_then_recombine_is_the_identity() {
        let mut rng = TestRng(0xA9A97);
        let mut checked = 0;
        for _ in 0..300 {
            let num = rng.poly(4, 6);
            let den = rng.poly(4, 6);
            if den.is_zero() {
                continue;
            }
            let f = URatFun::new(num, den).expect("non-zero denominator");
            let pf = f.partial_fractions().expect("decomposition must succeed");
            assert_eq!(pf.recombine().expect("ok"), f, "apart/recombine on {f}");
            for term in &pf.terms {
                assert!(term.power >= 1);
                assert!(!term.numerator.is_zero());
                assert!(term.base.is_monic());
                assert!(term.base.degree_or_zero() >= 1);
                assert!(term.numerator.degree_or_zero() < term.base.degree_or_zero());
                assert!(
                    term.base.factor().expect("ok").is_irreducible(),
                    "{} must be irreducible over ℚ",
                    term.base
                );
            }
            checked += 1;
        }
        assert!(checked > 200, "the generator produced too few usable cases");
    }

    #[test]
    fn normal_form_invariants_hold_under_arithmetic() {
        let mut rng = TestRng(0x2B1E);
        for _ in 0..300 {
            let a = URatFun::new(rng.poly(3, 5), {
                let d = rng.poly(3, 5);
                if d.is_zero() {
                    UPoly::one()
                } else {
                    d
                }
            })
            .expect("non-zero denominator");
            let b = URatFun::new(rng.poly(3, 5), {
                let d = rng.poly(3, 5);
                if d.is_zero() {
                    UPoly::one()
                } else {
                    d
                }
            })
            .expect("non-zero denominator");
            for f in [&a + &b, &a - &b, &a * &b] {
                assert!(f.denom().is_monic(), "{f} has a non-monic denominator");
                assert_eq!(
                    f.numer().gcd(f.denom()),
                    UPoly::one(),
                    "{f} is not in lowest terms"
                );
                if f.is_zero() {
                    assert_eq!(f.denom(), &UPoly::one());
                }
            }
            // Field laws, on exact arithmetic.
            assert_eq!(&(&a + &b) - &b, a);
            if !b.is_zero() {
                assert_eq!(&(&a * &b).checked_div(&b).expect("non-zero divisor"), &a);
            }
        }
    }

    #[test]
    fn evaluation_agrees_with_the_decomposition() {
        // The decomposition is not just structurally right, it has the same
        // value everywhere it is defined.
        let mut rng = TestRng(0x7A1E);
        for _ in 0..120 {
            let num = rng.poly(3, 5);
            let den = rng.poly(3, 5);
            if den.is_zero() {
                continue;
            }
            let f = URatFun::new(num, den).expect("ok");
            let pf = f.partial_fractions().expect("ok");
            for k in -4i64..=4 {
                let at = rat_int(k);
                let direct = f.eval(&at);
                let mut summed = Some(pf.polynomial.eval(&at));
                for term in &pf.terms {
                    let piece = term.to_uratfun().expect("ok").eval(&at);
                    summed = match (summed, piece) {
                        (Some(s), Some(v)) => Some(s + v),
                        _ => None,
                    };
                }
                assert_eq!(direct, summed, "value mismatch for {f} at {at}");
            }
        }
    }
}
