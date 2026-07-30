// Promote the 136 component baseline pages to rich pages. Re-reads each component
// from the .frees std-lib (ports, parameters, constitutive equations, variants —
// verbatim, authoritative), and adds a curated physical description, domain port
// semantics, a worked [Run:] example where one exists in examples.ts, and a
// reference. Drops `generated: true`.
//
// Run: node scripts/build-doc-manifest.mjs && node scripts/enrich-components.mjs

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REF = path.join(__dirname, '../src/docs/reference');
const REPO = path.resolve(__dirname, '../..');
const SRC = path.join(__dirname, '../src');
const read = (p) => fs.readFileSync(p, 'utf-8');

// ── Example bindings ─────────────────────────────────────────────────────────
const exSrc = read(path.join(SRC, 'examples.ts'));
const exBlocks = [];
{ const re = /id:\s*'([^']+)'([\s\S]*?)(?=\n  \},)/g; let m;
  while ((m = re.exec(exSrc)) !== null) exBlocks.push([m[1], m[2]]); }
const boundExamples = (name) => exBlocks.filter(([, t]) => new RegExp('\\b' + name + '\\b').test(t)).map(([id]) => id);

// ── Domain port semantics + references ───────────────────────────────────────
const DOMAIN = {
  fluid: { across: 'thermofluid ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`; a node enforces equal `P` and `Σṁ = 0`', ref: 'Standard fluid-mechanics literature' },
  ac: { across: 'refrigerant/air ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`', ref: 'Standard refrigeration-engineering literature' },
  electrical: { across: 'electrical ports carry potential `V` and current `I`; a node enforces equal `V` and `ΣI = 0` (Kirchhoff)', ref: 'Standard circuit-analysis literature' },
  twophase: { across: 'two-phase refrigerant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h` (quality/void follow from the properties)', ref: 'Standard two-phase flow and boiling/condensation literature' },
  mechanical: { across: 'rotational ports carry angular velocity `ω` and torque `τ` (`Στ = 0`); translational ports carry velocity `v` and force `F` (`ΣF = 0`)', ref: 'Standard system-dynamics literature' },
  heat: { across: 'thermal ports carry temperature `T` and heat-flow rate `Q̇`; a node enforces equal `T` and `ΣQ̇ = 0`', ref: 'Standard heat-transfer literature' },
  moistair: { across: 'humid-air ports carry pressure `P`, dry-air mass-flow `ṁ_da`, enthalpy `h`, and humidity ratio `W`', ref: 'Standard psychrometrics literature' },
  powertrain: { across: 'rotational ports carry angular velocity `ω` and torque `τ`, with vehicle-level speed/force signals', ref: 'Standard vehicle-propulsion-systems literature' },
  pneumatic: { across: 'compressible-gas ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h` (ISO 6358 flow)', ref: 'ISO 6358 — Pneumatic fluid power: flow-rate characteristics' },
  hydraulic: { across: 'oil-hydraulic ports carry pressure `P`, mass-flow `ṁ`, and enthalpy `h`', ref: 'Standard hydraulic-control literature' },
  liquid: { across: 'single-phase liquid-coolant ports carry pressure `P`, mass-flow `ṁ`, and specific enthalpy `h`', ref: 'Standard heat-transfer literature' },
  control: { across: 'signal ports carry the measured and commanded scalar values', ref: 'Standard control-systems literature' },
};
const FORMALISM = 'Bond-graph / acausal lumped-parameter system-modeling formalism (standard system-dynamics literature)';

