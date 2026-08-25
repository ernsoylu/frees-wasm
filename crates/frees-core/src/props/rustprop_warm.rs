//! Warm-started `(P, Hmass)` / `(P, Smass)` flashes for
//! [`super::rustprop_backend::RustpropBackend`].
//!
//! # Why this lives here and not in rustprop
//!
//! Upstream CoolProp has **no** `HmolarP` guess path: `update_with_guesses`
//! accepts `PQ`, `QT`, `PT`, `DmolarQ`, `HmolarQ` and `QSmolar` and nothing
//! else. Putting a warm `(P,h)` entry point into rustprop would therefore be
//! an invention, and rustprop's whole contract is "exactly what CoolProp 8
//! does". So rustprop keeps `PtFlash`, `solver_rho_tp_guessed`, `HeosState`
//! and `resolve_fluid` public *for this adapter*, and the warm start lives on
//! the frees side of the seam — where a solver-local speed optimisation
//! belongs.
//!
//! # What it does
//!
//! rustprop's cold `(P,h)` flash is upstream's `HSU_P_flash`: a phase
//! determination, then a **bisection in temperature** to a 2^-29 relative
//! bracket — about thirty density solves. Measured on this machine that is
//! ~380 us. Inside a Newton loop the engine asks for a state a hair away from
//! the one it just asked for, and the previous answer is a near-perfect seed:
//! two or three steps of Newton on `T` over rustprop's *guessed-density*
//! solve, and the flash is done in tens of microseconds.
//!
//! # Why a locality gate is not optional
//!
//! `PtFlash::solver_rho_tp_guessed` is upstream's `update_TP_guessrho` path:
//! Householder4 on the pressure residual from the caller's guess, with **no
//! phase determination and no stability check**. Handed a seed on the wrong
//! side of the dome it will happily converge onto a *metastable* root and
//! report it as the answer. Two gates keep that from happening:
//!
//! 1. **Locality** ([`seed_is_local`]) — the query must be within a
//!    calibrated distance of the cached state, measured as a relative
//!    pressure ratio and a temperature-equivalent caloric offset. Calibrated
//!    by the perturbation sweep in
//!    `tests/rustprop_warm_calibration.rs`; see [`GATE_LN_P`] /
//!    [`GATE_DT_REL`].
//! 2. **Stability** ([`accept_state`]) — the *converged* state must be the
//!    stable root at its own temperature: liquid only if `p > p_sat(T)` and
//!    `rho > rho_l(T)`, gas only if `p < p_sat(T)` and `rho < rho_v(T)`. A
//!    root between the two saturation densities, or on the wrong side of the
//!    saturation pressure, is metastable or unstable and is refused. The
//!    saturation densities come from the same superancillary rustprop's own
//!    phase determination uses, so this costs three Chebyshev evaluations.
//!
//! Anything either gate refuses — and every dome-region input, every
//! first-touch state, every non-convergence — takes the cold path and is
//! counted in [`stats`].
//!
//! # What this adapter does *not* claim: pseudo-pure fluids
//!
//! Only rustprop HEOS **pure** fluids — the ones carrying a superancillary —
//! are this adapter's traffic. A pseudo-pure fluid (Air, the R-blends) is
//! declined at the door in [`try_props_si`], before either counter moves, and
//! its `(P,Hmass)`/`(P,Smass)` call reaches `rustprop::props_si` untouched.
//!
//! That was not always so; see D6 in `docs/decisions/0009-rustprop-backend.md`.
//! Until Wave-2 the adapter served Air itself, because rustprop had no
//! pseudo-pure `(P,X)` flash at all and the adapter's machinery *is* a sequence
//! of `(T,p)` solves. Wave-2 R6/R7 ported upstream's pseudo-pure `HSU_P` and
//! `(D,P)` flashes, and that flash is *cheap* — measured 5.0 us against the
//! adapter's 4.5 us, a 1.1x "speed-up" for a hand-rolled locality gate on a
//! fluid whose saturation state the gate cannot even bracket. So the Air path
//! was retired rather than re-justified. Two consequences are load-bearing
//! here: the superancillary is now unconditional past the gate (so
//! [`accept_state`] may call `PtFlash::sat`, which *panics* on a pseudo-pure),
//! and Air is still served — by rustprop, one delegation away.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rustprop::heos::flash_pt::PtFlash;
use rustprop::heos::flash_px::HeosState;
use rustprop_core::params::Phase;

// ---------------------------------------------------------------------------
// Calibrated constants
// ---------------------------------------------------------------------------

