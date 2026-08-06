//! The property tables this build ships, and the backend they serve.
//!
//! Decision D1 (`docs/decisions/0001-property-backend.md`) chose precomputed
//! phase-split `(P,h)` tables, generated offline by native CoolProp
//! (`tools/table-gen`), as the browser build's real-fluid property source. This
//! module is where that decision becomes a running engine: it carries the
//! generated `FRPHTAB1` artifacts, decodes them once, and installs the resulting
//! [`TableBackend`](super::propfun::TableBackend) as the process's
//! [`RealFluid`](super::propfun::RealFluid).
//!
//! # Linked, not fetched — and why that diverges from D1
//!
//! D1 says the tables are "lazily fetched per fluid, not linked into the wasm,
//! so the wasm budget is untouched". They are linked here instead, with
//! `include_bytes!`. The reason is that a solve is **synchronous**: `PropsSI`
//! is called from inside the Newton residual, and there is no point in that
//! call stack where a Rust engine can await a `fetch`. Making it work the D1
//! way means either pre-fetching every table at worker start-up — which pays
//! the same bytes on every page load, and pays them *before* the first solve
//! rather than during it — or restructuring the solver around an async property
//! source. Linking costs 526 KB of the 900 KB the wasm budget had spare and
//! keeps the engine usable from `frees-cli`, the parity harness and the browser
//! through one code path.
//!
//! The seam D1 wanted is still here: [`install_from_bytes`] takes an arbitrary
//! `FRPHTAB1` slice, so a fetched table can be added at runtime without
//! recompiling. What is linked is a floor, not a ceiling.
//!
//! # What is here
//!
//! Two fluids, water and R134a, at the grid D1 measured (`n_sat = 512`,
//! `144 × 72` per phase piece, `f32`, normalized liquid coordinate). Their
//! measured error against CoolProp 8.0.0 is in
//! `fixtures/proptables/ERROR-REPORT.json`: worst case `2.1e-4` relative, and
//! that single number is entropy in subcooled liquid near the triple point
//! where water's entropy reference passes through zero — `0.05 J/kg·K` in
//! absolute terms. Everything the pending fluid fixtures actually touch is at or
//! below `6.6e-5`.
//!
//! Everything else — every other fluid, humid air, transport properties,
//! supercritical states, mixtures and incompressibles — is **declined by name**
//! by the backend rather than approximated. See
//! [`TableBackend`](super::propfun::TableBackend).

use std::sync::Arc;

use crate::diag::Result;
use crate::props::auxtable::AuxTable;
use crate::props::propfun::{self, TableBackend};
use crate::props::satsplit::SaturationSplitTable;

/// Water, `FRPHTAB1`, generated from CoolProp 8.0.0.
pub const WATER_PHTAB: &[u8] = include_bytes!("data/water.phtab");
/// R134a, `FRPHTAB1`, generated from CoolProp 8.0.0.
pub const R134A_PHTAB: &[u8] = include_bytes!("data/r134a.phtab");
/// R1234yf, `FRPHTAB1`, generated from CoolProp 8.0.0.
pub const R1234YF_PHTAB: &[u8] = include_bytes!("data/r1234yf.phtab");

/// Aqueous mono-ethylene glycol, `FRAUX1` (`tools/aux-gen`).
pub const MEG_FRAUX: &[u8] = include_bytes!("data/meg.fraux");
/// Aqueous mono-propylene glycol, `FRAUX1`.
pub const MPG_FRAUX: &[u8] = include_bytes!("data/mpg.fraux");
/// Air single-phase transport over `(P,T)`, `FRAUX1`.
pub const AIR_FRAUX: &[u8] = include_bytes!("data/air.fraux");
/// Water transport on the saturation line, `FRAUX1`.
pub const WATER_SAT_FRAUX: &[u8] = include_bytes!("data/water-sat.fraux");
/// R134a transport on the saturation line, `FRAUX1`.
pub const R134A_SAT_FRAUX: &[u8] = include_bytes!("data/r134a-sat.fraux");
/// R1234yf transport on the saturation line, `FRAUX1`.
pub const R1234YF_SAT_FRAUX: &[u8] = include_bytes!("data/r1234yf-sat.fraux");

/// Every split table linked into this build, in the order they are installed.
pub const BUILTIN_TABLES: [(&str, &[u8]); 3] = [
    ("water.phtab", WATER_PHTAB),
    ("r134a.phtab", R134A_PHTAB),
    ("r1234yf.phtab", R1234YF_PHTAB),
];

