//! Behavioural gates for the moist-air recovery, desiccant and terminal
//! components.
//!
//! These assert the *defining property* of each component rather than a golden
//! number, because that is what a regression would break. A sensible exchanger
//! that quietly starts moving moisture, or a desiccant wheel that leaves the air
//! cooler, is still dimensionally consistent and would pass a residual check —
//! it just would not be the device any more.
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

/// A sealed sensible exchanger moves heat and **no** moisture. With no carryover
/// and no leakage the humidity ratio out must equal the humidity ratio in
/// exactly — not approximately.
#[test]
fn sensible_air_to_air_hx_moves_heat_but_not_moisture() {
    let values = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0155, mdot=1.0)
MoistAirSource EA(P=101325, T=297.15, W=0.0093, mdot=1.0)
MoistAirSink   S1()
MoistAirSink   S2()
SensibleAirToAirHX HX(eff=0.70, eatr=0, oacf=1)
connect(OA.out, HX.sup_in)
connect(HX.sup_out, S1.in)
connect(EA.out, HX.exh_in)
connect(HX.exh_out, S2.in)
T_sup_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
dW_sup    = S1.W - 0.0155
dW_exh    = S2.W - 0.0093
"#,
    );

    // Not one part in 1e12 of moisture may cross.
    assert!(
        get(&values, "dW_sup").abs() < 1e-12,
        "supply humidity ratio moved by {}",
        get(&values, "dW_sup")
    );
    assert!(get(&values, "dW_exh").abs() < 1e-12);

    // Heat did cross, toward the cooler exhaust, by roughly eff x the spread.
    let t_out = get(&values, "T_sup_out") - 273.15;
    assert!(
        (t_out - 27.3).abs() < 0.3,
        "expected the supply near 27.3 C at eff = 0.70, got {t_out:.2} C"
    );
}

/// The same exchanger with the two Standard 84 leakage terms switched on. A
/// rotary matrix carries exhaust air round with it and a plate core leaks it
/// through the seals; either way `eatr` is the fraction of the supply leaving
/// stream that is really exhaust air, and it is the ONLY route by which a
/// sensible device moves moisture. `oacf` is the supply-side leakage that makes
/// the exchanger deliver less outdoor air than it is fed.
///
/// What must hold with both of them on is that nothing is created or lost: the
/// dry air, the water and the energy that enter the device all leave it.
#[test]
fn sensible_hx_carryover_and_leakage_conserve_the_whole_device() {
    let values = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0155, mdot=1.0)
MoistAirSource EA(P=101325, T=297.15, W=0.0093, mdot=1.2)
MoistAirSink   S1()
MoistAirSink   S2()
SensibleAirToAirHX HX(eff=0.70, eatr=0.05, oacf=1.04)
connect(OA.out, HX.sup_in)
connect(HX.sup_out, S1.in)
connect(EA.out, HX.exh_in)
connect(HX.exh_out, S2.in)
m_sup_out = S1.mdot
m_exh_out = S2.mdot
W_sup_out = S1.W
res_air   = (S1.mdot + S2.mdot) - (1.0 + 1.2)
res_water = (S1.mdot * S1.W + S2.mdot * S2.W) - (1.0 * 0.0155 + 1.2 * 0.0093)
h_oa_in    = Enthalpy(AirH2O, T=308.15, P=101325, W=0.0155)
h_ea_in    = Enthalpy(AirH2O, T=297.15, P=101325, W=0.0093)
res_energy = (S1.mdot * S1.h + S2.mdot * S2.h) - (1.0 * h_oa_in + 1.2 * h_ea_in)
"#,
    );

    // 4% of the supply never makes it out of the supply side...
    assert!(
        (get(&values, "m_sup_out") - 1.0 / 1.04).abs() < 1e-12,
        "oacf did not scale the supply outlet flow"
    );
    // ...and turns up on the exhaust side rather than vanishing.
    assert!(
        get(&values, "res_air").abs() < 1e-12,
        "dry air is not conserved"
    );
    assert!(
        get(&values, "res_water").abs() < 1e-12,
        "water is not conserved: {}",
        get(&values, "res_water")
    );
    assert!(
        get(&values, "res_energy").abs() < 1e-6,
        "energy is not conserved: {}",
        get(&values, "res_energy")
    );

    // The carryover moves moisture, and exactly 5% of the way to the exhaust.
    let expected = 0.0155 + 0.05 * (0.0093 - 0.0155);
    assert!(
        (get(&values, "W_sup_out") - expected).abs() < 1e-12,
        "eatr did not carry moisture across: {} vs {expected}",
        get(&values, "W_sup_out")
    );
}

