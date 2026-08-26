[Topic: thermo]
# Thermophysical Properties Reference (Real Fluids & Gas)

frees ships a high-precision fluid-properties database. **Every property function returns a value in SI base units** (J/kg, K, Pa, kg/m³, …) regardless of the units you annotate inputs with — annotate inputs for convenience, then convert outputs if you need engineering units.

## Material classes
1. **Real fluids** — multi-phase fluids such as `Water`, `R134a`, `Ammonia`, `CarbonDioxide`. Each lookup needs exactly **two** independent properties.
2. **Ideal gases** — spelled formulas (`CO2`, `N2`, `CH4`, `O2`, `Air`) evaluated with NASA thermodynamic polynomials. Use these for gas-cycle work where real-fluid effects are negligible.
3. **Aqueous glycols** — incompressible coolants written as base + mass percent: `EG50` (50% ethylene glycol), `PG30` (30% propylene glycol). Queries need Temperature (`T`) and Pressure (`P`).

## Specifying a state
Pass the fluid name first, then **two** named coordinates. The recognized coordinate keys are `T` (temperature), `P` (pressure), `h` (enthalpy), `s` (entropy), `v` (specific volume), `x` (quality: 0 saturated liquid, 1 saturated vapour), `u` (internal energy), `D` (density).

```
{ Boiler exit: superheated steam }
h3 = Enthalpy(Water, P=P_boiler, T=480 [C])     { J/kg, from kPa + C inputs }
s3 = Entropy(Water, P=P_boiler, T=480 [C])

{ Condenser exit: saturated liquid (x = 0) }
h1 = Enthalpy(Water, P=P_cond, x=0)
v1 = Volume(Water, P=P_cond, x=0)

{ Turbine exit: isentropic, so s4 = s3 -> P and s fix the state }
h4 = Enthalpy(Water, P=P_cond, s=s3)
```

## Standard property functions
Every function takes the fluid name first, then coordinates:

| Function | Property | SI Unit | Example |
| --- | --- | --- | --- |
| `Temperature` | Absolute temperature | K | `T = Temperature(Water, P=P1, h=h1)` |
| `Pressure` | Absolute pressure | Pa | `P = Pressure(R134a, T=T1, x=1)` |
| `Enthalpy` | Specific enthalpy | J/kg | `h = Enthalpy(Steam, T=T1, P=P1)` |
| `Entropy` | Specific entropy | J/kg-K | `s = Entropy(Nitrogen, T=T1, P=P1)` |
| `Density` | Mass density | kg/m³ | `rho = Density(EG50, T=T1, P=P1)` |
| `Volume` | Specific volume | m³/kg | `v = Volume(Water, P=P1, x=0)` |
| `IntEnergy` | Specific internal energy | J/kg | `u = IntEnergy(Water, T=T1, P=P1)` |
| `Gibbs` | Specific Gibbs free energy | J/kg | `g = Gibbs(Water, T=T1, P=P1)` |
| `Cp` / `Specheat` | Specific heat $C_p$ | J/kg-K | `c = Cp(Air, T=T1, P=P1)` |
| `Cv` | Specific heat $C_v$ | J/kg-K | `c_v = Cv(Air, T=T1, P=P1)` |
| `Viscosity` | Dynamic viscosity | Pa-s | `mu = Viscosity(Water, T=T1, P=P1)` |
| `Conductivity` | Thermal conductivity | W/m-K | `k = Conductivity(Water, T=T1, P=P1)` |
| `SoundSpeed` | Speed of sound | m/s | `c = SoundSpeed(Air, T=T1, P=P1)` |

[Diagram: DependentProperties]

> **Common pitfall:** because outputs are SI, a Rankine-cycle efficiency `eta = (h3-h4-w_pump)/(h3-h2)` is dimensionless and correct as-is — but if you want results in kJ/kg for display, divide the enthalpies by 1000 (or annotate a derived variable `[kJ/kg]`).

## Thermophysical utility functions
- **`P_sat(Fluid, T=t)`** — saturation pressure at temperature $T$.
- **`T_sat(Fluid, P=p)`** — saturation temperature at pressure $P$.
- **`MolarMass(Fluid)`** — molar mass (kg/mol) of real fluids, ideal-gas species, or arbitrary formulas (`C8H18`, `Ca(OH)2`).
- **`HeatingValue(Fuel, 'LHV'|'HHV')`** — lower or higher heating value (J/kg).
- **`StoichAFR(Fuel)`** — stoichiometric air-fuel ratio (mass basis).
- **`IsIdealGas(Fluid)`** — `1` if treated as ideal, else `0`.
- **`Phase$(Fluid, T=t, P=p)`** — phase string: `'liquid'`, `'gas'`, `'twophase'`, `'supercritical'`.
- **`P_crit` / `T_crit` / `v_crit` / `T_triple`** — critical and triple-point constants.
- **`CompressibilityFactor(Fluid, T=t, P=p)`** — $Z = Pv/(RT)$.
- **`StagnationTemp(T, V, cp)`** — $T_0 = T + V^2/(2c_p)$ (K).
- **`StagnationPres(P, T, T0, k)`** — $P_0 = P(T_0/T)^{k/(k-1)}$ (Pa).
- **`SurfaceTension(Fluid, T=t)`** — surface tension (N/m).