/// Every `FRAUX1` grid linked into this build.
pub const BUILTIN_AUX: [(&str, &[u8]); 6] = [
    ("meg.fraux", MEG_FRAUX),
    ("mpg.fraux", MPG_FRAUX),
    ("air.fraux", AIR_FRAUX),
    ("water-sat.fraux", WATER_SAT_FRAUX),
    ("r134a-sat.fraux", R134A_SAT_FRAUX),
    ("r1234yf-sat.fraux", R1234YF_SAT_FRAUX),
];

/// Decodes the linked tables.
///
/// A malformed artifact is an error, not a panic: the decode is a real parse of
/// bytes that a regenerated `tools/table-gen` run could get wrong, and a build
/// whose tables do not load should say so rather than abort a wasm module.
pub fn builtin_tables() -> Result<Vec<SaturationSplitTable>> {
    BUILTIN_TABLES
        .iter()
        .map(|(_, bytes)| SaturationSplitTable::decode_generated(bytes))
        .collect()
}

/// Decodes the linked auxiliary grids, on the same terms.
pub fn builtin_aux() -> Result<Vec<AuxTable>> {
    BUILTIN_AUX
        .iter()
        .map(|(_, bytes)| AuxTable::decode(bytes))
        .collect()
}

/// Installs the linked tables as the engine's real-fluid backend, replacing
/// whatever was installed before, and returns the fluids now served.
///
/// Callers that want this to happen exactly once should use
/// [`install_builtin_once`].
pub fn install_builtin() -> Result<Vec<String>> {
    let tables = builtin_tables()?;
    let aux = builtin_aux()?;
    let backend = TableBackend::with_aux(tables, aux);
    let fluids = backend.all_served();
    propfun::install(Arc::new(backend));
    Ok(fluids)
}

/// Installs `bytes` — one `FRPHTAB1` file — **alongside** the tables already
/// served, replacing any table for the same fluid.
///
/// This is the runtime seam D1 asked for: a host that fetches a table for a
/// fluid this build does not link can hand the slice straight here. It is also
/// how a future `coolprop.wasm` path would be layered in, by installing a
/// backend that consults the tables first.
pub fn install_from_bytes(bytes: &[u8]) -> Result<Vec<String>> {
    let incoming = SaturationSplitTable::decode_generated(bytes)?;
    let mut tables = builtin_tables()?;
    tables.retain(|t| !t.fluid().eq_ignore_ascii_case(incoming.fluid()));
    tables.push(incoming);
    let backend = TableBackend::with_aux(tables, builtin_aux()?);
    let fluids = backend.all_served();
    propfun::install(Arc::new(backend));
    Ok(fluids)
}

