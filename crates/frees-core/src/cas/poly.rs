//! Exact polynomial algebra over ℚ — the layer every other CAS module stands on.
//!
//! Port note: **there is no Java file to transcribe here.** The parent engine
//! reached Symja for all of this ([`crate::cas`] explains why Symja cannot
//! ship), so this module is written from the algorithms rather than translated.
//! That makes its correctness the project's problem rather than the oracle's,
//! which is why nearly every routine here is self-verifying: the factoriser
//! multiplies its answer back out and compares, the modular GCD trial-divides
//! its candidate into both inputs, and the Hensel lift checks its own product
//! against the input before recombination is allowed to trust it.
//!
//! # Exactness is the invariant
//!
//! Coefficients are [`Rat`] = `num_rational::BigRational`. Nothing in this file
//! or in [`crate::cas::ratfun`] may fall back to `f64`: a factoriser that loses
//! exactness returns answers that are plausible and wrong, which is strictly
//! worse than refusing. Every routine that cannot deliver an exact answer
//! returns [`PolyError`] instead of guessing.
//!
//! # What is here
//!
//! * [`UPoly`] — dense univariate polynomials over ℚ: ring operations, division
//!   with remainder, evaluation, derivative, content / primitive part.
//! * [`UPoly::gcd`] — a **modular (small-prime + CRT) GCD** with an exact
//!   trial-division certificate, falling back to primitive PRS. This is the hot
//!   path for `Cancel`/`Together` and the correctness bottleneck for `Apart`.
//! * [`UPoly::square_free`] — Yun's algorithm.
//! * [`UPoly::factor`] — factorisation over ℚ by **Zassenhaus**: Cantor–Zassenhaus
//!   modulo a good small prime, quadratic Hensel lifting past the
//!   Landau–Mignotte bound, then subset recombination over ℤ.
//! * [`MPoly`] — sparse multivariate polynomials over ℚ (ring operations,
//!   division with remainder, evaluation, content). Multivariate **GCD and
//!   factorisation are deliberately not implemented**; see the module's
//!   "Known limits" section.
//!
//! # Known limits (stated, not hidden)
//!
//! * Zassenhaus recombination is exponential in the number of modular factors.
//!   Inputs that split into many modular factors but few rational ones — the
//!   Swinnerton-Dyer polynomials are the classic family — hit
//!   [`PolyError::FactorTooHard`] rather than running forever or returning a
//!   partial answer. The thresholds are [`MAX_MODULAR_FACTORS`] and
//!   [`MAX_RECOMBINATIONS`].
//! * The factoriser monicises a non-monic input by the substitution
//!   `f̃(x) = lc^(n-1)·f(x/lc)`. That keeps every lifted polynomial monic (a
//!   large simplification) at the cost of coefficients growing by `lc^(n-1)`.
//!   For the degrees frees actually reaches — transfer functions and Laplace
//!   denominators, degree ≤ 10 — this is free. For degree 50 with a large
//!   leading coefficient it is not, and a leading-coefficient-tracking
//!   Zassenhaus would be the fix.
//! * [`MPoly::gcd`] does not exist. `Cancel`/`Together`/`Apart` in frees are all
//!   with respect to one variable, so the univariate GCD is what they need;
//!   multivariate rational simplification would need a multivariate GCD
//!   (Brown/Zippel) that nothing downstream currently asks for.
//!
//! # Ergonomics
//!
//! Arithmetic is exposed through the `std::ops` traits rather than inherent
//! `add`/`mul` methods (which would trip `clippy::should_implement_trait`).
//! Both `&a + &b` and `a + b` work.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Add, Mul, Neg, Sub};
use std::sync::OnceLock;

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// The coefficient field: exact rationals with unbounded numerator and
/// denominator. Never substitute `f64` for this.
pub type Rat = BigRational;

/// Build a [`Rat`] from a numerator/denominator pair of `i64`s.
///
/// Returns `None` when `den` is zero. Convenience for callers and tests; the
/// engine itself works with [`BigInt`] directly.
pub fn rat(num: i64, den: i64) -> Option<Rat> {
    if den == 0 {
        return None;
    }
    Some(Rat::new(BigInt::from(num), BigInt::from(den)))
}

/// Build an integral [`Rat`].
pub fn rat_int(value: i64) -> Rat {
    Rat::from_integer(BigInt::from(value))
}

/// A modular factorisation with more than this many factors is refused rather
/// than recombined: the subset search is `2^r`.
pub const MAX_MODULAR_FACTORS: usize = 32;

/// Hard budget on recombination trial divisions, so a pathological input fails
/// fast in a browser tab instead of hanging it.
pub const MAX_RECOMBINATIONS: usize = 60_000;

/// Yun's loop and the Cantor–Zassenhaus splitting loop both terminate by a
/// theorem rather than by construction; these caps turn a hypothetical bug into
/// an error instead of a hang.
const MAX_YUN_ITERATIONS: usize = 512;
const MAX_SPLIT_ATTEMPTS: usize = 512;

/// Multivariate division terminates because the monomial order is a well-order;
/// the cap only turns a hypothetical bug into an error rather than a hang.
const MAX_DIVISION_STEPS: usize = 1 << 20;

/// Work budget for the recursive multivariate GCD. Primitive PRS terminates,
/// but its coefficients can swell; exhausting this budget makes
/// [`MPoly::gcd`] fall back to the content × monomial gcd — an under-reduced
/// but still correct answer — instead of stalling.
pub const MAX_MGCD_STEPS: usize = 20_000;

/// Failures that are real answers, not guesses.
///
/// Deliberately small (no boxed payloads needed) so `clippy::result_large_err`
/// stays quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolyError {
    /// Division by the zero polynomial, or by the zero rational function.
    DivisionByZero,
    /// The input factors into more pieces modulo every tried prime than the
    /// recombination search can afford. The factorisation is *unknown*, not
    /// partial — callers must not present the input as irreducible.
    FactorTooHard {
        modular_factors: usize,
        limit: usize,
    },
    /// No usable prime was found (every candidate divided the leading
    /// coefficient or left a non-square-free image).
    NoUsablePrime,
    /// An identity this module proves for itself did not hold. Always a bug
    /// here, never bad user input — but reported rather than asserted so a
    /// wasm build cannot abort the tab.
    Internal(&'static str),
}

impl fmt::Display for PolyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolyError::DivisionByZero => f.write_str("division by the zero polynomial"),
            PolyError::FactorTooHard {
                modular_factors,
                limit,
            } => write!(
                f,
                "cannot factor: {modular_factors} modular factors exceeds the recombination \
                 budget ({limit})"
            ),
            PolyError::NoUsablePrime => f.write_str("no usable prime for the factorisation"),
            PolyError::Internal(what) => write!(f, "internal CAS invariant violated: {what}"),
        }
    }
}

impl std::error::Error for PolyError {}

/// `Result` specialised to [`PolyError`].
pub type PolyResult<T> = std::result::Result<T, PolyError>;

// ---------------------------------------------------------------------------
// Univariate polynomials over ℚ
// ---------------------------------------------------------------------------

/// A dense univariate polynomial over ℚ.
///
/// Coefficients are stored **ascending** — `coeffs[i]` multiplies `x^i` — and
/// the representation is always trimmed, so the last entry (if any) is non-zero
/// and the zero polynomial is the empty vector. `PartialEq` is therefore
/// mathematical equality.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UPoly {
    coeffs: Vec<Rat>,
}

impl UPoly {
    /// The zero polynomial.
    pub fn zero() -> UPoly {
        UPoly { coeffs: Vec::new() }
    }

    /// The constant `1`.
    pub fn one() -> UPoly {
        UPoly::constant(Rat::one())
    }

    /// The polynomial `x`.
    pub fn x() -> UPoly {
        UPoly::monomial(1, Rat::one())
    }

    /// A constant polynomial (the zero polynomial when `c == 0`).
    pub fn constant(c: Rat) -> UPoly {
        UPoly::from_coeffs(vec![c])
    }

    /// `c · x^degree`.
    pub fn monomial(degree: usize, c: Rat) -> UPoly {
        if c.is_zero() {
            return UPoly::zero();
        }
        let mut coeffs = vec![Rat::zero(); degree];
        coeffs.push(c);
        UPoly { coeffs }
    }

    /// From ascending coefficients; trailing zeros are trimmed.
    pub fn from_coeffs(mut coeffs: Vec<Rat>) -> UPoly {
        while coeffs.last().is_some_and(Zero::is_zero) {
            coeffs.pop();
        }
        UPoly { coeffs }
    }

    /// From ascending integer coefficients — the shape most call sites and
    /// tests want.
    pub fn from_ints(coeffs: &[i64]) -> UPoly {
        UPoly::from_coeffs(coeffs.iter().map(|&c| rat_int(c)).collect())
    }

    /// From ascending `(numerator, denominator)` pairs.
    ///
    /// Returns `None` if any denominator is zero.
    pub fn from_ratios(coeffs: &[(i64, i64)]) -> Option<UPoly> {
        let mut out = Vec::with_capacity(coeffs.len());
        for &(n, d) in coeffs {
            out.push(rat(n, d)?);
        }
        Some(UPoly::from_coeffs(out))
    }

    /// Is this the zero polynomial?
    pub fn is_zero(&self) -> bool {
        self.coeffs.is_empty()
    }

    /// Degree, or `None` for the zero polynomial.
    ///
    /// `None` rather than `-1` on purpose: every caller has to decide what the
    /// zero polynomial means for it, and the type makes that decision visible.
    pub fn degree(&self) -> Option<usize> {
        self.coeffs.len().checked_sub(1)
    }

    /// Degree of a non-zero polynomial, `0` for the zero polynomial. Use only
    /// where the zero case is genuinely equivalent to a constant.
    pub fn degree_or_zero(&self) -> usize {
        self.degree().unwrap_or(0)
    }

    /// Ascending coefficient slice.
    pub fn coeffs(&self) -> &[Rat] {
        &self.coeffs
    }

    /// The coefficient of `x^i` (zero when out of range).
    pub fn coeff(&self, i: usize) -> Rat {
        self.coeffs.get(i).cloned().unwrap_or_else(Rat::zero)
    }

    /// Leading coefficient; zero for the zero polynomial.
    pub fn lc(&self) -> Rat {
        self.coeffs.last().cloned().unwrap_or_else(Rat::zero)
    }

    /// Is this a constant (including zero)?
    pub fn is_constant(&self) -> bool {
        self.coeffs.len() <= 1
    }

    /// The constant value, or `None` if the degree is positive.
    pub fn as_constant(&self) -> Option<Rat> {
        match self.coeffs.len() {
            0 => Some(Rat::zero()),
            1 => Some(self.coeffs[0].clone()),
            _ => None,
        }
    }

    /// Multiply every coefficient by `k`.
    pub fn scale(&self, k: &Rat) -> UPoly {
        if k.is_zero() {
            return UPoly::zero();
        }
        UPoly {
            coeffs: self.coeffs.iter().map(|c| c * k).collect(),
        }
    }

    /// `self^n`.
    pub fn pow(&self, n: usize) -> UPoly {
        let mut result = UPoly::one();
        let mut base = self.clone();
        let mut e = n;
        while e > 0 {
            if e & 1 == 1 {
                result = &result * &base;
            }
            e >>= 1;
            if e > 0 {
                base = &base * &base;
            }
        }
        result
    }

    /// Quotient and remainder: `self = q·divisor + r` with `deg r < deg divisor`.
    ///
    /// Exact over the field ℚ, so this never approximates. Fails only when
    /// `divisor` is zero.
    pub fn div_rem(&self, divisor: &UPoly) -> PolyResult<(UPoly, UPoly)> {
        let Some(dd) = divisor.degree() else {
            return Err(PolyError::DivisionByZero);
        };
        let Some(nd) = self.degree() else {
            return Ok((UPoly::zero(), UPoly::zero()));
        };
        if nd < dd {
            return Ok((UPoly::zero(), self.clone()));
        }
        let inv_lc = divisor.lc().recip();
        let mut rem = self.coeffs.clone();
        let mut quot = vec![Rat::zero(); nd - dd + 1];
        for k in (0..=nd - dd).rev() {
            let factor = &rem[k + dd] * &inv_lc;
            if factor.is_zero() {
                continue;
            }
            for (i, dc) in divisor.coeffs.iter().enumerate() {
                rem[k + i] -= &factor * dc;
            }
            quot[k] = factor;
        }
        Ok((UPoly::from_coeffs(quot), UPoly::from_coeffs(rem)))
    }

    /// `self / divisor` when the division is exact, `None` otherwise.
    pub fn exact_div(&self, divisor: &UPoly) -> Option<UPoly> {
        let (q, r) = self.div_rem(divisor).ok()?;
        r.is_zero().then_some(q)
    }

    /// Does `self` divide `other` exactly? (`false` for the zero divisor.)
    pub fn divides(&self, other: &UPoly) -> bool {
        !self.is_zero() && other.exact_div(self).is_some()
    }

    /// Horner evaluation at an exact rational point.
    pub fn eval(&self, at: &Rat) -> Rat {
        let mut acc = Rat::zero();
        for c in self.coeffs.iter().rev() {
            acc = acc * at + c;
        }
        acc
    }

