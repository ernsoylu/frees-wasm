//! Control systems: transfer functions, state space, design and response.
//!
//! Port of `cas/{PolynomialHelpers,TransferFunction,StateSpace,TimeResponse,
//! ControllerDesign,PidTuner}.java` plus `parser/ControlSystemsFlattener.java`
//! (1,978) and `ast/ControlSystemsEvaluator.java` (1,140) — ~5,600 LOC.
//!
//! # This is NOT the CAS
//!
//! The thing people assume Symja provides, and it does not: every one of these
//! is frees' own Java. Transfer-function algebra, symbolic `ss ↔ tf`, LQR
//! Riccati, pole placement, PID tuning and time responses are all transcribable
//! like any other phase. Only the 13 string ops in [`crate::cas`] ever touched
//! Symja.
//!
//! `LINEARIZE` (Phase 7, `crate::dae::linearize`) already produces the numeric
//! `(A, B, C, D)` this suite consumes.
pub mod design;
pub mod eval;
pub mod flatten;
pub mod pid;
pub mod response;
pub mod ss;
pub mod tf;
