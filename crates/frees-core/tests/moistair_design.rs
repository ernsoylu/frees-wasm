//! Behavioural gates for the coils, the airside economizer, the terminal units
//! and the system blocks that a psychrometric design is actually built from.
//!
//! Same doctrine as `moistair_recovery.rs`: assert the *defining property* of
//! each component, not a golden number. A cooling coil that quietly stops
//! satisfying the three equivalent bypass-factor forms, a wrap-around loop that
//! starts creating a little energy, or an induction terminal that begins
//! condensing are all still dimensionally consistent and would pass a residual
//! check — they just would not be the device any more.
//!
//! Requires the `rustprop-backend` feature: every equation here runs through
//! `HAPropsSI`, and without a humid-air backend the documents cannot solve.

#![cfg(feature = "rustprop-backend")]

use frees_core::props::propfun;
use frees_core::props::rustprop_backend::RustpropBackend;
use frees_core::{solve, SolverSettings};
use std::sync::Arc;

fn with_backend() {
    if propfun::backend().is_none() {
        propfun::install(Arc::new(RustpropBackend));
    }
}

fn solved(source: &str) -> std::collections::BTreeMap<String, f64> {
    with_backend();
    let solution =
        solve(source, &SolverSettings::default()).unwrap_or_else(|e| panic!("did not solve: {e}"));
    solution.values.into_iter().collect()
}

fn get(values: &std::collections::BTreeMap<String, f64>, name: &str) -> f64 {
    *values
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no variable {name}; have {:?}", values.keys()))
        .1
}

// ── The apparatus dew point coil ────────────────────────────────────────────

/// The bypass factor has three textbook definitions — on temperature, on
/// humidity ratio and on enthalpy — and every psychrometrics text asserts they
/// are the same number. They are the same only because the leaving state is a
/// straight-line interpolation toward the ADP, so this is the property that
/// makes a coil result reproducible against any book.
///
/// The W and h forms hold by construction. The temperature form does not: dry
/// bulb is recovered from `(h, W)` and both move along the line, so `t` is a
/// ratio of two linear functions rather than a linear one. It lands within a
/// fifth of a percent anyway, which is *why* the texts treat all three as
/// interchangeable — and pinning that here is what would catch someone
/// "simplifying" the construction into something that only satisfies one.
#[test]
fn apparatus_dew_point_coil_satisfies_all_three_bypass_factor_forms() {
    let values = solved(
        r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0112, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(T_adp=283.15, BF=0.12)
connect(EN.out, CC.in)
connect(CC.out, S1.in)
T_lvg = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
W_adp = HumRat(AirH2O, T=283.15, P=101325, R=1)
h_adp = Enthalpy(AirH2O, T=283.15, P=101325, W=W_adp)
h_ent = Enthalpy(AirH2O, T=300.15, P=101325, W=0.0112)
bf_t  = (T_lvg - 283.15) / (300.15 - 283.15)
bf_w  = (S1.W - W_adp) / (0.0112 - W_adp)
bf_h  = (S1.h - h_adp) / (h_ent - h_adp)
"#,
    );

    for form in ["bf_w", "bf_h"] {
        assert!(
            (get(&values, form) - 0.12).abs() < 1e-9,
            "{form} = {}, not the 0.12 it was built from",
            get(&values, form)
        );
    }
    let bf_t = get(&values, "bf_t");
    assert!(
        (bf_t - 0.12).abs() < 5e-3,
        "the temperature form drifted to {bf_t}; the three are supposed to agree"
    );
}