// ── Per-component one-line physical descriptions ─────────────────────────────
const CDESC = {
  // fluid
  Accumulator: 'A fluid accumulator — a compliance volume that stores fluid under pressure and buffers flow transients.',
  Boiler: 'Adds heat to a fluid stream, raising its enthalpy (and generating vapor at saturation).',
  Compressor: 'Raises the pressure of a fluid stream, computing the work from an isentropic efficiency.',
  Condenser: 'Rejects heat from a fluid stream to a coolant/ambient, condensing it.',
  Duct: 'A flow passage that imposes a pressure drop on the stream.',
  ExpansionValve: 'Throttles a fluid to a lower pressure isenthalpically (Joule–Thomson).',
  Fan: 'Adds a pressure rise to a gas/air stream, computing the fan work.',
  FanCurve: 'A fan whose pressure rise follows a tabulated pressure–flow performance curve.',
  FlowSensor: 'Measures the mass flow of a stream (a pass-through sensor).',
  HeatExchanger: 'Transfers heat between two fluid streams across a wall.',
  Mixer: 'Combines two fluid streams into one, with flow-weighted enthalpy mixing.',
  Nozzle: 'Accelerates a flow, converting enthalpy into kinetic energy.',
  Pipe: 'A flow passage that imposes a frictional pressure drop.',
  Pump: 'Raises the pressure of a liquid stream, computing the work from a pump efficiency.',
  Sink: 'A fluid boundary that absorbs a stream at a set pressure.',
  Source: 'A fluid boundary that supplies a stream at set conditions.',
  Splitter: 'Divides a fluid stream into two branches.',
  Throttle: 'An isenthalpic pressure-reducing restriction.',
  Turbine: 'Extracts work from an expanding fluid stream, computing it from an isentropic efficiency.',
  Turbocharger: 'A turbine-driven compressor pair coupled on a common shaft.',
  TwoZoneHX: 'A two-zone heat exchanger resolving distinct thermal regions.',
  Valve: 'A flow restriction characterized by a flow/pressure-drop coefficient.',
  // ac
  AirCoil: 'An air-to-refrigerant coil (the air side of an evaporator or condenser).',
  Chiller: 'A refrigerant-to-coolant chiller transferring heat between the two loops.',
  EXV: 'An electronic expansion valve with a commanded opening.',
  TXV: 'A thermostatic expansion valve that meters refrigerant to hold a target superheat.',
  // electrical
  Battery: 'An electrical battery modeled as an EMF in series with an internal resistance.',
  Battery2RC: 'A battery with two RC branches for second-order transient terminal behavior.',
  BatteryRC: 'A battery with one RC branch for first-order transient terminal behavior.',
  BatteryThermal: 'A battery with a coupled thermal model relating losses to temperature.',
  BatteryTransient: 'A transient battery model carrying state-of-charge dynamics.',
  Capacitor: 'A capacitor storing charge, with `i = C dV/dt`.',
  CurrentSource: 'An ideal current source.',
  DCMotor: 'A DC motor — an electrical-to-mechanical transducer (back-EMF and torque constants).',
  Diode: 'A nonlinear diode with an exponential current–voltage characteristic.',
  FuelCellStack: 'A PEM fuel-cell stack producing voltage from its polarization curve.',
  Ground: 'The electrical reference node (`V = 0`).',
  HeatingResistor: 'A resistor that dissipates its electrical power as heat (electrical→thermal transducer).',
  Inductor: 'An inductor storing magnetic energy, with `V = L di/dt`.',
  PMSM: 'A permanent-magnet synchronous motor.',
  Resistor: 'An Ohmic resistor, `V = R·I`.',
  VoltageSource: 'An ideal voltage source.',
  // twophase
  BlendMixer: 'A gas-blend (mixture) mixing junction carrying the species rider.',
  BlendSensor: 'A sensor reading the state of a gas-blend stream.',
  BlendSink: 'A boundary absorbing a gas-blend stream.',
  BlendSource: 'A boundary supplying a gas-blend stream of set composition.',
  BoilingVessel: 'A rigid vessel boiling a two-phase fluid (rigid two-phase boil-off).',
  MovingBoundaryCondenser: 'A moving-boundary condenser tracking the two-phase/subcooled zone lengths.',
  MovingBoundaryEvaporator: 'A moving-boundary evaporator tracking the two-phase/superheat zone lengths.',
  ProportionalReliefValve: 'A pressure-relief valve whose opening rises proportionally above the set pressure.',
  SteamReliefValve: 'A steam relief valve venting above the set pressure.',
  ThreeZoneHX: 'A three-zone (subcooled / two-phase / superheat) heat exchanger.',
  TwoPhaseCap: 'A two-phase capacitive volume (a pressure-compliance node).',
  TwoPhaseChamber: 'A two-phase control volume.',
  TwoPhaseCompressor: 'A refrigerant compressor with selectable isentropic/volumetric variants.',
  TwoPhaseCondenser: 'A two-phase condenser rejecting heat from the refrigerant.',
  TwoPhaseCondenserFloat: 'A two-phase condenser whose pressure floats with the charge/ambient balance.',
  TwoPhaseCondenserUA: 'A two-phase condenser sized by an overall conductance `UA`.',
  TwoPhaseEnthalpySource: 'A two-phase boundary fixing the stream enthalpy.',
  TwoPhaseEvaporator: 'A two-phase evaporator absorbing heat into the refrigerant.',
  TwoPhaseEvaporatorUA: 'A two-phase evaporator sized by an overall conductance `UA`.',
  TwoPhaseExpansionValve: 'A refrigerant expansion valve (isenthalpic throttle).',
  TwoPhaseFlowRes: 'A two-phase flow resistance relating pressure drop to mass flow.',
  TwoPhaseInventory: 'Tracks the refrigerant charge inventory across the circuit.',
  TwoPhaseMixer: 'Mixes two two-phase streams with flow-weighted enthalpy.',
  TwoPhasePipe: 'A two-phase pipe with a Lockhart–Martinelli frictional pressure drop.',
  TwoPhasePressureSink: 'A two-phase boundary fixing the pressure (sink).',
  TwoPhasePressureSource: 'A two-phase boundary fixing the pressure (source).',
  TwoPhaseReceiver: 'A liquid receiver buffering refrigerant charge at saturation.',
  TwoPhaseSensor: 'A sensor reading the two-phase stream state.',
  TwoPhaseSink: 'A boundary absorbing a two-phase stream.',
  TwoPhaseSource: 'A boundary supplying a two-phase stream.',
  TwoPhaseSourcePH: 'A two-phase source specified by pressure and enthalpy `(P, h)`.',
  TwoPhaseVolume: 'A finite-volume two-phase control volume with mass and energy states (`(p, h)` states).',
  TXVSuperheat: 'A thermostatic expansion valve that meters flow to hold a target superheat.',
  // mechanical
  Clutch: 'A friction clutch coupling/decoupling two rotational shafts.',
  ForceSource: 'A prescribed translational force.',
  Friction: 'A friction element opposing motion.',
  Gear: 'A gear pair imposing a fixed speed/torque ratio between two shafts.',
  Inertia: 'A rotational inertia, `τ = J dω/dt`.',
  MechGround: 'The rotational reference (`ω = 0`).',
  Planetary: 'A planetary gearset relating sun, ring, and carrier speeds.',
  RotationalDamper: 'A rotational viscous damper, `τ = c·ω`.',
  RotationalSpring: 'A torsional spring, `τ = k·θ`.',
  SpeedSource: 'A prescribed angular velocity.',
  TorqueSource: 'A prescribed torque.',
  TransDamper: 'A translational viscous damper, `F = c·v`.',
  TransGround: 'The translational reference (`v = 0`).',
  TransMass: 'A translational mass, `F = m dv/dt`.',
  // heat
  Conduction: 'A conductive thermal resistance (Fourier), `Q̇ = (T1 − T2)/R`.',
  ContactResistance: 'A thermal contact resistance between two surfaces.',
  Convection: 'A convective link (Newton’s law of cooling), `Q̇ = h·A·ΔT`.',
  HeatSource: 'A prescribed heat input to a thermal node.',
  MassGen: 'A mass/heat generation source term.',
  Radiation: 'A radiative exchange link (Stefan–Boltzmann), `Q̇ = εσA(T1⁴ − T2⁴)`.',
  ThermalMass: 'A lumped thermal capacitance, `C dT/dt = Q̇`.',
  ThermalSensor: 'A temperature sensor (pass-through).',
  ThermalSource: 'A prescribed-temperature boundary.',
  // moistair
  CoolingCoil: 'Cools and (below dew point) dehumidifies a humid-air stream.',
  HeatingCoil: 'Heats a humid-air stream at constant humidity ratio.',
  Humidifier: 'Adds moisture to a humid-air stream, raising its humidity ratio.',
  MixingBox: 'Mixes two humid-air streams with flow-weighted enthalpy and humidity ratio.',
  MoistAirSink: 'A humid-air boundary absorbing a stream.',
  MoistAirSource: 'A humid-air boundary supplying a stream of set state.',
  MoistAirWallHX: 'A humid-air-to-wall heat exchanger.',
  // powertrain
  Engine: 'An internal-combustion engine acting as a torque source.',
  GradeRoadLoad: 'A vehicle road load including the road-grade contribution.',
  MeanValueEngine: 'A mean-value engine model (cycle-averaged torque and flows).',
  RoadLoad: 'A vehicle road load (aerodynamic drag + rolling resistance).',
  Transmission: 'A gearbox/transmission imposing a ratio between engine and wheels.',
  // pneumatic
  GasMixer: 'Mixes pneumatic gas streams, carrying the species composition rider.',
  GasPipe: 'A pneumatic pipe with compressible-flow pressure drop.',
  GasSource: 'A boundary supplying gas at set conditions.',
  PneumaticActuator: 'A pneumatic cylinder/actuator converting pressure to force.',
  PneumaticAtmosphere: 'An atmospheric (ambient-pressure) pneumatic boundary.',
  PneumaticOrifice: 'A pneumatic orifice metering flow by ISO 6358 (sonic conductance).',
  PneumaticServoValve: 'A pneumatic servo valve with a commanded spool position.',
  PneumaticSupply: 'A pneumatic pressure supply.',
  PneumaticVolume: 'A pneumatic control volume (compressible capacitance).',
  // hydraulic
  HydraulicCylinder: 'A hydraulic actuator converting flow/pressure to motion/force.',
  HydraulicOrifice: 'A hydraulic orifice metering flow by `ṁ ∝ √Δp`.',
  HydraulicPump: 'A hydraulic pump delivering flow against pressure.',
  HydraulicSupply: 'A hydraulic pressure supply.',
  HydraulicTank: 'A hydraulic reservoir at (near) atmospheric pressure.',
  HydraulicValve: 'A hydraulic valve metering flow vs. pressure drop.',
  ReliefValve: 'A pressure-relief valve that opens above its set pressure.',
  // liquid
  LiquidColdPlate: 'A liquid cold plate cooling an electronics/heat load.',
  LiquidMixer: 'Mixes two single-phase liquid streams.',
  LiquidOrifice: 'A liquid orifice metering flow vs. pressure drop.',
  LiquidPipe: 'A single-phase liquid pipe with frictional pressure drop.',
  LiquidPump: 'A single-phase liquid pump.',
  LiquidSink: 'A liquid boundary absorbing a stream.',
  LiquidSource: 'A liquid boundary supplying a stream of set state.',
  LiquidVolume: 'A single-phase liquid control volume.',
  LiquidWallHX: 'A liquid-to-wall heat exchanger.',
  // control
  PIThermostat: 'A proportional–integral thermostat controller driving an actuator to a setpoint.',
};

