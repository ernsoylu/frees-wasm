//! The transient wall-clock budget — the boundary-installed half of the Java
//! `OdeIntegrator.guard`'s `System.nanoTime()` deadline (Wave C1, closing the
//! Phase 7–8 gap 3 "182 s of spinning worker with no cancel").
//!
//! Core has no clock on `wasm32-unknown-unknown`, so the *boundary* owns the
//! time source: it installs a predicate (typically `now_ms()` against the
//! request's `elapsedTimeSeconds`, capped like the Java's
//! `MAX_ELAPSED_SECONDS_CAP`) before a solve and clears it after. Nothing is
//! installed on the native/parity path, so the replay — where one stiff
//! fixture alone is a third of the wall clock — can never be killed by it,
//! and the deadline error can never reach a golden comparison. That is also
//! why the message is carried alongside the predicate: the classification
//! must be distinct and the wording the boundary's, without a new
//! `FreesError` variant (`diag.rs` is a contract file).
//!
//! A thread-local, like `cas::ops`' ceiling note: the wasm build is
//! single-threaded, and the native test runner's threads each get their own
//! empty slot.

use std::cell::RefCell;

type Predicate = Box<dyn Fn() -> bool>;

std::thread_local! {
    static DEADLINE: RefCell<Option<(Predicate, String)>> = const { RefCell::new(None) };
}

/// Install the budget predicate and the message a strike reports, replacing
/// any previous one. The boundary pairs this with [`clear`] in a guard so a
/// panic or early return cannot leak the deadline into the next request.
pub fn install(predicate: Predicate, message: String) {
    DEADLINE.with(|slot| *slot.borrow_mut() = Some((predicate, message)));
}

/// Remove the installed deadline (idempotent).
pub fn clear() {
    DEADLINE.with(|slot| *slot.borrow_mut() = None);
}

/// The installed message when the predicate fires; `None` otherwise — and
/// always `None` when nothing is installed, which is every native caller.
pub(crate) fn strike() -> Option<String> {
    DEADLINE.with(|slot| {
        let slot = slot.borrow();
        match slot.as_ref() {
            Some((predicate, message)) if predicate() => Some(message.clone()),
            _ => None,
        }
    })
}
