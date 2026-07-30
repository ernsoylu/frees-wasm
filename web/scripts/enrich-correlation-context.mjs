// Add engineering usage context to the correlation reference pages: a richer
// Description (what it computes and why) plus an "Applicability" section (which
// system/side/component, the conditions it is valid for, and how it feeds into a
// model). The auto-enriched pages had only the terse registry one-liner + formula.
//
// Run: node scripts/enrich-correlation-context.mjs

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REF = path.join(__dirname, '../src/docs/reference');

// name -> { desc?: replacement Description paragraph, where, when, use }
const C = {
  // ── Two-phase: condensation Nusselt ──
  nu_cavallini_zecchin: {
    desc: 'Returns the **in-tube condensation Nusselt number** by the Cavallini–Zecchin correlation; the condensing-side film coefficient follows as `h = Nu·k_l/D_h`. It is one of the standard shear-dominated condensation correlations.',
    where: 'The condensing two-phase refrigerant **inside the tubes of a condenser or gas-cooler**.',
    when: 'Annular, vapor-shear-controlled in-tube condensation with a turbulent liquid film; evaluate at the local vapor quality `x` (integrate across the pass for a mean value).',
    use: 'Convert to a film coefficient `h = Nu·k_l/D_h`, then combine it with the air/coolant side and the wall via [`ua_hx`](ua_hx). Alternatives: [`nu_shah`](nu_shah) (broader range) and [`nu_traviss`](nu_traviss).',
  },
  nu_shah: {
    desc: 'Returns the **in-tube condensation Nusselt number** by the Shah correlation — an enhancement on the liquid-only Nusselt number that captures the thinning film and vapor shear.',
    where: 'The condensing two-phase refrigerant side of a condenser / gas-cooler.',
    when: 'In-tube condensation across a wide quality and reduced-pressure range; depends on the reduced pressure `p_red`.',
    use: 'Gives the condensing film coefficient (`h = Nu·k_l/D_h`) for the refrigerant side. A robust general-purpose alternative to [`nu_cavallini_zecchin`](nu_cavallini_zecchin) / [`nu_traviss`](nu_traviss).',
  },
  // ── Two-phase: single-phase building blocks ──
  nu_dittus_boelter: {
    desc: 'Returns the **single-phase turbulent Nusselt number** `Nu = 0.023 Re^0.8 Pr^n`. It is both a stand-alone single-phase film coefficient and the **liquid-only baseline** that flow-boiling and condensation correlations enhance.',
    where: 'Fully-developed turbulent single-phase flow in a tube — or the liquid-only reference inside a two-phase correlation.',
    when: 'Smooth tube, `Re ≳ 10⁴`, `0.7 ≲ Pr ≲ 120`; `n = 0.4` when heating the fluid, `0.3` when cooling.',
    use: 'Feeds the convective term of the Chen flow-boiling model and the liquid-only term of [`nu_shah`](nu_shah)/[`nu_cavallini_zecchin`](nu_cavallini_zecchin). Use [`nu_gnielinski`](nu_gnielinski) for better transitional accuracy.',
  },
  nu_gnielinski: {
    desc: 'Returns the **single-phase Nusselt number** by the Gnielinski correlation — more accurate than Dittus–Boelter, especially in the transitional-turbulent band.',
    where: 'Single-phase liquid or gas flow in a tube/channel (the preferred single-phase baseline).',
    when: 'Smooth tube, `3000 ≲ Re ≲ 5×10⁶`, `0.5 ≲ Pr ≲ 2000`; uses the Darcy friction factor.',
    use: 'The single-phase film coefficient (`h = Nu·k/D_h`) for coolant/oil/air lines, and the liquid-only baseline for two-phase correlations.',
  },
  // ── Two-phase: flow-boiling (Chen superposition) ──
  chen_f: {
    desc: 'Returns the **convective enhancement factor `F`** of the Chen flow-boiling model — the factor by which two-phase convection exceeds the liquid-only value, as a function of the Martinelli parameter.',
    where: 'Saturated flow boiling of a refrigerant in evaporator tubes.',
    when: 'Saturated (not subcooled) flow boiling; `F ≥ 1`, rising as the vapor fraction grows.',
    use: 'Used with [`chen_s`](chen_s) in the Chen superposition `h = F·h_conv + S·h_nb`, where `h_conv` is the liquid-only convective coefficient.',
  },
  chen_s: {
    desc: 'Returns the **nucleate-boiling suppression factor `S`** of the Chen flow-boiling model — the factor that throttles the pool-boiling nucleate term as the bulk velocity rises.',
    where: 'Saturated flow boiling of a refrigerant in evaporator tubes.',
    when: 'Saturated flow boiling; `S ≤ 1`, falling as the two-phase Reynolds number increases.',
    use: 'Used with [`chen_f`](chen_f) in the Chen superposition `h = F·h_conv + S·h_nb`.',
  },
  // ── Two-phase: frictional pressure-drop multipliers ──
  lm_phi2: {
    desc: 'Returns the **Chisholm two-phase frictional multiplier** `φ_l² = 1 + C/X + 1/X²` on the liquid-alone pressure gradient — i.e. how much more pressure two-phase flow drops than the liquid flowing alone.',
    where: 'Two-phase frictional pressure drop in refrigerant evaporator/condenser passages.',
    when: 'Separated two-phase flow; the Chisholm constant `C` ranges 5 (laminar–laminar) to 20 (turbulent–turbulent).',
    use: 'Multiply the liquid-only Darcy gradient by `φ_l²` (with [`lm_martinelli_tt`](lm_martinelli_tt) supplying `X`) to get the two-phase frictional `ΔP`. Friedel ([`friedel_phi2`](friedel_phi2)) is an alternative.',
  },
  lm_martinelli_tt: {
    desc: 'Returns the **turbulent–turbulent Lockhart–Martinelli parameter `X_tt`** — the ratio of the liquid-alone to vapor-alone pressure gradients that two-phase correlations key on.',
    where: 'The independent variable for two-phase heat-transfer and pressure-drop correlations.',
    when: 'Both phases turbulent (the usual refrigerant case).',
    use: 'Feeds [`lm_phi2`](lm_phi2), the Chen factors, and many two-phase Nusselt correlations.',
  },
  friedel_phi2: {
    desc: 'Returns the **Friedel two-phase frictional multiplier** on the liquid-only pressure drop — an alternative to Chisholm that uses the Froude and Weber numbers for broader validity.',
    where: 'Two-phase frictional pressure drop in refrigerant passages.',
    when: 'Recommended in the standard two-phase literature for `μ_l/μ_g < 1000`; covers a wider mass-flux range than the simple Chisholm form.',
    use: 'Multiply the liquid-only gradient by the multiplier to get the two-phase `ΔP`; an alternative to [`lm_phi2`](lm_phi2).',
  },
  momentum_flux: {
    desc: 'Returns the **separated-flow momentum flux** — the acceleration pressure change (outlet − inlet) caused by the change in vapor quality, not by friction.',
    where: 'The acceleration `ΔP` term along an evaporator/condenser pass.',
    when: 'Wherever quality changes appreciably (vapor generation in an evaporator accelerates the flow).',
    use: 'Add it to the frictional ([`lm_phi2`](lm_phi2)) and gravitational ([`dp_gravity`](dp_gravity)) terms for the total pass `ΔP`.',
  },
  // ── Two-phase: void fraction ──
  void_homogeneous: {
    desc: 'Returns the **homogeneous (no-slip) void fraction** — the vapor volume fraction assuming both phases move at the same velocity.',
    where: 'The vapor fraction `α` used in two-phase density, charge, and gravitational-head terms.',
    when: 'High-mass-flux / bubbly flow where slip is negligible; the simplest model (it overpredicts `α` at low mass flux).',
    use: 'Feeds the two-phase mixture density and [`dp_gravity`](dp_gravity). For better accuracy use [`void_zivi`](void_zivi) or [`void_rouhani`](void_rouhani).',
  },
  void_zivi: {
    desc: 'Returns the **Zivi void fraction**, using a slip ratio `S = (ρ_l/ρ_g)^{1/3}` from minimum-entropy-production — more realistic than the no-slip model.',
    where: 'The vapor fraction `α` for separated two-phase flow.',
    when: 'Moderate mass flux with appreciable slip; better than homogeneous, simpler than drift-flux.',
    use: 'Feeds mixture density / charge / static-head calculations.',
  },
  void_rouhani: {
    desc: 'Returns the **Rouhani–Axelsson drift-flux void fraction** (the default) — it accounts for both phase slip and the radial distribution of vapor.',
    where: 'The general-purpose vapor fraction `α` for refrigerant evaporators and condensers.',
    when: 'Recommended across flow regimes and mass fluxes; the default void model.',
    use: 'Feeds the refrigerant charge inventory, mixture density, and the gravitational pressure term.',
  },
  zone_ramp: {
    desc: 'Returns a **smooth `tanh(L/ε)` ramp** that fades a moving-boundary zone in/out as its length `L` approaches zero. It is a numerical `C¹` smoothing, not a physical correlation.',
    where: 'Moving-boundary heat-exchanger models (subcooled / two-phase / superheat zones).',
    when: 'Whenever a zone length can shrink to zero during a solve/transient.',
    use: 'Blends a zone\'s contribution smoothly so the corrector does not chatter at a regime switch.',
  },
};