/// Locality gate, pressure axis: the largest `|ln(p / p_seed)|` a warm solve
/// may be asked to cross. `0.10` is +-10.5 % in pressure.
///
/// # Calibration
///
/// `tests/rustprop_warm_calibration.rs` sweeps the seed-to-query distance
/// over the three fluids this adapter claims (Water, R134a, R1234yf — Air left
/// with the pseudo-pure path, see the module docs), fourteen base states
/// spanning liquid / vapour / compressed-liquid / supercritical, both
/// caloric inputs, and three perturbation directions, **with this gate
/// switched off** — the stability gate stays on, because it is the one that
/// makes a wrong root impossible rather than merely unlikely. Its oracle is
/// rustprop's own `(T,P)` flash, which is *exact* in `T` (temperature is an
/// input there, so there is no bisection bracket to blur the comparison).
///
/// The measured result, ladder rungs `1e-4` through `2.0` on both axes:
///
/// * **the warm solve never diverges.** Worst deviation from the exact
///   `(T,P)` state anywhere on the ladder: `1.9e-13` on `T` and `8.5e-13` on
///   `Dmolar` — the latter at Water 700 K / 30 MPa, where `βT ≈ 9` amplifies
///   the temperature error. Not one accepted solve was a wrong root.
/// * what fails at distance is **convergence**, and it fails by *refusing*:
///   the Newton runs out of its step budget and the call goes cold.
///
/// So the gate's job is hit-rate, not safety, and it is set at the widest
/// rung where every swept state still converged inside [`WARM_MAX_STEPS`].
/// On the pressure axis that was `0.35`; `0.10` is the gate, a factor of
/// three and a half inside it.
const GATE_LN_P: f64 = 0.10;

/// Locality gate, caloric axis: the largest temperature-equivalent offset
/// `|Δx / (dx/dT)_P| / T_seed` a warm solve may be asked to cross — 1 % of
/// the seed's temperature.
///
/// Same sweep, same finding (see [`GATE_LN_P`]): no divergence at any
/// distance, only refusals. The caloric axis is the harder one for the step
/// budget — `1e-2` was the widest rung where all 28 swept states converged
/// inside [`WARM_MAX_STEPS`] (at `2e-2`, 26 of 28 did), so `1e-2` is the
/// gate. (Re-measured at Wave-3 D6 on the Air-free base list; the rung the
/// gate sits on did not move, only the denominator.)
const GATE_DT_REL: f64 = 0.01;

/// Newton on `T` stops when the step falls below this fraction of `T`. The
/// state returned is then consistent to ~1e-13 in temperature — nearly four
/// orders tighter than the cold path's own 2^-29 bisection bracket (which
/// leaves up to `9.3e-10` relative), so the warm answer is never the looser
/// of the two, and the sweep above measures exactly that.
///
/// # Do not tighten it. Measured 2026-08-25.
///
/// The question came from the `props_si` cache: warm and cold answers for the
/// same state differ by up to `1.694e-10`, and it was worth knowing whether
/// that is *this* tolerance being loose. It is not — it is the cold path's
/// bracket, and this Newton is already on the root. Against an independent
/// reference (the equation `x(T, rho(T,p)) = x*` bisected to the last
/// representable bit over rustprop's own `(T,P)` flash, which shares no
/// machinery with either flash), at R2's probe state
/// `T(R134a, P = 3.5e5, Hmass = 1.0e5)`: cold sits `1.694e-10` from that root,
/// this tolerance puts warm `1.35e-14` from it, and running the Newton to its
/// own fixed point puts it `1.47e-16` from it — one ulp.
///
/// Tightening therefore buys nothing measurable and costs cold fallbacks. Over
/// 297 `(P,Hmass)` states, convergence inside the shipped
/// [`WARM_MAX_STEPS`] budget against the tolerance asked for:
///
/// ```text
///   tol     1e-11  1e-12  1e-13  1e-14  1e-15  1e-16     0
///   @4      297    297    297    292    282    216      190   of 297
///   @8      297    297    297    296    286    227      194   of 297
/// ```
///
/// The median distance from the exact root is `0` at every one of those
/// tolerances, `1e-11` included: past a point the Newton is landing on the root
/// and the stopping test is only deciding whether to believe it. `1e-13` is the
/// tightest rung where all 297 still converge inside four steps, and every
/// refusal below it is a ~380 us cold flash bought for nothing.
const NEWTON_T_TOL: f64 = 1e-13;