// ── Parse components from .frees (balanced parser) ───────────────────────────
const compDir = path.join(REPO, 'backend/core/src/main/resources/components');
const compInfo = {};
for (const file of fs.readdirSync(compDir).filter((f) => f.endsWith('.frees'))) {
  const domain = file.replace(/\.frees$/, '');
  const lines = read(path.join(compDir, file)).split('\n');
  for (let i = 0; i < lines.length; i++) {
    const head = lines[i].match(/^\s*COMPONENT\s+(\w+)\s*(?:\(([^)]*)\))?/);
    if (!head) continue;
    const name = head[1];
    const ports = (head[2] || '').split(',').map((s) => s.trim()).filter(Boolean);
    const params = []; const shared = []; const variants = [];
    let cur = null; let depth = 0;
    for (i++; i < lines.length; i++) {
      const raw = lines[i].replace(/\s+$/, ''); const l = raw.trim();
      if (/^END\b/.test(l)) { if (depth === 0) break; depth--; cur = null; continue; }
      const vm = l.match(/^VARIANT\s+(\w+)(?:\s+REQUIRE\s+(.+))?/);
      if (vm) { depth++; cur = { name: vm[1], require: (vm[2] || '').split(',').map((s) => s.trim()).filter(Boolean), eqs: [] }; variants.push(cur); continue; }
      const pm = l.match(/^PARAM\s+(.+)$/);
      if (pm) { pm[1].split(',').forEach((p) => params.push(p.trim())); continue; }
      if (!l || /^(REQUIRE|OUTPUT|MODEL|\{|\}|\/\/)/.test(l)) continue;
      (cur ? cur.eqs : shared).push(raw.replace(/^\s{0,4}/, ''));
    }
    compInfo[name] = { domain, ports, params, shared, variants };
  }
}