/// The condensate leaves the airstream carrying its own enthalpy, so the load
/// the plant sees is not the air-side enthalpy drop. The term is under one part
/// in a hundred — which is exactly why it gets dropped silently — so both
/// numbers are reported rather than conflated.
#[test]
fn apparatus_dew_point_coil_credits_the_condensate_enthalpy() {
    let values = solved(
        r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0112, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(T_adp=283.15, BF=0.12)
connect(EN.out, CC.in)
connect(CC.out, S1.in)
q_air  = CC.Q_air
q_load = CC.Q
shr    = CC.SHR
m_cond = CC.mdot_w
"#,
    );

    let q_air = get(&values, "q_air");
    let q_load = get(&values, "q_load");
    assert!(q_air > 0.0 && q_load > 0.0, "the coil should be cooling");
    assert!(
        q_load < q_air,
        "the condensate credit must reduce the load, not raise it"
    );
    let credit = (q_air - q_load) / q_air;
    assert!(
        (0.002..0.02).contains(&credit),
        "condensate credit {credit:.4} of the total is outside the expected sub-1% band"
    );

    // Real condensate, and a sensible heat ratio in the range a comfort coil
    // works in — not 1.0, which would mean the coil never got wet.
    assert!(get(&values, "m_cond") > 0.0);
    let shr = get(&values, "shr");
    assert!(
        (0.5..0.9).contains(&shr),
        "SHR = {shr:.3} is not a comfort-cooling coil"
    );
}

/// A face-and-bypass coil modulates by DAMPER, so its bypass factor is a control
/// variable rather than a coil characteristic. At full face the leaving air sits
/// on the saturation line; at half face the same coil, at the same water
/// temperature, leaves it far off it — a state no single-path coil produces.
#[test]
fn face_and_bypass_coil_modulates_by_damper_not_by_water() {
    let source = |u_face: f64| {
        format!(
            r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0112, mdot=1.0)
MoistAirSink   S1()
ThermalSource  WT(T=280.15)
FaceAndBypassCoil FB(u_face={u_face}, eps=0.90)
connect(EN.out, FB.in)
connect(FB.out, S1.in)
connect(WT.port, FB.wall)
rh_out = RelHum(AirH2O, h=S1.h, P=101325, W=S1.W)
bf     = FB.BF
q      = FB.Q
"#
        )
    };

    let full = solved(&source(1.0));
    let half = solved(&source(0.5));

    // The bypass factor is one minus the damper position, exactly.
    assert!((get(&full, "bf") - 0.0).abs() < 1e-12);
    assert!((get(&half, "bf") - 0.5).abs() < 1e-12);

    assert!(
        get(&full, "rh_out") > 0.98,
        "full face on a coil this cold should leave saturated air, got RH {}",
        get(&full, "rh_out")
    );
    assert!(
        get(&half, "rh_out") < 0.85,
        "half face should leave the air well off saturation, got RH {}",
        get(&half, "rh_out")
    );

    // And half the face is roughly half the duty — the damper IS the capacity.
    let ratio = get(&half, "q") / get(&full, "q");
    assert!(
        (0.45..0.55).contains(&ratio),
        "half face gave {ratio:.3} of the full-face duty"
    );
}

// ── The heat-pipe wrap-around ───────────────────────────────────────────────

/// A wrap-around loop is PASSIVE. Whatever it takes out of the air entering the
/// coil it must put back into the air leaving it — exactly, not nearly. A
/// residual here would be energy the device invented, which is the one thing it
/// cannot do, and it is why the reheat leg is closed on the precool leg's
/// enthalpy rather than on a second effectiveness.
#[test]
fn heat_pipe_wrap_around_returns_exactly_the_heat_it_removed() {
    let values = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0170, mdot=1.0)