/// Newton steps a *seeded* solve may take.
///
/// One more than the number of quadratic halvings it takes to get from the
/// gate's edge to [`NEWTON_T_TOL`]: `1e-2 -> 1e-4 -> 1e-8 -> 1e-16`, and the
/// step that confirms convergence is evaluated rather than assumed. The sweep
/// measures the budget directly — at four steps every state inside the gate
/// converged, at three, six of twenty-eight did not.
const WARM_MAX_STEPS: usize = 4;

/// How far off the saturation pressure a converged state must sit before the
/// stability gate will call it single-phase. Inside this band the two
/// single-phase roots are barely distinguishable from the saturated ones and
/// the cold path — which resolves the dome with the superancillary rather
/// than with a density solve — is the honest answer.
const P_SAT_MARGIN: f64 = 1e-3;

// ---------------------------------------------------------------------------
// Inputs and outputs this adapter recognises
// ---------------------------------------------------------------------------

/// Which caloric variable the input pair carries. `Umass` is deliberately
/// absent: `(P,U)` is not a pair the frees engine forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caloric {
    /// Molar enthalpy [J/mol].
    Hmolar,
    /// Molar entropy [J/mol/K].
    Smolar,
}

impl Caloric {
    fn value(self, flash: &PtFlash, t: f64, rhomolar: f64) -> f64 {
        match self {
            Caloric::Hmolar => flash.eos.hmolar(t, rhomolar),
            Caloric::Smolar => flash.eos.smolar(t, rhomolar),
        }
    }

    /// `(dx/dT)` at constant pressure from the molar `cp` at the state: `cp`
    /// itself for enthalpy, `cp/T` for entropy. Both are strictly positive
    /// for a stable state, which is also how a density solve that landed on
    /// an unstable root gets caught.
    fn dvalue_dt(self, cp: f64, t: f64) -> f64 {
        match self {
            Caloric::Hmolar => cp,
            Caloric::Smolar => cp / t,
        }
    }
}

/// The outputs the adapter serves off a converged state.
///
/// Every arm below is the corresponding line of rustprop's own private
/// `keyed_output`, so an adapter answer and a `props_si` answer for the same
/// state are the same double — asserted over a grid in
/// `tests/rustprop_warm.rs::cold_path_matches_props_si_bitwise`. Outputs
/// that need more than `(T, rho)` and the molar mass — `Q`, the transport
/// pair, `speed_of_sound`, `Z`, `Gmass`, `Prandtl`,
/// `isobaric_expansion_coefficient` — are **not** listed and go to
/// `props_si` untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Out {
    T,
    P,
    Dmass,
    Dmolar,
    Hmass,
    Hmolar,
    Smass,
    Smolar,
    Umass,
    Umolar,
    Cpmass,
    Cpmolar,
    Cvmass,
    Cvmolar,
}

impl Out {
    /// Only the exact spellings [`crate::props::propfun`] emits are matched;
    /// anything else (including rustprop's own aliases) delegates.
    fn parse(output: &str) -> Option<Out> {
        Some(match output {
            "T" => Out::T,
            "P" => Out::P,
            "Dmass" => Out::Dmass,
            "Dmolar" => Out::Dmolar,
            "Hmass" => Out::Hmass,
            "Hmolar" => Out::Hmolar,
            "Smass" => Out::Smass,
            "Smolar" => Out::Smolar,
            "Umass" => Out::Umass,
            "Umolar" => Out::Umolar,
            "Cpmass" => Out::Cpmass,
            "Cpmolar" => Out::Cpmolar,
            "Cvmass" => Out::Cvmass,
            "Cvmolar" => Out::Cvmolar,
            _ => return None,
        })
    }

    /// Whether this output *is* one of the two inputs. rustprop's `props_si`
    /// answers those from the echo route without a state update ("if all
    /// outputs are also inputs, never do a state update"), which is both
    /// faster than anything here and the behaviour to preserve.
    fn is_echo_of(self, caloric: Caloric) -> bool {
        match self {
            Out::P => true,
            Out::Hmass => caloric == Caloric::Hmolar,
            Out::Smass => caloric == Caloric::Smolar,
            _ => false,
        }
    }