// HX correlations that already carry SIDE:/HX: text — give them a tidy Applicability
// section too (the registry description is kept; we just add conditions + how-used).
const HX = {
  nu_zukauskas: { where: 'Air/gas in cross-flow over a tube bank (the air side of a fin-and-tube radiator/condenser).', when: 'External cross-flow; the constants `C, m` depend on the in-line/staggered arrangement and the Reynolds band.', use: 'Gives the air-side film coefficient `h = Nu·k/D`; combine with the refrigerant/coolant side and wall via [`ua_hx`](ua_hx).' },
  nu_tubebank: { where: 'Air/gas over an in-line or staggered tube bank.', when: 'Cross-flow; `arr$` selects the arrangement and the Reynolds-band coefficients.', use: 'Air-side `h` for a fin-and-tube core.' },
  nu_hilpert: { where: 'Air/gas over a single cylinder or a sparse bank.', when: 'Cross-flow over an isolated tube; band-dependent `C, m`.', use: 'Air-side `h` for bare-tube / low-density-bank exchangers.' },
  nu_colburn: { where: 'Air/gas through a compact finned surface.', when: 'Compact plate-fin / louvered-fin cores, characterized by a Colburn `j`-factor.', use: 'Air-side `h = j·Re·Pr^{1/3}·k/D_h`; pair the friction with [`f_fin`](f_fin).' },
  nu_churchill_chu: { where: 'Natural convection from a surface to still air / quiescent fluid.', when: 'Free (buoyancy-driven) convection, characterized by the Rayleigh number.', use: 'Passive/low-flow surfaces; blend with a forced-convection `Nu` via [`nu_blend`](nu_blend).' },
  nu_blend: { where: 'Any surface with combined natural + forced convection.', when: 'Mixed convection where neither mechanism dominates.', use: 'Combines two Nusselt numbers as `(Nu₁³ + Nu₂³)^{1/3}`.' },
  nu_plate: { where: 'A single-phase stream in a brazed/gasketed plate heat exchanger (BPHE).', when: 'Chevron-plate channels; depends on the chevron angle `β`.', use: 'Plate-side `h` for either stream of a plate HX.' },
  nu_gungor_winterton: { where: 'Boiling two-phase refrigerant in evaporator tubes.', when: 'Saturated flow boiling, enhancing the liquid-only Nusselt with the boiling number and Martinelli parameter.', use: 'Evaporator refrigerant-side `h`; an alternative to the Chen ([`chen_f`](chen_f)/[`chen_s`](chen_s)) and Shah boiling models.' },
  nu_traviss: { where: 'Condensing two-phase refrigerant in tube/microchannel condensers.', when: 'In-tube condensation, annular-flow dominated.', use: 'Condenser refrigerant-side `h`; alternative to [`nu_shah`](nu_shah)/[`nu_cavallini_zecchin`](nu_cavallini_zecchin).' },
  dp_1phase: { where: 'A single-phase liquid/gas line (coolant, water, air channel, pipe).', when: 'Single-phase Darcy flow; turbulent or laminar.', use: 'Friction `ΔP` for radiator/CAC fluid channels and connecting lines.' },
  dp_mueller_steinhagen: { where: 'Two-phase refrigerant in an evaporator/condenser line.', when: 'Two-phase frictional drop; an alternative to the Chisholm/Friedel route ([`dp_2phase`](dp_2phase)).', use: 'Interpolates between the all-liquid and all-vapor drops over the quality range.' },
  dp_compact_core: { where: 'Air/gas through a compact finned core.', when: 'Includes the entrance, acceleration, core-friction and exit terms of the standard compact-core formulation.', use: 'Air-side `ΔP` for a fin-and-tube / plate-fin radiator, condenser, or CAC.' },
  dp_gravity: { where: 'Two-phase refrigerant in a vertical riser/downcomer.', when: 'Static-head term; sign follows the flow direction `θ`.', use: 'Add to the frictional and acceleration terms for the total vertical-pass `ΔP`.' },
  dp_2phase_avg: { where: 'Two-phase refrigerant along an evaporator/condenser pass.', when: 'Integrates the two-phase multiplier over `n` quality cells from `x_in` to `x_out`.', use: 'A quality-averaged frictional `ΔP` for a whole pass.' },
  j_fin: { where: 'The air/gas finned side of a compact surface.', when: 'Plain / wavy / louvered / offset-strip fin surfaces (`surface$`).', use: 'Air-side `h` via the Colburn `j`-factor; pair with [`f_fin`](f_fin).' },
  f_fin: { where: 'The air/gas finned side of a compact surface.', when: 'Same fin surfaces as [`j_fin`](j_fin).', use: 'Air-side friction (Fanning) for the core `ΔP`.' },
};