MoistAirSink   S1()
HeatPipeWrapAround HP(eff=0.55)
ApparatusDewPointCoil CC(T_adp=282.15, BF=0.10)
connect(OA.out, HP.pre_in)
connect(HP.pre_out, CC.in)
connect(CC.out, HP.re_in)
connect(HP.re_out, S1.in)
h_oa    = Enthalpy(AirH2O, T=308.15, P=101325, W=0.0170)
q_pre   = 1.0 * (h_oa - HP.pre_out.h)
q_re    = 1.0 * (S1.h - HP.re_in.h)
T_coil  = Temperature(AirH2O, h=HP.re_in.h, P=101325, W=HP.re_in.W)
T_sup   = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
dW_pre  = HP.pre_out.W - 0.0170
dW_re   = S1.W - HP.re_in.W
"#,
    );

    let q_pre = get(&values, "q_pre");
    let q_re = get(&values, "q_re");
    assert!(q_pre > 100.0, "the precool leg did nothing: {q_pre} W");
    assert!(
        (q_pre - q_re).abs() / q_pre < 1e-9,
        "the loop is not passive: took {q_pre} W out, put {q_re} W back"
    );

    // Both legs are sensible: a heat pipe transfers no mass.
    assert!(get(&values, "dW_pre").abs() < 1e-12);
    assert!(get(&values, "dW_re").abs() < 1e-12);

    // And the reheat is real — the supply leaves warmer than the coil left it,
    // with no energy input anywhere in the document.
    assert!(
        get(&values, "T_sup") > get(&values, "T_coil") + 1.0,
        "no reheat: coil left {} K, supply left {} K",
        get(&values, "T_coil"),
        get(&values, "T_sup")
    );
}

/// The design claim for a wrap-around: the coil sees air already precooled
/// toward its dew point, so more of its duty goes to moisture. The coil's own
/// sensible heat ratio has to fall.
#[test]
fn heat_pipe_wrap_around_shifts_the_coil_toward_latent_duty() {
    let bare = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0170, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(T_adp=282.15, BF=0.10)
connect(OA.out, CC.in)
connect(CC.out, S1.in)
shr = CC.SHR
"#,
    );
    let wrapped = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0170, mdot=1.0)
MoistAirSink   S1()
HeatPipeWrapAround HP(eff=0.55)
ApparatusDewPointCoil CC(T_adp=282.15, BF=0.10)
connect(OA.out, HP.pre_in)
connect(HP.pre_out, CC.in)
connect(CC.out, HP.re_in)
connect(HP.re_out, S1.in)
shr = CC.SHR
"#,
    );

    let bare_shr = get(&bare, "shr");
    let wrapped_shr = get(&wrapped, "shr");
    assert!(
        wrapped_shr < bare_shr - 0.05,
        "the wrap-around should shift the coil toward latent duty: \
         SHR went {bare_shr:.3} -> {wrapped_shr:.3}"
    );
}

// ── Liquid desiccant ────────────────────────────────────────────────────────

/// An internally cooled contactor removes the heat of absorption as it is
/// released. Hold the solution at the entering air temperature and the leaving
/// air is at that temperature too — the process runs STRAIGHT DOWN a constant
/// dry-bulb line, which is the dehumidification-only vector, and the only device
/// in the library that draws it. Gatley uses exactly this machine, a sprayed
/// lithium-chloride coil, as the textbook example of the process.
#[test]
fn cooled_liquid_desiccant_draws_the_dehumidification_only_vector() {
    let values = solved(
        r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0130, mdot=1.0)
MoistAirSink   S1()
ThermalSource  SOL(T=300.15)
LiquidDesiccantContactor LD(model$=cooled, eff_L=0.70, W_eq=0.0040, eps_T=0.85, f_excess=0)
connect(EN.out, LD.in)
connect(LD.out, S1.in)
connect(SOL.port, LD.wall)
T_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
q     = LD.Q
"#,
    );

    // Straight down: the dry bulb does not move at all.
    let t_out = get(&values, "T_out");
    assert!(
        (t_out - 300.15).abs() < 1e-9,
        "the dehumidification-only vector is vertical; dry bulb moved to {t_out}"
    );
    // And the air is drier by the latent effectiveness against W_eq.
    let expected = 0.0130 - 0.70 * (0.0130 - 0.0040);
    assert!((get(&values, "s1$w") - expected).abs() < 1e-12);
    // The coolant carries the whole heat of absorption away — that is what makes
    // the vector vertical instead of sloping right.
    assert!(
        get(&values, "q") > 5000.0,
        "the coolant should be removing the absorption heat, got {} W",
        get(&values, "q")
    );
}

