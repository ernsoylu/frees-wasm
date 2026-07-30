// Replace the worthless generic argument/parameter descriptions ("Numeric
// argument." / "String argument." / "Output value.") and the missing component
// Parameters descriptions with real, per-name meanings, units, and — for string
// arguments — the allowed option enums. Operates on the committed reference pages.
//
// Hand-authored rows (anything other than the generic placeholders) are preserved;
// only generic cells are rewritten. Component Parameters tables (2-column) gain a
// Description column.
//
// Run: node scripts/fix-arg-descriptions.mjs

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REF = path.join(__dirname, '../src/docs/reference');

// ── String-argument option enums: per-function, then global default ──────────
const OPT_FN = {
  hx_effectiveness: { 'type$': ['counterflow', 'parallel'] },
  hx_ntu: { 'type$': ['counterflow', 'parallel'] },
  mach_a_astar: { 'regime$': ['subsonic', 'supersonic'] },
  beta_oblique: { 'branch$': ['weak', 'strong'] },
  gram: { 'type$': ['c', 'o'] },
  nu_tubebank: { 'arr$': ['inline', 'staggered'] },
  j_fin: { 'surface$': ['plain', 'wavy', 'louvered', 'offset'] },
  f_fin: { 'surface$': ['plain', 'wavy', 'louvered', 'offset'] },
  pidtune: { 'type$': ['P', 'PI', 'PD', 'PID'] },
  c2d: { "'zoh'": ['zoh', 'tustin'] },
  d2c: { "'tustin'": ['tustin', 'zoh'] },
};
const OPT_GLOBAL = {
  'geom$': ['wall', 'cylinder', 'sphere'],
  'model$': ['SRK', 'PR'],
  'phase$': ['vapor', 'liquid'],
};
// Component string params with option sets (label + allowed values).
const OPT_COMP = {
  'domain$': { label: 'Connector fluid family', opts: ['fluid', 'gas', 'oil', 'moistair', 'liquid', 'twophase'] },
  'arr$': { label: 'Flow arrangement (passed to hx_effectiveness)', opts: ['counterflow', 'parallel'] },
};

