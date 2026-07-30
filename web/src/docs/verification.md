[Topic: verification]
# Verification Suite

Engineers should not have to take a solver's word for it. Every case on this page ships in the repository as a test fixture (`backend/core/src/test/resources/validation/`) and runs as part of the backend test suite **on every commit** — the values below are enforced by CI, not curated by hand. Each fixture's header states its **basis**: the closed-form derivation, exact arithmetic, or public-standard table value the expectation rests on, so every number can be audited without trusting frees itself. Property-model comparisons are deliberately excluded so no expectation depends on the property backend.

Reproduce locally:

```text
cd backend && ./gradlew :core:test --tests "com.frees.backend.core.ValidationSuiteTest"
```

## Nonlinear algebra

| Case | Verified result | Basis |
| --- | --- | --- |
| Coupled power-ratio pair | x = 4.6940124, y = 3.8021744 (±1e-6) | Direct substitution into x² + y³ = 77 with y = x/1.23456 |
| Monotone cubic | x = 2 exactly | 8 + 2 = 10; the root is unique (derivative 3x² + 1 > 0) |
| Transcendental x·eˣ = 2e² | x = 2 exactly | x·eˣ strictly increasing for x > −1 |

## Thermodynamics

| Case | Verified result | Basis |
| --- | --- | --- |
| Carnot efficiency | η = 0.5 exactly | 1 − 300/600 |
| Air-standard Otto cycle | η = 0.5647247 (±1e-6) | 1 − 8^(−0.4); 8^0.4 = 2^1.2 = 2.2973967 |
| Isentropic compression | T₂ = 579.209 K (±0.01) | 300 · 10^(1/3.5) |
| Ideal Brayton cycle | η = 0.4820525 (±1e-6) | 1 − 10^(−1/3.5) |
| Isothermal expansion work | W = 59 679.97 J (±0.05) | 1 · 287 · 300 · ln 2 |

## Heat transfer

| Case | Verified result | Basis |
| --- | --- | --- |
| Lumped-capacitance cooling | T = 360.653 K (±1e-3) | τ = mc/(hA) = 200 s; e^(−0.5) = 0.60653066 |
| Straight-fin efficiency | η = 0.48201379 (±1e-6) | tanh(2)/2 |
| Parallel-plate radiation | q = 13 121.25 W/m² (±0.05) | Exact fourth powers; resistance denominator 1.5 |
| Critical insulation radius | r = 0.02 m exactly | k/h |
| Counterflow ε-NTU | ε = 0.7746003 (±1e-5) | Closed form at NTU = 2, Cr = 0.5 |

## Fluid mechanics & atmosphere

| Case | Verified result | Basis |
| --- | --- | --- |
| Hagen–Poiseuille pressure drop | Δp = 40.743665 Pa (±1e-4) | 1.28e−6 / (π · 1e−8) |
| Reynolds number | Re = 99 800 exactly | 998 · 2 · 0.05 / 0.001 |
| Hydrostatic column | P = 100 959.07 Pa (±0.05) | ρgh, exact multiplication |
| Isentropic flow at Mach 2 | T₀/T = 1.8 exact; P₀/P = 7.824449 (±1e-4) | 1 + 0.2M²; 1.8^3.5 = 5.832·√1.8 |
| Standard atmosphere, 11 km | T = 216.65 K, P = 22 632 Pa, ρ = 0.36392 kg/m³ | U.S. Standard Atmosphere 1976 published tropopause values, against the built-in `isa_T`/`isa_P`/`isa_rho` |

## Dynamics (ODE integration)

| Case | Verified result | Basis |
| --- | --- | --- |
| Exponential decay | y(1) = 0.36787944 (±1e-4) | Exact solution e^(−t) |
| RC step response | V(1) = 6.3212056 V (±1e-4) | 10 · (1 − e^(−1)), τ = RC = 1 s |
| Harmonic oscillator | x(1) = 1 (±2e-3) | Return after exactly one period, ω = 2π |
| Logistic growth | y(5) = 0.9428256 (±1e-4) | Closed form 1/(1 + 9e^(−5)) |

## Control systems

| Case | Verified result | Basis |
| --- | --- | --- |
| Routh–Hurwitz, stable cubic | 0 RHP poles, stable | First column 1, 2, 2.5, 1 — no sign change |
| Routh–Hurwitz, unstable cubic | 2 RHP poles, unstable | Factors as (s+2)(s² − s + 1); pair at Re +0.5 |
| Static error constants (type 0) | Kp = 4, Kv = 0, Ka = 0 exactly | G(0) = 16/4 |
| Tustin discretization of 1/s | [0.05, 0.05]/[1, −1] exactly | (Ts/2)(z+1)/(z−1) at Ts = 0.1 |

## Linear algebra, signals & statistics

| Case | Verified result | Basis |
| --- | --- | --- |
| 2×2 linear system | x = 1, y = 3 exactly | Elimination/Cramer, det = 5 |
| Triangular 5×5 determinant | det = 120 exactly | Diagonal product 1·2·3·4·5 (exercises the runtime LU path) |
| Symmetric 2×2 eigenvalues | λ = 1, 3 exactly | Characteristic roots 2 ± 1 |
| FFT of a unit impulse | Flat unit spectrum exactly | DFT of [1,0,0,0] is 1 in every bin |
| Least squares on collinear points | slope 2, intercept 1, R² = 1 exactly | Points lie exactly on y = 2x + 1 |

## Uncertainty propagation

| Case | Verified result | Basis |
| --- | --- | --- |
| Product RSS | f = 12; σ_f = 0.7211103 (±1e-4) | √(y²σx² + x²σy²) = √0.52 |

## Component networks

| Case | Verified result | Basis |
| --- | --- | --- |
| Resistive voltage divider | V_mid = 5 V exactly | E · R₂/(R₁+R₂) with equal resistors |
| Series conduction chain | T_interface = 333.333 K (±1e-3) | Q = ΔT/(R₁+R₂) = 133.33 W; 400 − 0.5·Q |

## Adding a case

A validation case is one `.frees` file: the problem, a `// BASIS:` header explaining how the expected value is derived *independently of frees*, and one `// EXPECT <var> = <value> tol <abs>` directive per asserted quantity (`// EXPECT-UNC` for a propagated uncertainty). Drop the file in `backend/core/src/test/resources/validation/` and the suite picks it up automatically — a case with no directive fails, because an unasserted case verifies nothing.

[Related: started, gs-units-check]
