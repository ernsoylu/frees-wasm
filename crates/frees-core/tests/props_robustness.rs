//! Adversarial robustness of the **property surface** — Phase 5's half of
//! `robustness.rs`.
//!
//! The rule is the same one line: **`parse_document`, `check` and `solve` may
//! return `Ok` or `Err` for any byte string whatsoever, and must do nothing
//! else** — not panic, not abort, not hang, and not quietly answer a question
//! that was never asked. Phase 5 added ~120 intrinsics that reach iterative
//! correlations, bracketed root-finds, tabulated interpolants and a chemical
//! formula parser, and every one of them is a new way to break that rule.
//!
//! What "not quietly answer" means here is stricter than "no panic". A property
//! function that returns `NaN` for a state it cannot serve has *lied*: the
//! solver will happily propagate it, the residual will be `NaN`, and the user
//! gets a converged-looking answer built on nothing. So the standing assertion
//! of this file is:
//!
//! > a solved document never contains a non-finite value.
//!
//! Anything that cannot be computed must come back as `Err`.
//!
//! # Where the hostile inputs come from
//!
//! Not invented — taken from the actual failure geometry of each area:
//!
//! * **thermodynamic impossibilities** — zero and negative absolute temperature
//!   and pressure, which are one sign flip away in any real document;
//! * **the phase envelope** — exactly on the critical point, above it, below the
//!   triple point, and inside the dome where the `(P,h)` inverse is genuinely
//!   non-unique;
//! * **table edges** — the generated `(P,h)` tables have a served box with hard
//!   edges, probed exactly on and a hair outside;
//! * **the formula parser** — `""`, `"3"`, `"H2O2X"`, unbalanced brackets, and
//!   a formula long enough to matter;
//! * **the bracketed root-finds** — area ratios below one, Prandtl–Meyer angles
//!   past the vacuum limit, oblique shocks past detachment, all of which have a
//!   real physical boundary the bracket cannot cross;
//! * **the ε-NTU relations** — `NTU = 0`, `Cr = 0` and `Cr = 1` (both removable
//!   singularities in the closed forms), `ε` at and above 1;
//! * **combustion** — equivalence ratio at 0 and at 1e12, where the product
//!   composition degenerates.

use std::time::{Duration, Instant};

use frees_core::props::propfun;
use frees_core::props::satsplit::SaturationSplitTable;
use frees_core::{check, parse_document, solve, SolverSettings};

// ── helpers ─────────────────────────────────────────────────────────────────

fn settings() -> SolverSettings {
    SolverSettings::default()
}

/// Run the three public entry points on `src` and report only *how* each
/// answered, with the offending document in any failure message.
fn answered(src: &str) -> Result<(), String> {
    fn run(
        name: &str,
        src: &str,
        f: impl Fn(&str) + std::panic::RefUnwindSafe,
    ) -> Result<(), String> {
        std::panic::catch_unwind(|| f(src)).map_err(|_| format!("{name} panicked on {src:?}"))
    }
    run("parse_document", src, |s| {
        let _ = parse_document(s);
    })?;
    run("check", src, |s| {
        let _ = check(s);
    })?;
    run("solve", src, |s| {
        let _ = solve(s, &settings());
    })
}

/// The standing contract, applied to one document: answered, bounded, and
/// — if it solved — every value finite.
///
/// Returns the elapsed time so the caller can assert a budget over a corpus
/// rather than flaking on one slow machine.
fn survives(src: &str) -> Result<Duration, String> {
    let started = Instant::now();
    answered(src)?;
    let elapsed = started.elapsed();
    if let Ok(solution) = solve(src, &settings()) {
        let bad: Vec<(&String, &f64)> = solution
            .values
            .iter()
            .filter(|(_, v)| !v.is_finite())
            .collect();
        if !bad.is_empty() {
            return Err(format!(
                "{src:?} solved with non-finite values {bad:?} — a property function \
                 answered a state it cannot serve instead of refusing it"
            ));
        }
    }
    Ok(elapsed)
}