    /// The output at a single-phase `(T, p, rho)`.
    fn read(self, flash: &PtFlash, t: f64, p: f64, rhomolar: f64) -> f64 {
        let mm = flash.eos.molar_mass;
        match self {
            Out::T => t,
            Out::P => p,
            Out::Dmolar => rhomolar,
            Out::Dmass => rhomolar * mm,
            Out::Hmolar => flash.eos.hmolar(t, rhomolar),
            Out::Hmass => flash.eos.hmolar(t, rhomolar) / mm,
            Out::Smolar => flash.eos.smolar(t, rhomolar),
            Out::Smass => flash.eos.smolar(t, rhomolar) / mm,
            Out::Umolar => flash.eos.umolar(t, rhomolar),
            Out::Umass => flash.eos.umolar(t, rhomolar) / mm,
            Out::Cpmolar => flash.eos.cpmolar(t, rhomolar),
            Out::Cpmass => flash.eos.cpmolar(t, rhomolar) / mm,
            Out::Cvmolar => flash.eos.cvmolar(t, rhomolar),
            Out::Cvmass => flash.eos.cvmolar(t, rhomolar) / mm,
        }
    }

    /// The output at a rustprop [`HeosState`], two-phase states included:
    /// `state_hmolar` / `state_smolar` / `state_umolar` are rustprop's own
    /// lever-rule mixers, and `cp`/`cv` are — as upstream — the raw
    /// single-phase formulas at the mixture density.
    fn read_state(self, flash: &PtFlash, state: &HeosState) -> f64 {
        let mm = flash.eos.molar_mass;
        match self {
            Out::T => state.t(),
            Out::P => state.p(),
            Out::Dmolar => state.rhomolar(),
            Out::Dmass => state.rhomolar() * mm,
            Out::Hmolar => flash.state_hmolar(state),
            Out::Hmass => flash.state_hmolar(state) / mm,
            Out::Smolar => flash.state_smolar(state),
            Out::Smass => flash.state_smolar(state) / mm,
            Out::Umolar => flash.state_umolar(state),
            Out::Umass => flash.state_umolar(state) / mm,
            Out::Cpmolar => flash.eos.cpmolar(state.t(), state.rhomolar()),
            Out::Cpmass => flash.eos.cpmolar(state.t(), state.rhomolar()) / mm,
            Out::Cvmolar => flash.eos.cvmolar(state.t(), state.rhomolar()),
            Out::Cvmass => flash.eos.cvmolar(state.t(), state.rhomolar()) / mm,
        }
    }
}

/// Orients an unordered input pair into `(p, x_mass, which caloric)`.
fn orient(name1: &str, value1: f64, name2: &str, value2: f64) -> Option<(f64, f64, Caloric)> {
    let caloric = |n: &str| match n {
        "Hmass" => Some(Caloric::Hmolar),
        "Smass" => Some(Caloric::Smolar),
        _ => None,
    };
    if name1 == "P" {
        return Some((value1, value2, caloric(name2)?));
    }
    if name2 == "P" {
        return Some((value2, value1, caloric(name1)?));
    }
    None
}

// ---------------------------------------------------------------------------
// The caches and the counters
// ---------------------------------------------------------------------------

/// One converged single-phase state, keyed by fluid identity and input pair.
#[derive(Debug, Clone, Copy)]
struct Seed {
    /// `&'static FluidData` address — the fluid's identity, the same key
    /// rustprop's own per-fluid flash cache uses.
    fluid: usize,
    caloric: Caloric,
    p: f64,
    /// The molar caloric value at the state.
    x: f64,
    t: f64,
    rhomolar: f64,
    /// Molar `cp` at the state: the locality gate's temperature scale.
    cp: f64,
}

/// The last converged state per (fluid, input pair). At most one entry per
/// pair per linked fluid, so a `Vec` scanned linearly is the whole index.
static SEEDS: Mutex<Vec<Seed>> = Mutex::new(Vec::new());

/// One `PtFlash` per fluid. rustprop keeps its own equivalent cache behind a
/// private function, so the adapter keeps its own rather than reaching into
/// it; construction is a `HelmholtzEos` plus the superancillary `T(ln p)`
/// fit, paid once per fluid per process.
static FLASHES: Mutex<Vec<(usize, Arc<PtFlash>)>> = Mutex::new(Vec::new());

static WARM_SERVED: AtomicU64 = AtomicU64::new(0);
static COLD_FALLBACKS: AtomicU64 = AtomicU64::new(0);

/// Warm-path observability: how many eligible `(P,Hmass)`/`(P,Smass)` calls
/// the warm start served, and how many fell back to the cold flash.
///
/// Calls the adapter never had a shot at — a different input pair, an output
/// it does not serve, a fluid rustprop's HEOS registry does not know — are
/// counted in neither: they are not fallbacks, they are somebody else's
/// traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WarmStats {
    /// Calls served by a warm (seeded Newton) solve.
    pub warm: u64,
    /// Eligible calls that took the cold flash instead.
    pub cold_fallbacks: u64,
}

