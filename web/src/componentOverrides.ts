// Optional presentation polish for the Component Browser/Wizard, layered on top
// of the auto-generated componentCatalog. A component WITHOUT an entry renders
// as a single flat parameter list (works for all 136). An entry can:
//   - group params into labeled sections (Fluids / Geometry / Performance …),
//   - attach an illustration key (a placeholder box until real art exists),
//   - add a short note shown under the header.
//
// Extensibility hook: polishing a component is just adding one object here — no
// changes to the modal or the generator. Param names must match the catalog
// (a build-time-independent map; unknown names are ignored, leftover params
// fall into an "Other" section automatically).
export interface ComponentSection {
  title: string
  params: string[]
}

export interface ComponentOverride {
  sections?: ComponentSection[]
  illustration?: string // asset/illustration key; placeholder for now
  notes?: string
}

export const COMPONENT_OVERRIDES: Record<string, ComponentOverride> = {
  Chiller: {
    sections: [
      { title: 'Fluids', params: ['ref$', 'cool$'] },
      { title: 'Geometry', params: ['D', 'L'] },
      { title: 'Performance', params: ['U_tp', 'U_sh', 'eps_zone', 'UA_cool'] },
    ],
    illustration: 'heat-exchanger',
    notes: 'Two-phase refrigerant evaporator coupled to a single-phase coolant loop.',
  },
  HeatExchanger: {
    sections: [
      { title: 'Fluids', params: ['hot$', 'cold$'] },
      { title: 'Configuration', params: ['arr$'] },
      { title: 'Performance', params: ['UA'] },
    ],
    illustration: 'heat-exchanger',
    notes: 'ε-NTU heat exchanger; arr$ selects the flow arrangement (counterflow / parallel / crossflow).',
  },
  TwoZoneHX: {
    sections: [
      { title: 'Fluids', params: ['hot$', 'cold$'] },
      { title: 'Configuration', params: ['arr$'] },
      { title: 'Performance', params: ['UA'] },
    ],
    illustration: 'heat-exchanger',
  },
  MovingBoundaryEvaporator: {
    sections: [
      { title: 'Fluid', params: ['fluid$'] },
      { title: 'Geometry', params: ['D', 'L'] },
      { title: 'Performance', params: ['U_tp', 'U_sh', 'eps_zone'] },
    ],
    illustration: 'heat-exchanger',
    notes: 'Moving-boundary two-phase evaporator with a wall heat port.',
  },
  MovingBoundaryCondenser: {
    sections: [
      { title: 'Fluid', params: ['fluid$'] },
      { title: 'Geometry', params: ['D', 'L'] },
      { title: 'Performance', params: ['U_tp', 'U_sh', 'eps_zone'] },
    ],
    illustration: 'heat-exchanger',
    notes: 'Moving-boundary two-phase condenser with a wall heat port.',
  },
  LiquidWallHX: {
    sections: [
      { title: 'Fluid', params: ['fluid$'] },
      { title: 'Performance', params: ['UA'] },
    ],
    illustration: 'heat-exchanger',
    notes: 'Single-phase coolant heat exchanger with a wall heat port (pairs with a refrigerant evaporator).',
  },
}