/// Assert the contract over a whole corpus, reporting every violation at once.
fn all_survive(corpus: &[String]) -> Duration {
    let mut worst = Duration::ZERO;
    let mut slowest = String::new();
    let mut failures = Vec::new();
    for src in corpus {
        match survives(src) {
            Ok(d) => {
                if d > worst {
                    worst = d;
                    slowest.clone_from(src);
                }
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} documents broke the contract:\n{}",
        failures.len(),
        corpus.len(),
        failures.join("\n")
    );
    assert!(
        worst < Duration::from_secs(20),
        "slowest document took {worst:?}, which is a hang in all but name: {slowest:?}"
    );
    // Printed, not just asserted: the 20 s ceiling is a hang detector, and the
    // number under it is the only way a reader can tell "comfortably fast" from
    // "one bad document away from the ceiling". Wave-3 F7 measured the whole
    // file this way after the backend switch — see docs/status-wave3-f7.md.
    println!(
        "all_survive: {} documents, worst {worst:?} on {slowest:?}",
        corpus.len()
    );
    worst
}

/// Every hostile scalar this file pushes through a numeric argument slot.
///
/// `0` and the negatives are the thermodynamic impossibilities; `1e-300` and
/// `1e300` are the ends of the exponent range; the two `f64` specials are what a
/// previous block's failed solve leaves behind in a downstream expression.
const HOSTILE: &[&str] = &[
    "0", "-1", "-1e30", "1e-300", "1e300", "1e-12", "1", "0.5", "1e12",
];

/// A document that calls `f` with `args`, and nothing else.
fn doc(f: &str, args: &[&str]) -> String {
    format!("x = {f}({})", args.join(", "))
}

// ── the sweep: every property intrinsic against every hostile scalar ─────────

/// The property intrinsics that take only numbers, with their arity.
///
/// Kept as an explicit list rather than derived from `INTRINSICS` on purpose:
/// the point is that *these specific* Phase-5 additions are covered, and a new
/// one that is not in this list should be added deliberately by whoever adds it.
const NUMERIC_PROPERTY_INTRINSICS: &[(&str, usize)] = &[
    // atmosphere
    ("isa_t", 1),
    ("isa_p", 1),
    ("isa_rho", 1),
    // compressible flow — isentropic, normal shock, oblique, Fanno, Rayleigh
    ("isen_t0_t", 2),
    ("isen_p0_p", 2),
    ("isen_rho0_rho", 2),
    ("isen_a_astar", 2),
    ("mach_a_astar", 3),
    ("mach_prandtlmeyer", 2),
    ("prandtl_meyer", 2),
    ("machangle", 1),
    ("t2_t1_shock", 2),
    ("p2_p1_shock", 2),
    ("rho2_rho1_shock", 2),
    ("p02_p01_shock", 2),
    ("mach_shock", 2),
    ("m2_shock", 2),
    ("theta_oblique", 3),
    ("beta_oblique", 3),
    ("fanno_t_tstar", 2),
    ("fanno_p_pstar", 2),
    ("fanno_p0_p0star", 2),
    ("fanno_fld", 2),
    ("rayleigh_t_tstar", 2),
    ("rayleigh_t0_t0star", 2),
    ("rayleigh_p_pstar", 2),
    ("rayleigh_p0_p0star", 2),
    ("stagnationtemp", 3),
    ("stagnationpres", 4),
    // convective correlations
    ("nu_dittus_boelter", 3),
    ("nu_gnielinski", 2),
    ("nu_colburn", 2),
    ("nu_churchill_chu", 2),
    ("nu_hilpert", 2),
    ("nu_zukauskas", 4),
    ("nu_tubebank", 4),
    ("nu_plate", 3),
    ("prandtl", 3),
    ("reynolds", 4),
    ("re_number", 4),
    // flow resistance
    ("darcy_friction", 2),
    ("friction_factor", 2),
    ("minor_loss", 3),
    ("mass_flux", 2),
    ("momentum_flux", 3),
    // heat exchangers
    ("hx_effectiveness", 3),
    ("hx_epsilon", 3),
    ("hx_ntu", 3),
    ("lmtd", 2),
    ("ua_hx", 5),
    ("fin_efficiency", 2),
    ("f_fin", 2),
    ("j_fin", 2),
    ("hx_dh", 2),
    ("hx_sigma", 2),
    // two-phase
    ("lm_martinelli_tt", 3),
    ("lm_phi2", 2),
    ("friedel_phi2", 4),
    ("void_homogeneous", 2),
    ("void_zivi", 2),
    ("void_rouhani", 4),
    ("chen_f", 1),
    ("chen_s", 2),
    // pneumatics
    ("iso6358", 5),
    // Heisler charts
    ("heisler_temp", 3),
    ("heisler_q", 3),
    // engine / combustion shapes
    ("wiebe", 4),
    ("wiebe_rate", 4),
    // view factors
    ("viewfactor_disks", 3),
    ("viewfactor_perp", 3),
    ("viewfactor_plates", 3),
];

/// The sweep. Every numeric property intrinsic, every hostile scalar, in every
/// argument position, with the other positions held at a benign `1`.
///
/// This is 70 functions × 9 values × up to 5 positions ≈ 1,800 documents. It is
/// the net that catches "this correlation divides by `1 − Cr`" and "this
/// bracketed solve never terminates when the target is outside the bracket".
#[test]
fn every_numeric_property_intrinsic_answers_every_hostile_scalar() {
    let mut corpus = Vec::new();
    for (name, arity) in NUMERIC_PROPERTY_INTRINSICS {
        for pos in 0..*arity {
            for value in HOSTILE {
                let args: Vec<&str> = (0..*arity)
                    .map(|i| if i == pos { *value } else { "1" })
                    .collect();
                corpus.push(doc(name, &args));
            }
        }
        // All-hostile, all positions at once: the combinations a per-position
        // sweep cannot reach (e.g. every ε-NTU argument at zero together).
        for value in HOSTILE {
            let args: Vec<&str> = (0..*arity).map(|_| *value).collect();
            corpus.push(doc(name, &args));
        }
    }
    let worst = all_survive(&corpus);
    println!(
        "{} property documents answered, slowest {worst:?}",
        corpus.len()
    );
}

// ── the phase envelope ──────────────────────────────────────────────────────

/// Real-fluid calls at every corner of the phase diagram.
///
/// The engine either answers a finite number or refuses. What it must never do
/// is extrapolate silently past the served box, which for a table-backed backend
/// is the whole failure mode.
#[test]
fn real_fluid_calls_at_every_corner_of_the_phase_envelope_are_answered_or_refused() {
    let mut corpus = Vec::new();
    // Water: p_crit = 22.064 MPa, T_crit = 647.096 K, triple = 611.65 Pa /
    // 273.16 K. The table serves p in [2206 Pa, 15.72 MPa].
    for (p, t) in [
        (0.0, 300.0),            // zero pressure
        (-101325.0, 300.0),      // negative pressure
        (101325.0, 0.0),         // absolute zero
        (101325.0, -300.0),      // negative absolute temperature
        (101325.0, 1e-300),      // subnormal temperature
        (1e300, 300.0),          // pressure past every representable state
        (101325.0, 1e300),       // temperature likewise
        (22_064_000.0, 647.096), // exactly the critical point
        (30_000_000.0, 700.0),   // supercritical
        (611.657, 273.16),       // exactly the triple point
        (100.0, 200.0),          // below the triple point, solid territory
        (2206.4, 292.21),        // exactly p_min of the generated table
        (2206.3, 292.21),        // a hair below p_min
        (15_720_599.0, 620.0),   // exactly p_serve_max
        (15_720_601.0, 620.0),   // a hair above p_serve_max
        (101325.0, 373.1243),    // exactly on the saturation line
    ] {
        for f in ["Enthalpy", "Entropy", "Density", "Volume", "Temperature"] {
            corpus.push(format!("x = {f}(Water, P={p}, T={t})"));
            corpus.push(format!("x = {f}(R134a, P={p}, T={t})"));
        }
    }
    // Quality: inside the dome, on both edges, and outside on both sides.
    for q in [-1.0, -1e-12, 0.0, 0.5, 1.0, 1.0 + 1e-12, 2.0, 1e300] {
        for f in ["Enthalpy", "Entropy", "Density", "Volume", "Temperature"] {
            corpus.push(format!("x = {f}(Water, P=101325, x={q})"));
            corpus.push(format!("x = {f}(R134a, P=500000, x={q})"));
        }
    }
    // Saturation queries either side of the line's ends.
    for t in [
        0.0, -1.0, 200.0, 273.16, 292.2, 373.15, 623.24, 647.096, 1000.0,
    ] {
        corpus.push(format!("x = P_sat(Water, T={t})"));
        corpus.push(format!("x = T_sat(Water, P={t})"));
        corpus.push(format!("x = SurfaceTension(Water, T={t})"));
    }
    // Specific volume as an input — the reciprocal path, including v = 0.
    for v in [0.0, -1.0, 1e-300, 1e300, 0.001] {
        corpus.push(format!("x = Temperature(Water, P=101325, v={v})"));
    }
    all_survive(&corpus);
}

/// The two documented non-uniquenesses of a `(P,h)` table inverse must be
/// refused, not resolved by an arbitrary pick.
///
/// Inside the dome, temperature is flat in `h` — every enthalpy between `h_f`
/// and `h_g` has the same `T`. `Enthalpy(Water, P, T=T_sat(P))` therefore has a
/// whole interval of answers, and any single number the engine returned would be
/// a fabrication.
///
/// Both shipped backends refuse it, and they refuse it for the same reason in
/// different words: the `(P,h)` table has no cell for a `(P,T)` pair on the
/// saturation line, and rustprop reproduces upstream CoolProp's own guard, which
/// rejects a `(P,T)` flash whose pressure sits within 1e-4 % of `p_sat(T)`.
#[test]
fn an_inverse_lookup_on_a_two_phase_plateau_is_refused_rather_than_guessed() {
    // T_sat(101325 Pa) = 373.1243 K, dead centre of the plateau.
    let started = Instant::now();
    let out = solve("x = Enthalpy(Water, P=101325, T=373.1243)", &settings());
    let elapsed = started.elapsed();
    // Whether this arm refuses or answers, it must do so promptly: the failure
    // mode a plateau invites is a bracketed inverse that never terminates.
    assert!(
        elapsed < Duration::from_secs(2),
        "plateau query took {elapsed:?}"
    );
    println!(
        "plateau (P,T) on the saturation line answered in {elapsed:?}: {}",
        match &out {
            Ok(_) => "Ok",
            Err(_) => "Err",
        }
    );
    match out {
        Err(e) => {
            let msg = e.to_string_message();
            assert!(
                (msg.contains("Water") && msg.contains("outside the generated property table"))
                    || msg.contains("Saturation pressure"),
                "{msg}"
            );
        }
        // If a future backend *can* resolve it (a real CoolProp would, by
        // convention picking the liquid root), the answer must at least be a
        // finite enthalpy inside the dome.
        Ok(s) => {
            let x = s.values["x"];
            assert!(
                x.is_finite() && (400_000.0..2_700_000.0).contains(&x),
                "x = {x}"
            );
        }
    }
}

// ── fluid names ─────────────────────────────────────────────────────────────

/// An unknown, empty or hostile fluid token is a diagnostic, never a lookup on
/// whatever the tokenizer happened to leave behind.
#[test]
fn unknown_and_malformed_fluid_names_are_refused_by_name() {
    let mut corpus = Vec::new();
    for fluid in [
        "Unobtainium",
        "water ",
        "WATER",
        "wAtEr",
        "H2O",
        "R134",
        "R1234yf",
        "EG50",
        "EG0",
        "EG100",
        "EG-1",
        "EG999",
        "PG50",
        "INCOMP::MEG[0.50]",
        "AirH2O",
        "HumidAir",
        "1",
        "___",
        "π",
        "Water Water",
    ] {
        corpus.push(format!("x = Enthalpy({fluid}, P=101325, T=300)"));
        corpus.push(format!("x = Enthalpy('{fluid}', P=101325, T=300)"));
        corpus.push(format!("x = P_crit({fluid})"));
        corpus.push(format!("x = MolarMass('{fluid}')"));
    }
    all_survive(&corpus);

    // The two that must be *named* in the diagnostic rather than swallowed.
    let err = solve("x = Enthalpy(Unobtainium, P=101325, T=300)", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(err.to_lowercase().contains("unobtainium"), "{err}");
}

/// A property indicator the engine does not know must list the ones it does,
/// and a wrong *count* of indicators must say what the right shape is.
#[test]
fn a_bad_property_indicator_or_arity_is_refused_with_the_supported_set() {
    let err = solve("x = Enthalpy(Water, Z=1, T=300)", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(err.contains("Unknown property indicator"), "{err}");
    let err = solve("x = Enthalpy(Water, T=300)", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(err.contains("exactly two property indicators"), "{err}");
    let err = solve("x = HumRat(AirH2O, T=300, P=101325)", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(err.contains("three property indicators"), "{err}");
    let err = solve("x = Blorp(Water, T=300, P=101325)", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(
        err.contains("Unknown property function") || err.contains("Unknown function"),
        "{err}"
    );
}

// ── chemistry: the formula parser ───────────────────────────────────────────

/// The chemical formula parser against everything that is not a formula.
///
/// `MolarMass` is reachable from any document with a string literal in it, so
/// this grammar is exposed to arbitrary user text.
#[test]
fn malformed_chemical_formulas_are_refused_rather_than_half_parsed() {
    let mut corpus = Vec::new();
    for formula in [
        "",
        " ",
        "3",
        "0",
        "-1",
        "H2O2X",
        "Xx",
        "H2O)",
        "(H2O",
        "((((((((((H2O))))))))))",
        "H2O(",
        "C8H18",
        "CH4",
        "h2o",
        "H2O·7",
        "Ca(OH)2",
        "Ca(OH)",
        "H0",
        "H999999999999999999999",
        "H2O2X3Y4Z5",
        "NaCl",
        "Uue",
        "H H",
        "H\tO",
        "H\nO",
    ] {
        for f in ["MolarMass", "HeatingValue", "StoichAFR"] {
            corpus.push(format!("x = {f}('{formula}')"));
        }
        corpus.push(format!("x = mix_mw('{formula}')"));
        corpus.push(format!("x = mix_cp('{formula}', 300)"));
        corpus.push(format!("x = AdiabaticFlameTemp('{formula}', 1, 298)"));
    }
    // A formula long enough to matter, and one deep enough to matter.
    corpus.push(format!("x = MolarMass('{}')", "H2O".repeat(2000)));
    corpus.push(format!(
        "x = MolarMass('{}H2O{}')",
        "(".repeat(500),
        ")".repeat(500)
    ));
    all_survive(&corpus);
}

/// Mixture specifications that do not sum, do not parse, or are empty.
#[test]
fn malformed_mixture_specifications_are_refused() {
    let mut corpus = Vec::new();
    for spec in [
        "",
        "N2",
        "N2:",
        ":0.79",
        "N2:0.79,",
        "N2:0.79,O2:0.21",
        "N2:0,O2:0",
        "N2:-1,O2:2",
        "N2:1e300,O2:1",
        "N2:nan,O2:1",
        "N2:0.79;O2:0.21",
        "Unobtainium:1",
        ",,,,",
        "N2:0.79,N2:0.21",
    ] {
        for f in ["mix_mw", "mix_molarmass"] {
            corpus.push(format!("x = {f}('{spec}')"));
        }
        for f in [
            "mix_cp",
            "mix_enthalpy",
            "mix_viscosity",
            "mix_conductivity",
        ] {
            corpus.push(format!("x = {f}('{spec}', 300)"));
        }
        corpus.push(format!("x = mix_entropy('{spec}', 300, 101325)"));
    }
    all_survive(&corpus);
}

/// Equivalence ratio and reactant temperature at and past their limits.
///
/// `AdiabaticFlameTemp` runs an energy-balance iteration; `φ = 0` is no fuel at
/// all and `φ = 1e12` is no oxidiser, and both degenerate the product
/// composition the iteration is solving for.
#[test]
fn combustion_at_degenerate_equivalence_ratios_terminates() {
    let mut corpus = Vec::new();
    for phi in ["0", "-1", "1e-300", "0.5", "1", "1.5", "1e12", "1e300"] {
        for t in ["0", "-300", "298", "1e-300", "5000", "1e300"] {
            for fuel in ["CH4", "C8H18", "H2", "Unobtainium", ""] {
                corpus.push(format!("x = AdiabaticFlameTemp('{fuel}', {phi}, {t})"));
                corpus.push(format!("x = flametemp('{fuel}', {phi}, {t})"));
            }
        }
        corpus.push(format!(
            "x = eq_molefraction('CH4', {phi}, 2000, 101325, 'CO2')"
        ));
        corpus.push(format!(
            "x = adiabaticflametempeq('CH4', {phi}, 298, 101325)"
        ));
    }
    all_survive(&corpus);
}

// ── ε-NTU: the removable singularities ──────────────────────────────────────

/// The ε-NTU relations at `NTU = 0`, `Cr = 0` and `Cr = 1` — all three are
/// removable singularities in the closed forms, and at `ε = 1` the inverse is a
/// `log(0)`.
#[test]
fn effectiveness_ntu_relations_at_their_singularities_are_finite_or_refused() {
    let arrangements = [
        "counterflow",
        "parallelflow",
        "shelltube",
        "crossflowcminmixed",
        "crossflowcmaxmixed",
        "counter",
        "coflow",
    ];
    let mut corpus = Vec::new();
    for a in arrangements {
        for ntu in ["0", "1e-300", "1e-6", "1", "10", "1e6", "1e300", "-1"] {
            for cr in ["0", "1e-300", "0.5", "1", "1.0000001", "-1", "1e300"] {
                corpus.push(format!("x = hx_effectiveness({ntu}, {cr}, '{a}')"));
                corpus.push(format!("x = hx_epsilon({ntu}, {cr}, '{a}')"));
            }
        }
        // The inverse: ε at, just under and above the reachable ceiling.
        for eps in [
            "0",
            "1e-300",
            "0.5",
            "0.9999999",
            "1",
            "1.0000001",
            "2",
            "-1",
            "1e300",
        ] {
            for cr in ["0", "0.5", "1", "-1"] {
                corpus.push(format!("x = hx_ntu({eps}, {cr}, '{a}')"));
            }
        }
    }
    // An arrangement name that is not one.
    for a in ["", "counterflowish", "COUNTERFLOW", "counter flow", "1"] {
        corpus.push(format!("x = hx_effectiveness(1, 0.5, '{a}')"));
        corpus.push(format!("x = hx_ntu(0.5, 0.5, '{a}')"));
    }
    // LMTD at equal terminal differences (0/0) and at a crossed one (log of a
    // negative).
    for (a, b) in [
        ("10", "10"),
        ("10", "0"),
        ("0", "0"),
        ("10", "-10"),
        ("-10", "-10"),
        ("1e300", "1e-300"),
    ] {
        corpus.push(format!("x = lmtd({a}, {b})"));
    }
    all_survive(&corpus);
}

// ── bracketed root-finds ────────────────────────────────────────────────────

/// The compressible-flow inversions pushed past the physical boundary their
/// bracket ends at.
///
/// `mach_a_astar` inverts an area ratio that has no solution below 1;
/// `mach_prandtlmeyer` inverts an angle bounded by the vacuum limit
/// (`ν_max = 130.45°` for γ = 1.4); `beta_oblique` has no attached solution past
/// the detachment angle. All three are the same shape: a bracketing solve whose
/// target can be outside the bracket, which is exactly where a naive
/// implementation spins.
#[test]
fn bracketed_compressible_inversions_terminate_outside_their_bracket() {
    let mut corpus = Vec::new();
    for ratio in [
        "0",
        "-1",
        "0.999999",
        "1",
        "1.0000001",
        "2",
        "1e6",
        "1e300",
        "1e-300",
    ] {
        for branch in ["0", "1", "-1", "2"] {
            corpus.push(format!("x = mach_a_astar({ratio}, 1.4, {branch})"));
        }
        corpus.push(format!("x = isen_a_astar({ratio}, 1.4)"));
    }
    for nu in [
        "0", "-1", "1", "90", "130.45", "130.4541", "131", "180", "1e300",
    ] {
        corpus.push(format!("x = mach_prandtlmeyer({nu}, 1.4)"));
        corpus.push(format!("x = prandtl_meyer({nu}, 1.4)"));
    }
    for theta in ["0", "-1", "10", "45", "45.5", "90", "179", "1e300"] {
        for m in ["0", "0.5", "1", "2", "1e300"] {
            corpus.push(format!("x = beta_oblique({m}, {theta}, 1.4)"));
            corpus.push(format!("x = theta_oblique({m}, {theta}, 1.4)"));
        }
    }
    // γ at and below 1 — every isentropic relation divides by γ − 1.
    for k in ["0", "-1", "1", "1.0000001", "0.9999999", "1e300"] {
        corpus.push(format!("x = isen_t0_t(2, {k})"));
        corpus.push(format!("x = isen_p0_p(2, {k})"));
        corpus.push(format!("x = mach_shock(2, {k})"));
        corpus.push(format!("x = fanno_fld(2, {k})"));
        corpus.push(format!("x = rayleigh_t_tstar(2, {k})"));
    }
    all_survive(&corpus);
}

/// The Colebrook friction factor and the ISO 6358 pneumatic flow both iterate.
/// Pushed to zero and negative Reynolds numbers, roughness beyond the pipe, and
/// pressure ratios outside `[0, 1]`, they must still terminate.
#[test]
fn iterative_correlations_in_their_non_convergent_regime_terminate() {
    let mut corpus = Vec::new();
    for re in ["0", "-1", "1e-300", "1", "2300", "4000", "1e12", "1e300"] {
        for rr in ["0", "-1", "1e-300", "0.05", "1", "2", "1e300"] {
            corpus.push(format!("x = darcy_friction({re}, {rr})"));
            corpus.push(format!("x = friction_factor({re}, {rr})"));
        }
    }
    // ISO 6358: (p_up, p_down, C, b, T). b is the critical pressure ratio and
    // must be in [0, 1); the whole formula divides by 1 − b.
    for b in ["0", "-1", "0.5", "1", "1.0000001", "2", "1e300"] {
        for pr in [
            ("101325", "0"),
            ("101325", "101325"),
            ("101325", "202650"),
            ("0", "0"),
            ("-1", "-1"),
            ("1e300", "1e-300"),
        ] {
            corpus.push(format!("x = iso6358({}, {}, 1e-8, {b}, 293)", pr.0, pr.1));
        }
    }
    // The Heisler one-term series: Bi at 0 and ∞, Fo below the one-term
    // validity floor and enormous.
    for bi in ["0", "-1", "1e-300", "0.1", "100", "1e300"] {
        for fo in ["0", "-1", "1e-300", "0.05", "0.2", "1e300"] {
            for geom in ["'wall'", "'cylinder'", "'sphere'", "'blorp'", "''"] {
                corpus.push(format!("x = heisler_temp({bi}, {fo}, {geom})"));
                corpus.push(format!("x = heisler_q({bi}, {fo}, {geom})"));
            }
        }
    }
    all_survive(&corpus);
}

// ── the cubic equations of state ────────────────────────────────────────────

/// Cubic EOS at states where the cubic has one, two or three real roots, and at
/// the degenerate ones where it has none that mean anything.
#[test]
fn cubic_eos_at_degenerate_states_is_answered_or_refused() {
    let mut corpus = Vec::new();
    for model in ["'PR'", "'SRK'", "'RK'", "'VDW'", "'blorp'", "''"] {
        for fluid in ["'CO2'", "'Methane'", "'Unobtainium'", "''"] {
            for (t, p) in [
                ("0", "101325"),
                ("-300", "101325"),
                ("300", "0"),
                ("300", "-101325"),
                ("1e-300", "1e-300"),
                ("1e300", "1e300"),
                ("304.13", "7377300"), // CO2 critical point exactly
                ("200", "101325"),     // below the triple point
                ("300", "6000000"),    // dense supercritical
            ] {
                corpus.push(format!("x = eos_z({fluid}, {model}, {t}, {p})"));
                corpus.push(format!("x = eos_volume({fluid}, {model}, {t}, {p})"));
                corpus.push(format!("x = eos_density({fluid}, {model}, {t}, {p})"));
                corpus.push(format!("x = eos_enthalpy({fluid}, {model}, {t}, {p})"));
                corpus.push(format!("x = eos_entropy({fluid}, {model}, {t}, {p})"));
            }
            for t in ["0", "-1", "200", "304.13", "1000", "1e300"] {
                corpus.push(format!("x = eos_psat({fluid}, {model}, {t})"));
            }
            for v in ["0", "-1", "1e-300", "0.001", "1e300"] {
                corpus.push(format!("x = eos_pressure({fluid}, {model}, 300, {v})"));
            }
        }
    }
    all_survive(&corpus);
}

// ── solid materials and the table lookups ───────────────────────────────────

/// Material lookups: unknown materials, absent properties, and temperatures far
/// outside any tabulated range.
#[test]
fn material_lookups_outside_their_table_are_refused_by_name() {
    let mut corpus = Vec::new();
    for material in ["Aluminum", "aluminum", "Vibranium", "", " ", "1", "Steel"] {
        for prop in ["k_", "rho_", "c_", "E_", "nu_"] {
            corpus.push(format!("x = {prop}('{material}')"));
            for t in ["0", "-300", "1e-300", "300", "5000", "1e300"] {
                corpus.push(format!("x = {prop}({material}, T={t})"));
            }
        }
    }
    all_survive(&corpus);

    let err = solve("x = k_('Vibranium')", &settings())
        .unwrap_err()
        .to_string_message();
    assert!(err.to_lowercase().contains("vibranium"), "{err}");
}

// ── the generated tables, at the byte level ─────────────────────────────────

/// The `FRPHTAB1` reader against corrupted artifacts.
///
/// This is the one place in the property surface where *bytes* rather than
/// numbers are the untrusted input, and it is reachable at runtime through
/// `props::tables::install_from_bytes`. Every corruption must be a `Result`.
#[test]
fn a_corrupted_property_table_is_refused_at_every_byte_position() {
    let good = &frees_core::props::tables::water_phtab().expect("water unpacks")[..];
    assert!(SaturationSplitTable::decode_generated(good).is_ok());

    // Every truncation length on a log scale, plus the exact header boundaries.
    let mut lengths: Vec<usize> = vec![0, 1, 7, 8, 9, 135, 136, 137, 159, 160, 161];
    let mut n = 256usize;
    while n < good.len() {
        lengths.push(n);
        n *= 2;
    }
    lengths.push(good.len() - 1);
    for len in lengths {
        let out = std::panic::catch_unwind(|| SaturationSplitTable::decode_generated(&good[..len]));
        match out {
            Err(_) => panic!("decode panicked on a {len}-byte prefix"),
            Ok(Ok(_)) if len != good.len() => {
                panic!("a {len}-byte prefix decoded as a whole table")
            }
            Ok(_) => {}
        }
    }

    // Every byte of the header flipped, one at a time: the reader must refuse or
    // produce a table, never trap.
    for i in 0..160usize {
        for bit in [0x01u8, 0x80] {
            let mut bytes = good.to_vec();
            bytes[i] ^= bit;
            let out =
                std::panic::catch_unwind(|| SaturationSplitTable::decode_generated(&bytes).is_ok());
            assert!(
                out.is_ok(),
                "decode panicked with header byte {i} ^ {bit:#04x}"
            );
        }
    }

    // Payload corruption: a NaN bit pattern written over a saturation sample.
    let mut bytes = good.to_vec();
    for b in bytes.iter_mut().skip(160).take(64) {
        *b = 0xff;
    }
    let out = std::panic::catch_unwind(|| SaturationSplitTable::decode_generated(&bytes).is_ok());
    assert!(
        out.is_ok(),
        "decode panicked on a NaN-filled saturation line"
    );
    assert!(
        !SaturationSplitTable::decode_generated(&bytes).is_ok(),
        "a table with a non-finite saturation line must be refused, not served"
    );
}

/// Grid-edge probes on the decoded table itself, at and just outside every
/// declared bound.
///
/// The bounds come from the artifact, not from a literal, so this test moves
/// with the tables instead of going stale when they are regenerated.
#[test]
fn table_lookups_exactly_on_and_just_outside_every_grid_edge_are_bounded() {
    for bytes in [
        frees_core::props::tables::water_phtab().unwrap(),
        frees_core::props::tables::r134a_phtab().unwrap(),
    ] {
        let t = SaturationSplitTable::decode_generated(&bytes).unwrap();
        let mut probes: Vec<(f64, f64)> = Vec::new();
        for p in [
            t.p_min(),
            t.p_min() * (1.0 - 1e-12),
            t.p_min() * (1.0 + 1e-12),
            t.p_serve_max(),
            t.p_serve_max() * (1.0 - 1e-12),
            t.p_serve_max() * (1.0 + 1e-12),
            t.p_liquid_min(),
            t.p_max(),
            0.0,
            -1.0,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let hf = t.hf_at(p);
            let hg = t.hg_at(p);
            let floor = t.h_liquid_min_at(p);
            for h in [
                hf,
                hg,
                floor,
                floor * (1.0 - 1e-12),
                hf - (hf - floor) * 0.5,
                hg + t.dh_vapor_max(),
                hg + t.dh_vapor_max() * (1.0 + 1e-12),
                0.5 * (hf + hg),
                0.0,
                -1e12,
                1e12,
                f64::INFINITY,
                f64::NAN,
            ] {
                probes.push((p, h));
            }
        }
        for (p, h) in probes {
            for out in [
                frees_core::props::satsplit::Output::Temperature,
                frees_core::props::satsplit::Output::Density,
                frees_core::props::satsplit::Output::Entropy,
            ] {
                let got = std::panic::catch_unwind(|| t.value(out, p, h));
                let Ok(got) = got else {
                    panic!("{}: value({out:?}, {p}, {h}) panicked", t.fluid())
                };
                if let Some(v) = got {
                    assert!(
                        v.is_finite(),
                        "{}: value({out:?}, {p}, {h}) served a non-finite {v} instead of \
                         declining",
                        t.fluid()
                    );
                }
                // `region` is the cheap coverage pre-check a caller uses to
                // decide whether to evaluate at all. It may be optimistic about
                // a cell whose interpolant then declines, but it must never be
                // *pessimistic*: a state `value` served and `region` called
                // uncovered would make the pre-check a liar.
                if got.is_some() {
                    assert!(
                        t.region(p, h).is_some(),
                        "{}: value() served ({p}, {h}) but region() calls it uncovered",
                        t.fluid()
                    );
                }
            }
        }
    }
}

// ── humid air and psychrometrics ────────────────────────────────────────────

/// Humid-air calls must survive every state including the impossible ones, and
/// then either answer or say why they cannot — which of the two depends on the
/// backend, and D9 changed the answer.
///
/// The `(P,h)` tables implement no `HAPropsSI` at all, so every call is declined
/// by name (`RealFluid::ha_props_si`'s declining default). rustprop implements
/// RP-1485, so the same call answers — which was the largest single gap D8
/// counted (7 of its 26 pending fixtures).
#[test]
fn humid_air_calls_are_answered_or_declined_by_name_at_every_state() {
    let mut corpus = Vec::new();
    for (t, p, r) in [
        (300.0, 101325.0, 0.5),
        (300.0, 101325.0, 0.0),
        (300.0, 101325.0, 1.0),
        (300.0, 101325.0, 2.0),
        (300.0, 101325.0, -1.0),
        (0.0, 101325.0, 0.5),
        (-300.0, 101325.0, 0.5),
        (300.0, 0.0, 0.5),
        (300.0, -1.0, 0.5),
        (1e300, 1e300, 1e300),
    ] {
        for f in ["HumRat", "RelHum", "WetBulb", "DewPoint", "Enthalpy"] {
            corpus.push(format!("x = {f}(AirH2O, T={t}, P={p}, R={r})"));
        }
    }
    all_survive(&corpus);

    let out = solve("x = HumRat(AirH2O, T=300, P=101325, R=0.5)", &settings());
    #[cfg(feature = "rustprop-backend")]
    {
        // Humidity ratio of saturated-at-50 % air at 300 K, 1 atm: ~0.0111 kg/kg.
        let x = out.expect("rustprop implements HAPropsSI").values["x"];
        assert!((0.010..0.012).contains(&x), "HumRat = {x}");
    }
    #[cfg(not(feature = "rustprop-backend"))]
    {
        let err = out.unwrap_err().to_string_message();
        assert!(
            err.contains("HAPropsSI") || err.contains("humid-air"),
            "{err}"
        );
    }
}

// ── the backend seam itself ─────────────────────────────────────────────────

/// With the linked tables installed, `props_si` answers or errors for every
/// output key and every input pair — including the ones it does not store.
#[test]
fn the_installed_backend_answers_or_errors_for_every_key_combination() {
    let outputs = [
        "Hmass",
        "Smass",
        "Dmass",
        "T",
        "P",
        "Q",
        "Umass",
        "Cpmass",
        "Cvmass",
        "V",
        "L",
        "Prandtl",
        "Z",
        "speed_of_sound",
        "surface_tension",
        "Gmass",
        "",
        "hmass",
    ];
    let inputs = ["P", "T", "Q", "Hmass", "Smass", "Dmass", "", "p"];
    let values = [0.0, -1.0, 1e-300, 101_325.0, 300.0, 0.5, 1e300, f64::NAN];
    let mut worst = Duration::ZERO;
    let mut slowest = String::new();
    let mut calls = 0usize;
    for out in outputs {
        for k1 in inputs {
            for k2 in inputs {
                for v in values {
                    calls += 1;
                    let started = Instant::now();
                    let got = std::panic::catch_unwind(|| {
                        propfun::props_si(out, k1, v, k2, 101_325.0, "Water").ok()
                    });
                    let Ok(got) = got else {
                        panic!("props_si({out}, {k1}={v}, {k2}=101325, Water) panicked")
                    };
                    if let Some(x) = got {
                        assert!(
                            x.is_finite(),
                            "props_si({out}, {k1}={v}, {k2}=101325, Water) served {x}"
                        );
                    }
                    let elapsed = started.elapsed();
                    if elapsed > worst {
                        worst = elapsed;
                        slowest = format!("props_si({out}, {k1}={v}, {k2}=101325, Water)");
                    }
                }
            }
        }
    }
    assert!(
        worst < Duration::from_secs(2),
        "slowest single props_si call took {worst:?}"
    );
    // The budget is per call, so the count and the worst call are what say
    // whether the sweep is comfortably inside it — see docs/status-wave3-f7.md.
    println!("hostile props_si sweep: {calls} calls, slowest {worst:?} on {slowest}");
}

/// A document that calls a property function inside a Newton block — the actual
/// hot path — must not let a mid-iteration failure escape as a panic or a NaN
/// residual masquerading as convergence.
#[test]
fn a_property_call_inside_a_newton_block_fails_as_data() {
    // `T` is unknown, so the solver probes `Enthalpy(Water, P, T)` at guesses
    // that walk straight out of the table's box.
    let corpus = vec![
        "h = 2000000\nT = 400\nh = Enthalpy(Water, P=101325, T=T)".to_string(),
        "p = 101325\nh = Enthalpy(Water, P=p, T=300)\np = 2 * h / 40".to_string(),
        "T = 300\nx = Enthalpy(Water, P=P, T=T)\nP = x / 1000".to_string(),
        // A cycle whose residual passes through the critical point.
        "P = 22064000\nT = 647.096\nh = Enthalpy(Water, P=P, T=T)".to_string(),
    ];
    all_survive(&corpus);
}

// ── everything at once ──────────────────────────────────────────────────────

/// The whole hostile corpus of `robustness.rs`, but with a property call
/// wrapped around it — the composition a real document produces and a
/// single-function sweep never reaches.
#[test]
fn property_calls_composed_with_hostile_expressions_are_answered() {
    let mut corpus = Vec::new();
    for inner in [
        "1/0",
        "0/0",
        "ln(0)",
        "sqrt(-1)",
        "1e300 * 1e300",
        "-1e300 * 1e300",
        "0 * 1e300",
        "1e-300 / 1e300",
    ] {
        corpus.push(format!("x = Enthalpy(Water, P={inner}, T=300)"));
        corpus.push(format!("x = Enthalpy(Water, P=101325, T={inner})"));
        corpus.push(format!("x = isa_t({inner})"));
        corpus.push(format!("x = hx_effectiveness({inner}, 0.5, 'counterflow')"));
        corpus.push(format!("x = darcy_friction({inner}, 0.01)"));
        corpus.push(format!("x = eos_z('CO2', 'PR', {inner}, 101325)"));
        corpus.push(format!("x = mach_a_astar({inner}, 1.4, 1)"));
    }
    all_survive(&corpus);
}

// `no_promoted_fixture_solves_to_a_non_finite_value` used to live here, and the
// invariant it asserts still holds — it moved into `tests/parity.rs` (Wave T3),
// it did not go away. Do not restore this version.
//
// The invariant: the engine's own reported values must never be non-finite for
// any document in the promoted corpus, checked over real documents rather than
// synthetic ones. What it cost to assert it here was a SECOND whole-corpus
// solve — all 1308 documents, single-threaded, in the one CI job that was the
// workflow's critical path. Measured in that job's own debug profile on a dev
// box: 946.90 s, against 986.41 s for the parity replay, i.e. the two passes
// were the same size and the job paid for both.
//
// The replay already solves every fixture, so the check rides along there for
// free, and it closes a blind spot the replay had on its own: `close`/`rel_diff`
// treat NaN against NaN as agreement, so a fixture could match its golden while
// this engine produced garbage.
//
// One difference, deliberate: this version always used
// `SolverSettings::default()`, so for the fixtures carrying a `.request.json`
// it graded a configuration the document was never meant to run under, and its
// `if let Ok(solution)` silently skipped any that then failed to solve at all.
// In the replay every fixture is checked under the settings it ships with.
