//! Computer algebra — a from-scratch replacement for Symja.
//!
//! Port target: `../frEES/backend/core/.../cas/` (11 files, 3,291 LOC).
//!
//! # Why this is written rather than depended on
//!
//! The Java reaches Symja (`matheclipse-core`, **LGPL-3.0**) through a
//! deliberately narrow bridge: `CasEngine` sends a string, gets a string back.
//! Only **13 operations** are reachable. Neither Symja nor `symbolica` can ship
//! here — LGPL's relinking freedom cannot be honoured by a statically linked
//! wasm binary, and Symbolica forbids redistribution outright. frees is MIT, so
//! both are contradictions rather than inconveniences
//! (`docs/dependency-map.md` §4).
//!
//! # The 13 operations, and how each is met
//!
//! * **Rational algebra (9)** — `Factor`, `Expand`, `Simplify`, `Together`,
//!   `Cancel`, `Numerator`, `Denominator`, `Apart`, `Collect`: exact arithmetic
//!   over ℚ in [`poly`] and [`ratfun`].
//! * **`D`** — routes to the already-ported `crate::differentiator`.
//! * **`LaplaceTransform` / `InverseLaplaceTransform`** — a transform table
//!   plus partial fractions, which `Apart` supplies ([`laplace`]).
//! * **`Integrate`** — the genuinely hard one. Scoped to a pattern-matched
//!   table; anything outside it must be **refused by name**, never guessed.
//!
//! # Exactness
//!
//! Coefficients are exact rationals, not `f64`. A CAS that loses exactness in
//! its factoriser produces answers that are plausible and wrong, which is worse
//! than refusing.
pub mod engine;
pub mod laplace;
pub mod ops;
pub mod poly;
pub mod ratfun;