/// The current [`WarmStats`].
pub fn stats() -> WarmStats {
    WarmStats {
        warm: WARM_SERVED.load(Ordering::Relaxed),
        cold_fallbacks: COLD_FALLBACKS.load(Ordering::Relaxed),
    }
}

/// Zeroes the counters and forgets every cached state — the next call to
/// each (fluid, pair) is a cold one again. Tests need it; a host that wants
/// a clean measurement window can use it too.
pub fn reset() {
    WARM_SERVED.store(0, Ordering::Relaxed);
    COLD_FALLBACKS.store(0, Ordering::Relaxed);
    SEEDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn seed_of(fluid: usize, caloric: Caloric) -> Option<Seed> {
    SEEDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|s| s.fluid == fluid && s.caloric == caloric)
        .copied()
}

fn remember(seed: Seed) {
    let mut seeds = SEEDS.lock().unwrap_or_else(|e| e.into_inner());
    match seeds
        .iter_mut()
        .find(|s| s.fluid == seed.fluid && s.caloric == seed.caloric)
    {
        Some(slot) => *slot = seed,
        None => seeds.push(seed),
    }
}

// ---------------------------------------------------------------------------
// The one call into rustprop's guessed-density solve
// ---------------------------------------------------------------------------

/// Every guessed-density solve in this module goes through here.
///
/// **RUSTPROP SIGNATURE CHANGE LANDED** (Wave-2 R6, integrated 2026-08-18).
/// `solver_rho_tp_guessed` grew a fourth parameter, the phase to impose:
///
/// ```text
///   before: solver_rho_tp_guessed(&self, t, p, rho_guess)               -> Result<f64>
///   now:    solver_rho_tp_guessed(&self, t, p, rho_guess, phase: Phase) -> Result<f64>
/// ```
///
/// We pass [`Phase::NotImposed`], which is **behaviour-identical to the
/// three-argument form this adapter was calibrated against**. That was
/// verified against the merged rustprop source rather than assumed:
///
/// * on success it takes the catch-all `_ => Ok(rho)` arm, skipping the
///   `Phase::Liquid` / `Phase::Gas` stability retries;
/// * on failure it skips the new `Supercritical | SupercriticalGas` Brent
///   branch and falls into the same `t > t_critical()` tail the
///   three-argument form ended in.
///
/// Passing `Phase::Liquid` or `Phase::Gas` here would be a silent behaviour
/// change, not a refinement: those arms activate upstream's phase-imposed
/// restarts (Householder4 from `3 * rho_reducing` and from `1e-6`
/// respectively), which can converge to a *different root* than the one this
/// adapter's locality and stability gates were calibrated against. Do not
/// "improve" this into a real phase.
fn solve_rho_tp_guessed(flash: &PtFlash, t: f64, p: f64, rho_guess: f64) -> Option<f64> {
    flash
        .solver_rho_tp_guessed(t, p, rho_guess, Phase::NotImposed)
        .ok()
        .filter(|rho| rho.is_finite() && *rho > 0.0)
}

// ---------------------------------------------------------------------------
// The solve, and the two gates
// ---------------------------------------------------------------------------

/// Where a [`newton_t`] solve starts and how far it may go.
///
/// There is no step clamp: every solve this module runs starts from a *local*
/// seed inside the locality gate, so a clamp would only ever hide a Newton
/// that was already going wrong. (The unseeded pseudo-pure solve that needed
/// one left with the Air path — see the module docs.)
#[derive(Debug, Clone, Copy)]
struct Start {
    t: f64,
    rhomolar: f64,
    max_steps: usize,
}

/// Newton on `T` at fixed `p`: `x(T, rho(T,p)) = target`, with the exact
/// derivative `(dx/dT)_P`. Returns a consistent `(T, rho, cp)` whose remaining
/// temperature error is below [`NEWTON_T_TOL`] relative, or `None` if it did
/// not get there inside `start.max_steps`. The molar `cp` comes free — the
/// derivative at the accepted state already had to be evaluated — and is what
/// the seed carries as its gate scale.
fn newton_t(
    flash: &PtFlash,
    caloric: Caloric,
    p: f64,
    target: f64,
    start: Start,
) -> Option<(f64, f64, f64)> {
    let Start {
        mut t,
        rhomolar: mut rho,
        max_steps,
    } = start;
    for _ in 0..=max_steps {
        rho = solve_rho_tp_guessed(flash, t, p, rho)?;
        let x = caloric.value(flash, t, rho);
        let cp = flash.eos.cpmolar(t, rho);
        let dxdt = caloric.dvalue_dt(cp, t);
        if !x.is_finite() || !dxdt.is_finite() || dxdt <= 0.0 {
            // A non-positive (dx/dT)_P is not a state a stable fluid has; the
            // density solve landed somewhere unphysical.
            return None;
        }
        let dt = (x - target) / dxdt;
        if (dt / t).abs() <= NEWTON_T_TOL {
            return Some((t, rho, cp));
        }
        let t_next = t - dt;
        if !(t_next.is_finite() && t_next > 0.0) || t_next == t {
            return None;
        }
        t = t_next;
    }
    None
}