// ── Global argument glossary (unambiguous physics/math names) ────────────────
const G = {
  // thermodynamic state
  P: 'Pressure [Pa].', P0: 'Stagnation (total) pressure [Pa].', P1: 'Upstream pressure [Pa].', P2: 'Downstream pressure [Pa].',
  Pup: 'Upstream pressure [Pa].', Pdown: 'Downstream pressure [Pa].', Patm: 'Atmospheric pressure [Pa].',
  T: 'Temperature [K].', T0: 'Stagnation (total) temperature [K].', Tup: 'Upstream temperature [K].',
  T_react: 'Reactant inlet temperature [K].', T_amb: 'Ambient temperature [K].',
  v: 'Specific volume [m³/kg].', rho: 'Density [kg/m³].', rho_l: 'Saturated-liquid density [kg/m³].', rho_g: 'Saturated-vapor density [kg/m³].',
  rho_in: 'Inlet density [kg/m³].', rho_out: 'Outlet density [kg/m³].', rho_mean: 'Mean density [kg/m³].',
  mu: 'Dynamic viscosity [Pa·s].', mu_l: 'Liquid dynamic viscosity [Pa·s].', mu_g: 'Vapor dynamic viscosity [Pa·s].',
  cp: 'Specific heat at constant pressure [J/kg·K].', sigma: 'Surface tension [N/m].',
  // flow / geometry
  mdot: 'Mass flow rate [kg/s].', G: 'Mass flux G = ṁ/Aflow [kg/m²·s].', V: 'Velocity [m/s].',
  Dh: 'Hydraulic diameter [m].', D: 'Diameter [m].', L: 'Length [m].',
  Aflow: 'Free-flow (minimum) cross-sectional area [m²].', Afrontal: 'Frontal (face) area [m²].',
  Atotal: 'Total convective surface area [m²].', Afin: 'Fin (secondary) surface area [m²].',
  A1: 'Side-1 area [m²].', A2: 'Side-2 area [m²].',
  // dimensionless groups
  Re: 'Reynolds number.', Re_l: 'Liquid-only Reynolds number.', Pr: 'Prandtl number.', Pr_l: 'Liquid Prandtl number.', Pr_w: 'Wall-temperature Prandtl number.',
  Bi: 'Biot number h·s/k.', Fo: 'Fourier number α·t/s².', Bo: 'Boiling number.', Ra: 'Rayleigh number.',
  M: 'Mach number.', M1: 'Upstream Mach number (≥ 1).', k: 'Ratio of specific heats (e.g. 1.4 for air).',
  NTU: 'Number of transfer units UA/Cmin.', Cr: 'Capacity ratio Cmin/Cmax (0–1).', eps: 'Effectiveness ε (0–1).',
  Nu1: 'First Nusselt number to blend.', Nu2: 'Second Nusselt number to blend.', Nu_l: 'Liquid-only Nusselt number.',
  j: 'Colburn j-factor.', f: 'Fanning/Darcy friction factor.', mL: 'Fin parameter–length product m·L.', eta_fin: 'Single-fin efficiency.',
  // two-phase
  x: 'Vapor quality (0–1).', x_in: 'Inlet vapor quality (0–1).', x_out: 'Outlet vapor quality (0–1).',
  alpha: 'Void fraction (0–1).', X: 'Lockhart–Martinelli parameter.', Xtt: 'Turbulent–turbulent Martinelli parameter.', X_tt: 'Turbulent–turbulent Martinelli parameter.',
  Co: 'Convection number.', p_red: 'Reduced pressure P/Pcrit.', F: 'Convective enhancement factor.', S: 'Nucleate-suppression factor.',
  // shocks / compressible
  beta: 'Oblique-shock wave angle [rad].', theta: 'Flow-deflection angle [rad].', nu: 'Prandtl–Meyer angle [rad].',
  A_Astar: 'Area ratio A/A* (≥ 1).', alt: 'Geopotential altitude [m].',
  // combustion
  phi: 'Equivalence ratio (1 = stoichiometric).', theta0: 'Start of combustion [deg].', dtheta: 'Combustion duration [deg].',
  // control
  num: 'Numerator coefficients (descending powers of s).', den: 'Denominator coefficients (descending powers of s).',
  num1: 'First system numerator coefficients.', den1: 'First system denominator coefficients.',
  num2: 'Second system numerator coefficients.', den2: 'Second system denominator coefficients.',
  omega: 'Frequency vector [rad/s].', Ts: 'Sample time [s].', Td: 'Time delay [s].', order: 'Approximation order.',
  u: 'Input signal samples.', wc: 'Target gain-crossover frequency [rad/s].',
  // calculus / tables / strings
  expr: 'Expression to evaluate.', var: 'Integration variable.', lower: 'Lower limit.', upper: 'Upper limit.',
  s$: 'String literal.', sub$: 'Substring to search for.', arr$: 'Array range, e.g. data[1:n].',
  fluid$: 'Fluid name (e.g. Water, R134a, Air).', fuel$: "Fuel name/formula (e.g. 'CH4').",
  comp$: "Mixture composition string, e.g. 'N2:0.79,O2:0.21'.", species$: 'Product species name (e.g. CO, NO).',
  n: 'Order / number of terms.', xstar: 'Dimensionless position (0 = centre, 1 = surface).',
  a: 'First operand.', b: 'Second operand.', Rwall: 'Wall conductive resistance [K/W].',
  h1: 'Side-1 film coefficient [W/m²·K].', h2: 'Side-2 film coefficient [W/m²·K].',
  // additional coverage
  z: 'Argument (complex or real).', y: 'Value / second coordinate.', A: 'Matrix.', B: 'Matrix operand.', C: 'Empirical constant.',
  xvals: 'Independent-variable data (vector).', yvals: 'Dependent-variable data (vector).',
  x1: 'First value.', x2: 'Second value.', m: 'Shape / form parameter.', col: 'Name of a result-table column.',
  p: 'Probability (0–1) / percentile rank.', xv: 'Point at which to evaluate.', val: 'Target value to cross.',
  theta_deg: 'Angle [deg].', term: 'Series-term expression.', run: 'Parametric run index.', row: 'Row index (1-based).',
  rel_rough: 'Relative wall roughness ε/D.', lo: 'Lower bound.', hi: 'Upper bound.', i: 'Index.', df: 'Degrees of freedom.',
  Ke: 'Exit (expansion) loss coefficient.', Kc: 'Contraction (entrance) loss coefficient.', K: 'Loss coefficient / gain.',
  beta_deg: 'Chevron / wave angle [deg].', AoverAc: 'Area ratio A/Ac.', sys1: 'First system (num, den).', sys2: 'Second system (num, den).',
  "'col'": 'Name of a result-table column (string).', "'t'": 'Name of a TABLE block (string).',
  '...': 'Additional values (variadic).', 'α': 'Scalar coefficient α.', 'β': 'Scalar coefficient β.', 'arr[1:n]': 'Array range, e.g. data[1:n].',
};
// Control-systems matrices (category-specific meaning of A/B/C/D/Q/R/...).
const G_CTRL = {
  A: 'State matrix.', B: 'Input matrix.', C: 'Output matrix.', D: 'Direct feedthrough term.',
  Q: 'State weighting matrix (⪰ 0).', R: 'Input weighting / measurement-noise matrix.', G: 'Process-noise input matrix.',
  M: 'Input matrix (B for controllability, C for observability).', P: 'Similarity transform (x = P·z).',
  t: 'Time samples [s].', y: 'Output samples.',
};
const G_MATRIX = { A: 'Square input matrix.', B: 'Matrix / vector operand.', v: 'Vector.', n: 'Dimension.', m: 'Row count.' };