    /// `d(self)/dx`.
    pub fn derivative(&self) -> UPoly {
        if self.coeffs.len() < 2 {
            return UPoly::zero();
        }
        let coeffs = self
            .coeffs
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, c)| c * Rat::from_integer(BigInt::from(i)))
            .collect();
        UPoly::from_coeffs(coeffs)
    }

    /// The **non-negative** rational content: the unique `c ≥ 0` with
    /// `self = c · pp` where `pp` has coprime integer coefficients.
    ///
    /// `c` carries no sign — `(-2x - 4)` has content `2`, not `-2`. Callers that
    /// want a sign-normalised split take `lc().signum()` separately; that is
    /// what [`UPoly::factor`] does, and it is why `Factor(x/2 + 1/3)` comes back
    /// as `1/6·(3x + 2)` exactly as the Java oracle spells it.
    pub fn content(&self) -> Rat {
        if self.is_zero() {
            return Rat::zero();
        }
        let mut num_gcd = BigInt::zero();
        let mut den_lcm = BigInt::one();
        for c in &self.coeffs {
            num_gcd = num_gcd.gcd(c.numer());
            den_lcm = den_lcm.lcm(c.denom());
        }
        Rat::new(num_gcd, den_lcm)
    }

    /// `self / content()` — integer coefficients with gcd 1, sign of the
    /// original leading coefficient preserved. Zero maps to zero.
    pub fn primitive_part(&self) -> UPoly {
        let c = self.content();
        if c.is_zero() {
            return UPoly::zero();
        }
        self.scale(&c.recip())
    }

    /// `self / lc(self)`. The zero polynomial maps to itself.
    pub fn monic(&self) -> UPoly {
        if self.is_zero() {
            return UPoly::zero();
        }
        let lc = self.lc();
        if lc.is_one() {
            return self.clone();
        }
        self.scale(&lc.recip())
    }

    /// Is the leading coefficient exactly 1?
    pub fn is_monic(&self) -> bool {
        !self.is_zero() && self.lc().is_one()
    }

    /// The **monic** greatest common divisor.
    ///
    /// `gcd(0, 0)` is `0`; otherwise the result is monic, so the answer is
    /// canonical rather than "up to a unit". Primary algorithm: reduce both
    /// inputs to primitive integer polynomials, compute modular GCDs over a
    /// sequence of primes, CRT-combine, and accept a candidate only once exact
    /// trial division into **both** inputs succeeds. That certificate is what
    /// makes the modular route safe: a candidate that divides both and whose
    /// degree equals the minimum modular-GCD degree seen *is* the GCD. If the
    /// prime supply is exhausted, primitive PRS finishes the job.
    pub fn gcd(&self, other: &UPoly) -> UPoly {
        if self.is_zero() {
            return other.monic();
        }
        if other.is_zero() {
            return self.monic();
        }
        let a = ZPoly::from_upoly(self);
        let b = ZPoly::from_upoly(other);
        zpoly_gcd(&a, &b).to_upoly().monic()
    }

    /// Extended Euclid: returns `(g, s, t)` with `s·self + t·other = g` and `g`
    /// the monic GCD. All three are zero when both inputs are zero.
    ///
    /// This runs over ℚ directly (not through the integer path) because the
    /// cofactors are what callers want, and [`crate::cas::ratfun`] uses it for
    /// modular inversion inside the partial-fraction split.
    pub fn ext_gcd(&self, other: &UPoly) -> (UPoly, UPoly, UPoly) {
        let (mut r0, mut r1) = (self.clone(), other.clone());
        let (mut s0, mut s1) = (UPoly::one(), UPoly::zero());
        let (mut t0, mut t1) = (UPoly::zero(), UPoly::one());
        while !r1.is_zero() {
            let Ok((q, r)) = r0.div_rem(&r1) else { break };
            r0 = std::mem::replace(&mut r1, r);
            let s_next = &s0 - &(&q * &s1);
            s0 = std::mem::replace(&mut s1, s_next);
            let t_next = &t0 - &(&q * &t1);
            t0 = std::mem::replace(&mut t1, t_next);
        }
        if r0.is_zero() {
            return (UPoly::zero(), UPoly::zero(), UPoly::zero());
        }
        let inv = r0.lc().recip();
        (r0.scale(&inv), s0.scale(&inv), t0.scale(&inv))
    }

    /// Square-free decomposition by **Yun's algorithm**.
    ///
    /// Returns `[(aᵢ, i)]` with each `aᵢ` monic, square-free, pairwise coprime
    /// and of positive degree, such that `self = lc(self) · Π aᵢ^i`. Constants
    /// return an empty list.
    pub fn square_free(&self) -> PolyResult<Vec<(UPoly, usize)>> {
        if self.is_constant() {
            return Ok(Vec::new());
        }
        let f = self.monic();
        let fp = f.derivative();
        let a0 = f.gcd(&fp);
        let mut b = f
            .exact_div(&a0)
            .ok_or(PolyError::Internal("Yun: gcd(f, f') does not divide f"))?;
        let mut c = fp
            .exact_div(&a0)
            .ok_or(PolyError::Internal("Yun: gcd(f, f') does not divide f'"))?;
        let mut d = &c - &b.derivative();

        let mut out = Vec::new();
        for i in 1..=MAX_YUN_ITERATIONS {
            let a = b.gcd(&d);
            if a.degree_or_zero() > 0 {
                out.push((a.clone(), i));
            }
            b = b
                .exact_div(&a)
                .ok_or(PolyError::Internal("Yun: gcd(b, d) does not divide b"))?;
            c = d
                .exact_div(&a)
                .ok_or(PolyError::Internal("Yun: gcd(b, d) does not divide d"))?;
            d = &c - &b.derivative();
            if b.is_constant() {
                return Ok(out);
            }
        }
        Err(PolyError::Internal("Yun: iteration cap reached"))
    }

    /// Complete factorisation over ℚ.
    ///
    /// See [`Factorization`] for the shape of the answer. The result is
    /// verified — the factors are multiplied back out and compared against the
    /// input — so a bug in the factoriser surfaces as
    /// [`PolyError::Internal`], never as a plausible wrong answer.
    pub fn factor(&self) -> PolyResult<Factorization> {
        if self.is_zero() {
            return Ok(Factorization {
                unit: Rat::zero(),
                factors: Vec::new(),
            });
        }
        if self.is_constant() {
            return Ok(Factorization {
                unit: self.lc(),
                factors: Vec::new(),
            });
        }

        let mut factors: Vec<(UPoly, usize)> = Vec::new();
        for (sf, mult) in self.square_free()? {
            for irreducible in factor_square_free(&sf)? {
                factors.push((irreducible, mult));
            }
        }
        factors.sort_by(compare_factors);

        // Derive the unit from the product rather than tracking it through the
        // pipeline, then verify. Self-correcting *and* self-checking.
        let mut product = UPoly::one();
        for (f, m) in &factors {
            product = &product * &f.pow(*m);
        }
        if product.is_zero() {
            return Err(PolyError::Internal("factor: empty product"));
        }
        let unit = self.lc() / product.lc();
        if product.scale(&unit) != *self {
            return Err(PolyError::Internal(
                "factor: product does not reproduce input",
            ));
        }
        Ok(Factorization { unit, factors })
    }

    /// The **monic** irreducible factors with multiplicities, so that
    /// `self = lc(self) · Π baseᵢ^eᵢ`.
    ///
    /// This is the shape partial fractions and the inverse Laplace transform
    /// want: a monic irreducible base is exactly `s - a` or
    /// `s² + b·s + c`, ready to be read as a pole.
    pub fn monic_factors(&self) -> PolyResult<Vec<(UPoly, usize)>> {
        Ok(self
            .factor()?
            .factors
            .into_iter()
            .map(|(f, m)| (f.monic(), m))
            .collect())
    }

    /// Render with an explicit variable name, ascending in degree — the same
    /// term order the Java oracle prints (`1+2*x+x^2`).
    ///
    /// This is a debugging and test aid. User-facing spelling belongs to
    /// `cas::ops`, which owns the `Expr` round trip.
    pub fn to_string_in(&self, var: &str) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        let mut out = String::new();
        for (i, c) in self.coeffs.iter().enumerate() {
            if c.is_zero() {
                continue;
            }
            if !out.is_empty() {
                out.push_str(if c.is_negative() { "-" } else { "+" });
            } else if c.is_negative() {
                out.push('-');
            }
            let mag = c.abs();
            let show_coeff = i == 0 || !mag.is_one();
            if show_coeff {
                out.push_str(&mag.to_string());
                if i > 0 {
                    out.push('*');
                }
            }
            match i {
                0 => {}
                1 => out.push_str(var),
                _ => out.push_str(&format!("{var}^{i}")),
            }
        }
        out
    }
}

impl fmt::Display for UPoly {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_in("x"))
    }
}

/// Deterministic ordering for factor lists: by degree, then by coefficients.
fn compare_factors(a: &(UPoly, usize), b: &(UPoly, usize)) -> Ordering {
    a.0.degree_or_zero()
        .cmp(&b.0.degree_or_zero())
        .then_with(|| a.0.coeffs.cmp(&b.0.coeffs))
        .then_with(|| a.1.cmp(&b.1))
}

/// The result of [`UPoly::factor`].
///
/// `f = unit · Π factorᵢ^multᵢ` **exactly**. Each factor is irreducible over ℚ,
/// has integer coefficients with gcd 1, and has a positive leading coefficient
/// — which is why `Factor(2x² + 4x + 2)` reads `2·(x + 1)²` and
/// `Factor(6x² + 5x + 1)` reads `(2x + 1)(3x + 1)`, matching the oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factorization {
    /// The rational unit: `sign(lc) · content` of the input.
    pub unit: Rat,
    /// Irreducible primitive factors with positive leading coefficients, paired
    /// with their multiplicities, in a deterministic order.
    pub factors: Vec<(UPoly, usize)>,
}

impl Factorization {
    /// Multiply the factorisation back out. Used by the tests and available to
    /// callers that want to re-check a result they are about to display.
    pub fn expand(&self) -> UPoly {
        let mut product = UPoly::constant(self.unit.clone());
        for (f, m) in &self.factors {
            product = &product * &f.pow(*m);
        }
        product
    }

    /// Is the input irreducible over ℚ (a single factor of multiplicity one and
    /// positive degree)?
    pub fn is_irreducible(&self) -> bool {
        self.factors.len() == 1 && self.factors[0].1 == 1 && self.factors[0].0.degree_or_zero() > 0
    }
}

// --- operator impls -------------------------------------------------------

impl Neg for &UPoly {
    type Output = UPoly;
    fn neg(self) -> UPoly {
        UPoly {
            coeffs: self.coeffs.iter().map(|c| -c).collect(),
        }
    }
}

impl Neg for UPoly {
    type Output = UPoly;
    fn neg(self) -> UPoly {
        -&self
    }
}

impl Add for &UPoly {
    type Output = UPoly;
    fn add(self, rhs: &UPoly) -> UPoly {
        let n = self.coeffs.len().max(rhs.coeffs.len());
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.coeff(i) + rhs.coeff(i));
        }
        UPoly::from_coeffs(out)
    }
}

impl Sub for &UPoly {
    type Output = UPoly;
    fn sub(self, rhs: &UPoly) -> UPoly {
        let n = self.coeffs.len().max(rhs.coeffs.len());
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.coeff(i) - rhs.coeff(i));
        }
        UPoly::from_coeffs(out)
    }
}

impl Mul for &UPoly {
    type Output = UPoly;
    fn mul(self, rhs: &UPoly) -> UPoly {
        if self.is_zero() || rhs.is_zero() {
            return UPoly::zero();
        }
        let mut out = vec![Rat::zero(); self.coeffs.len() + rhs.coeffs.len() - 1];
        for (i, a) in self.coeffs.iter().enumerate() {
            if a.is_zero() {
                continue;
            }
            for (j, b) in rhs.coeffs.iter().enumerate() {
                if b.is_zero() {
                    continue;
                }
                out[i + j] += a * b;
            }
        }
        UPoly::from_coeffs(out)
    }
}

impl Add for UPoly {
    type Output = UPoly;
    fn add(self, rhs: UPoly) -> UPoly {
        &self + &rhs
    }
}

impl Sub for UPoly {
    type Output = UPoly;
    fn sub(self, rhs: UPoly) -> UPoly {
        &self - &rhs
    }
}

impl Mul for UPoly {
    type Output = UPoly;
    fn mul(self, rhs: UPoly) -> UPoly {
        &self * &rhs
    }
}

// ---------------------------------------------------------------------------
// Integer polynomials — the working representation for GCD and factorisation
// ---------------------------------------------------------------------------

/// Dense integer polynomial, ascending and trimmed.
///
/// Private on purpose: it is a computational detail of the GCD and the
/// factoriser, not part of the CAS surface. Working in ℤ rather than in ℚ keeps
/// the modular reductions and the Landau–Mignotte bounds honest and avoids a
/// `BigRational` normalisation on every coefficient operation.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ZPoly(Vec<BigInt>);

impl ZPoly {
    fn from_vec(mut v: Vec<BigInt>) -> ZPoly {
        while v.last().is_some_and(Zero::is_zero) {
            v.pop();
        }
        ZPoly(v)
    }

    fn zero() -> ZPoly {
        ZPoly(Vec::new())
    }

    fn one() -> ZPoly {
        ZPoly(vec![BigInt::one()])
    }

    /// The primitive part of `p`, as integers, with the sign of `p`'s leading
    /// coefficient preserved.
    fn from_upoly(p: &UPoly) -> ZPoly {
        let pp = p.primitive_part();
        ZPoly::from_vec(pp.coeffs.iter().map(|c| c.numer().clone()).collect())
    }