/// The adiabatic contactor is the other machine entirely: nothing leaves through
/// the wall, so the heat of absorption stays in the air and it leaves WARMER and
/// drier. `f_excess` is Gatley's observation that the real dry-bulb rise runs
/// 20-30% above what pure latent-to-sensible conversion predicts, written as
/// what he says it is — a multiplier on the gain, not a fudge on the enthalpy.
#[test]
fn adiabatic_liquid_desiccant_leaves_the_air_warmer_than_pure_conversion() {
    let source = |f_excess: f64| {
        format!(
            r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0130, mdot=1.0)
MoistAirSink   S1()
ThermalSource  SOL(T=300.15)
LiquidDesiccantContactor LD(model$=adiabatic, eff_L=0.70, W_eq=0.0040, eps_T=0, f_excess={f_excess})
connect(EN.out, LD.in)
connect(LD.out, S1.in)
connect(SOL.port, LD.wall)
T_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
q     = LD.Q
"#
        )
    };

    let pure = solved(&source(0.0));
    let real = solved(&source(0.25));

    let rise_pure = get(&pure, "T_out") - 300.15;
    let rise_real = get(&real, "T_out") - 300.15;

    // Warmer and drier: the mirror image of evaporative cooling.
    assert!(
        rise_pure > 8.0,
        "pure conversion of this much latent heat should warm the air by ~10 K, got {rise_pure:.2}"
    );
    assert!(get(&pure, "s1$w") < 0.0130);

    // f_excess is a 25% larger gain, exactly.
    assert!(
        (rise_real / rise_pure - 1.25).abs() < 1e-6,
        "f_excess = 0.25 should give a 25% larger rise: {rise_pure:.3} -> {rise_real:.3}"
    );

    // Adiabatic means adiabatic.
    assert!(get(&pure, "q").abs() < 1e-12);
}

// ── Two-stage evaporative cooling ───────────────────────────────────────────

/// A direct evaporative cooler can never go below the entering wet bulb. The way
/// past that limit is to move the wet bulb first: the indirect stage cools the
/// primary air sensibly, which lowers its wet bulb, and the direct stage then
/// runs up the LOWER wet-bulb line. Staged must beat direct-only at the same
/// entering state and the same direct-stage effectiveness, or there is no reason
/// to build one.
#[test]
fn two_stage_evaporative_cooling_beats_a_single_direct_stage() {
    let staged = solved(
        r#"
MoistAirSource PA(P=101325, T=308.15, W=0.0080, mdot=1.0)
MoistAirSource SA(P=101325, T=308.15, W=0.0080, mdot=1.0)
MoistAirSink   S1()
MoistAirSink   S2()
IndirectDirectEvaporativeCooler ID(wbde=0.70, eff_sec=0.85, eff_dir=0.85)
connect(PA.out, ID.pri_in)
connect(ID.pri_out, S1.in)
connect(SA.out, ID.sec_in)
connect(ID.sec_out, S2.in)
T_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
"#,
    );
    let direct = solved(
        r#"
MoistAirSource PA(P=101325, T=308.15, W=0.0080, mdot=1.0)
MoistAirSink   S1()
EvaporativeCooler DEC(eff=0.85)
connect(PA.out, DEC.in)
connect(DEC.out, S1.in)
T_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
T_wb  = WetBulb(AirH2O, T=308.15, P=101325, W=0.0080)
"#,
    );

    let t_staged = get(&staged, "T_out");
    let t_direct = get(&direct, "T_out");
    let t_wb = get(&direct, "T_wb");

    assert!(
        t_staged < t_direct - 3.0,
        "staging should buy several kelvin: staged {:.2} C vs direct {:.2} C",
        t_staged - 273.15,
        t_direct - 273.15
    );
    // And it goes below the limit a single direct stage is stuck behind.
    assert!(
        t_staged < t_wb,
        "the point of staging is passing the entering wet bulb of {:.2} C; got {:.2} C",
        t_wb - 273.15,
        t_staged - 273.15
    );
    // The primary is still wetted — this is not free.
    assert!(get(&staged, "s1$w") > 0.0080);
}