let done = 0;
for (const [name, info] of Object.entries(compInfo)) {
  const file = path.join(REF, 'components', info.domain, name.replace(/[^A-Za-z0-9_]/g, '_') + '.md');
  if (!fs.existsSync(file)) continue;
  const dom = DOMAIN[info.domain] || { across: 'typed acausal ports', ref: '' };
  const desc = CDESC[name] || `A reusable acausal ${info.domain}-domain component.`;
  const ex = boundExamples(name);
  const paramRows = info.params.length
    ? ['## Parameters', '', '| Parameter | Type |', '| --- | --- |', ...info.params.map((p) => { const nm = p.split('=')[0].trim(); return `| \`${nm}\` | ${nm.endsWith('$') ? 'String' : 'Number'} |`; }), '']
    : [];
  const eqRows = info.shared.length ? ['## Constitutive Equations', '', 'Instantiating the component expands these acausal equations (over its port members and parameters) into scalar equations solved by the standard Newton/Tarjan pipeline:', '', '```', ...info.shared, '```', ''] : [];
  const variantRows = info.variants.length
    ? ['## Model Variants', '', 'Selected via the `model$` parameter; each adds its own equations (and `REQUIRE`d parameters):', '',
        ...info.variants.flatMap((v) => [`### \`${v.name}\`${v.require.length ? ' — requires `' + v.require.join('`, `') + '`' : ''}`, '', ...(v.eqs.length ? ['```', ...v.eqs, '```'] : ['_No additional equations (uses the shared body)._']), ''])]
    : [];
  const exRows = ex.length ? ['## Examples', '', `Instantiated in the verified example below:`, '', `[Run: ${ex[0]}]`, ''] : [];
  const refs = [FORMALISM, ...(dom.ref ? [dom.ref] : [])];

  const body = [
    '---',
    `name: ${name}`,
    `category: Component (${info.domain})`,
    `summary: ${desc.replace(/`/g, '')}`,
    `related: []`,
    `examples: [${ex.join(', ')}]`,
    `tags: [${[name.toLowerCase(), 'component', info.domain, 'acausal'].join(', ')}]`,
    'references:',
    ...refs.map((r) => `  - "${r.replace(/[*`]/g, '')}"`),
    '---', '',
    `# ${name}`, '',
    desc, '',
    `## Domain`, '',
    `A reusable **acausal ${info.domain}-domain** component — its ${dom.across}. Instantiate it and connect its ports; the constitutive equations below expand into the global scalar system.`, '',
    ...(info.ports.length ? ['## Ports', '', '`' + info.ports.join('`, `') + '`', ''] : []),
    `## Usage`, '', '```', `${name} inst(${info.params.map((p) => p.split('=')[0].trim()).join(', ') || '...'})`, '```', '',
    ...paramRows, ...eqRows, ...variantRows, ...exRows,
    '## References', '', ...refs.map((r, i) => `${i + 1}. ${r.replace(/`/g, '')}.`), '',
  ].join('\n');
  fs.writeFileSync(file, body);
  done++;
}
console.log(`enrich-components: ${done} component pages promoted to rich.`);