    fn to_upoly(&self) -> UPoly {
        UPoly::from_coeffs(
            self.0
                .iter()
                .map(|c| Rat::from_integer(c.clone()))
                .collect(),
        )
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn degree(&self) -> Option<usize> {
        self.0.len().checked_sub(1)
    }

    fn degree_or_zero(&self) -> usize {
        self.degree().unwrap_or(0)
    }

    fn lc(&self) -> BigInt {
        self.0.last().cloned().unwrap_or_else(BigInt::zero)
    }

    fn coeff(&self, i: usize) -> BigInt {
        self.0.get(i).cloned().unwrap_or_else(BigInt::zero)
    }

    fn sub(&self, rhs: &ZPoly) -> ZPoly {
        let n = self.0.len().max(rhs.0.len());
        ZPoly::from_vec((0..n).map(|i| self.coeff(i) - rhs.coeff(i)).collect())
    }

    fn scale(&self, k: &BigInt) -> ZPoly {
        if k.is_zero() {
            return ZPoly::zero();
        }
        ZPoly::from_vec(self.0.iter().map(|c| c * k).collect())
    }

    /// Exact division of every coefficient by `k`, or `None`.
    fn scale_div(&self, k: &BigInt) -> Option<ZPoly> {
        if k.is_zero() {
            return None;
        }
        let mut out = Vec::with_capacity(self.0.len());
        for c in &self.0 {
            let (q, r) = c.div_rem(k);
            if !r.is_zero() {
                return None;
            }
            out.push(q);
        }
        Some(ZPoly::from_vec(out))
    }

    /// Non-negative gcd of the coefficients; zero for the zero polynomial.
    fn content(&self) -> BigInt {
        let mut g = BigInt::zero();
        for c in &self.0 {
            g = g.gcd(c);
        }
        g
    }

    fn primitive_part(&self) -> ZPoly {
        let c = self.content();
        if c.is_zero() {
            return ZPoly::zero();
        }
        self.scale_div(&c).unwrap_or_else(ZPoly::zero)
    }

    /// Primitive with a **positive** leading coefficient — the canonical form
    /// for a GCD or an irreducible factor.
    fn primitive_positive(&self) -> ZPoly {
        let p = self.primitive_part();
        if p.lc().is_negative() {
            ZPoly::from_vec(p.0.iter().map(|c| -c).collect())
        } else {
            p
        }
    }

    /// `self / divisor` when exact over ℤ, `None` otherwise.
    fn exact_div(&self, divisor: &ZPoly) -> Option<ZPoly> {
        let dd = divisor.degree()?;
        let Some(nd) = self.degree() else {
            return Some(ZPoly::zero());
        };
        if nd < dd {
            return None;
        }
        let dlc = divisor.lc();
        let mut rem = self.0.clone();
        let mut quot = vec![BigInt::zero(); nd - dd + 1];
        for k in (0..=nd - dd).rev() {
            let (q, r) = rem[k + dd].div_rem(&dlc);
            if !r.is_zero() {
                return None;
            }
            if q.is_zero() {
                continue;
            }
            for (i, dc) in divisor.0.iter().enumerate() {
                rem[k + i] -= &q * dc;
            }
            quot[k] = q;
        }
        ZPoly::from_vec(rem)
            .is_zero()
            .then(|| ZPoly::from_vec(quot))
    }

    /// Pseudo-remainder: `lc(divisor)^(deg self - deg divisor + 1) · self mod divisor`.
    fn pseudo_rem(&self, divisor: &ZPoly) -> Option<ZPoly> {
        let dd = divisor.degree()?;
        let Some(nd) = self.degree() else {
            return Some(ZPoly::zero());
        };
        if nd < dd {
            return Some(self.clone());
        }
        let dlc = divisor.lc();
        let mut rem = self.clone();
        for _ in 0..=(nd - dd) {
            let Some(rd) = rem.degree() else { break };
            if rd < dd {
                break;
            }
            let shift = rd - dd;
            let factor = rem.lc();
            // rem ← dlc·rem − factor·x^shift·divisor  (the leading term cancels)
            let mut shifted = vec![BigInt::zero(); shift];
            shifted.extend(divisor.0.iter().map(|c| c * &factor));
            rem = rem.scale(&dlc).sub(&ZPoly::from_vec(shifted));
        }
        Some(rem)
    }

    /// `ceil(‖self‖₂)` — the Landau–Mignotte input. Overestimates by at most 1.
    ///
    /// Accumulated as an unsigned magnitude so the integer square root cannot be
    /// handed a negative value.
    fn norm2_ceil(&self) -> BigInt {
        let mut sum = BigUint::zero();
        for c in &self.0 {
            let m = c.magnitude();
            sum += m * m;
        }
        BigInt::from(sum.sqrt() + BigUint::one())
    }

    /// Coefficients reduced into `[0, p)`.
    fn mod_p(&self, p: u64) -> Vec<u64> {
        let bp = BigInt::from(p);
        let mut out: Vec<u64> = self
            .0
            .iter()
            .map(|c| c.mod_floor(&bp).to_u64().unwrap_or(0))
            .collect();
        fp_trim(&mut out);
        out
    }
}

// ---------------------------------------------------------------------------
// GCD over ℤ[x]: modular with an exact certificate, primitive PRS as backstop
// ---------------------------------------------------------------------------

/// Primitive GCD with a positive leading coefficient.
fn zpoly_gcd(a: &ZPoly, b: &ZPoly) -> ZPoly {
    if a.is_zero() {
        return b.primitive_positive();
    }
    if b.is_zero() {
        return a.primitive_positive();
    }
    let content_gcd = a.content().gcd(&b.content());
    let ap = a.primitive_positive();
    let bp = b.primitive_positive();
    if ap.degree_or_zero() == 0 || bp.degree_or_zero() == 0 {
        return ZPoly::one().scale(&content_gcd);
    }
    let primitive = modular_gcd(&ap, &bp).unwrap_or_else(|| primitive_prs_gcd(&ap, &bp));
    primitive.scale(&content_gcd)
}

/// Brown's modular GCD, restricted to what we can certify.
///
/// Returns `None` when the prime supply runs out, in which case the caller
/// falls back to PRS. A returned value has been trial-divided into both inputs,
/// so it is a common divisor; because its degree equals the minimum degree of
/// any modular GCD seen (and that degree bounds the true GCD's from above), it
/// *is* the GCD.
fn modular_gcd(a: &ZPoly, b: &ZPoly) -> Option<ZPoly> {
    let target_lc = a.lc().gcd(&b.lc());
    let mut best_degree = usize::MAX;
    let mut crt_value: Vec<BigInt> = Vec::new();
    let mut crt_modulus = BigInt::one();

    for &p in gcd_primes() {
        let bp = BigInt::from(p);
        if (a.lc().mod_floor(&bp)).is_zero() || (b.lc().mod_floor(&bp)).is_zero() {
            continue;
        }
        let ga = fp_gcd(&a.mod_p(p), &b.mod_p(p), p);
        let dg = fp_degree(&ga)?;
        if dg == 0 {
            // A modular GCD of degree 0 proves the true GCD is a unit.
            return Some(ZPoly::one());
        }
        // Scale so the image's leading coefficient matches gcd(lc a, lc b); the
        // true GCD's leading coefficient divides that.
        let want = target_lc.mod_floor(&bp).to_u64().unwrap_or(0);
        if want == 0 {
            continue;
        }
        let scaled = fp_scale(&ga, want, p);

        match dg.cmp(&best_degree) {
            Ordering::Less => {
                best_degree = dg;
                crt_value = scaled.iter().map(|&c| BigInt::from(c)).collect();
                crt_modulus = bp;
            }
            Ordering::Equal => {
                crt_value = crt_combine(&crt_value, &crt_modulus, &scaled, p);
                crt_modulus *= &bp;
            }
            // Unlucky prime: its image is too coarse. Discard it.
            Ordering::Greater => continue,
        }

        let candidate =
            ZPoly::from_vec(symmetric_lift(&crt_value, &crt_modulus)).primitive_positive();
        if candidate.degree_or_zero() != best_degree {
            continue;
        }
        if a.exact_div(&candidate).is_some() && b.exact_div(&candidate).is_some() {
            return Some(candidate);
        }
    }
    None
}

/// CRT-combine a residue vector mod `m` with one mod `p` into a vector mod `m·p`.
fn crt_combine(value: &[BigInt], m: &BigInt, other: &[u64], p: u64) -> Vec<BigInt> {
    let bp = BigInt::from(p);
    let m_mod_p = m.mod_floor(&bp).to_u64().unwrap_or(0);
    let inv = fp_inv(m_mod_p, p);
    let n = value.len().max(other.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let vi = value.get(i).cloned().unwrap_or_else(BigInt::zero);
        let oi = other.get(i).copied().unwrap_or(0);
        // delta = (oi − vi) · m⁻¹  (mod p)
        let vi_mod_p = vi.mod_floor(&bp).to_u64().unwrap_or(0);
        let diff = (oi + p - vi_mod_p) % p;
        let delta = fp_mul_u64(diff, inv, p);
        out.push(vi + m * BigInt::from(delta));
    }
    out
}

/// Map residues in `[0, m)` into the symmetric range `(-m/2, m/2]`.
fn symmetric_lift(value: &[BigInt], m: &BigInt) -> Vec<BigInt> {
    let half = m / 2;
    value
        .iter()
        .map(|c| {
            let r = c.mod_floor(m);
            if r > half {
                r - m
            } else {
                r
            }
        })
        .collect()
}

/// Primitive PRS: the unconditionally-correct backstop.
///
/// Slower than subresultant PRS (it takes a content GCD every step instead of
/// dividing by a predicted factor), and that is the point — the fallback's job
/// is to be obviously right, not fast. The modular path above is the fast one,
/// and the tests check the two against each other.
fn primitive_prs_gcd(a: &ZPoly, b: &ZPoly) -> ZPoly {
    let mut u = a.primitive_positive();
    let mut v = b.primitive_positive();
    if u.degree_or_zero() < v.degree_or_zero() {
        std::mem::swap(&mut u, &mut v);
    }
    while !v.is_zero() {
        let Some(r) = u.pseudo_rem(&v) else {
            return ZPoly::one();
        };
        u = v;
        v = r.primitive_positive();
    }
    u.primitive_positive()
}

// ---------------------------------------------------------------------------
// 𝔽_p[x]
// ---------------------------------------------------------------------------
//
// Coefficients live in `[0, p)` with `p < 2^31`, so every product fits a `u64`.
// Vectors are ascending and trimmed, matching `UPoly`/`ZPoly`.

fn fp_trim(v: &mut Vec<u64>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn fp_degree(v: &[u64]) -> Option<usize> {
    v.len().checked_sub(1)
}

fn fp_is_zero(v: &[u64]) -> bool {
    v.is_empty()
}

fn fp_mul_u64(a: u64, b: u64, p: u64) -> u64 {
    (a % p) * (b % p) % p
}

/// `a⁻¹ mod p` by the extended Euclidean algorithm; `0` for `a ≡ 0`.
fn fp_inv(a: u64, p: u64) -> u64 {
    let a = a % p;
    if a == 0 {
        return 0;
    }
    let (mut old_r, mut r) = (a as i128, p as i128);
    let (mut old_s, mut s) = (1i128, 0i128);
    while r != 0 {
        let q = old_r / r;
        let next_r = old_r - q * r;
        old_r = r;
        r = next_r;
        let next_s = old_s - q * s;
        old_s = s;
        s = next_s;
    }
    let inv = old_s.rem_euclid(p as i128);
    inv as u64
}

fn fp_sub(a: &[u64], b: &[u64], p: u64) -> Vec<u64> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        out.push((x + p - y) % p);
    }
    fp_trim(&mut out);
    out
}

fn fp_mul(a: &[u64], b: &[u64], p: u64) -> Vec<u64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u64; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        if x == 0 {
            continue;
        }
        for (j, &y) in b.iter().enumerate() {
            out[i + j] = (out[i + j] + x * y) % p;
        }
    }
    fp_trim(&mut out);
    out
}

fn fp_scale(a: &[u64], k: u64, p: u64) -> Vec<u64> {
    let k = k % p;
    if k == 0 {
        return Vec::new();
    }
    let mut out: Vec<u64> = a.iter().map(|&c| c * k % p).collect();
    fp_trim(&mut out);
    out
}

fn fp_div_rem(a: &[u64], b: &[u64], p: u64) -> Option<(Vec<u64>, Vec<u64>)> {
    let db = fp_degree(b)?;
    let Some(da) = fp_degree(a) else {
        return Some((Vec::new(), Vec::new()));
    };
    if da < db {
        return Some((Vec::new(), a.to_vec()));
    }
    let inv_lc = fp_inv(b[db], p);
    let mut rem = a.to_vec();
    let mut quot = vec![0u64; da - db + 1];
    for k in (0..=da - db).rev() {
        let factor = fp_mul_u64(rem[k + db], inv_lc, p);
        if factor == 0 {
            continue;
        }
        quot[k] = factor;
        for (i, &bc) in b.iter().enumerate() {
            let sub = fp_mul_u64(factor, bc, p);
            rem[k + i] = (rem[k + i] + p - sub) % p;
        }
    }
    fp_trim(&mut rem);
    fp_trim(&mut quot);
    Some((quot, rem))
}

fn fp_rem(a: &[u64], b: &[u64], p: u64) -> Vec<u64> {
    fp_div_rem(a, b, p).map_or_else(Vec::new, |(_, r)| r)
}

fn fp_monic(a: &[u64], p: u64) -> Vec<u64> {
    match fp_degree(a) {
        None => Vec::new(),
        Some(d) => fp_scale(a, fp_inv(a[d], p), p),
    }
}

fn fp_gcd(a: &[u64], b: &[u64], p: u64) -> Vec<u64> {
    let mut u = a.to_vec();
    let mut v = b.to_vec();
    while !fp_is_zero(&v) {
        let r = fp_rem(&u, &v, p);
        u = v;
        v = r;
    }
    fp_monic(&u, p)
}

/// Extended Euclid in 𝔽_p[x]: `(g, s, t)` with `s·a + t·b = g`, `g` monic.
fn fp_ext_gcd(a: &[u64], b: &[u64], p: u64) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    let (mut s0, mut s1) = (vec![1u64], Vec::new());
    let (mut t0, mut t1) = (Vec::new(), vec![1u64]);
    while !fp_is_zero(&r1) {
        let Some((q, r)) = fp_div_rem(&r0, &r1, p) else {
            break;
        };
        r0 = std::mem::replace(&mut r1, r);
        let s_next = fp_sub(&s0, &fp_mul(&q, &s1, p), p);
        s0 = std::mem::replace(&mut s1, s_next);
        let t_next = fp_sub(&t0, &fp_mul(&q, &t1, p), p);
        t0 = std::mem::replace(&mut t1, t_next);
    }
    match fp_degree(&r0) {
        None => (Vec::new(), Vec::new(), Vec::new()),
        Some(d) => {
            let inv = fp_inv(r0[d], p);
            (
                fp_scale(&r0, inv, p),
                fp_scale(&s0, inv, p),
                fp_scale(&t0, inv, p),
            )
        }
    }
}

fn fp_derivative(a: &[u64], p: u64) -> Vec<u64> {
    if a.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<u64> = a
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, &c)| fp_mul_u64(c, (i as u64) % p, p))
        .collect();
    fp_trim(&mut out);
    out
}

/// `base^exp mod modulus` in 𝔽_p[x], square-and-multiply over the exponent bits.
fn fp_pow_mod(base: &[u64], exp: &BigUint, modulus: &[u64], p: u64) -> Vec<u64> {
    if fp_degree(modulus).unwrap_or(0) == 0 {
        return Vec::new();
    }
    let mut result = vec![1u64];
    let mut acc = fp_rem(base, modulus, p);
    let bits = exp.bits();
    for i in 0..bits {
        if exp.bit(i) {
            result = fp_rem(&fp_mul(&result, &acc, p), modulus, p);
        }
        if i + 1 < bits {
            acc = fp_rem(&fp_mul(&acc, &acc, p), modulus, p);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Primes
// ---------------------------------------------------------------------------

/// Odd primes below 2^16, sieved once. Products of any two fit in a `u64` with
/// room to spare, which is what `fp_mul_u64` relies on.
fn sieve_primes() -> &'static [u64] {
    static PRIMES: OnceLock<Vec<u64>> = OnceLock::new();
    PRIMES.get_or_init(|| {
        const LIMIT: usize = 1 << 16;
        let mut is_composite = vec![false; LIMIT];
        let mut out = Vec::new();
        for n in 3..LIMIT {
            if is_composite[n] {
                continue;
            }
            if n % 2 == 1 {
                out.push(n as u64);
            }
            let mut m = n * n;
            while m < LIMIT {
                is_composite[m] = true;
                m += n;
            }
        }
        out
    })
}

/// Primes for the modular GCD, largest first — fewer CRT rounds that way.
fn gcd_primes() -> &'static [u64] {
    static PRIMES: OnceLock<Vec<u64>> = OnceLock::new();
    PRIMES.get_or_init(|| {
        let mut v: Vec<u64> = sieve_primes().iter().rev().copied().take(256).collect();
        v.dedup();
        v
    })
}

/// Primes for the factoriser, smallest first — Cantor–Zassenhaus costs
/// `O(log p)` modular squarings per level, so small is cheap.
fn factor_primes() -> &'static [u64] {
    static PRIMES: OnceLock<Vec<u64>> = OnceLock::new();
    PRIMES.get_or_init(|| sieve_primes().iter().copied().take(200).collect())
}

// ---------------------------------------------------------------------------
// Deterministic PRNG for Cantor–Zassenhaus
// ---------------------------------------------------------------------------

/// SplitMix64. Seeded from the polynomial being split, so `Factor` is
/// **reproducible**: the same input always produces the same output, which the
/// golden-fixture comparison depends on.
struct SplitMix(u64);

impl SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn seed_from(f: &[u64]) -> SplitMix {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &c in f {
        h ^= c;
        h = h.wrapping_mul(0x1000_0000_01B3);
    }
    SplitMix(h | 1)
}