function applicBlock(c) {
  return ['## Applicability', '',
    `- **Where it applies:** ${c.where}`,
    `- **Valid when:** ${c.when}`,
    `- **How it\'s used:** ${c.use}`, ''].join('\n');
}

let n = 0;
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
  const p = path.join(d, e.name);
  if (e.isDirectory()) return walk(p);
  if (!e.name.endsWith('.md') || e.name.startsWith('_')) return;
  let src = fs.readFileSync(p, 'utf-8');
  const name = (src.match(/^name:\s*(.+)$/m) || [])[1]?.trim();
  const c = C[name] || HX[name];
  if (!c) return;
  let changed = false;

  // Replace the terse Description paragraph (two-phase family only).
  if (c.desc) {
    src = src.replace(/(## Description\n\n)([^\n]+)(\n)/, (full, head, _old, tail) => {
      changed = true; return head + c.desc + tail;
    });
  }
  // Insert an Applicability section before Input Arguments (once).
  if (!/## Applicability/.test(src)) {
    src = src.replace(/(\n## Input Arguments\n)/, '\n' + applicBlock(c) + '$1');
    changed = true;
  }
  if (changed) { fs.writeFileSync(p, src); n++; }
});
walk(REF);
console.log(`enrich-correlation-context: added usage context to ${n} correlation pages.`);
