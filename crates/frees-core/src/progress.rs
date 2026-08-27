//! Solve progress — the host-installed half of a long solve's feedback.
//!
//! Same shape, and the same reason, as [`crate::ode::deadline`]: core has no
//! way to reach a UI (and must never depend on `wasm-bindgen`), so the
//! *boundary* owns the channel. It installs a sink before a solve and clears it
//! after; core calls into this module at the two places where a solve visibly
//! advances, and nothing at all happens when no sink is installed.
//!
//! Nothing is installed on the native/parity path, so the replay cannot be
//! perturbed by it and a progress call can never reach a golden comparison.
//!
//! # The span
//!
//! A solve is a sequence of Tarjan blocks, and a block may itself contain a
//! transient whose integration is most of the wall clock. Both want to report,
//! and neither knows about the other, so the module carries a **span**: the
//! `[base, base + width)` slice of the overall 0…1 bar that the current scope
//! owns. [`enter`] subdivides the *current* span rather than the whole bar, so
//! the block loop can hand block `i` of `n` its `1/n` slice and whatever runs
//! inside it can report within that slice without either knowing about the
//! other. The guard restores the enclosing span on drop.
//!
//! # The claim, and why it is not optional
//!
//! `engine::run_blocks` serves the top-level system *and* every pinned
//! subsystem — the transient's per-step algebraic solve, Integral quadrature.
//! Left to itself it therefore reports `0, ¼, ½, ¾` once per integration step,
//! thousands of times, and a bar driven by that sits still and flickers rather
//! than advancing. (Measured, not imagined: it is what the first version of
//! this module did, and what
//! `tests/progress_and_deadline.rs::a_solve_reports_progress_that_only_ever_moves_forward`
//! caught.)
//!
//! So a scope with a genuinely monotone signal — the integrator, which has
//! `t/tf` — takes a [`Claim`] on its span. While a claim is held, [`enter`] and
//! [`report`] are silent, and the only thing that reaches the sink is
//! [`Claim::report`]. Blocks report between transients; the transient reports
//! inside itself; the bar only ever moves forward.
//!
//! A thread-local, like the deadline: the wasm build is single-threaded, and
//! the native test runner's threads each get their own empty slot.

use std::cell::{Cell, RefCell};

type Sink = Box<dyn Fn(f64)>;

std::thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    /// `(base, width)` of the span the current scope owns, in overall bar units.
    static SPAN: Cell<(f64, f64)> = const { Cell::new((0.0, 1.0)) };
    /// How many enclosing scopes hold a [`Claim`]. Non-zero silences everything
    /// except the claimants themselves.
    static CLAIMED: Cell<u32> = const { Cell::new(0) };
}

/// Install the sink a solve reports through, replacing any previous one, and
/// reset the span and claim depth. The boundary pairs this with [`clear`] in a
/// guard so a panic or an early return cannot leak it into the next request.
pub fn install(sink: Sink) {
    SINK.with(|slot| *slot.borrow_mut() = Some(sink));
    reset();
}

/// Remove the installed sink and reset the span (idempotent).
pub fn clear() {
    SINK.with(|slot| *slot.borrow_mut() = None);
    reset();
}

fn reset() {
    SPAN.with(|span| span.set((0.0, 1.0)));
    CLAIMED.with(|c| c.set(0));
}

/// Whether anything is listening. Every reporting site checks this first, so a
/// build with no sink — every native caller — pays one thread-local read.
#[inline]
pub fn active() -> bool {
    SINK.with(|slot| slot.borrow().is_some())
}

#[inline]
fn claimed() -> bool {
    CLAIMED.with(|c| c.get() > 0)
}

/// Sends `fraction` of the current span to the sink, claim check bypassed.
fn emit(fraction: f64) {
    if !fraction.is_finite() {
        return;
    }
    let (base, width) = SPAN.with(|s| s.get());
    let overall = (base + width * fraction.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    SINK.with(|slot| {
        if let Some(sink) = slot.borrow().as_ref() {
            sink(overall);
        }
    });
}

/// Restores the enclosing span when dropped.
pub struct SpanGuard(Option<(f64, f64)>);

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            SPAN.with(|span| span.set(previous));
        }
    }
}

/// Narrow the current span to the sub-slice `[at, at + width)` *of it*, and
/// report the slice's start. Returns a guard that restores the enclosing span.
///
/// A no-op — and a guard that restores nothing — when no sink is installed or
/// an enclosing scope holds a [`Claim`].
pub fn enter(at: f64, width: f64) -> SpanGuard {
    if !active() || claimed() {
        return SpanGuard(None);
    }
    let (base, span) = SPAN.with(|s| s.get());
    SPAN.with(|s| s.set((base + span * at, span * width)));
    emit(0.0);
    SpanGuard(Some((base, span)))
}