// ── The airside economizer ──────────────────────────────────────────────────

/// An economizer is gated TWICE: the differential test against the return air,
/// and a fixed high limit above which it locks out regardless. Failing to AND
/// the two is the classic economizer fault — dampers wide open on a hot day
/// because the return air happens to be hotter still.
#[test]
fn economizer_opens_for_free_cooling_and_locks_out_at_the_high_limit() {
    let source = |t_oa: f64, t_ret: f64, lim: f64| {
        format!(
            r#"
MoistAirSink S1()
Economizer EC(model$=drybulb, mdot_sup=1.0, f_min=0.20, lim={lim}, band=0.5)
connect(EC.mix_out, S1.in)
EC.oa_in.P  = 101325
EC.oa_in.W  = 0.0060
EC.oa_in.h  = Enthalpy(AirH2O, T={t_oa}, P=101325, W=0.0060)
EC.ret_in.P = 101325
EC.ret_in.W = 0.0093
EC.ret_in.h = Enthalpy(AirH2O, T={t_ret}, P=101325, W=0.0093)
f_oa = EC.f_oa
"#
        )
    };

    // Cool, dry outdoor air under the high limit: dampers wide open.
    let free = solved(&source(288.15, 297.15, 297.15));
    assert!(
        get(&free, "f_oa") > 0.99,
        "15 C outdoor air against 24 C return should give full economizer, got {}",
        get(&free, "f_oa")
    );

    // Hot outdoor air: back to the ventilation minimum.
    let hot = solved(&source(303.15, 297.15, 297.15));
    assert!(
        (get(&hot, "f_oa") - 0.20).abs() < 1e-3,
        "30 C outdoor air should close to f_min, got {}",
        get(&hot, "f_oa")
    );

    // The case that separates a correct economizer from a naive one: outdoor air
    // is cooler than the return, so the differential test says open — but it is
    // above the fixed high limit, so it must stay shut anyway.
    let over_limit = solved(&source(299.15, 303.15, 297.15));
    assert!(
        (get(&over_limit, "f_oa") - 0.20).abs() < 1e-3,
        "26 C outdoor air is over a 24 C high limit and must lock out even though \
         the return is hotter; got f_oa = {}",
        get(&over_limit, "f_oa")
    );
}

/// Enthalpy changeover sees what dry-bulb changeover cannot: outdoor air that is
/// cooler than the return and yet carries more total energy, because it is wet.
/// Admitting it costs the coil more than recirculating would. Same air, same
/// component, two strategies, opposite decisions.
#[test]
fn enthalpy_changeover_rejects_humid_air_that_dry_bulb_changeover_admits() {
    let source = |model: &str, lim: f64, band: f64| {
        format!(
            r#"
MoistAirSink S1()
Economizer EC(model$={model}, mdot_sup=1.0, f_min=0.20, lim={lim}, band={band})
connect(EC.mix_out, S1.in)
EC.oa_in.P  = 101325
EC.oa_in.W  = 0.0160
EC.oa_in.h  = Enthalpy(AirH2O, T=297.15, P=101325, W=0.0160)
EC.ret_in.P = 101325
EC.ret_in.W = 0.0093
EC.ret_in.h = Enthalpy(AirH2O, T=299.15, P=101325, W=0.0093)
f_oa = EC.f_oa
"#
        )
    };

    // Dry bulb: 24 C outdoor against 26 C return, under a 27 C limit -> open.
    let db = solved(&source("drybulb", 300.15, 0.5));
    assert!(
        get(&db, "f_oa") > 0.95,
        "dry-bulb changeover should admit this air, got {}",
        get(&db, "f_oa")
    );

    // Enthalpy: the same air carries about 15 kJ/kg MORE than the return, so a
    // strategy that counts the latent penalty shuts the dampers.
    let h = solved(&source("enthalpy", 70000.0, 1000.0));
    assert!(
        (get(&h, "f_oa") - 0.20).abs() < 1e-3,
        "enthalpy changeover should reject humid outdoor air, got {}",
        get(&h, "f_oa")
    );
}