// ---------------------------------------------------------------------------
// Factorisation over 𝔽_p: distinct-degree then equal-degree
// ---------------------------------------------------------------------------

/// Factor a monic square-free `f` over 𝔽_p into monic irreducibles.
///
/// The `(degree bucket, factor degree)` structure is what certifies
/// irreducibility: a factor pulled out of the degree-`d` bucket that itself has
/// degree `d` is irreducible, with no further test needed.
fn fp_factor_square_free(f: &[u64], p: u64) -> PolyResult<Vec<Vec<u64>>> {
    let mut rng = seed_from(f);
    let mut out = Vec::new();
    for (d, part) in fp_distinct_degree(f, p) {
        for piece in fp_equal_degree(&part, d, p, &mut rng)? {
            if fp_degree(&piece) != Some(d) {
                return Err(PolyError::Internal(
                    "equal-degree split produced a wrong-degree factor",
                ));
            }
            out.push(piece);
        }
    }
    Ok(out)
}

/// Split `f` into `(d, product of its irreducible factors of degree d)`.
fn fp_distinct_degree(f: &[u64], p: u64) -> Vec<(usize, Vec<u64>)> {
    let x = vec![0u64, 1];
    let mut v = f.to_vec();
    let mut h = x.clone();
    let mut d = 0usize;
    let mut out = Vec::new();
    let bp = BigUint::from(p);
    while fp_degree(&v).unwrap_or(0) >= 2 * (d + 1) {
        d += 1;
        h = fp_pow_mod(&h, &bp, &v, p);
        let g = fp_gcd(&fp_sub(&h, &x, p), &v, p);
        if fp_degree(&g).unwrap_or(0) > 0 {
            if let Some((q, _)) = fp_div_rem(&v, &g, p) {
                v = q;
            }
            out.push((d, g));
            h = fp_rem(&h, &v, p);
        }
    }
    if fp_degree(&v).unwrap_or(0) > 0 {
        let dv = fp_degree(&v).unwrap_or(0);
        out.push((dv, v));
    }
    out
}

/// Cantor–Zassenhaus: split a product of degree-`d` irreducibles.
fn fp_equal_degree(f: &[u64], d: usize, p: u64, rng: &mut SplitMix) -> PolyResult<Vec<Vec<u64>>> {
    let n = fp_degree(f).unwrap_or(0);
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == d {
        return Ok(vec![f.to_vec()]);
    }
    // (p^d − 1)/2. `p` is always odd here (the sieve starts at 3), so this is
    // an integer and the classical odd-characteristic split applies.
    let exponent = (BigUint::from(p).pow(d as u32) - BigUint::one()) / BigUint::from(2u32);
    for _ in 0..MAX_SPLIT_ATTEMPTS {
        let mut a: Vec<u64> = (0..n).map(|_| rng.next_u64() % p).collect();
        fp_trim(&mut a);
        if fp_degree(&a).unwrap_or(0) == 0 {
            continue;
        }
        let mut g = fp_gcd(&a, f, p);
        if fp_degree(&g).unwrap_or(0) == 0 {
            let b = fp_pow_mod(&a, &exponent, f, p);
            g = fp_gcd(&fp_sub(&b, &[1], p), f, p);
        }
        let dg = fp_degree(&g).unwrap_or(0);
        if dg == 0 || dg == n {
            continue;
        }
        let Some((cofactor, _)) = fp_div_rem(f, &g, p) else {
            continue;
        };
        let mut left = fp_equal_degree(&fp_monic(&g, p), d, p, rng)?;
        let right = fp_equal_degree(&fp_monic(&cofactor, p), d, p, rng)?;
        left.extend(right);
        return Ok(left);
    }
    Err(PolyError::Internal(
        "Cantor–Zassenhaus failed to split within the attempt cap",
    ))
}

// ---------------------------------------------------------------------------
// Hensel lifting (mod m → mod m²), and the monic multi-factor lift
// ---------------------------------------------------------------------------

/// A polynomial with coefficients reduced into `[0, m)`.
type ZmPoly = Vec<BigInt>;

fn zm_norm(v: Vec<BigInt>, m: &BigInt) -> ZmPoly {
    let mut out: Vec<BigInt> = v.into_iter().map(|c| c.mod_floor(m)).collect();
    while out.last().is_some_and(Zero::is_zero) {
        out.pop();
    }
    out
}

fn zm_from_fp(v: &[u64]) -> ZmPoly {
    v.iter().map(|&c| BigInt::from(c)).collect()
}

fn zm_degree(v: &[BigInt]) -> Option<usize> {
    v.len().checked_sub(1)
}

fn zm_add(a: &[BigInt], b: &[BigInt], m: &BigInt) -> ZmPoly {
    let n = a.len().max(b.len());
    let zero = BigInt::zero();
    let out = (0..n)
        .map(|i| a.get(i).unwrap_or(&zero) + b.get(i).unwrap_or(&zero))
        .collect();
    zm_norm(out, m)
}

fn zm_sub(a: &[BigInt], b: &[BigInt], m: &BigInt) -> ZmPoly {
    let n = a.len().max(b.len());
    let zero = BigInt::zero();
    let out = (0..n)
        .map(|i| a.get(i).unwrap_or(&zero) - b.get(i).unwrap_or(&zero))
        .collect();
    zm_norm(out, m)
}

fn zm_mul(a: &[BigInt], b: &[BigInt], m: &BigInt) -> ZmPoly {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![BigInt::zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            out[i + j] += x * y;
        }
    }
    zm_norm(out, m)
}

/// Division by a **monic** polynomial mod `m` — always exact, no inverse needed.
fn zm_div_rem_monic(a: &[BigInt], b: &[BigInt], m: &BigInt) -> Option<(ZmPoly, ZmPoly)> {
    let db = zm_degree(b)?;
    let Some(da) = zm_degree(a) else {
        return Some((Vec::new(), Vec::new()));
    };
    if da < db {
        return Some((Vec::new(), a.to_vec()));
    }
    let mut rem = a.to_vec();
    let mut quot = vec![BigInt::zero(); da - db + 1];
    for k in (0..=da - db).rev() {
        let factor = rem[k + db].clone().mod_floor(m);
        if factor.is_zero() {
            continue;
        }
        for (i, bc) in b.iter().enumerate() {
            rem[k + i] = (&rem[k + i] - &factor * bc).mod_floor(m);
        }
        quot[k] = factor;
    }
    Some((zm_norm(quot, m), zm_norm(rem, m)))
}

/// One quadratic Hensel step, `mod m` → `mod m²`.
///
/// von zur Gathen & Gerhard, *Modern Computer Algebra*, Algorithm 15.10.
/// Preconditions: `f ≡ g·h (mod m)`, `s·g + t·h ≡ 1 (mod m)`, `g` and `h`
/// monic. `f` monic keeps `g*` and `h*` monic, which every caller here relies
/// on — the postcondition is checked rather than assumed.
struct HenselState {
    g: ZmPoly,
    h: ZmPoly,
    s: ZmPoly,
    t: ZmPoly,
}

fn hensel_step(f: &[BigInt], state: &HenselState, m: &BigInt) -> Option<HenselState> {
    let m2 = m * m;
    let f2 = zm_norm(f.to_vec(), &m2);
    let g = zm_norm(state.g.clone(), &m2);
    let h = zm_norm(state.h.clone(), &m2);
    let s = zm_norm(state.s.clone(), &m2);
    let t = zm_norm(state.t.clone(), &m2);

    // e ← f − g·h;  (q, r) ← s·e divided by h;  g* ← g + t·e + q·g;  h* ← h + r
    let e = zm_sub(&f2, &zm_mul(&g, &h, &m2), &m2);
    let (q, r) = zm_div_rem_monic(&zm_mul(&s, &e, &m2), &h, &m2)?;
    let g_next = zm_add(
        &zm_add(&g, &zm_mul(&t, &e, &m2), &m2),
        &zm_mul(&q, &g, &m2),
        &m2,
    );
    let h_next = zm_add(&h, &r, &m2);

    // Both must stay monic of the same degree, or the preconditions of the next
    // step (and of recombination) are gone.
    if zm_degree(&g_next) != zm_degree(&g) || zm_degree(&h_next) != zm_degree(&h) {
        return None;
    }
    if !g_next.last().is_some_and(One::is_one) || !h_next.last().is_some_and(One::is_one) {
        return None;
    }

    // b ← s·g* + t·h* − 1;  (c, d) ← s·b divided by h*;  s* ← s − d;
    // t* ← t − t·b − c·g*
    let one = vec![BigInt::one()];
    let b = zm_sub(
        &zm_add(&zm_mul(&s, &g_next, &m2), &zm_mul(&t, &h_next, &m2), &m2),
        &one,
        &m2,
    );
    let (c, d) = zm_div_rem_monic(&zm_mul(&s, &b, &m2), &h_next, &m2)?;
    let s_next = zm_sub(&s, &d, &m2);
    let t_next = zm_sub(
        &zm_sub(&t, &zm_mul(&t, &b, &m2), &m2),
        &zm_mul(&c, &g_next, &m2),
        &m2,
    );

    Some(HenselState {
        g: g_next,
        h: h_next,
        s: s_next,
        t: t_next,
    })
}

/// Lift `f ≡ gp·hp (mod p)` up to `f ≡ G·H (mod target)`.
fn hensel_lift_pair(
    f: &[BigInt],
    gp: &[u64],
    hp: &[u64],
    p: u64,
    target: &BigInt,
) -> Option<(ZmPoly, ZmPoly)> {
    let (d, s, t) = fp_ext_gcd(gp, hp, p);
    if fp_degree(&d) != Some(0) {
        return None; // not coprime mod p — this prime is unusable
    }
    let mut state = HenselState {
        g: zm_from_fp(gp),
        h: zm_from_fp(hp),
        s: zm_from_fp(&s),
        t: zm_from_fp(&t),
    };
    let mut m = BigInt::from(p);
    while m < *target {
        state = hensel_step(f, &state, &m)?;
        m = &m * &m;
    }
    // Bring both onto exactly the working modulus the caller uses.
    Some((zm_norm(state.g, target), zm_norm(state.h, target)))
}

/// Lift a complete modular factorisation to `modulus`, by a balanced binary
/// tree of pairwise lifts.
fn hensel_lift_all(
    f: &[BigInt],
    factors: &[Vec<u64>],
    p: u64,
    modulus: &BigInt,
) -> Option<Vec<ZmPoly>> {
    if factors.len() == 1 {
        return Some(vec![f.to_vec()]);
    }
    let mid = factors.len() / 2;
    let mut gp = vec![1u64];
    for piece in &factors[..mid] {
        gp = fp_mul(&gp, piece, p);
    }
    let mut hp = vec![1u64];
    for piece in &factors[mid..] {
        hp = fp_mul(&hp, piece, p);
    }
    let (g, h) = hensel_lift_pair(f, &gp, &hp, p, modulus)?;
    let mut left = hensel_lift_all(&g, &factors[..mid], p, modulus)?;
    let right = hensel_lift_all(&h, &factors[mid..], p, modulus)?;
    left.extend(right);
    Some(left)
}

// ---------------------------------------------------------------------------
// Zassenhaus over ℚ
// ---------------------------------------------------------------------------

/// Factor a square-free monic-over-ℚ polynomial into primitive irreducible
/// factors with positive leading coefficients.
fn factor_square_free(f: &UPoly) -> PolyResult<Vec<UPoly>> {
    let deg = f.degree_or_zero();
    if deg == 0 {
        return Ok(Vec::new());
    }
    let z = ZPoly::from_upoly(f).primitive_positive();
    if deg == 1 {
        return Ok(vec![z.to_upoly()]);
    }

    // Monicise by x ← x/L: f̃(x) = L^(n−1)·f(x/L) has integer coefficients, is
    // monic, and factors in bijection with f. Documented in the module header
    // as the one place this factoriser trades generality for simplicity.
    let lc = z.lc();
    let monicised = if lc.is_one() {
        z.clone()
    } else {
        let mut coeffs = Vec::with_capacity(z.0.len());
        for (i, c) in z.0.iter().enumerate() {
            match (deg - 1).checked_sub(i) {
                Some(e) => coeffs.push(c * lc.pow(e as u32)),
                // i == deg: c == lc, and c·L^(−1) == 1.
                None => coeffs.push(BigInt::one()),
            }
        }
        ZPoly::from_vec(coeffs)
    };

    let pieces = zassenhaus_monic(&monicised)?;

    // Map back: g(x) = pp(h(L·x)).
    let mut out = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let mapped = if lc.is_one() {
            piece
        } else {
            ZPoly::from_vec(
                piece
                    .0
                    .iter()
                    .enumerate()
                    .map(|(i, c)| c * lc.pow(i as u32))
                    .collect(),
            )
        };
        out.push(mapped.primitive_positive().to_upoly());
    }
    Ok(out)
}

/// Zassenhaus proper: monic square-free `f ∈ ℤ[x]`, degree ≥ 2.
fn zassenhaus_monic(f: &ZPoly) -> PolyResult<Vec<ZPoly>> {
    let n = f.degree_or_zero();

    // Pick the prime that yields the fewest modular factors, out of the first
    // few usable candidates. Fewer factors means a smaller recombination search.
    let mut best: Option<(u64, Vec<Vec<u64>>)> = None;
    let mut usable = 0;
    for &p in factor_primes() {
        let fp_image = f.mod_p(p);
        if fp_degree(&fp_image) != Some(n) {
            continue;
        }
        let g = fp_gcd(&fp_image, &fp_derivative(&fp_image, p), p);
        if fp_degree(&g) != Some(0) {
            continue; // not square-free mod p
        }
        let Ok(pieces) = fp_factor_square_free(&fp_monic(&fp_image, p), p) else {
            continue;
        };
        // Certify the modular factorisation before trusting its structure.
        let mut check = vec![1u64];
        for piece in &pieces {
            check = fp_mul(&check, piece, p);
        }
        if check != fp_monic(&fp_image, p) {
            return Err(PolyError::Internal(
                "modular factorisation does not reproduce f mod p",
            ));
        }
        if pieces.len() == 1 {
            return Ok(vec![f.clone()]); // irreducible mod p ⟹ irreducible over ℚ
        }
        if best.as_ref().is_none_or(|(_, b)| pieces.len() < b.len()) {
            best = Some((p, pieces));
        }
        usable += 1;
        if usable >= 5 {
            break;
        }
    }
    let Some((p, pieces)) = best else {
        return Err(PolyError::NoUsablePrime);
    };
    if pieces.len() > MAX_MODULAR_FACTORS {
        return Err(PolyError::FactorTooHard {
            modular_factors: pieces.len(),
            limit: MAX_MODULAR_FACTORS,
        });
    }

    // Landau–Mignotte: every factor h of the monic f satisfies ‖h‖∞ ≤ 2ⁿ‖f‖₂.
    // A modulus above twice that makes symmetric-range reconstruction exact.
    let bound = (BigInt::one() << n) * f.norm2_ceil();
    let target = bound * 2 + BigInt::one();
    let mut modulus = BigInt::from(p);
    while modulus < target {
        modulus = &modulus * &modulus;
    }

    let f_mod = zm_norm(f.0.clone(), &modulus);
    let Some(lifted) = hensel_lift_all(&f_mod, &pieces, p, &modulus) else {
        return Err(PolyError::Internal("Hensel lifting failed"));
    };
    // Certify the lift: without this, a lifting bug would silently turn a
    // reducible polynomial into a "prime" one.
    let mut check = vec![BigInt::one()];
    for piece in &lifted {
        check = zm_mul(&check, piece, &modulus);
    }
    if check != f_mod {
        return Err(PolyError::Internal(
            "Hensel lift does not reproduce f mod p^k",
        ));
    }

    recombine(f, &lifted, &modulus)
}