/// Installs the linked tables the first time it is called, and does nothing
/// afterwards.
///
/// Every public entry point of the crate calls this, so a caller never has to
/// remember to. It is deliberately **not** idempotent-by-reinstall: a test that
/// swaps in a recorded backend, or a host that installed a richer one, must not
/// have it yanked back out from under them by the next `solve`.
///
/// A decode failure is swallowed here — the entry points cannot return a
/// property error before they have parsed anything, and every real-fluid call
/// then fails with the honest "no backend installed" diagnostic. The failure is
/// visible through [`install_builtin`], which the unit tests call directly.
pub fn install_builtin_once() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = install_builtin();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::satsplit::{LiquidCoord, Region};

    /// The linked artifacts decode, and they are the geometry D1 chose.
    ///
    /// If `tools/table-gen` is re-run with different flags and the output copied
    /// in, this is what notices.
    #[test]
    fn the_linked_tables_decode_at_the_geometry_d1_chose() {
        let tables = builtin_tables().expect("linked tables must decode");
        assert_eq!(tables.len(), 3);
        let names: Vec<&str> = tables.iter().map(|t| t.fluid()).collect();
        assert_eq!(names, ["Water", "R134a", "R1234yf"]);
        for t in &tables {
            assert_eq!(t.liquid_coord(), LiquidCoord::Normalized, "{}", t.fluid());
            assert!(t.has_liquid(), "{}", t.fluid());
            // The normalized axis is dimensionless and D1 sizes it at 0.9.
            assert_eq!(t.dh_liquid_max(), 0.9, "{}", t.fluid());
            assert!(t.p_min() > 0.0 && t.p_serve_max() > t.p_min());
        }
    }

    /// The `FRPHTAB1` header is not decoration: the bytes it declares have to
    /// land on the values CoolProp produced.
    ///
    /// Ground truth is `tools/golden-dumper` against the real `libCoolProp.so`
    /// (8.0.0), the same oracle the golden fixtures use. The tolerance is the
    /// measured table error from `fixtures/proptables/ERROR-REPORT.json`, not a
    /// number chosen to make the test pass.
    #[test]
    fn water_states_match_the_coolprop_oracle_within_the_measured_error() {
        let tables = builtin_tables().unwrap();
        let water = &tables[0];
        // Oracle: T(P=101325, Hmass=h) for saturated-liquid-ish and wet steam.
        // h_f(101325 Pa) = 419_099.34 J/kg, T_sat = 373.1243 K.
        let tsat = water.tsat_at(101_325.0);
        assert!(
            (tsat - 373.124_295_847_286_2).abs() / 373.124_3 < 1e-4,
            "T_sat(1 atm) = {tsat}"
        );
        let hf = water.hf_at(101_325.0);
        assert!(
            (hf - 419_099.340_939_9).abs() / 419_099.34 < 1e-4,
            "h_f(1 atm) = {hf}"
        );
        // Wet steam at 1 atm, x = 0.5: T is the saturation temperature.
        let hg = water.hg_at(101_325.0);
        let half = 0.5 * (hf + hg);
        assert_eq!(water.region(101_325.0, half), Some(Region::TwoPhase));
    }

    /// `install_builtin` really reaches the dispatcher, and a `prop$` call that
    /// used to fail with "no backend installed" now answers.
    #[test]
    fn installing_the_builtin_tables_answers_a_real_fluid_call() {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::backend();
        let fluids = install_builtin().expect("install");
        assert_eq!(
            fluids,
            [
                "Water",
                "R134a",
                "R1234yf",
                "INCOMP::MEG",
                "INCOMP::MPG",
                "Air"
            ]
        );
        // Enthalpy(Water, P=101325, x=0) — the `rankine-cycle` state 1 shape.
        let h = propfun::evaluate("prop$enthalpy$water$p$x", &[101_325.0, 0.0]).unwrap();
        assert!(
            (h - 419_099.340_939_9).abs() / 419_099.34 < 1e-4,
            "h_f(1 atm) = {h}"
        );
        // Enthalpy(EG50, P=200000, T=305) — the `ev-thermal-management` coolant
        // shape, which had no answer at all before the aux grids. Oracle:
        // CoolProp 8.0.0 `INCOMP::MEG[0.50]` = 39687.033 J/kg.
        let h = propfun::evaluate("prop$enthalpy$eg50$p$t", &[200_000.0, 305.0]).unwrap();
        assert!(
            (h - 39_687.033).abs() / 39_687.033 < 1e-3,
            "h(EG50, 2 bar, 305 K) = {h}"
        );
        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }
    }

    /// A table this build does not link can be added at runtime.
    #[test]
    fn install_from_bytes_adds_a_fluid_without_dropping_the_linked_ones() {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::backend();
        // Re-installing water's own bytes must replace it, not duplicate it.
        let fluids = install_from_bytes(WATER_PHTAB).expect("install");
        // Three split tables plus the three aux-served names, and water exactly
        // once — re-installing its own bytes must replace it, not duplicate it.
        assert_eq!(fluids.len(), 6, "{fluids:?}");
        assert_eq!(
            fluids.iter().filter(|f| *f == "Water").count(),
            1,
            "{fluids:?}"
        );
        for want in ["Water", "R134a", "R1234yf", "INCOMP::MEG", "Air"] {
            assert!(fluids.iter().any(|f| f == want), "{want} in {fluids:?}");
        }
        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }
    }

    /// Garbage in is a `Result`, never a panic — this is the one place a
    /// caller-supplied byte string reaches the table reader.
    #[test]
    fn a_hostile_byte_string_is_refused_rather_than_trusted() {
        for bad in [
            &b""[..],
            &b"FRPHTAB1"[..],
            &b"FREESSP1 not this format at all"[..],
            &[0xffu8; 200][..],
        ] {
            let err = install_from_bytes(bad).unwrap_err().to_string_message();
            assert!(!err.is_empty(), "{bad:?}");
        }
        // A truncated real file: the header is valid, the payload is not there.
        let truncated = &WATER_PHTAB[..WATER_PHTAB.len() / 2];
        let err = install_from_bytes(truncated)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("declares"), "{err}");
        // A real file with one header byte corrupted.
        let mut flipped = WATER_PHTAB.to_vec();
        flipped[10] = 7; // elem_kind
        let err = install_from_bytes(&flipped)
            .unwrap_err()
            .to_string_message();
        assert!(err.contains("elem_kind"), "{err}");
    }
}