// ── Terminal units ──────────────────────────────────────────────────────────

/// An induction terminal has no drain and is not built to have one, so its
/// secondary coil must run dry. The induced stream's humidity ratio is carried
/// through untouched and `margin_dp` is what makes the design checkable rather
/// than assumed: negative means the terminal is condensing into the occupied
/// space.
#[test]
fn induction_unit_keeps_its_secondary_coil_dry_and_reports_its_margin() {
    let source = |t_water: f64| {
        format!(
            r#"
MoistAirSink  S1()
ThermalSource WT(T={t_water})
InductionUnit IU(ratio=3.0, eps=0.55)
connect(IU.out, S1.in)
connect(WT.port, IU.wall)
IU.pri_in.P    = 101325
IU.pri_in.W    = 0.0070
IU.pri_in.h    = Enthalpy(AirH2O, T=286.15, P=101325, W=0.0070)
IU.pri_in.mdot = 0.03
IU.ind_in.P    = 101325
IU.ind_in.W    = 0.0093
IU.ind_in.h    = Enthalpy(AirH2O, T=297.15, P=101325, W=0.0093)
m_ind  = IU.ind_in.mdot
m_out  = S1.mdot
W_mix  = (0.03 * 0.0070 + 3.0 * 0.03 * 0.0093) / (0.03 + 3.0 * 0.03)
dW     = S1.W - W_mix
margin = IU.margin_dp
"#
        )
    };

    // Water above the room dew point: the design case.
    let dry = solved(&source(289.15));

    // The induction ratio sets the secondary flow off the primary — no fan.
    assert!((get(&dry, "m_ind") - 0.09).abs() < 1e-12);
    assert!((get(&dry, "m_out") - 0.12).abs() < 1e-12);

    // The coil moves heat only, so the leaving humidity is the pure mix of the
    // two streams and nothing else.
    assert!(
        get(&dry, "dW").abs() < 1e-12,
        "the secondary coil moved moisture it has no drain for: {}",
        get(&dry, "dW")
    );
    assert!(
        get(&dry, "margin") > 0.0,
        "16 C water under a 12.9 C room dew point should be a positive margin"
    );

    // Cold water: the margin has to go negative rather than the terminal
    // silently condensing onto the ceiling.
    let wet = solved(&source(283.15));
    assert!(
        get(&wet, "margin") < 0.0,
        "10 C water under a 12.9 C dew point must report a negative margin"
    );
}

/// A radiant panel conditions a space without moving air, so it draws no process
/// vector at all — it couples the zone to the water and shows up on a
/// psychrometric design as sensible load the air system no longer carries.
///
/// One signed characteristic covers both directions: the same panel heats when
/// the water is above the room and cools when it is below, and the equation must
/// not have to be swapped between the two.
#[test]
fn radiant_panel_signs_its_own_direction_and_guards_its_dew_point() {
    let source = |t_zone: f64, t_water: f64| {
        format!(
            r#"
ThermalSource ZN(T={t_zone})
ThermalSource WT(T={t_water})
RadiantPanel RP(A=10, C=8.92, n=1.1, eps_dT=0.01, W_room=0.0093, P_room=101325)
connect(ZN.port, RP.zone)
connect(WT.port, RP.wall)
Q      = RP.Q
margin = RP.margin_dp
"#
        )
    };

    // Chilled ceiling: 26 C room, 16 C water. Heat leaves the zone.
    let cooling = solved(&source(299.15, 289.15));
    let q_cool = get(&cooling, "Q");
    assert!(q_cool > 0.0, "a chilled ceiling removes heat: {q_cool} W");
    assert!(
        (60.0..1600.0).contains(&q_cool),
        "{q_cool} W over 10 m2 is not a plausible panel output"
    );
    assert!(
        get(&cooling, "margin") > 0.0,
        "16 C water over a 12.9 C room dew point is a safe design"
    );

    // Heating panel: 20 C room, 35 C water. Same equation, opposite sign.
    let heating = solved(&source(293.15, 308.15));
    assert!(
        get(&heating, "Q") < 0.0,
        "warm water must reverse the sign, got {} W",
        get(&heating, "Q")
    );

    // Cold water below the room dew point: the sizing constraint, not a warning.
    let condensing = solved(&source(299.15, 283.15));
    assert!(
        get(&condensing, "margin") < 0.0,
        "10 C water under a 12.9 C room dew point must report a negative margin"
    );
}