[Related: humidair, state-tables, chemistry]

[Topic: solid-materials]
# Solid Material Properties Reference

frees carries bulk (room-temperature) physical properties for common engineering solids, so you don't have to look up a conductivity or Young's modulus by hand. Available materials: `Aluminum`, `Copper`, `Steel`, `StainlessSteel`, `Iron`, `Brass`, `Bronze`, `Gold`, `Silver`, `Lead`, `Nickel`, `Titanium`, `Tungsten`, `Zinc`, `Magnesium`, `Concrete`, `Glass`, `Brick`, `Wood`, `Ice`. Values are representative constants.

## Property functions
Each takes the material name as its first argument. The trailing underscore is part of the name.

| Function | Property | Unit |
| --- | --- | --- |
| `k_(Material)` / `k_(Material, T=t)` | thermal conductivity | W/m-K |
| `c_(Material)` / `c_(Material, T=t)` | specific heat | J/kg-K |
| `rho_(Material)` | density | kg/m³ |
| `E_(Material)` | Young's modulus | Pa |
| `nu_(Material)` | Poisson's ratio | — |

`k_` and `c_` accept an optional temperature `T` (kelvin). For the well-characterised metals (aluminum, copper, steel, iron, nickel, titanium, tungsten) a linear correction about the 300 K reference is applied; for other materials, or when `T` is omitted, the room-temperature value is returned. `rho_`, `E_`, and `nu_` are constants.

> A material that doesn't carry a requested property (e.g. `E_(Ice)` exists but `nu_(Brick)` doesn't) raises a clear error rather than returning a wrong value.

## Example
```
{ Steady conduction through an aluminum slab }
T_hot = 400 [K];  T_cold = 300 [K]
L = 0.1 [m];  A = 2 [m^2]
k = k_(Aluminum)                 { ~237 W/m-K }
q = k * A * (T_hot - T_cold) / L { watts }
```

[Related: thermo, chemistry, ref-index]

[Topic: chemistry]
# Chemistry & Combustion

Chemical calculations and combustion analysis for hydrocarbons, alcohols, and common species.

## Molar mass
`MolarMass` resolves real fluids, ideal-gas species, **or** arbitrary chemical formulas straight from the periodic table:
```
M  = MolarMass(CarbonDioxide)   { 0.04401 kg/mol }
M2 = MolarMass(C8H18)           { 0.11423 kg/mol }
M3 = MolarMass('Al2(SO4)3')     { quote formulas containing parentheses }
```
Formulas are **case-sensitive** (element symbols matter): `Co` is cobalt, `CO` is carbon monoxide. Quote any formula containing parentheses.

## Combustion functions
- **`HeatingValue(Fuel, mode)`** — heating value in J/kg. `mode` is `'LHV'` (water as vapour) or `'HHV'` (water as liquid).
- **`StoichAFR(Fuel)`** — stoichiometric air-fuel ratio on a mass basis.

```
{ Stoichiometric combustion of octane }
LHV = HeatingValue(C8H18, 'LHV')   { ~44.4 MJ/kg }
afr = StoichAFR(C8H18)             { ~15.0 }
```

## Radiation view factors
Closed-form diffuse view factors (Howell catalog) for the three configurations textbooks usually read off charts. Arguments are lengths in consistent units; the result is the dimensionless `F_12`.
- **`viewfactor_perp(w1, w2, L)`** — two perpendicular rectangles sharing an edge of length `L` (Howell C-14).
- **`viewfactor_plates(a, b, L)`** — two aligned parallel rectangles `a × b` separated by `L` (Howell C-11).
- **`viewfactor_disks(r1, r2, L)`** — coaxial parallel disks, radius `r1` → `r2`, separated by `L` (Howell C-41).

```
F_12 = viewfactor_perp(1 [m], 1 [m], 1 [m])   { ~0.2000 }
F_21 = viewfactor_disks(1 [m], 0.5 [m], 0.4 [m])
```