/// Report `fraction` of the current span as done. Silent while a [`Claim`] is
/// held; a non-finite fraction is dropped rather than sent, because a progress
/// bar is not worth a panic and `tf == t0` would divide by zero at a call site.
pub fn report(fraction: f64) {
    if claimed() {
        return;
    }
    emit(fraction);
}

/// Exclusive ownership of the current span's progress: while this is alive,
/// [`enter`] and [`report`] are silent and [`Claim::report`] is the only thing
/// that reaches the sink.
///
/// Taken by a scope whose own signal is monotone and meaningful — the ODE
/// integrator's `t/tf` — over nested work whose signal is neither.
pub struct Claim {
    /// `false` when nothing was listening, so `Drop` has nothing to undo.
    held: bool,
}

impl Claim {
    /// Report `fraction` of the claimed span. Bypasses the claim check that
    /// silences everyone else — that is what the claim is for.
    #[inline]
    pub fn report(&self, fraction: f64) {
        if self.held {
            emit(fraction);
        }
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.held {
            CLAIMED.with(|c| c.set(c.get().saturating_sub(1)));
        }
    }
}

/// Claim the current span. A no-op claim when nothing is listening, so the
/// caller needs no `cfg` or branch of its own.
pub fn claim() -> Claim {
    if !active() {
        return Claim { held: false };
    }
    CLAIMED.with(|c| c.set(c.get() + 1));
    Claim { held: true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    /// Collect everything reported through an installed sink.
    fn recording() -> Rc<RefCell<Vec<f64>>> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        install(Box::new(move |f| sink.borrow_mut().push(f)));
        seen
    }

    #[test]
    fn nothing_is_installed_by_default() {
        clear();
        assert!(!active());
        // The reporting sites must be safe to call with no sink.
        report(0.5);
        claim().report(0.5);
    }

    #[test]
    fn a_span_maps_a_local_fraction_onto_the_overall_bar() {
        let seen = recording();
        {
            // Block 2 of 4 owns [0.5, 0.75).
            let _guard = enter(0.5, 0.25);
            report(0.5);
        }
        report(1.0);
        clear();
        assert_eq!(*seen.borrow(), vec![0.5, 0.625, 1.0]);
    }

    #[test]
    fn spans_nest_and_the_guard_restores_the_enclosing_one() {
        let seen = recording();
        {
            let _outer = enter(0.5, 0.5); // [0.5, 1.0)
            {
                let _inner = enter(0.5, 0.5); // [0.75, 1.0)
                report(1.0);
            }
            // Back in the outer span.
            report(0.0);
        }
        clear();
        assert_eq!(*seen.borrow(), vec![0.5, 0.75, 1.0, 0.5]);
    }

    /// The transient's shape: a claim over a block loop that would otherwise
    /// reset the bar once per integration step.
    #[test]
    fn a_claim_silences_nested_block_reporting() {
        let seen = recording();
        {
            let _block = enter(0.0, 0.5); // the transient's block owns [0, 0.5)
            let claim = claim();
            claim.report(0.0);
            for _step in 0..3 {
                // What `run_blocks` does per step inside the integrator.
                let _nested = enter(0.0, 0.25);
                report(1.0);
            }
            claim.report(1.0);
        }
        clear();
        // The nested loop contributed nothing; only the claim's own two.
        assert_eq!(*seen.borrow(), vec![0.0, 0.0, 0.5]);
    }

    #[test]
    fn dropping_a_claim_lets_later_blocks_report_again() {
        let seen = recording();
        {
            let claim = claim();
            claim.report(1.0);
        }
        report(0.25);
        clear();
        assert_eq!(*seen.borrow(), vec![1.0, 0.25]);
    }

    #[test]
    fn a_non_finite_fraction_is_dropped_and_the_rest_is_clamped() {
        let seen = recording();
        report(f64::NAN);
        report(f64::INFINITY);
        report(-3.0);
        report(7.0);
        clear();
        assert_eq!(*seen.borrow(), vec![0.0, 1.0]);
    }

    #[test]
    fn clear_resets_span_and_claim_so_the_next_solve_starts_whole() {
        let seen = recording();
        std::mem::forget(enter(0.5, 0.5)); // leak both guards: the state stays
        std::mem::forget(claim());
        clear();
        let seen2 = recording();
        report(1.0);
        clear();
        assert_eq!(*seen2.borrow(), vec![1.0]);
        drop(seen);
    }
}