/// A fan coil is blow-through: the fan is upstream, so its heat lands in the air
/// the coil then has to cool. Draw it through instead and the same watts reheat
/// the supply air the coil just conditioned. The arrangement is not cosmetic,
/// and this is the difference it makes with every other part held identical.
#[test]
fn fan_coil_is_blow_through_and_that_changes_where_the_fan_heat_lands() {
    let blow_through = solved(
        r#"
MoistAirSource RM(P=101325, T=297.15, W=0.0093, mdot=0.5)
MoistAirSink   S1()
ThermalSource  WT(T=280.15)
FanCoilUnit FC(K=0, foul=1, dP=250, eta=0.55, eps=0.65)
connect(RM.out, FC.in)
connect(FC.out, S1.in)
connect(WT.port, FC.wall)
T_sup = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
"#,
    );
    let draw_through = solved(
        r#"
MoistAirSource RM(P=101325, T=297.15, W=0.0093, mdot=0.5)
MoistAirSink   S1()
ThermalSource  WT(T=280.15)
AirFilter      FL(K=0, foul=1)
MoistAirWallHX CO(model$=eps_t, eps=0.65)
MoistAirFan    FN(dP=250, eta=0.55)
connect(RM.out, FL.in)
connect(FL.out, CO.in)
connect(CO.out, FN.in)
connect(FN.out, S1.in)
connect(WT.port, CO.wall)
T_sup = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
W_fan = FN.W_el
"#,
    );

    let t_blow = get(&blow_through, "T_sup");
    let t_draw = get(&draw_through, "T_sup");
    assert!(
        get(&draw_through, "W_fan") > 100.0,
        "the fan needs to be doing enough work for the ordering to matter"
    );
    assert!(
        t_blow < t_draw,
        "blow-through lets the coil take part of the fan heat back out, so it must \
         supply COLDER air than draw-through: {t_blow} K vs {t_draw} K"
    );
}

// ── The DOAS ────────────────────────────────────────────────────────────────

/// The reason a DOAS exists is that it SEPARATES the two loads: the coil is
/// driven to a dew point low enough to absorb the whole building's latent gain,
/// and the reheat then moves the dry bulb back without touching the humidity.
///
/// Both halves of that are asserted here, because either alone is unremarkable.
/// The supply must leave drier than the room — otherwise the terminals have to
/// take latent load they have no capacity for — and changing the reheat must
/// move the temperature while leaving the humidity ratio bit-identical.
#[test]
fn doas_separates_the_latent_load_from_the_sensible_one() {
    let source = |q_reheat: f64| {
        format!(
            r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0180, mdot=1.0)
MoistAirSource EX(P=101325, T=297.15, W=0.0093, mdot=1.0)
MoistAirSink   S1()
MoistAirSink   S2()
DOAS DA(eff_h=0.70, eff_w=0.70, T_adp=280.15, BF=0.10, Q_reheat={q_reheat})
connect(OA.out, DA.oa_in)
connect(DA.sup_out, S1.in)
connect(EX.out, DA.exh_in)
connect(DA.exh_out, S2.in)
T_sup    = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
Tdp_sup  = DewPoint(AirH2O, h=S1.h, P=101325, W=S1.W)
Tdp_room = DewPoint(AirH2O, T=297.15, P=101325, W=0.0093)
"#
        )
    };

    let low = solved(&source(4000.0));
    let high = solved(&source(12000.0));

    // Drier than the room, by enough to absorb its latent gain.
    let tdp_sup = get(&low, "Tdp_sup");
    let tdp_room = get(&low, "Tdp_room");
    assert!(
        tdp_sup < tdp_room - 3.0,
        "the supply dew point {:.2} C must sit well under the room's {:.2} C, or the \
         terminals inherit latent load they cannot serve",
        tdp_sup - 273.15,
        tdp_room - 273.15
    );

    // Reheat is sensible: it moves the dry bulb and leaves the moisture alone.
    assert!(
        get(&high, "T_sup") > get(&low, "T_sup") + 5.0,
        "8 kW more reheat should be visible in the supply temperature"
    );
    assert!(
        (get(&high, "s1$w") - get(&low, "s1$w")).abs() < 1e-12,
        "reheat changed the humidity ratio, which would make it not reheat"
    );
}