/// Subset recombination over ℤ.
fn recombine(f: &ZPoly, lifted: &[ZmPoly], modulus: &BigInt) -> PolyResult<Vec<ZPoly>> {
    let mut remaining: Vec<usize> = (0..lifted.len()).collect();
    let mut current = f.clone();
    let mut out: Vec<ZPoly> = Vec::new();
    let mut budget = MAX_RECOMBINATIONS;
    let mut size = 1usize;

    while size * 2 <= remaining.len() {
        let mut idx: Vec<usize> = (0..size).collect();
        let mut split = false;
        loop {
            if budget == 0 {
                return Err(PolyError::FactorTooHard {
                    modular_factors: lifted.len(),
                    limit: MAX_RECOMBINATIONS,
                });
            }
            budget -= 1;

            let mut product = vec![BigInt::one()];
            for &k in &idx {
                product = zm_mul(&product, &lifted[remaining[k]], modulus);
            }
            let candidate = ZPoly::from_vec(symmetric_lift(&product, modulus));
            if candidate.degree_or_zero() > 0
                && candidate.degree_or_zero() < current.degree_or_zero()
                && divides_trailing(&current, &candidate)
            {
                if let Some(cofactor) = current.exact_div(&candidate) {
                    out.push(candidate.primitive_positive());
                    current = cofactor;
                    for k in idx.iter().rev() {
                        remaining.remove(*k);
                    }
                    split = true;
                    break;
                }
            }
            if !next_combination(&mut idx, remaining.len()) {
                break;
            }
        }
        if !split {
            size += 1;
        }
    }
    if current.degree_or_zero() > 0 {
        out.push(current.primitive_positive());
    }
    Ok(out)
}

/// Cheap pre-filter: a divisor's trailing coefficient divides the dividend's.
/// Kills the great majority of wrong subsets before a full trial division.
fn divides_trailing(dividend: &ZPoly, divisor: &ZPoly) -> bool {
    let a = dividend.coeff(0);
    let b = divisor.coeff(0);
    if b.is_zero() {
        return a.is_zero();
    }
    (a % b).is_zero()
}