/// Is the query provably in the seed's neighbourhood? The two axes are a
/// relative pressure ratio and a temperature-equivalent caloric offset; see
/// [`GATE_LN_P`] and [`GATE_DT_REL`] for the calibration.
fn seed_is_local(seed: &Seed, p: f64, x: f64) -> bool {
    if !(seed.p > 0.0 && seed.t > 0.0) {
        return false;
    }
    if (p / seed.p).ln().abs() > GATE_LN_P {
        return false;
    }
    let dxdt = seed.caloric.dvalue_dt(seed.cp, seed.t);
    if !dxdt.is_finite() || dxdt <= 0.0 {
        return false;
    }
    ((x - seed.x) / dxdt).abs() / seed.t <= GATE_DT_REL
}

/// Is this one of the **pure** fluids the adapter claims?
///
/// The superancillary is the discriminator rustprop itself uses: every one of
/// its 130 pure HEOS fluids carries one, and the six pseudo-pures (Air, the
/// R-blends) carry none. It is not a taste question — [`accept_state`] reaches
/// for `PtFlash::sat`, which *panics* without it, so this predicate is what
/// makes the rest of the module's superancillary use unconditional. Every entry
/// point calls it before any solve runs.
fn is_pure(flash: &PtFlash) -> bool {
    flash.fluid().eos.superancillary.is_some()
}