/// The ADP construction only applies to a coil that actually gets wet. Below the
/// entering dew point there is no apparatus dew point to interpolate toward, and
/// a coil model that carries on interpolating anyway will hand back a leaving
/// humidity ratio ABOVE the entering one — a cooling coil that humidifies.
///
/// The `dry` rung is the coil in that regime, and `margin_adp` is what says which
/// rung a given design belongs on: positive means the surface is below the
/// entering dew point and the coil is wet.
#[test]
fn a_dry_coil_does_not_get_the_apparatus_dew_point_construction() {
    // Entering air at 30 C and W = 0.005 has a dew point near 4 C, well below a
    // 10 C surface: this coil cannot condense anything.
    let dry = solved(
        r#"
MoistAirSource EN(P=101325, T=303.15, W=0.0050, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(model$=dry, T_adp=283.15, BF=0.12)
connect(EN.out, CC.in)
connect(CC.out, S1.in)
T_out  = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
margin = CC.margin_adp
shr    = CC.SHR
m_cond = CC.mdot_w
"#,
    );

    assert!(
        get(&dry, "margin") < 0.0,
        "a 10 C surface under a 4 C entering dew point must report a negative margin"
    );
    assert!(
        (get(&dry, "s1$w") - 0.0050).abs() < 1e-12,
        "a dry coil moves no moisture at all, got W = {}",
        get(&dry, "s1$w")
    );
    assert!(get(&dry, "m_cond").abs() < 1e-12, "there is no condensate");
    // Entirely sensible — to within the only thing that separates the two sides
    // of the ratio, which is that `Q_sens` uses the cp of the ENTERING state
    // while `Q_air` is a real enthalpy difference across a 17 K span. The 3e-4
    // gap is that curvature in cp, and it is also the honest precision of a
    // reported SHR.
    assert!(
        (get(&dry, "shr") - 1.0).abs() < 1e-3,
        "a dry coil is entirely sensible, got SHR = {}",
        get(&dry, "shr")
    );
    // The bypass factor still means what it means, on temperature.
    let expected = 283.15 + 0.12 * (303.15 - 283.15);
    assert!((get(&dry, "T_out") - expected).abs() < 0.05);

    // The same air on the wet rung is the failure this exists to prevent: it
    // would leave the coil WETTER than it entered.
    let misapplied = solved(
        r#"
MoistAirSource EN(P=101325, T=303.15, W=0.0050, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(model$=wet, T_adp=283.15, BF=0.12)
connect(EN.out, CC.in)
connect(CC.out, S1.in)
margin = CC.margin_adp
"#,
    );
    assert!(get(&misapplied, "margin") < 0.0);
    assert!(
        get(&misapplied, "s1$w") > 0.0050,
        "the wet construction off its domain should be visibly wrong — that is why \
         margin_adp is exported"
    );

    // And a coil that IS wet reports a positive margin, so the two are told apart
    // by the number rather than by judgement.
    let wet = solved(
        r#"
MoistAirSource EN(P=101325, T=300.15, W=0.0112, mdot=1.0)
MoistAirSink   S1()
ApparatusDewPointCoil CC(T_adp=283.15, BF=0.12)
connect(EN.out, CC.in)
connect(CC.out, S1.in)
margin = CC.margin_adp
"#,
    );
    assert!(get(&wet, "margin") > 0.0);
}