## Transient conduction (Heisler charts)
When a solid is suddenly exposed to convection and the Biot number is large enough that internal gradients matter (`Bi > 0.1` — where lumped capacitance fails), frees gives the one-term approximation, the computational replacement for reading Heisler/Gröber charts. Accurate for Fourier number `Fo ≥ 0.2`.
- **`heisler_temp(geometry$, Bi, Fo, xstar)`** — dimensionless temperature $\theta^* = (T - T_\infty)/(T_i - T_\infty)$ at position `xstar` (0 centre, 1 surface).
- **`heisler_q(geometry$, Bi, Fo)`** — fraction of maximum heat transfer, $Q/Q_0$.

`geometry$` is `'wall'` (half-thickness L), `'cylinder'` (radius r0), or `'sphere'` (radius r0). The characteristic length `Lc` is `L` for the wall and `r0` for cylinder/sphere, so `Bi = h·Lc/k` and `Fo = α·t/Lc²`.

```
h = 100 [W/m^2-K];  k = 0.6 [W/m-K]
alpha = 0.15e-6 [m^2/s];  L = 0.02 [m];  t = 600 [s]

Bi = h * L / k
Fo = alpha * t / L^2
theta_c = heisler_temp('wall', Bi, Fo, 0)   { centre }
theta_s = heisler_temp('wall', Bi, Fo, 1)   { surface }
Q_ratio = heisler_q('wall', Bi, Fo)         { heat removed fraction }
```

[Related: thermo, solid-materials, ref-index]

[Topic: humidair]
# Psychrometrics (AirH2O / Humid Air)

Humid-air calculations use the special fluid name `AirH2O`. Unlike pure fluids, every query needs **three** independent coordinates, one of which must be total pressure (`P`).

## Coordinate indicators
| Key | Meaning | Unit |
| --- | --- | --- |
| `T` | Dry-bulb temperature | K |
| `P` | Total (atmospheric) pressure | Pa |
| `R` | Relative humidity | 0–1 |
| `W` | Humidity ratio | kg water / kg dry air |
| `D` | Dew-point temperature | K |
| `B` | Wet-bulb temperature | K |
| `H` | Specific enthalpy of moist air | J/kg dry air |

> **Unit note:** `AirH2O` works in SI internally. If your problem is in °F/psia, convert inputs to K/Pa (and enthalpy outputs back to Btu/lb by dividing by 2326). Several HVAC examples in the Examples Library show this conversion explicitly.

## Dedicated psychrometric functions
- **`HumRat(AirH2O, T=t, P=p, R=phi)`** — humidity ratio $\omega$.
- **`RelHum(AirH2O, T=t, P=p, W=w)`** — relative humidity $\phi$.
- **`WetBulb(AirH2O, T=t, P=p, R=phi)`** — wet-bulb temperature.
- **`DewPoint(AirH2O, T=t, P=p, R=phi)`** — dew-point temperature.

## Worked example
```
T_db = 25 [C]            { dry-bulb }
P_atm = 101325 [Pa]
phi = 0.60               { 60% relative humidity }

w       = HumRat(AirH2O, T=T_db, P=P_atm, R=phi)
T_dew   = DewPoint(AirH2O, T=T_db, P=P_atm, R=phi)
T_wet   = WetBulb(AirH2O, T=T_db, P=P_atm, R=phi)
h_moist = Enthalpy(AirH2O, T=T_db, P=P_atm, R=phi)   { J/kg dry air }
```

[Related: thermo, state-tables, ref-fluids]

[Topic: state-tables]
# Fluid State Tables (STATE TABLE)

A `STATE TABLE` groups the variables that make up one thermodynamic circuit and binds them to a single fluid. Use it whenever a model has state points — it keeps related variables together and unlocks the Fluid States window.

## Declaring a state table
List the state variables in the header, then declare the fluid inside the block:
```
STATE TABLE WaterLoop(P1, T1, h1, P2, T2, h2)
  FLUID = Water
END
```

## Why use state tables?
- **Fluid isolation** — a `P1` in a water loop and a `P1` in an R134a loop stay separate; lookups never mix fluids.
- **Auto-fill** — after solving, click **Fill Missing Values** in the Fluid States window to compute every other property (`s`, `v`, `x`, …) from the declared fluid.
- **Cycle overlay** — lets you overlay the whole cycle as a connected path on a property chart (T-s, P-h).

## Two-circuit example
```
STATE TABLE WaterLoop(Pw_1, Tw_1, hw_1, Pw_2, Tw_2, hw_2)
  FLUID = Water
END

STATE TABLE RefrigerantLoop(Pref_1, xref_1, href_1, Pref_2, Tref_2, href_2)
  FLUID = R134a
END
```

[Related: thermo, plot-code]