/// Is the converged `(T, p, rho)` the **stable** root — the one rustprop's
/// own phase determination would have picked?
///
/// It is decided at the converged temperature, which is stricter than deciding
/// it at the query pressure: `rho_l(T)` and `rho_v(T)` bound the
/// metastable/unstable band at `T`, so a root outside that band on the side
/// that matches `p` vs `p_sat(T)` is the stable one, full stop.
///
/// **The fluid must be [`is_pure`]** — `PtFlash::sat` panics without a
/// superancillary. Both entry points ([`try_props_si`] and
/// [`calibration_warm_solve`]) decline a pseudo-pure fluid before any solve
/// runs, which is what makes that unconditional here.
fn accept_state(flash: &PtFlash, t: f64, p: f64, rhomolar: f64) -> bool {
    if !(t.is_finite() && t > 0.0 && rhomolar.is_finite() && rhomolar > 0.0) {
        return false;
    }
    // Stay inside the temperature window rustprop's cold `HSU_P_flash`
    // brackets ([T_min, 1.5*T_max]); its liquid floor can be higher still
    // (the melting line), but every seed this path can reach descends from a
    // state the cold flash converged on, so the warm path cannot wander
    // below it on its own.
    if !(t >= flash.t_triple() && t <= 1.5 * flash.fluid().eos.t_max) {
        return false;
    }
    let tc = flash.t_critical();
    if t >= tc {
        // Above the critical temperature the isotherm is monotone in density,
        // so the root the guessed solve found is the only one there is.
        return true;
    }
    let Ok(sat) = flash.sat().qt_flash(t, 0.0) else {
        return false;
    };
    let liquid = p > sat.p * (1.0 + P_SAT_MARGIN) && rhomolar > sat.rho_l;
    let gas = p < sat.p * (1.0 - P_SAT_MARGIN) && rhomolar < sat.rho_v;
    liquid || gas
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// The adapter's answer to one `props_si` call, or `None` when the caller
/// must delegate to `rustprop::props_si` unchanged.
///
/// `None` covers everything: an input pair or output this adapter does not
/// recognise, a fluid outside rustprop's HEOS registry, a gate refusal whose
/// cold path also declined, a flash that errored (so the caller's delegation
/// reproduces rustprop's message verbatim).
pub(crate) fn try_props_si(
    output: &str,
    name1: &str,
    value1: f64,
    name2: &str,
    value2: f64,
    fluid: &str,
) -> Option<f64> {
    let (p, x_mass, caloric) = orient(name1, value1, name2, value2)?;
    let out = Out::parse(output)?;
    if out.is_echo_of(caloric) {
        return None; // rustprop's echo route is already O(1)
    }
    if !(p.is_finite() && p > 0.0 && x_mass.is_finite()) {
        return None;
    }
    // A backend-prefixed or incompressible spelling (`IF97::Water`,
    // `INCOMP::MEG[0.50]`) is not this adapter's business; the registry
    // lookup below would decline it anyway, but saying so here is cheaper
    // and states the intent.
    if fluid.contains(':') {
        return None;
    }
    // NOTE: the `&'static FluidData` type is never named in this module — it
    // lives in `rustprop-core`, which frees-core does not depend on directly.
    // Inference carries it from `resolve_fluid` to `PtFlash::new`.
    let data = rustprop::props_api::resolve_fluid(fluid).ok()?;
    let key = std::ptr::from_ref(data) as usize;
    let flash = {
        let mut cache = FLASHES.lock().unwrap_or_else(|e| e.into_inner());
        match cache.iter().find(|(k, _)| *k == key) {
            Some((_, f)) => Arc::clone(f),
            None => {
                let f = Arc::new(PtFlash::new(data));
                cache.push((key, Arc::clone(&f)));
                f
            }
        }
    };
    // A pseudo-pure fluid (Air, the R-blends) is not this adapter's traffic:
    // rustprop's own pseudo-pure `HSU_P` flash serves the pair directly and
    // costs about what a warm solve does, and there is no superancillary here
    // to prove a root stable with. Declined before either counter moves — see
    // the module docs, and D6 in `docs/decisions/0009-rustprop-backend.md`.
    if !is_pure(&flash) {
        return None;
    }
    let x = x_mass * flash.eos.molar_mass;

    if let Some(v) = warm_serve(&flash, key, caloric, p, x, out) {
        WARM_SERVED.fetch_add(1, Ordering::Relaxed);
        return Some(v);
    }
    COLD_FALLBACKS.fetch_add(1, Ordering::Relaxed);
    cold_serve(&flash, key, caloric, p, x, out)
}

/// The warm path: a cached state, both gates, a seeded Newton.
fn warm_serve(
    flash: &PtFlash,
    key: usize,
    caloric: Caloric,
    p: f64,
    x: f64,
    out: Out,
) -> Option<f64> {
    let seed = seed_of(key, caloric)?;
    if !seed_is_local(&seed, p, x) {
        return None;
    }
    let (t, rho, cp) = newton_t(
        flash,
        caloric,
        p,
        x,
        Start {
            t: seed.t,
            rhomolar: seed.rhomolar,
            max_steps: WARM_MAX_STEPS,
        },
    )?;
    if !accept_state(flash, t, p, rho) {
        return None;
    }
    remember(Seed {
        fluid: key,
        caloric,
        p,
        x,
        t,
        rhomolar: rho,
        cp,
    });
    Some(out.read(flash, t, p, rho))
}

/// The cold path — and the only thing that ever *creates* a seed.
///
/// This is rustprop's own `HSU_P_flash`, called directly rather than through
/// `props_si`: same function, same molar conversion, and the outputs come off
/// the state through the same expressions `keyed_output` uses, so the answer is
/// the same double `props_si` would have returned (asserted bitwise in
/// `tests/rustprop_warm.rs`). Going through `props_si` instead would mean
/// flashing twice for every dome-region state, which is exactly the traffic a
/// refrigeration document generates most of.
fn cold_serve(
    flash: &PtFlash,
    key: usize,
    caloric: Caloric,
    p: f64,
    x: f64,
    out: Out,
) -> Option<f64> {
    let state = match caloric {
        Caloric::Hmolar => flash.hmolar_p_state(x, p),
        Caloric::Smolar => flash.p_smolar_state(p, x),
    }
    .ok()?;
    // Only a single-phase state is a usable seed: the warm path solves for a
    // single-phase root and has no business starting inside the dome.
    if let HeosState::SinglePhase { t, rhomolar, .. } = state {
        remember(Seed {
            fluid: key,
            caloric,
            p,
            x,
            t,
            rhomolar,
            cp: flash.eos.cpmolar(t, rhomolar),
        });
    }
    Some(out.read_state(flash, &state))
}

// ---------------------------------------------------------------------------
// Calibration seam
// ---------------------------------------------------------------------------

/// One warm solve from an **explicit** seed with the locality gate switched
/// off — the stability gate still applies. This is what
/// `tests/rustprop_warm_calibration.rs` sweeps to find where a seeded solve
/// stops reproducing the exact state, and therefore where [`GATE_LN_P`] and
/// [`GATE_DT_REL`] have to sit. `Ok` carries the converged `(T, Dmolar)`;
/// `Err` says which of the two refused, so the sweep can tell a Newton that
/// ran out of steps from a root the stability gate would not vouch for.
///
/// Not `#[cfg(test)]`: the calibration lives in an integration test, which
/// links the crate from outside.
#[doc(hidden)]
pub fn calibration_warm_solve(
    fluid: &str,
    caloric_is_enthalpy: bool,
    p: f64,
    x_mass: f64,
    seed_t: f64,
    seed_dmolar: f64,
    max_steps: usize,
) -> std::result::Result<(f64, f64), &'static str> {
    let caloric = if caloric_is_enthalpy {
        Caloric::Hmolar
    } else {
        Caloric::Smolar
    };
    let data = rustprop::props_api::resolve_fluid(fluid).map_err(|_| "fluid")?;
    let flash = PtFlash::new(data);
    // The same door [`try_props_si`] shuts: pseudo-pure fluids are not this
    // adapter's traffic, and [`accept_state`] would panic reaching for a
    // superancillary that is not there.
    if !is_pure(&flash) {
        return Err("pseudo-pure");
    }
    let x = x_mass * flash.eos.molar_mass;
    let start = Start {
        t: seed_t,
        rhomolar: seed_dmolar,
        max_steps,
    };
    let (t, rho, _cp) = newton_t(&flash, caloric, p, x, start).ok_or("newton")?;
    if !accept_state(&flash, t, p, rho) {
        return Err("stability");
    }
    Ok((t, rho))
}