/// A total-energy exchanger is rated on TWO independent effectivenesses —
/// sensible on temperature, latent on humidity ratio — which is what separates
/// it from `EnthalpyWheel`'s single enthalpy effectiveness. A membrane core
/// whose sensible and latent ratings differ cannot be written the other way.
#[test]
fn total_energy_exchanger_rates_sensible_and_latent_separately() {
    let values = solved(
        r#"
MoistAirSource OA(P=101325, T=308.15, W=0.0155, mdot=1.0)
MoistAirSource EA(P=101325, T=297.15, W=0.0093, mdot=1.0)
MoistAirSink   S1()
MoistAirSink   S2()
TotalEnergyExchanger MX(eps_s=0.70, eps_L=0.50, eatr=0, oacf=1)
connect(OA.out, MX.sup_in)
connect(MX.sup_out, S1.in)
connect(EA.out, MX.exh_in)
connect(MX.exh_out, S2.in)
T_sup_out = Temperature(AirH2O, h=S1.h, P=101325, W=S1.W)
W_sup_out = S1.W
res_water = (S1.mdot * S1.W + S2.mdot * S2.W) - (0.0155 + 0.0093)
"#,
    );

    // Temperature follows eps_s, on the temperature spread.
    let t_out = get(&values, "T_sup_out");
    let t_expected = 308.15 + 0.70 * (297.15 - 308.15);
    assert!(
        (t_out - t_expected).abs() < 0.05,
        "sensible leg: {t_out} vs {t_expected}"
    );

    // Humidity follows eps_L, on the humidity spread — a different fraction of a
    // different quantity. A single enthalpy effectiveness could not produce both.
    let w_expected = 0.0155 + 0.50 * (0.0093 - 0.0155);
    assert!(
        (get(&values, "W_sup_out") - w_expected).abs() < 1e-12,
        "latent leg: {} vs {w_expected}",
        get(&values, "W_sup_out")
    );

    assert!(
        get(&values, "res_water").abs() < 1e-12,
        "water is not conserved"
    );
}

/// A desiccant wheel leaves the process air **warmer and drier**: the latent
/// heat released as vapour is sorbed reappears as sensible heat. This is the one
/// chart direction — down and to the right — that no other component here draws,
/// so it is worth asserting as a direction and not only as a number.
#[test]
fn desiccant_wheel_leaves_process_air_warmer_and_drier() {
    let values = solved(
        r#"
MoistAirSource PR(P=101325, T=303.15, W=0.0140, mdot=1.0)
MoistAirSource RG(P=101325, T=353.15, W=0.0100, mdot=1.0)
MoistAirSink   S3()
MoistAirSink   S4()
DesiccantWheel DW(eff_L=0.75, W_eq=0.0030, f_carry=0.05)
connect(PR.out, DW.proc_in)
connect(DW.proc_out, S3.in)
connect(RG.out, DW.reg_in)
connect(DW.reg_out, S4.in)
T_proc_out = Temperature(AirH2O, h=S3.h, P=101325, W=S3.W)
W_proc_out = S3.W
W_reg_out  = S4.W
"#,
    );

    let t_out = get(&values, "T_proc_out");
    let w_out = get(&values, "W_proc_out");
    assert!(
        t_out > 303.15,
        "process air must leave warmer than 30 C, got {:.2} C",
        t_out - 273.15
    );
    assert!(
        w_out < 0.0140,
        "process air must leave drier, got W = {w_out}"
    );

    // Latent effectiveness against the equilibrium humidity ratio.
    let expected = 0.0140 - 0.75 * (0.0140 - 0.0030);
    assert!((w_out - expected).abs() < 1e-9, "{w_out} vs {expected}");

    // Moisture is conserved: what leaves the process stream joins the
    // regeneration stream, on the dry-air basis.
    let w_reg = get(&values, "W_reg_out");
    assert!(
        (w_reg - (0.0100 + (0.0140 - w_out))).abs() < 1e-9,
        "regeneration stream did not receive the removed moisture"
    );
}