/// Advance `idx` to the next combination of `size` indices out of `n`.
fn next_combination(idx: &mut [usize], n: usize) -> bool {
    let k = idx.len();
    if k == 0 || k > n {
        return false;
    }
    let mut i = k;
    while i > 0 {
        i -= 1;
        if idx[i] < n - (k - i) {
            idx[i] += 1;
            for j in i + 1..k {
                idx[j] = idx[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Sparse multivariate polynomials over ℚ
// ---------------------------------------------------------------------------

/// An exponent vector, aligned to its owner's variable list.
///
/// Derived `Ord` gives lexicographic order on exponent vectors, which is a
/// legitimate monomial order (compatible with multiplication and a well-order
/// on ℕⁿ), so `BTreeMap` iteration is a deterministic term order.
///
/// **A `Mono` is meaningful only against the [`MPoly::vars`] list it came
/// from.** [`MPoly`] drops variables that stop occurring, which re-indexes its
/// monomials, so two `Mono`s from different polynomials must be aligned onto a
/// common variable list before they are compared, divided or multiplied.
/// Getting this wrong is not a rounding error — it made an early version of
/// `div_rem` loop forever, believing `y` was divisible by `x`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mono(Vec<u32>);

impl Mono {
    /// The exponent of variable `i` in the owner's variable list.
    pub fn exponent(&self, i: usize) -> u32 {
        self.0.get(i).copied().unwrap_or(0)
    }

    /// Sum of all exponents.
    pub fn total_degree(&self) -> u32 {
        self.0.iter().sum()
    }

    /// The raw exponent slice.
    pub fn exponents(&self) -> &[u32] {
        &self.0
    }
}

/// A sparse multivariate polynomial over ℚ.
///
/// Variables are held by name, sorted and de-duplicated, so two polynomials
/// built independently compare and combine correctly. Binary operations unify
/// the two variable lists first.
///
/// GCD and factorisation are **not** provided here — see the module header. The
/// operations that are here are what `Expand`, `Collect` and the `Expr`
/// round-trip in `cas::ops` need.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MPoly {
    vars: Vec<String>,
    terms: BTreeMap<Mono, Rat>,
}

impl MPoly {
    /// The zero polynomial (no variables).
    pub fn zero() -> MPoly {
        MPoly::default()
    }

    /// The constant `1`.
    pub fn one() -> MPoly {
        MPoly::constant(Rat::one())
    }

    /// A constant.
    pub fn constant(c: Rat) -> MPoly {
        let mut terms = BTreeMap::new();
        if !c.is_zero() {
            terms.insert(Mono(Vec::new()), c);
        }
        MPoly {
            vars: Vec::new(),
            terms,
        }
    }

    /// The polynomial `name`.
    pub fn var(name: &str) -> MPoly {
        let mut terms = BTreeMap::new();
        terms.insert(Mono(vec![1]), Rat::one());
        MPoly {
            vars: vec![name.to_string()],
            terms,
        }
    }

    /// Lift a univariate polynomial into the multivariate world.
    pub fn from_upoly(p: &UPoly, var: &str) -> MPoly {
        let mut terms = BTreeMap::new();
        for (i, c) in p.coeffs().iter().enumerate() {
            if c.is_zero() {
                continue;
            }
            terms.insert(Mono(vec![i as u32]), c.clone());
        }
        let mut out = MPoly {
            vars: vec![var.to_string()],
            terms,
        };
        out.prune();
        out
    }

    /// The variable names, sorted.
    pub fn vars(&self) -> &[String] {
        &self.vars
    }

    /// The terms, in the module's monomial order.
    pub fn terms(&self) -> impl Iterator<Item = (&Mono, &Rat)> {
        self.terms.iter()
    }

    /// Number of non-zero terms.
    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn is_constant(&self) -> bool {
        self.terms.keys().all(|m| m.0.iter().all(|&e| e == 0))
    }

    /// The constant value, or `None` if any variable actually occurs.
    pub fn as_constant(&self) -> Option<Rat> {
        if !self.is_constant() {
            return None;
        }
        Some(
            self.terms
                .values()
                .next()
                .cloned()
                .unwrap_or_else(Rat::zero),
        )
    }

    /// Highest total degree; `None` for the zero polynomial.
    pub fn total_degree(&self) -> Option<u32> {
        self.terms.keys().map(Mono::total_degree).max()
    }

    /// Highest exponent of `var`; `0` if it does not occur.
    pub fn degree_in(&self, var: &str) -> u32 {
        let Some(i) = self.vars.iter().position(|v| v == var) else {
            return 0;
        };
        self.terms.keys().map(|m| m.exponent(i)).max().unwrap_or(0)
    }

    /// The leading `(monomial, coefficient)` in the module's monomial order.
    pub fn leading_term(&self) -> Option<(&Mono, &Rat)> {
        self.terms.iter().next_back()
    }

    /// The leading coefficient in the module's monomial order; zero for the
    /// zero polynomial.
    pub fn lc(&self) -> Rat {
        self.leading_term()
            .map_or_else(Rat::zero, |(_, c)| c.clone())
    }

    /// Multiply every coefficient by `k`.
    pub fn scale(&self, k: &Rat) -> MPoly {
        if k.is_zero() {
            return MPoly::zero();
        }
        MPoly {
            vars: self.vars.clone(),
            terms: self.terms.iter().map(|(m, c)| (m.clone(), c * k)).collect(),
        }
    }

    /// `self^n`.
    pub fn pow(&self, n: usize) -> MPoly {
        let mut result = MPoly::one();
        let mut base = self.clone();
        let mut e = n;
        while e > 0 {
            if e & 1 == 1 {
                result = &result * &base;
            }
            e >>= 1;
            if e > 0 {
                base = &base * &base;
            }
        }
        result
    }

    /// Non-negative rational content (see [`UPoly::content`]).
    pub fn content(&self) -> Rat {
        if self.is_zero() {
            return Rat::zero();
        }
        let mut num_gcd = BigInt::zero();
        let mut den_lcm = BigInt::one();
        for c in self.terms.values() {
            num_gcd = num_gcd.gcd(c.numer());
            den_lcm = den_lcm.lcm(c.denom());
        }
        Rat::new(num_gcd, den_lcm)
    }

    /// `self / content()`.
    pub fn primitive_part(&self) -> MPoly {
        let c = self.content();
        if c.is_zero() {
            return MPoly::zero();
        }
        self.scale(&c.recip())
    }

    /// Full evaluation. Returns `None` if any occurring variable is unbound.
    pub fn eval(&self, bindings: &BTreeMap<String, Rat>) -> Option<Rat> {
        let mut values = Vec::with_capacity(self.vars.len());
        for v in &self.vars {
            values.push(bindings.get(v)?.clone());
        }
        let mut acc = Rat::zero();
        for (mono, coeff) in &self.terms {
            let mut term = coeff.clone();
            for (i, &e) in mono.0.iter().enumerate() {
                for _ in 0..e {
                    term *= &values[i];
                }
            }
            acc += term;
        }
        Some(acc)
    }

    /// Collapse to a [`UPoly`] when at most `var` occurs.
    pub fn to_upoly(&self, var: &str) -> Option<UPoly> {
        if self.vars.iter().any(|v| v != var) {
            return None;
        }
        let idx = self.vars.iter().position(|v| v == var);
        let mut coeffs: Vec<Rat> = Vec::new();
        for (mono, c) in &self.terms {
            let e = idx.map_or(0, |i| mono.exponent(i)) as usize;
            if coeffs.len() <= e {
                coeffs.resize(e + 1, Rat::zero());
            }
            coeffs[e] += c;
        }
        Some(UPoly::from_coeffs(coeffs))
    }

    /// Coefficients of `self` viewed as a polynomial in `var`: index `i` holds
    /// the coefficient of `var^i`, itself a polynomial in the other variables.
    ///
    /// This is what `Collect(a*x + b*x + c, x)` needs.
    pub fn coeffs_in(&self, var: &str) -> Vec<MPoly> {
        let idx = self.vars.iter().position(|v| v == var);
        let top = self.degree_in(var) as usize;
        let others: Vec<String> = self
            .vars
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != idx)
            .map(|(_, v)| v.clone())
            .collect();
        let mut buckets: Vec<BTreeMap<Mono, Rat>> = vec![BTreeMap::new(); top + 1];
        for (mono, c) in &self.terms {
            let e = idx.map_or(0, |i| mono.exponent(i)) as usize;
            // Rebuild from the variable list, not from the monomial's own
            // length, so every key in a bucket has exactly `others.len()`
            // entries and the monomial order stays well defined.
            let reduced: Vec<u32> = (0..self.vars.len())
                .filter(|i| Some(*i) != idx)
                .map(|i| mono.exponent(i))
                .collect();
            *buckets[e].entry(Mono(reduced)).or_insert_with(Rat::zero) += c;
        }
        buckets
            .into_iter()
            .map(|terms| {
                let mut p = MPoly {
                    vars: others.clone(),
                    terms,
                };
                p.prune();
                p
            })
            .collect()
    }

    /// Multivariate division with a single divisor, in the module's monomial
    /// order: returns `(q, r)` with `self = q·divisor + r` and no term of `r`
    /// divisible by the leading term of `divisor`.
    ///
    /// Note this is the *one-divisor* division algorithm, not a Gröbner-basis
    /// normal form: `r` is not a canonical representative of `self` modulo the
    /// ideal generated by `divisor` unless the division happens to be exact.
    ///
    /// The whole loop runs on raw term maps in one fixed variable frame. Doing
    /// it on [`MPoly`] values instead does not work: every intermediate
    /// subtraction prunes away variables that have cancelled, which re-indexes
    /// the monomials, and comparing the next leading monomial against the
    /// divisor's then compares different variables against each other.
    pub fn div_rem(&self, divisor: &MPoly) -> PolyResult<(MPoly, MPoly)> {
        if divisor.is_zero() {
            return Err(PolyError::DivisionByZero);
        }
        let vars = union_vars(&self.vars, &divisor.vars);
        let divisor_terms = align_terms(&divisor.terms, &divisor.vars, &vars);
        let Some((dm, dc)) = divisor_terms
            .iter()
            .next_back()
            .map(|(m, c)| (m.clone(), c.clone()))
        else {
            return Err(PolyError::DivisionByZero);
        };

        let mut rest = align_terms(&self.terms, &self.vars, &vars);
        let mut quotient: BTreeMap<Mono, Rat> = BTreeMap::new();
        let mut remainder: BTreeMap<Mono, Rat> = BTreeMap::new();

        // Every iteration strictly lowers the leading monomial of `rest` in a
        // well-order, so this terminates. The cap only exists so that a bug
        // here surfaces as an error instead of a frozen browser tab.
        let mut budget = MAX_DIVISION_STEPS;
        while let Some((lm, lc)) = rest.iter().next_back().map(|(m, c)| (m.clone(), c.clone())) {
            if budget == 0 {
                return Err(PolyError::Internal(
                    "multivariate division did not terminate",
                ));
            }
            budget -= 1;
            match mono_div(&lm, &dm) {
                Some(shift) => {
                    let factor = &lc / &dc;
                    for (m, c) in &divisor_terms {
                        let key = mono_mul(&shift, m);
                        let slot = rest.entry(key).or_insert_with(Rat::zero);
                        *slot -= &factor * c;
                    }
                    rest.retain(|_, c| !c.is_zero());
                    *quotient.entry(shift).or_insert_with(Rat::zero) += factor;
                }
                None => {
                    rest.remove(&lm);
                    remainder.insert(lm, lc);
                }
            }
        }

        let mut q = MPoly {
            vars: vars.clone(),
            terms: quotient,
        };
        let mut r = MPoly {
            vars,
            terms: remainder,
        };
        q.prune();
        r.prune();
        Ok((q, r))
    }

    /// `self / divisor` when the division is exact, `None` otherwise.
    pub fn exact_div(&self, divisor: &MPoly) -> Option<MPoly> {
        let (q, r) = self.div_rem(divisor).ok()?;
        r.is_zero().then_some(q)
    }

    /// A **common divisor** of `self` and `other`, normalised to primitive
    /// integer coefficients with a positive leading coefficient.
    ///
    /// # This is not always the *greatest* common divisor
    ///
    /// The result is normalised to primitive integer coefficients with a
    /// positive leading coefficient, and — this is the load-bearing part — it
    /// is **certified** before being returned: whatever the algorithm produces
    /// is trial-divided into both inputs exactly, and rejected in favour of the
    /// always-safe content × monomial gcd if it does not divide. A rational
    /// function can therefore come out *under-reduced*, never mis-reduced.
    ///
    /// # Algorithm
    ///
    /// Rational content and the monomial gcd first (`x²y³` out of
    /// `x³y³ + x²y⁴`), then a recursive **primitive PRS**: pick a main
    /// variable, take the content in it (a GCD in one fewer variable,
    /// recursively), and run a pseudo-remainder sequence reducing to the
    /// primitive part every step. The single-variable base case is
    /// [`UPoly::gcd`], which is exact.
    ///
    /// # Where it gives up
    ///
    /// The PRS carries a work budget ([`MAX_MGCD_STEPS`]); a genuinely large
    /// multivariate problem exhausts it and falls back to the content ×
    /// monomial gcd rather than stalling a browser tab. Coefficient swell in
    /// PRS is real, and this module chose the algorithm that is easy to verify
    /// over the one that is fast (Brown's modular / Zippel's sparse GCD).
    pub fn gcd(&self, other: &MPoly) -> MPoly {
        if self.is_zero() {
            return other.normalised_divisor_form();
        }
        if other.is_zero() {
            return self.normalised_divisor_form();
        }
        // A non-zero constant on either side: over ℚ every non-zero rational is
        // a unit, so the gcd is `1` and the PRS below would spend its whole
        // budget rediscovering that. This is the same answer the general path
        // reaches — its `base` is the monomial gcd (all-zero exponents against
        // a constant) scaled by the rational content, and
        // `normalised_divisor_form` takes the primitive part of a constant to
        // exactly `1`.
        if self.is_constant() || other.is_constant() {
            return MPoly::one();
        }

        // Rational content and the monomial gcd: always valid divisors, and the
        // fallback if anything below declines to finish.
        let content = rat_gcd(&self.content(), &other.content());
        let vars = union_vars(&self.vars, &other.vars);
        let left = align_terms(&self.terms, &self.vars, &vars);
        let right = align_terms(&other.terms, &other.vars, &vars);
        let mut exps = vec![u32::MAX; vars.len()];
        for mono in left.keys().chain(right.keys()) {
            for (i, slot) in exps.iter_mut().enumerate() {
                *slot = (*slot).min(mono.exponent(i));
            }
        }
        let mut base = MPoly {
            vars: vars.clone(),
            terms: BTreeMap::from([(Mono(exps), content)]),
        };
        base.prune();

        let mut budget = MAX_MGCD_STEPS;
        let candidate = match multivariate_gcd(self, other, &mut budget) {
            Some(g) if !g.is_zero() => g,
            _ => base.clone(),
        };

        // Certify. A divisor that does not divide is worse than no divisor.
        if candidate.is_zero()
            || self.exact_div(&candidate).is_none()
            || other.exact_div(&candidate).is_none()
        {
            return base.normalised_divisor_form();
        }
        candidate.normalised_divisor_form()
    }

    /// Primitive integer coefficients with a positive leading coefficient — the
    /// canonical shape for a divisor and for a `RatFun` denominator.
    pub fn normalised_divisor_form(&self) -> MPoly {
        if self.is_zero() {
            return MPoly::zero();
        }
        let p = self.primitive_part();
        if p.lc().is_negative() {
            -&p
        } else {
            p
        }
    }

    /// Drop zero coefficients and variables that no longer occur.
    fn prune(&mut self) {
        self.terms.retain(|_, c| !c.is_zero());
        let used: Vec<bool> = (0..self.vars.len())
            .map(|i| self.terms.keys().any(|m| m.exponent(i) > 0))
            .collect();
        if used.iter().all(|&u| u) {
            return;
        }
        let vars: Vec<String> = self
            .vars
            .iter()
            .enumerate()
            .filter(|(i, _)| used[*i])
            .map(|(_, v)| v.clone())
            .collect();
        let terms = self
            .terms
            .iter()
            .map(|(m, c)| {
                let exps: Vec<u32> = (0..self.vars.len())
                    .filter(|i| used[*i])
                    .map(|i| m.exponent(i))
                    .collect();
                (Mono(exps), c.clone())
            })
            .collect();
        self.vars = vars;
        self.terms = terms;
    }
}

/// Recursive multivariate GCD by primitive PRS.
///
/// `None` means "declined" (budget exhausted, or an exact division that the
/// theory guarantees did not come out) — never "the gcd is 1". The caller
/// falls back to the content × monomial gcd and re-certifies either way.
fn multivariate_gcd(a: &MPoly, b: &MPoly, budget: &mut usize) -> Option<MPoly> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    if a.is_zero() {
        return Some(b.clone());
    }
    if b.is_zero() {
        return Some(a.clone());
    }
    let vars = union_vars(&a.vars, &b.vars);
    match vars.len() {
        // Both constant: the gcd is the rational content gcd.
        0 => Some(MPoly::constant(rat_gcd(&a.content(), &b.content()))),
        // One variable: hand it to the exact univariate GCD.
        1 => {
            let v = &vars[0];
            let (ua, ub) = (a.to_upoly(v)?, b.to_upoly(v)?);
            let content = rat_gcd(&a.content(), &b.content());
            Some(MPoly::from_upoly(&ua.gcd(&ub).primitive_part(), v).scale(&content))
        }
        // Several: recurse on the content, then run the PRS in the main
        // variable. `vars` is sorted, so the choice is deterministic.
        _ => {
            let v = vars[0].clone();
            let a_coeffs = a.coeffs_in(&v);
            let b_coeffs = b.coeffs_in(&v);
            let a_content = list_content(&a_coeffs, budget)?;
            let b_content = list_content(&b_coeffs, budget)?;
            let content = multivariate_gcd(&a_content, &b_content, budget)?;

            let mut u = divide_list(&a_coeffs, &a_content)?;
            let mut w = divide_list(&b_coeffs, &b_content)?;
            if u.len() < w.len() {
                std::mem::swap(&mut u, &mut w);
            }
            while !w.is_empty() {
                if *budget == 0 {
                    return None;
                }
                *budget -= 1;
                let r = pseudo_rem_list(&u, &w)?;
                u = w;
                let r_content = list_content(&r, budget)?;
                w = if r_content.is_zero() {
                    r
                } else {
                    divide_list(&r, &r_content)?
                };
            }
            let u_content = list_content(&u, budget)?;
            let primitive = divide_list(&u, &u_content)?;
            Some(&content * &from_coeffs_in(&primitive, &v))
        }
    }
}

/// GCD of a coefficient list — the content of a polynomial in its main variable.
fn list_content(coeffs: &[MPoly], budget: &mut usize) -> Option<MPoly> {
    let mut acc = MPoly::zero();
    for c in coeffs {
        acc = multivariate_gcd(&acc, c, budget)?;
        if acc.as_constant().is_some() && !acc.is_zero() {
            // A constant content cannot shrink further; stop early.
            break;
        }
    }
    Some(acc)
}

/// Divide every coefficient exactly, or decline.
fn divide_list(coeffs: &[MPoly], divisor: &MPoly) -> Option<Vec<MPoly>> {
    if divisor.is_zero() {
        return None;
    }
    let mut out = Vec::with_capacity(coeffs.len());
    for c in coeffs {
        out.push(if c.is_zero() {
            MPoly::zero()
        } else {
            c.exact_div(divisor)?
        });
    }
    trim_list(&mut out);
    Some(out)
}

fn trim_list(coeffs: &mut Vec<MPoly>) {
    while coeffs.last().is_some_and(MPoly::is_zero) {
        coeffs.pop();
    }
}

/// Pseudo-remainder of two polynomials given as coefficient lists in the main
/// variable, over the multivariate coefficient ring.
fn pseudo_rem_list(a: &[MPoly], b: &[MPoly]) -> Option<Vec<MPoly>> {
    let db = b.len().checked_sub(1)?;
    let mut rem = a.to_vec();
    trim_list(&mut rem);
    if rem.len() <= db {
        return Some(rem);
    }
    let blc = b[db].clone();
    for _ in 0..=(rem.len() - 1 - db) {
        trim_list(&mut rem);
        if rem.len() <= db {
            break;
        }
        let dr = rem.len() - 1;
        let shift = dr - db;
        let factor = rem[dr].clone();
        // rem ← blc·rem − factor·v^shift·b, whose leading term cancels.
        let mut next: Vec<MPoly> = rem.iter().map(|c| c * &blc).collect();
        for (i, bc) in b.iter().enumerate() {
            next[shift + i] = &next[shift + i] - &(&factor * bc);
        }
        rem = next;
    }
    trim_list(&mut rem);
    Some(rem)
}

/// Rebuild `Σ coeffs[i]·v^i`.
fn from_coeffs_in(coeffs: &[MPoly], var: &str) -> MPoly {
    let v = MPoly::var(var);
    let mut acc = MPoly::zero();
    for (i, c) in coeffs.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        acc = &acc + &(c * &v.pow(i));
    }
    acc
}

/// Non-negative gcd of two rationals: `gcd(numerators) / lcm(denominators)`.
/// The same convention [`UPoly::content`] uses, so contents compose.
pub fn rat_gcd(a: &Rat, b: &Rat) -> Rat {
    if a.is_zero() {
        return b.abs();
    }
    if b.is_zero() {
        return a.abs();
    }
    Rat::new(a.numer().gcd(b.numer()), a.denom().lcm(b.denom()))
}

fn union_vars(a: &[String], b: &[String]) -> Vec<String> {
    let mut out: Vec<String> = a.to_vec();
    out.extend(b.iter().cloned());
    out.sort();
    out.dedup();
    out
}

/// Where each variable of `to` sits in `from`, or `None` when it is new.
///
/// This is the same for every monomial being aligned, so it is computed **once
/// per [`align_terms`] call** rather than once per term. Inlined into the
/// per-term loop it made every ring operation cost
/// `O(terms · |to| · |from|)`, and a left-associated sum of *n* distinct
/// generators `O(n⁴)`: a 140-generator `Expand` measured **57 s**, a
/// 200-generator one **256 s**. Hoisting this alone takes them to 14.3 s and
/// 50.4 s; the constant-denominator short-circuit in
/// [`crate::cas::ratfun::RatFun`] takes them the rest of the way, to 0.107 s
/// and 0.412 s. Regression:
/// `tests/cas_control_robustness.rs::a_wide_sum_of_distinct_generators_stays_fast`.
fn align_index(from: &[String], to: &[String]) -> Vec<Option<usize>> {
    to.iter()
        .map(|v| from.iter().position(|f| f == v))
        .collect()
}

fn align_terms(terms: &BTreeMap<Mono, Rat>, from: &[String], to: &[String]) -> BTreeMap<Mono, Rat> {
    let index = align_index(from, to);
    let mut out: BTreeMap<Mono, Rat> = BTreeMap::new();
    for (m, c) in terms {
        let exps = index
            .iter()
            .map(|slot| slot.map_or(0, |i| m.exponent(i)))
            .collect();
        *out.entry(Mono(exps)).or_insert_with(Rat::zero) += c;
    }
    out.retain(|_, c| !c.is_zero());
    out
}

/// `a / b` as monomials, or `None` when `b ∤ a`. Both must already be aligned
/// to the same variable list.
fn mono_div(a: &Mono, b: &Mono) -> Option<Mono> {
    let n = a.0.len().max(b.0.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let x = a.exponent(i);
        let y = b.exponent(i);
        if x < y {
            return None;
        }
        out.push(x - y);
    }
    Some(Mono(out))
}

/// `a · b` as monomials; both must already be aligned to the same variable list.
fn mono_mul(a: &Mono, b: &Mono) -> Mono {
    let n = a.0.len().max(b.0.len());
    Mono((0..n).map(|i| a.exponent(i) + b.exponent(i)).collect())
}

fn mpoly_binary(a: &MPoly, b: &MPoly, negate_rhs: bool) -> MPoly {
    let vars = union_vars(&a.vars, &b.vars);
    let mut terms = align_terms(&a.terms, &a.vars, &vars);
    for (m, c) in align_terms(&b.terms, &b.vars, &vars) {
        let slot = terms.entry(m).or_insert_with(Rat::zero);
        if negate_rhs {
            *slot -= c;
        } else {
            *slot += c;
        }
    }
    let mut out = MPoly { vars, terms };
    out.prune();
    out
}

impl Add for &MPoly {
    type Output = MPoly;
    fn add(self, rhs: &MPoly) -> MPoly {
        mpoly_binary(self, rhs, false)
    }
}

impl Sub for &MPoly {
    type Output = MPoly;
    fn sub(self, rhs: &MPoly) -> MPoly {
        mpoly_binary(self, rhs, true)
    }
}

impl Mul for &MPoly {
    type Output = MPoly;
    fn mul(self, rhs: &MPoly) -> MPoly {
        if self.is_zero() || rhs.is_zero() {
            return MPoly::zero();
        }
        let vars = union_vars(&self.vars, &rhs.vars);
        let left = align_terms(&self.terms, &self.vars, &vars);
        let right = align_terms(&rhs.terms, &rhs.vars, &vars);
        let mut terms: BTreeMap<Mono, Rat> = BTreeMap::new();
        for (am, ac) in &left {
            for (bm, bc) in &right {
                let exps: Vec<u32> = (0..vars.len())
                    .map(|i| am.exponent(i) + bm.exponent(i))
                    .collect();
                *terms.entry(Mono(exps)).or_insert_with(Rat::zero) += ac * bc;
            }
        }
        let mut out = MPoly { vars, terms };
        out.prune();
        out
    }
}

impl Neg for &MPoly {
    type Output = MPoly;
    fn neg(self) -> MPoly {
        MPoly {
            vars: self.vars.clone(),
            terms: self.terms.iter().map(|(m, c)| (m.clone(), -c)).collect(),
        }
    }
}

impl Add for MPoly {
    type Output = MPoly;
    fn add(self, rhs: MPoly) -> MPoly {
        &self + &rhs
    }
}

impl Sub for MPoly {
    type Output = MPoly;
    fn sub(self, rhs: MPoly) -> MPoly {
        &self - &rhs
    }
}

impl Mul for MPoly {
    type Output = MPoly;
    fn mul(self, rhs: MPoly) -> MPoly {
        &self * &rhs
    }
}

impl Neg for MPoly {
    type Output = MPoly;
    fn neg(self) -> MPoly {
        -&self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(coeffs: &[i64]) -> UPoly {
        UPoly::from_ints(coeffs)
    }

    /// A tiny deterministic generator so the property tests are reproducible
    /// without a `rand` dependency.
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

    // --- basic ring laws ---------------------------------------------------

    #[test]
    fn trims_trailing_zeros() {
        assert!(p(&[0, 0, 0]).is_zero());
        assert_eq!(p(&[1, 2, 0, 0]).degree(), Some(1));
        assert_eq!(UPoly::zero().degree(), None);
        assert_eq!(UPoly::zero().degree_or_zero(), 0);
    }

    #[test]
    fn add_sub_mul_are_exact() {
        let a = p(&[1, 2, 3]);
        let b = p(&[-1, 4]);
        assert_eq!(&a + &b, p(&[0, 6, 3]));
        assert_eq!(&a - &b, p(&[2, -2, 3]));
        // (3x²+2x+1)(4x−1) = 12x³ + 5x² + 2x − 1
        assert_eq!(&a * &b, p(&[-1, 2, 5, 12]));
        assert_eq!(&a - &a, UPoly::zero());
    }

    #[test]
    fn rational_coefficients_stay_exact() {
        // (x/3)² == x²/9 exactly; no float would give this.
        let third = UPoly::from_ratios(&[(0, 1), (1, 3)]).expect("valid");
        let sq = &third * &third;
        assert_eq!(sq.coeff(2), rat(1, 9).expect("valid"));
    }

    #[test]
    fn pow_matches_repeated_multiplication() {
        let a = p(&[1, 1]);
        let mut expect = UPoly::one();
        for k in 0..8 {
            assert_eq!(a.pow(k), expect, "(x+1)^{k}");
            expect = &expect * &a;
        }
    }

    #[test]
    fn div_rem_reconstructs() {
        let a = p(&[-1, 0, 0, 0, 1]); // x⁴ − 1
        let b = p(&[1, 1]); // x + 1
        let (q, r) = a.div_rem(&b).expect("nonzero divisor");
        assert_eq!(&(&q * &b) + &r, a);
        assert!(r.is_zero());
        assert_eq!(q, p(&[-1, 1, -1, 1]));
    }

    #[test]
    fn div_by_zero_is_an_error_not_a_panic() {
        assert_eq!(
            p(&[1, 1]).div_rem(&UPoly::zero()),
            Err(PolyError::DivisionByZero)
        );
    }

    #[test]
    fn eval_is_horner_over_q() {
        let f = p(&[1, 2, 3]); // 3x² + 2x + 1
        assert_eq!(f.eval(&rat_int(2)), rat_int(17));
        assert_eq!(
            f.eval(&rat(1, 2).expect("valid")),
            rat(11, 4).expect("valid")
        );
    }

    #[test]
    fn derivative_is_exact() {
        assert_eq!(p(&[5, 3, 0, 7]).derivative(), p(&[3, 0, 21]));
        assert_eq!(p(&[4]).derivative(), UPoly::zero());
    }

    #[test]
    fn content_and_primitive_part() {
        // x/2 + 1/3 → content 1/6, primitive part 3x + 2 (the oracle's spelling)
        let f = UPoly::from_ratios(&[(1, 3), (1, 2)]).expect("valid");
        assert_eq!(f.content(), rat(1, 6).expect("valid"));
        assert_eq!(f.primitive_part(), p(&[2, 3]));
        // Content is non-negative even when the leading coefficient is not.
        let g = p(&[-4, -6]);
        assert_eq!(g.content(), rat_int(2));
        assert_eq!(g.primitive_part(), p(&[-2, -3]));
        assert_eq!(UPoly::zero().content(), Rat::zero());
    }

    // --- GCD ---------------------------------------------------------------

    #[test]
    fn gcd_known_values() {
        // gcd(x²−1, x²+2x+1) = x+1
        assert_eq!(p(&[-1, 0, 1]).gcd(&p(&[1, 2, 1])), p(&[1, 1]));
        // Coprime.
        assert_eq!(p(&[-1, 0, 1]).gcd(&p(&[1, 0, 1])), UPoly::one());
        // Result is monic even when neither input is.
        assert_eq!(p(&[-4, 0, 4]).gcd(&p(&[6, 12, 6])), p(&[1, 1]));
        // Zero cases.
        assert_eq!(UPoly::zero().gcd(&p(&[2, 4])), p(&[1, 2]).monic());
        assert_eq!(UPoly::zero().gcd(&UPoly::zero()), UPoly::zero());
    }

    #[test]
    fn gcd_handles_rational_coefficients() {
        let a = UPoly::from_ratios(&[(-1, 4), (0, 1), (1, 4)]).expect("valid"); // (x²−1)/4
        let b = UPoly::from_ratios(&[(1, 3), (2, 3), (1, 3)]).expect("valid"); // (x+1)²/3
        assert_eq!(a.gcd(&b), p(&[1, 1]));
    }

    #[test]
    fn gcd_divides_both_and_is_maximal() {
        let mut rng = TestRng(0x5EED);
        for _ in 0..250 {
            let a = rng.poly(4, 6);
            let b = rng.poly(4, 6);
            let common = rng.poly(3, 4);
            if common.is_zero() {
                continue;
            }
            let f = &a * &common;
            let g = &b * &common;
            if f.is_zero() || g.is_zero() {
                continue;
            }
            let d = f.gcd(&g);
            assert!(d.divides(&f), "gcd must divide f");
            assert!(d.divides(&g), "gcd must divide g");
            // The planted common factor must divide the gcd.
            assert!(
                common.monic().divides(&d),
                "planted factor {common} must divide gcd {d} of {f} and {g}"
            );
            assert!(d.is_monic());
        }
    }

    #[test]
    fn modular_gcd_agrees_with_primitive_prs() {
        let mut rng = TestRng(0xC0FFEE);
        for _ in 0..200 {
            let f = rng.poly(5, 9);
            let g = rng.poly(5, 9);
            if f.is_zero() || g.is_zero() {
                continue;
            }
            let a = ZPoly::from_upoly(&f).primitive_positive();
            let b = ZPoly::from_upoly(&g).primitive_positive();
            if a.degree_or_zero() == 0 || b.degree_or_zero() == 0 {
                continue;
            }
            let modular = modular_gcd(&a, &b).expect("prime supply must suffice here");
            let prs = primitive_prs_gcd(&a, &b);
            assert_eq!(modular, prs, "modular gcd disagrees with PRS on {f} / {g}");
        }
    }

    #[test]
    fn ext_gcd_satisfies_bezout() {
        let mut rng = TestRng(0xBEEF);
        for _ in 0..200 {
            let a = rng.poly(5, 7);
            let b = rng.poly(5, 7);
            let (g, s, t) = a.ext_gcd(&b);
            assert_eq!(&(&s * &a) + &(&t * &b), g, "Bezout identity for {a} / {b}");
            if !g.is_zero() {
                assert!(g.is_monic());
                assert!(g.divides(&a) || a.is_zero());
                assert!(g.divides(&b) || b.is_zero());
            }
        }
    }

    // --- square-free -------------------------------------------------------

    #[test]
    fn yun_separates_multiplicities() {
        // (x−1)³(x+1)²  — the oracle prints Factor as (-1+x)^3*(1+x)^2
        let f = &p(&[-1, 1]).pow(3) * &p(&[1, 1]).pow(2);
        let sf = f.square_free().expect("square-free decomposition");
        assert_eq!(sf, vec![(p(&[1, 1]), 2), (p(&[-1, 1]), 3)]);
    }

    #[test]
    fn yun_of_a_square_free_input_is_itself() {
        let f = p(&[-1, 0, 1]); // x²−1
        assert_eq!(f.square_free().expect("ok"), vec![(f.monic(), 1)]);
        assert!(p(&[7]).square_free().expect("ok").is_empty());
    }

    #[test]
    fn yun_reconstructs_the_input() {
        let mut rng = TestRng(0x11CE);
        for _ in 0..120 {
            let a = rng.poly(2, 4);
            let b = rng.poly(2, 4);
            let f = &(&a.pow(3) * &b.pow(2)) * &rng.poly(2, 4);
            if f.is_zero() || f.is_constant() {
                continue;
            }
            let mut product = UPoly::constant(f.lc());
            for (s, m) in f.square_free().expect("ok") {
                product = &product * &s.pow(m);
            }
            assert_eq!(product, f, "Yun must reconstruct {f}");
        }
    }

    // --- factorisation -----------------------------------------------------

    #[test]
    fn factor_matches_the_java_oracle_spellings() {
        // Every expectation below was read off the real Symja engine through
        // tools/golden-dumper's classpath, not guessed.

        // Factor(x^2-1) ==> (-1+x)*(1+x)
        let f = p(&[-1, 0, 1]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(1));
        assert_eq!(f.factors, vec![(p(&[-1, 1]), 1), (p(&[1, 1]), 1)]);

        // Factor(2*x^2+4*x+2) ==> 2*(1+x)^2
        let f = p(&[2, 4, 2]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(2));
        assert_eq!(f.factors, vec![(p(&[1, 1]), 2)]);

        // Factor(x^4-1) ==> (-1+x)*(1+x)*(1+x^2)
        let f = p(&[-1, 0, 0, 0, 1]).factor().expect("ok");
        assert_eq!(
            f.factors,
            vec![(p(&[-1, 1]), 1), (p(&[1, 1]), 1), (p(&[1, 0, 1]), 1)]
        );

        // Factor(x^2+1) ==> 1+x^2  (irreducible over ℚ — must NOT split)
        let f = p(&[1, 0, 1]).factor().expect("ok");
        assert!(f.is_irreducible());
        assert_eq!(f.factors, vec![(p(&[1, 0, 1]), 1)]);

        // Factor(x^3-2) ==> -2+x^3  (irreducible over ℚ)
        assert!(p(&[-2, 0, 0, 1]).factor().expect("ok").is_irreducible());

        // Factor(6*x^2+5*x+1) ==> (1+2*x)*(1+3*x)
        let f = p(&[1, 5, 6]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(1));
        assert_eq!(f.factors, vec![(p(&[1, 2]), 1), (p(&[1, 3]), 1)]);

        // Factor(x^5-x^4-2*x^3+2*x^2+x-1) ==> (-1+x)^3*(1+x)^2
        let f = p(&[-1, 1, 2, -2, -1, 1]).factor().expect("ok");
        assert_eq!(f.factors, vec![(p(&[-1, 1]), 3), (p(&[1, 1]), 2)]);

        // Factor(x/2+1/3) ==> 1/6*(2+3*x)
        let f = UPoly::from_ratios(&[(1, 3), (1, 2)])
            .expect("valid")
            .factor()
            .expect("ok");
        assert_eq!(f.unit, rat(1, 6).expect("valid"));
        assert_eq!(f.factors, vec![(p(&[2, 3]), 1)]);
    }

    /// A second oracle battery, all read off the real Symja engine. These are
    /// the shapes that break naive factorisers: irreducibles that split
    /// completely modulo every prime, Sophie-Germain quartics, cyclotomic
    /// towers, and rational content.
    #[test]
    fn factor_matches_the_oracle_on_hard_shapes() {
        // Factor(x^4+1) ==> 1+x^4. Irreducible over ℚ, yet it factors into two
        // quadratics modulo *every* prime — the canonical false positive.
        assert!(p(&[1, 0, 0, 0, 1]).factor().expect("ok").is_irreducible());

        // Factor(x^4+4) ==> (2-2*x+x^2)*(2+2*x+x^2)   (Sophie Germain)
        let f = p(&[4, 0, 0, 0, 1]).factor().expect("ok");
        assert_eq!(f.factors, vec![(p(&[2, -2, 1]), 1), (p(&[2, 2, 1]), 1)]);

        // Factor(x^6-1) ==> (-1+x)*(1+x)*(1-x+x^2)*(1+x+x^2)
        let f = p(&[-1, 0, 0, 0, 0, 0, 1]).factor().expect("ok");
        assert_eq!(
            f.factors,
            vec![
                (p(&[-1, 1]), 1),
                (p(&[1, 1]), 1),
                (p(&[1, -1, 1]), 1),
                (p(&[1, 1, 1]), 1),
            ]
        );

        // Factor(x^12-1) ==> (-1+x)*(1+x)*(1+x^2)*(1-x+x^2)*(1+x+x^2)*(1-x^2+x^4)
        let mut coeffs = vec![0i64; 13];
        coeffs[0] = -1;
        coeffs[12] = 1;
        let f = p(&coeffs).factor().expect("ok");
        assert_eq!(
            f.factors,
            vec![
                (p(&[-1, 1]), 1),
                (p(&[1, 1]), 1),
                (p(&[1, -1, 1]), 1),
                (p(&[1, 0, 1]), 1),
                (p(&[1, 1, 1]), 1),
                (p(&[1, 0, -1, 0, 1]), 1),
            ]
        );

        // Factor(x^6+2*x^3+1) ==> (1+x)^2*(1-x+x^2)^2
        let f = p(&[1, 0, 0, 2, 0, 0, 1]).factor().expect("ok");
        assert_eq!(f.factors, vec![(p(&[1, 1]), 2), (p(&[1, -1, 1]), 2)]);

        // Factor(4*x^4+4*x^2+1) ==> (1+2*x^2)^2
        let f = p(&[1, 0, 4, 0, 4]).factor().expect("ok");
        assert_eq!(f.factors, vec![(p(&[1, 0, 2]), 2)]);

        // Factor(105*x^2+22*x+1) ==> (1+7*x)*(1+15*x)
        let f = p(&[1, 22, 105]).factor().expect("ok");
        assert_eq!(f.factors, vec![(p(&[1, 7]), 1), (p(&[1, 15]), 1)]);

        // Factor(9*x^2-6*x+1) ==> (1-3*x)^2, which is (3x-1)^2 — the even power
        // makes the two spellings identical polynomials.
        let f = p(&[1, -6, 9]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(1));
        assert_eq!(f.factors, vec![(p(&[-1, 3]), 2)]);

        // Factor(x^2/4-1/9) ==> 1/36*(-2+3*x)*(2+3*x)
        let f = UPoly::from_ratios(&[(-1, 9), (0, 1), (1, 4)])
            .expect("valid")
            .factor()
            .expect("ok");
        assert_eq!(f.unit, rat(1, 36).expect("valid"));
        assert_eq!(f.factors, vec![(p(&[-2, 3]), 1), (p(&[2, 3]), 1)]);

        // Factor(60*x^3-60*x) ==> -60*(1-x)*x*(1+x), i.e. 60·(x−1)·x·(x+1)
        // once every factor carries a positive leading coefficient.
        let f = p(&[0, -60, 0, 60]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(60));
        assert_eq!(
            f.factors,
            vec![(p(&[-1, 1]), 1), (p(&[0, 1]), 1), (p(&[1, 1]), 1)]
        );

        // Factor(3*x^3-3*x^2-6*x) ==> -3*(2-x)*x*(1+x), i.e. 3·(x−2)·x·(x+1)
        let f = p(&[0, -6, -3, 3]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(3));
        assert_eq!(
            f.factors,
            vec![(p(&[-2, 1]), 1), (p(&[0, 1]), 1), (p(&[1, 1]), 1)]
        );

        // Factor(x^7-x) ==> (-1+x)*x*(1+x)*(1-x+x^2)*(1+x+x^2)
        let f = p(&[0, -1, 0, 0, 0, 0, 0, 1]).factor().expect("ok");
        assert_eq!(
            f.factors,
            vec![
                (p(&[-1, 1]), 1),
                (p(&[0, 1]), 1),
                (p(&[1, 1]), 1),
                (p(&[1, -1, 1]), 1),
                (p(&[1, 1, 1]), 1),
            ]
        );

        // Factor(2*x^4+2*x^3+2*x^2+2*x+2) ==> 2*(1+x+x^2+x^3+x^4)
        let f = p(&[2, 2, 2, 2, 2]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(2));
        assert_eq!(f.factors, vec![(p(&[1, 1, 1, 1, 1]), 1)]);

        // Factor(x^10-1) ==> (-1+x)*(1+x)*(1-x+x^2-x^3+x^4)*(1+x+x^2+x^3+x^4)
        let mut coeffs = vec![0i64; 11];
        coeffs[0] = -1;
        coeffs[10] = 1;
        let f = p(&coeffs).factor().expect("ok");
        assert_eq!(
            f.factors,
            vec![
                (p(&[-1, 1]), 1),
                (p(&[1, 1]), 1),
                (p(&[1, -1, 1, -1, 1]), 1),
                (p(&[1, 1, 1, 1, 1]), 1),
            ]
        );
    }

    #[test]
    fn factor_pulls_out_negative_units() {
        // −2x² + 2 = −2(x−1)(x+1)
        let f = p(&[2, 0, -2]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(-2));
        assert_eq!(f.factors, vec![(p(&[-1, 1]), 1), (p(&[1, 1]), 1)]);
        assert_eq!(f.expand(), p(&[2, 0, -2]));
    }

    #[test]
    fn factor_handles_repeated_irreducible_quadratics() {
        // (x²+1)³(x−3)
        let f = &p(&[1, 0, 1]).pow(3) * &p(&[-3, 1]);
        let fac = f.factor().expect("ok");
        assert_eq!(fac.factors, vec![(p(&[-3, 1]), 1), (p(&[1, 0, 1]), 3)]);
        assert_eq!(fac.expand(), f);
    }

    #[test]
    fn factor_handles_large_content_and_high_degree() {
        // 12·(2x+3)²·(x²+x+1)·(5x−7)
        let f = &(&(&p(&[12]) * &p(&[3, 2]).pow(2)) * &p(&[1, 1, 1])) * &p(&[-7, 5]);
        let fac = f.factor().expect("ok");
        assert_eq!(fac.expand(), f);
        assert_eq!(fac.unit, rat_int(12));
        let degrees: Vec<usize> = fac
            .factors
            .iter()
            .map(|(g, m)| g.degree_or_zero() * m)
            .collect();
        assert_eq!(degrees.iter().sum::<usize>(), f.degree_or_zero());
    }

    #[test]
    fn factor_of_a_constant_and_of_zero() {
        let f = p(&[7]).factor().expect("ok");
        assert_eq!(f.unit, rat_int(7));
        assert!(f.factors.is_empty());
        let z = UPoly::zero().factor().expect("ok");
        assert!(z.unit.is_zero());
        assert!(z.factors.is_empty());
        assert_eq!(z.expand(), UPoly::zero());
    }

    #[test]
    fn factor_then_multiply_back_is_the_identity() {
        let mut rng = TestRng(0xFACE);
        let mut checked = 0;
        for _ in 0..400 {
            let f = rng.poly(6, 8);
            if f.is_zero() {
                continue;
            }
            let fac = f.factor().expect("factorisation must succeed at this size");
            assert_eq!(fac.expand(), f, "factor/expand round trip on {f}");
            for (g, _) in &fac.factors {
                assert!(g.lc().is_positive(), "factors carry a positive lc");
                assert_eq!(g.content(), rat_int(1), "factors are primitive");
                assert!(
                    g.factor().expect("ok").is_irreducible(),
                    "{g} must itself be irreducible"
                );
            }
            checked += 1;
        }
        assert!(checked > 300, "the generator produced too few usable cases");
    }

    #[test]
    fn factor_finds_planted_factors() {
        let mut rng = TestRng(0xD00D);
        for _ in 0..150 {
            let a = rng.poly(2, 5);
            let b = rng.poly(2, 5);
            if a.is_constant() || b.is_constant() {
                continue;
            }
            let f = &a * &b;
            let fac = f.factor().expect("ok");
            assert_eq!(fac.expand(), f);
            // Every irreducible factor of `a` must appear in the factorisation.
            for (g, _) in a.factor().expect("ok").factors {
                assert!(
                    fac.factors.iter().any(|(h, _)| *h == g),
                    "{g} (a factor of {a}) is missing from the factorisation of {f}"
                );
            }
        }
    }

    #[test]
    fn monic_factors_reconstruct_with_the_leading_coefficient() {
        let f = p(&[1, 5, 6]); // 6x²+5x+1
        let mf = f.monic_factors().expect("ok");
        let mut product = UPoly::constant(f.lc());
        for (base, m) in &mf {
            assert!(base.is_monic());
            product = &product * &base.pow(*m);
        }
        assert_eq!(product, f);
    }

    #[test]
    fn cyclotomic_like_inputs_do_not_over_factor() {
        // x^4 + x^3 + x^2 + x + 1 is the 5th cyclotomic polynomial: irreducible
        // over ℚ but it splits into four linear factors modulo many primes, so
        // it exercises recombination rejecting every proper subset.
        let f = p(&[1, 1, 1, 1, 1]);
        assert!(f.factor().expect("ok").is_irreducible());
        // x^8 + x^7 − x^5 − x^4 − x^3 + x + 1 is the 15th cyclotomic polynomial.
        let g = p(&[1, 1, 0, -1, -1, -1, 0, 1, 1]);
        assert!(g.factor().expect("ok").is_irreducible());
    }

    #[test]
    fn factor_survives_a_swinnerton_dyer_polynomial() {
        // (x²−2)(x²−3) expanded: x⁴ − 5x² + 6. Irreducible mod many primes'
        // worth of pieces but genuinely a product of two quadratics.
        let f = p(&[6, 0, -5, 0, 1]);
        let fac = f.factor().expect("ok");
        assert_eq!(fac.expand(), f);
        assert_eq!(fac.factors, vec![(p(&[-3, 0, 1]), 1), (p(&[-2, 0, 1]), 1)]);

        // The degree-8 Swinnerton-Dyer polynomial (roots ±√2±√3±√5) is
        // irreducible over ℚ but splits completely modulo every prime — the
        // canonical worst case for Zassenhaus recombination.
        let sd8 = p(&[576, 0, -960, 0, 352, 0, -40, 0, 1]);
        assert!(sd8.factor().expect("ok").is_irreducible());
    }

    // --- multivariate ------------------------------------------------------

    #[test]
    fn mpoly_ring_operations() {
        let x = MPoly::var("x");
        let y = MPoly::var("y");
        let f = &(&x * &x) + &(&y * &MPoly::constant(rat_int(3)));
        let g = &x - &y;
        let sum = &f + &g;
        assert_eq!(sum.degree_in("x"), 2);
        assert_eq!(sum.degree_in("y"), 1);
        let product = &f * &g;
        // (x²+3y)(x−y) = x³ − x²y + 3xy − 3y²
        assert_eq!(product.term_count(), 4);
        assert_eq!(product.total_degree(), Some(3));
        let mut bind = BTreeMap::new();
        bind.insert("x".to_string(), rat_int(2));
        bind.insert("y".to_string(), rat_int(5));
        assert_eq!(product.eval(&bind), Some(rat_int((4 + 15) * (2 - 5))));
    }

    #[test]
    fn mpoly_zero_and_constants_collapse_variables() {
        let x = MPoly::var("x");
        let zero = &x - &x;
        assert!(zero.is_zero());
        assert!(zero.vars().is_empty());
        assert_eq!(zero.as_constant(), Some(Rat::zero()));
        assert_eq!(MPoly::constant(rat_int(4)).as_constant(), Some(rat_int(4)));
    }

    #[test]
    fn mpoly_div_rem_reconstructs() {
        let x = MPoly::var("x");
        let y = MPoly::var("y");
        let f = &(&(&x * &x) * &y) + &(&x * &MPoly::constant(rat_int(2)));
        let d = &x + &MPoly::constant(rat_int(1));
        let (q, r) = f.div_rem(&d).expect("nonzero divisor");
        assert_eq!(&(&q * &d) + &r, f);
        assert_eq!(
            MPoly::zero().div_rem(&MPoly::zero()),
            Err(PolyError::DivisionByZero)
        );
    }

    #[test]
    fn mpoly_content_and_univariate_round_trip() {
        let f = MPoly::from_upoly(&p(&[2, 4, 6]), "s");
        assert_eq!(f.content(), rat_int(2));
        assert_eq!(f.primitive_part().to_upoly("s"), Some(p(&[1, 2, 3])));
        assert_eq!(f.to_upoly("s"), Some(p(&[2, 4, 6])));
        // A genuinely multivariate polynomial refuses to collapse.
        let g = &MPoly::var("a") * &MPoly::var("b");
        assert_eq!(g.to_upoly("a"), None);
    }

    #[test]
    fn mpoly_gcd_known_values() {
        let x = MPoly::var("x");
        let y = MPoly::var("y");
        let one = MPoly::one();

        // Monomial gcd. The result is always *primitive*, so the common
        // numeric factor of 2 is normalised away rather than reported — the
        // content is handled separately by `RatFun::normalise`, which is the
        // only place it changes an answer.
        let a = &(&x.pow(3) * &y.pow(3)).scale(&rat_int(6))
            + &(&x.pow(2) * &y.pow(4)).scale(&rat_int(4));
        let b = (&x.pow(2) * &y.pow(3)).scale(&rat_int(10));
        assert_eq!(a.gcd(&b), &x.pow(2) * &y.pow(3));

        // Genuinely multivariate: gcd(x²−y², x+y) = x+y.
        assert_eq!((&(&x * &x) - &(&y * &y)).gcd(&(&x + &y)), &x + &y);

        // gcd(x³−y³, x²+xy+y²) = x²+xy+y².
        let cubic = &x.pow(3) - &y.pow(3);
        let quad = &(&(&x * &x) + &(&x * &y)) + &(&y * &y);
        assert_eq!(cubic.gcd(&quad), quad);

        // Coprime.
        assert_eq!((&x + &one).gcd(&(&y + &one)), one);

        // Zero cases.
        assert_eq!(
            MPoly::zero().gcd(&(&x * &MPoly::constant(rat_int(3)))),
            x.clone()
        );
        assert!(MPoly::zero().gcd(&MPoly::zero()).is_zero());

        // Univariate agrees with UPoly::gcd.
        let u = MPoly::from_upoly(&p(&[-1, 0, 1]), "s");
        let v = MPoly::from_upoly(&p(&[1, 2, 1]), "s");
        assert_eq!(u.gcd(&v), MPoly::from_upoly(&p(&[1, 1]), "s"));
    }

    #[test]
    fn mpoly_gcd_divides_both_and_finds_planted_factors() {
        let mut rng = TestRng(0x6CD);
        let names = ["x", "y", "z"];
        let build = |rng: &mut TestRng, terms: usize| -> MPoly {
            let mut acc = MPoly::zero();
            for _ in 0..terms {
                let v = names[(rng.next() as usize) % names.len()];
                let e = (rng.next() as usize) % 3;
                acc = &acc + &MPoly::var(v).pow(e).scale(&rat_int(rng.int(-4, 4)));
            }
            acc
        };
        let mut planted_found = 0;
        for _ in 0..150 {
            let common = build(&mut rng, 2);
            if common.is_zero() || common.as_constant().is_some() {
                continue;
            }
            let f = &build(&mut rng, 2) * &common;
            let g = &build(&mut rng, 2) * &common;
            if f.is_zero() || g.is_zero() {
                continue;
            }
            let d = f.gcd(&g);
            assert!(!d.is_zero());
            assert!(f.exact_div(&d).is_some(), "gcd {d:?} must divide f");
            assert!(g.exact_div(&d).is_some(), "gcd {d:?} must divide g");
            assert_eq!(d.content(), rat_int(1), "gcd must be primitive");
            assert!(!d.lc().is_negative(), "gcd must have a positive lc");
            if d.exact_div(&common.normalised_divisor_form()).is_some() {
                planted_found += 1;
            }
        }
        assert!(
            planted_found > 100,
            "the multivariate GCD should recover the planted factor almost always, got {planted_found}"
        );
    }

    #[test]
    fn mpoly_coeffs_in_is_what_collect_needs() {
        // Collect(x*a + x*b + c, x) ==> c + (a+b)*x   (oracle spelling)
        let expr = &(&MPoly::var("x") * &MPoly::var("a"))
            + &(&(&MPoly::var("x") * &MPoly::var("b")) + &MPoly::var("c"));
        let coeffs = expr.coeffs_in("x");
        assert_eq!(coeffs.len(), 2);
        assert_eq!(coeffs[0], MPoly::var("c"));
        assert_eq!(coeffs[1], &MPoly::var("a") + &MPoly::var("b"));
    }

    // --- internals ---------------------------------------------------------

    #[test]
    fn fp_arithmetic_is_a_field() {
        let p = 97u64;
        for a in 1..p {
            assert_eq!(fp_mul_u64(a, fp_inv(a, p), p), 1, "inverse of {a} mod {p}");
        }
        assert_eq!(fp_inv(0, p), 0);
    }

    #[test]
    fn fp_gcd_and_ext_gcd_agree() {
        let p = 101u64;
        for (a, b) in [
            (vec![1u64, 0, 1], vec![1u64, 1]),          // x²+1, x+1
            (vec![100u64, 0, 1], vec![1u64, 2, 1]),     // x²−1, x²+2x+1
            (vec![2u64, 3, 5, 1], vec![7u64, 0, 0, 4]), // arbitrary
        ] {
            let g = fp_gcd(&a, &b, p);
            let (ge, s, t) = fp_ext_gcd(&a, &b, p);
            assert_eq!(g, ge);
            // s·a + t·b == g, written as a subtraction so no extra helper is
            // needed outside the code under test.
            let lhs = fp_sub(&fp_mul(&s, &a, p), &fp_sub(&[], &fp_mul(&t, &b, p), p), p);
            assert_eq!(lhs, g);
        }
    }

    #[test]
    fn next_combination_enumerates_all_subsets() {
        let mut idx = vec![0usize, 1];
        let mut seen = vec![idx.clone()];
        while next_combination(&mut idx, 4) {
            seen.push(idx.clone());
        }
        assert_eq!(seen.len(), 6); // C(4,2)
        assert_eq!(seen.last(), Some(&vec![2usize, 3]));
    }

    #[test]
    fn sieve_produces_odd_primes_only() {
        let primes = sieve_primes();
        assert_eq!(primes.first(), Some(&3));
        assert!(primes.iter().all(|&p| p % 2 == 1));
        assert!(primes.contains(&65_521));
        // Spot-check primality the slow, obvious way.
        for &q in primes.iter().take(500) {
            assert!(
                (2..q).take_while(|d| d * d <= q).all(|d| q % d != 0),
                "{q} is not prime"
            );
        }
    }
}