/// The step budget a seeded warm solve gets, for the calibration sweep to
/// exercise the shipped configuration.
#[doc(hidden)]
pub const CALIBRATION_WARM_MAX_STEPS: usize = WARM_MAX_STEPS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orient_takes_either_order_and_nothing_else() {
        assert_eq!(
            orient("P", 1.0, "Hmass", 2.0),
            Some((1.0, 2.0, Caloric::Hmolar))
        );
        assert_eq!(
            orient("Smass", 2.0, "P", 1.0),
            Some((1.0, 2.0, Caloric::Smolar))
        );
        for (n1, n2) in [("P", "T"), ("T", "Q"), ("P", "Dmass"), ("Hmass", "Smass")] {
            assert_eq!(orient(n1, 1.0, n2, 2.0), None, "{n1}/{n2}");
        }
    }

    #[test]
    fn echo_outputs_are_left_to_props_si() {
        assert!(Out::P.is_echo_of(Caloric::Hmolar));
        assert!(Out::Hmass.is_echo_of(Caloric::Hmolar));
        assert!(!Out::Hmass.is_echo_of(Caloric::Smolar));
        assert!(Out::Smass.is_echo_of(Caloric::Smolar));
        assert!(!Out::T.is_echo_of(Caloric::Hmolar));
    }

    #[test]
    fn unrecognised_outputs_delegate() {
        for output in [
            "Q",
            "viscosity",
            "conductivity",
            "speed_of_sound",
            "Z",
            "Gmass",
            "Prandtl",
            "isobaric_expansion_coefficient",
            "Tcrit",
            "H",
        ] {
            assert!(Out::parse(output).is_none(), "{output}");
        }
    }

    /// The gate is symmetric in the pressure ratio and scales the caloric
    /// axis by the seed's own `(dx/dT)_P`.
    #[test]
    fn locality_gate_measures_both_axes() {
        let seed = Seed {
            fluid: 0,
            caloric: Caloric::Hmolar,
            p: 1.0e5,
            x: 1000.0,
            t: 300.0,
            rhomolar: 50.0,
            cp: 40.0,
        };
        assert!(seed_is_local(&seed, 1.0e5, 1000.0));
        // +-10 % in pressure is inside; +-12 % is not, either way round.
        assert!(seed_is_local(&seed, 1.10e5, 1000.0));
        assert!(seed_is_local(&seed, 1.0e5 / 1.10, 1000.0));
        assert!(!seed_is_local(&seed, 1.12e5, 1000.0));
        assert!(!seed_is_local(&seed, 1.0e5 / 1.12, 1000.0));
        // 1 % of 300 K is 3 K, i.e. 3 * cp = 120 J/mol of enthalpy.
        assert!(seed_is_local(&seed, 1.0e5, 1000.0 + 110.0));
        assert!(!seed_is_local(&seed, 1.0e5, 1000.0 + 130.0));
    }
}