/// Indirect evaporative cooling cools the primary stream **without** wetting it,
/// and approaches the secondary stream's entering wet bulb rather than its dry
/// bulb — which is why it still works where a dry-bulb approach would not.
#[test]
fn indirect_evaporative_cooler_cools_primary_without_wetting_it() {
    let values = solved(
        r#"
MoistAirSource PA(P=101325, T=308.15, W=0.0120, mdot=1.0)
MoistAirSource SA(P=101325, T=308.15, W=0.0060, mdot=1.0)
MoistAirSink   S5()
MoistAirSink   S6()
IndirectEvaporativeCooler IEC(wbde=0.70, eff_sec=0.85)
connect(PA.out, IEC.pri_in)
connect(IEC.pri_out, S5.in)
connect(SA.out, IEC.sec_in)
connect(IEC.sec_out, S6.in)
T_pri_out = Temperature(AirH2O, h=S5.h, P=101325, W=S5.W)
dW_pri    = S5.W - 0.0120
T_wb_sec  = WetBulb(AirH2O, T=308.15, P=101325, W=0.0060)
"#,
    );

    assert!(
        get(&values, "dW_pri").abs() < 1e-12,
        "the primary stream must not gain moisture: that is what indirect means"
    );

    // The primary is driven toward the secondary's WET BULB, not its dry bulb.
    // Both streams enter at 35 C, so any cooling at all proves the point.
    let t_out = get(&values, "T_pri_out");
    let t_wb = get(&values, "T_wb_sec");
    assert!(t_out < 308.15 - 1.0, "no cooling: {:.2} C", t_out - 273.15);
    assert!(
        t_out > t_wb,
        "cannot cool below the secondary wet bulb: {:.2} vs {:.2}",
        t_out - 273.15,
        t_wb - 273.15
    );
    let expected = 308.15 - 0.70 * (308.15 - t_wb);
    assert!((t_out - expected).abs() < 0.05);
}

/// A chilled beam is sensible-only and has no drain, so the design has to be
/// checked rather than assumed. `margin_dp` going negative is the condensing
/// case, and it must be visible as an output.
#[test]
fn chilled_beam_reports_its_condensation_margin() {
    let source = |t_wall: f64| {
        format!(
            r#"
MoistAirSource RM(P=101325, T=297.15, W=0.0110, mdot=0.5)
MoistAirSink   S7()
ThermalSource  WT(T={t_wall})
ChilledBeam    CB(eps=0.55)
connect(RM.out, CB.in)
connect(CB.out, S7.in)
connect(WT.port, CB.wall)
margin = CB.margin_dp
dW     = S7.W - 0.0110
"#
        )
    };

    // Water well above the room dew point: safe, and no moisture removed.
    let safe = solved(&source(289.15));
    assert!(
        get(&safe, "dW").abs() < 1e-12,
        "a chilled beam has no latent duty"
    );
    assert!(
        get(&safe, "margin") > 0.0,
        "16 C water under a 15.4 C dew point should be a positive margin"
    );

    // Water below the dew point: the margin must go negative rather than the
    // component silently condensing.
    let condensing = solved(&source(283.15));
    assert!(
        get(&condensing, "margin") < 0.0,
        "10 C water under a 15.4 C dew point must report a negative margin"
    );
}
