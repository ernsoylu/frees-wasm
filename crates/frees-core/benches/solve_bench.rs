//! Phase 12: the benchmark suite. Five documents spanning the engine's cost
//! centres, timed end-to-end through the public `solve` — parse, unit check,
//! Tarjan blocking, Newton, and (where the document asks) the component
//! expander, the property backend and the ODE integrators. End-to-end because
//! that is what a keystroke costs the user; nothing here times internals.
//!
//! The JVM comparison is NOT in this file — an oracle timing run is
//! `time tools/golden-dumper/run.sh <dir> <out>` over the same documents (see
//! docs/status-phase12.md for the measured table and its caveats). Keep the
//! document list here and the oracle timing directory in sync.
//!
//! Native-only: criterion's dependency tree does not build on
//! wasm32-unknown-unknown, and CI clippy compiles `--all-targets` for that
//! target — hence the per-item cfg guards and the stub `main` for wasm32.

#[cfg(not(target_arch = "wasm32"))]
use criterion::{criterion_group, criterion_main, Criterion};
#[cfg(not(target_arch = "wasm32"))]
use frees_core::{solve, SolverSettings};

/// The canonical two-block scalar document (Phase 0's first fixture).
#[cfg(not(target_arch = "wasm32"))]
const SCALAR: &str = "\
x = 4 [m] - y\n\
y = x / 2\n\
a = 2 * x\n\
";

#[cfg(not(target_arch = "wasm32"))]
fn doc(name: &str) -> String {
    let path = format!(
        "{}/../../fixtures/corpus/{name}.frees",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn bench_solve(c: &mut Criterion) {
    let settings = SolverSettings::default();

    let cases: Vec<(&str, String)> = vec![
        ("scalar_two_block", SCALAR.to_string()),
        // Real-fluid Rankine cycle: the property-table backend in the loop.
        ("rankine_cycle", doc("rankine-cycle")),
        // Component network: library expansion + the mixed system it emits.
        ("component_mvem", doc("components_bsweep_mvem_wotmap")),
        // Transient: a DYNAMIC block driving the ODE path end to end.
        ("transient_dyn", doc("dyn_accessor_read")),
        // Control CALLs: state space, LQR and the CAS-backed helpers.
        ("control_lqr", doc("ctl-lqr_3state")),
    ];

    for (name, source) in &cases {
        // Fail loudly outside the timer if a document stops solving — a bench
        // that times an error path reports a fantasy speedup.
        let probe = solve(source, &settings);
        assert!(probe.is_ok(), "{name} no longer solves: {:?}", probe.err());

        c.bench_function(name, |b| {
            b.iter(|| solve(std::hint::black_box(source), &settings))
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
criterion_group!(benches, bench_solve);
#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

/// wasm32: the bench target must still link under `clippy --all-targets`.
#[cfg(target_arch = "wasm32")]
fn main() {}