const isCtrl = (cat) => /Control/.test(cat);
const isMat = (cat) => /Matrix/.test(cat);

function optsFor(fn, arg) {
  const a = arg;
  const o = (OPT_FN[fn] && OPT_FN[fn][a]) || OPT_GLOBAL[a];
  return o ? `One of ${o.map((x) => '`' + x + '`').join(', ')}.` : null;
}

function argDesc(fn, cat, arg, isStr) {
  const key = arg.replace(/`/g, '');
  const opt = optsFor(fn, key);
  if (opt) return (key === 'type$' || key === 'regime$' || key === 'geom$' || key === 'branch$' || key === 'arr$' || key === 'surface$' || key === 'phase$' || key === 'model$')
    ? `Selector — ${opt}` : opt;
  if (isCtrl(cat) && G_CTRL[key]) return G_CTRL[key];
  if (isMat(cat) && G_MATRIX[key]) return G_MATRIX[key];
  if (G[key]) return G[key];
  if (key.endsWith('$')) return 'String argument.';
  return null; // leave unknown numeric as-is (rare)
}

// ── Component parameter glossary ─────────────────────────────────────────────
const CP = {
  eta: 'Efficiency (0–1).', eta_v: 'Volumetric efficiency (0–1).', eta_fin: 'Fin efficiency (0–1).',
  R: 'Resistance [Ω].', R0: 'Series (ohmic) resistance [Ω].', R1: 'First RC-branch resistance [Ω].', R2: 'Second RC-branch resistance [Ω].',
  C: 'Capacitance [F].', C1: 'First RC-branch capacitance [F].', C2: 'Second RC-branch capacitance [F].', L: 'Inductance [H].',
  Voc: 'Open-circuit voltage [V].', Vrc1_0: 'Initial first-RC voltage [V].', Vrc2_0: 'Initial second-RC voltage [V].', V0: 'Initial voltage / volume.',
  Kp: 'Proportional gain.', Ki: 'Integral gain.', Kd: 'Derivative gain.', Kv: 'Flow coefficient.', Tref: 'Reference (setpoint) temperature [K].',
  P: 'Pressure [Pa].', P0: 'Reference/initial pressure [Pa].', Patm: 'Atmospheric pressure [Pa].', Pcrack: 'Cracking (relief) pressure [Pa].',
  T: 'Temperature [K].', T0: 'Reference/initial temperature [K].', Tmax: 'Maximum temperature [K].', T_amb: 'Ambient temperature [K].',
  V: 'Volume [m³].', area: 'Area [m²].', L: 'Length [m].', D: 'Diameter [m].',
  UA: 'Overall conductance UA [W/K].', U_tp: 'Two-phase-zone overall coefficient [W/m²·K].', U_sh: 'Superheat-zone overall coefficient [W/m²·K].',
  mdot: 'Mass flow rate [kg/s].', rho: 'Density [kg/m³].', dP: 'Nominal pressure drop [Pa].', dP0: 'Reference pressure drop [Pa].',
  CdA: 'Discharge coefficient × area Cd·A [m²].', rough: 'Relative wall roughness.', eps: 'Effectiveness / roughness.', eps_zone: 'Zone-collapse smoothing width.',
  Q: 'Heat input [W].', Q0: 'Reference heat [W].', SH_set: 'Target superheat [K].', ratio: 'Gear / split ratio.', throttle: 'Throttle (0–1).',
  k: 'Stiffness / conductivity.', g: 'Gravitational acceleration [m/s²].', m: 'Mass [kg].', h: 'Heat-transfer coefficient [W/m²·K].', h0: 'Reference enthalpy [J/kg].',
  disp: 'Displacement volume [m³].', rpm: 'Rotational speed [rpm].', u: 'Specific internal energy [J/kg].',
  Cmin: 'Minimum heat-capacity rate [W/K].', h_target: 'Target value.', N: 'Number of cells/segments.',
  fluid$: 'Fluid name (e.g. Water, R134a, Air).', hot$: 'Hot-side fluid name (e.g. Water).', cold$: 'Cold-side fluid name (e.g. EG50).',
  ref$: 'Refrigerant name (e.g. R134a, R1234yf).', cool$: 'Coolant name (e.g. EG50, Water).', model$: 'Model variant — selects the physics body (see Model Variants).',
  // additional coverage
  x: 'Vapor quality / fraction (0–1).', Cv: 'Flow coefficient.', Cd: 'Discharge coefficient.', CdA0: 'Reference Cd·A [m²].', CdA_max: 'Maximum Cd·A [m²].',
  Crr: 'Rolling-resistance coefficient.', Caero: 'Aerodynamic drag term ½ρCdA [kg/m].', c: 'Damping / specific-heat coefficient.', b: 'Critical pressure ratio / coefficient.',
  z: 'Elevation [m].', y: 'Position / fraction.', w: 'Frequency [rad/s].', w0: 'Natural frequency [rad/s].', w_peak: 'Peak frequency [rad/s].', W: 'Humidity ratio [kg/kg] / work [W].',
  vs: 'Reference / slip velocity [m/s].', v0: 'Initial velocity [m/s].', Vrc0: 'Initial RC-branch voltage [V].',
  U_sc: 'Subcool-zone overall coefficient [W/m²·K].', U_cond: 'Condenser-zone overall coefficient [W/m²·K].', UA_cool: 'Coolant-side conductance [W/K].',
  Tpeak: 'Peak temperature [K].', Tout: 'Outlet temperature [K].', theta0: 'Initial angle [rad].', tau_valve: 'Valve time constant [s].', tau_bulb: 'Bulb time constant [s].',
  SOC0: 'Initial state of charge (0–1).', SH: 'Superheat [K].', SC_set: 'Target subcooling [K].', Rth: 'Thermal resistance [K/W].', Rs: 'Series resistance [Ω].', Rohm: 'Ohmic resistance [Ω].',
  rho_l: 'Liquid density [kg/m³].', rho_g: 'Vapor density [kg/m³].', rho_in: 'Inlet density [kg/m³].', Rgas: 'Specific gas constant [J/kg·K].',
  Qgen: 'Generated heat [W].', Pset: 'Set pressure [Pa].', poles: 'Number of magnetic pole pairs.', P_amb: 'Ambient pressure [Pa].', ncells: 'Number of cells.',
  mu_l: 'Liquid viscosity [Pa·s].', mu_g: 'Vapor viscosity [Pa·s].', mu: 'Dynamic viscosity [Pa·s].', mdot_w: 'Water/coolant mass flow [kg/s].', m0: 'Initial mass [kg].',
  lambda_pm: 'PM flux linkage [Wb].', Kt: 'Torque constant [N·m/A].', kgas: 'Gas specific-heat ratio.', Ke: 'Back-EMF constant [V·s/rad].', J: 'Inertia [kg·m²].',
  ilim: 'Current limit [A].', I0: 'Saturation current [A].', i0: 'Initial current [A].', I: 'Current [A].', h_w: 'Wall heat-transfer coefficient [W/m²·K].', htc: 'Heat-transfer coefficient [W/m²·K].',
  grade: 'Road grade (rise/run).', grad: 'Road grade (rise/run).', Gon: 'On-state conductance [S].', gam: 'Ratio of specific heats.', Fs: 'Static friction force [N].',
  FMEP_b: 'Friction-MEP slope coefficient.', FMEP_a: 'Friction-MEP constant [Pa].', Fc: 'Coulomb friction force [N].', F: 'Force [N].', Eth: 'Activation/threshold energy.',
  eta_t: 'Turbine efficiency (0–1).', eta_m: 'Mechanical efficiency (0–1).', eta_c: 'Compressor efficiency (0–1).', eps_air: 'Air-side effectiveness.', eng: 'Engagement fraction (0–1).',
  emis: 'Emissivity (0–1).', E0: 'Reference EMF [V].', E: 'EMF / voltage [V].', C_th: 'Thermal capacitance [J/K].', Cmax: 'Maximum capacity rate [W/K].',
  bv: 'Viscous-friction coefficient.', bf: 'Friction coefficient.', beta: 'Chevron angle [deg] / coefficient.', A_throat: 'Throat area [m²].', A_exit: 'Exit area [m²].',
  alpha: 'Void fraction / coefficient.', A: 'Area [m²].', V0: 'Initial volume [m³] / voltage [V].',
  cp: 'Specific heat [J/kg·K].', K: 'Gain / coefficient.', kk: 'Ratio of specific heats.',
};
function compParamDesc(p) {
  const key = p.replace(/`/g, '');
  const o = OPT_COMP[key];
  if (o) return `${o.label} — one of ${o.opts.map((x) => '`' + x + '`').join(', ')}.`;
  if (CP[key]) return CP[key];
  if (key.endsWith('$')) return 'Name/selector string (see the constitutive equations for usage).';
  return '—';
}

