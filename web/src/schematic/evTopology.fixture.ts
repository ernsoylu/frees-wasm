// Connection topology of the shipped EV thermal-management example, captured
// from the expander itself (SolveDtos.connectionsOf on the example document),
// so the drawing is tested against what the backend really sends.
//
// This is the acceptance case for a rendered schematic: a real two-loop system
// whose loops are INDISTINGUISHABLE by bond-graph domain — both are
// `domain: 'fluid'` — and only tell apart by connector type and working fluid
// (EG50 coolant vs. R1234yf refrigerant), plus a heat-domain radiator bank and
// a chiller wall that bridges the two loops.
import type { Connection } from './layout'

export const EV_TOPOLOGY: Connection[] = [
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["pumpin.out", "pump.in"], streams: ["pumpin.out", "pump.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["pump.out", "obat.in", "omot.in"], streams: ["pump.out", "obat.in", "omot.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["obat.out", "bcp.in"], streams: ["obat.out", "bcp.in"] },
  { domain: "heat", connector: null, fluid: null, endpoints: ["bcp.wall", "batt.port"], streams: ["bcp.wall", "batt.port"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["bcp.out", "chlc.in"], streams: ["bcp.out", "chlc.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["chlc.out", "mix.in1"], streams: ["chlc.out", "mix.in1"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["omot.out", "mcp.in"], streams: ["omot.out", "mcp.in"] },
  { domain: "heat", connector: null, fluid: null, endpoints: ["mcp.wall", "motor.port"], streams: ["mcp.wall", "motor.port"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["mcp.out", "mix.in2"], streams: ["mcp.out", "mix.in2"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["mix.out", "rad1.in"], streams: ["mix.out", "rad1.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["rad1.out", "or1.in"], streams: ["rad1.out", "or1.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["or1.out", "rad2.in"], streams: ["or1.out", "rad2.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["rad2.out", "or2.in"], streams: ["rad2.out", "or2.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["or2.out", "rad3.in"], streams: ["or2.out", "rad3.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["rad3.out", "or3.in"], streams: ["rad3.out", "or3.in"] },
  { domain: "fluid", connector: "liquid", fluid: "eg50", endpoints: ["or3.out", "pumpout.in"], streams: ["or3.out", "pumpout.in"] },
  { domain: "heat", connector: null, fluid: null, endpoints: ["amb.port", "rad1.wall", "rad2.wall", "rad3.wall"], streams: ["amb.port", "rad1.wall", "rad2.wall", "rad3.wall"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["feed.out", "chlr.in", "cabe.in"], streams: ["feed.out", "chlr.in", "cabe.in"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["chlr.out", "suc.in1"], streams: ["chlr.out", "suc.in1"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["cabe.out", "suc.in2"], streams: ["cabe.out", "suc.in2"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["suc.out", "cmp.in"], streams: ["suc.out", "cmp.in"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["cmp.out", "cond.in"], streams: ["cmp.out", "cond.in"] },
  { domain: "fluid", connector: "twophase", fluid: "r1234yf", endpoints: ["cond.out", "liq.in"], streams: ["cond.out", "liq.in"] },
  { domain: "heat", connector: null, fluid: null, endpoints: ["cabe.wall", "cabin.port"], streams: ["cabe.wall", "cabin.port"] },
  { domain: "heat", connector: null, fluid: null, endpoints: ["chlr.wall", "chlc.wall"], streams: ["chlr.wall", "chlc.wall"] },
]