// ── Rewrite a page ───────────────────────────────────────────────────────────
let fixedArgs = 0, fixedComp = 0;
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
  const p = path.join(d, e.name);
  if (e.isDirectory()) return walk(p);
  if (!e.name.endsWith('.md') || e.name.startsWith('_')) return;
  let src = fs.readFileSync(p, 'utf-8');
  const fn = (src.match(/^name:\s*(.+)$/m) || [])[1]?.trim().toLowerCase();
  const cat = (src.match(/^category:\s*(.+)$/m) || [])[1]?.trim() || '';
  let changed = false;

  // 1. Input Arguments — rewrite generic descriptions.
  src = src.replace(/(\| `([^`]+)` \| (String|Number) \| Yes \| )(Numeric argument\.|String argument\.)( \|)/g,
    (full, pre, arg, type, _old, post) => {
      const d2 = argDesc(fn, cat, arg, type === 'String');
      if (!d2) return full; changed = true; fixedArgs++;
      return pre + d2 + post;
    });

  // 2. Output Arguments — replace generic "Output value."
  src = src.replace(/(\| `([^`]+)` \| Number\/Array \| )Output value\.( \|)/g, (full, pre, arg, post) => {
    const key = arg.replace(/`/g, '');
    const d2 = (isCtrl(cat) && G_CTRL[key]) || G[key] || `Computed \`${key}\`.`;
    changed = true; return pre + d2 + post;
  });

  // 3. Component Parameters — add a Description column to the 2-col table.
  src = src.replace(/## Parameters\n\n\| Parameter \| Type \|\n\| --- \| --- \|\n((?:\| [^\n]*\|\n)+)/,
    (full, rows) => {
      const newRows = rows.trimEnd().split('\n').map((r) => {
        const m2 = r.match(/^\| `([^`]+)` \| (\w+) \|$/);
        if (!m2) return r;
        return `| \`${m2[1]}\` | ${m2[2]} | ${compParamDesc(m2[1])} |`;
      }).join('\n');
      changed = true; fixedComp++;
      return '## Parameters\n\n| Parameter | Type | Description |\n| --- | --- | --- |\n' + newRows + '\n';
    });

  // 3b. Fill any remaining placeholder '—' cells in 3-column Parameters tables.
  src = src.replace(/^\| `([^`]+)` \| (\w+) \| — \|$/gm, (full, name, type) => {
    const d2 = compParamDesc(name);
    if (d2 === '—') return full; changed = true;
    return `| \`${name}\` | ${type} | ${d2} |`;
  });

  if (changed) fs.writeFileSync(p, src);
});
walk(REF);
console.log(`fix-arg-descriptions: rewrote ${fixedArgs} argument descriptions, ${fixedComp} component parameter tables.`);
