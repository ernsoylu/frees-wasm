// Static catalogs of frees's built-in helpers, transcribed from the backend
// registries. Units and built-in constants are fetched live from /api/reference
// (see HelpPage's UnitsReference); these catalogs cover the function/procedure
// lists that have no machine-readable registry and would otherwise only be
// discoverable by reading the backend switch statements.

export interface FuncEntry {
  /** The name as written in code (case-insensitive at runtime). */
  name: string;
  /** One-line description of what it returns. */
  desc: string;
  /** Example call, or empty. */
  example?: string;
  /** SI unit of the result, where meaningful. */
  unit?: string;
}

export interface FuncGroup {
  title: string;
  blurb?: string;
  functions: FuncEntry[];
}

// ── Scalar math functions ───────────────────────────────────────────────────
// Source: backend ast/Evaluator.evalBuiltin switch.
export const MATH_FUNCTIONS: FuncGroup[] = [
  {
    title: 'Elementary',
    functions: [
      { name: 'abs(x)', desc: 'Absolute value', example: 'abs(-3) = 3' },
      { name: 'sqrt(x)', desc: 'Square root', unit: '√(input unit)' },
      { name: 'cbrt(x)', desc: 'Cube root' },
      { name: 'exp(x)', desc: 'e to the power x' },
      { name: 'ln(x)', desc: 'Natural logarithm' },
      { name: 'log10(x)', desc: 'Base-10 logarithm' },
      { name: 'log2(x)', desc: 'Base-2 logarithm' },
      { name: 'sign(x)', desc: 'Sign: -1, 0, or 1' },
      { name: 'step(x)', desc: 'Unit step: 1 if x≥0 else 0' },
    ],
  },
  {
    title: 'Trigonometric (radians)',
    blurb: 'Angles are radians. Use [deg] annotations or Convert to work in degrees.',
    functions: [
      { name: 'sin(x)', desc: 'Sine' },
      { name: 'cos(x)', desc: 'Cosine' },
      { name: 'tan(x)', desc: 'Tangent' },
      { name: 'arcsin(x)', desc: 'Inverse sine' },
      { name: 'arccos(x)', desc: 'Inverse cosine' },
      { name: 'arctan(x)', desc: 'Inverse tangent' },
      { name: 'atan2(y, x)', desc: 'Quadrant-correct angle of (x,y)', example: 'atan2(1,-1) = 2.356' },
    ],
  },
  {
    title: 'Hyperbolic',
    functions: [
      { name: 'sinh(x)', desc: 'Hyperbolic sine' },
      { name: 'cosh(x)', desc: 'Hyperbolic cosine' },
      { name: 'tanh(x)', desc: 'Hyperbolic tangent' },
      { name: 'arcsinh(x)', desc: 'Inverse hyperbolic sine' },
      { name: 'arccosh(x)', desc: 'Inverse hyperbolic cosine (x≥1)' },
      { name: 'arctanh(x)', desc: 'Inverse hyperbolic tangent (|x|<1)' },
    ],
  },
  {
    title: 'Rounding & integer',
    functions: [
      { name: 'round(x, d)', desc: 'Round to d decimal places', example: 'round(3.14159,3) = 3.142' },
      { name: 'floor(x)', desc: 'Round down' },
      { name: 'ceil(x)', desc: 'Round up' },
      { name: 'trunc(x)', desc: 'Discard fractional part' },
      { name: 'factorial(n)', desc: 'n! (integer)' },
    ],
  },
  {
    title: 'Conditional & series',
    blurb: 'If is the inline branch for the declarative top level; use IF…THEN inside FUNCTION/PROCEDURE bodies.',
    functions: [
      { name: 'If(a, b, lt, eq, gt)', desc: 'lt if a<b, eq if a=b, gt if a>b', example: 'If(temp,300,1.2,1.5,1.8)' },
      { name: 'Sum(i, lo, hi, term)', desc: 'Σ term over i', example: 'Sum(i,1,4,i^2) = 30' },
      { name: 'Product(i, lo, hi, term)', desc: 'Π term over i' },
      { name: 'min(a, b, …)', desc: 'Smallest argument' },
      { name: 'max(a, b, …)', desc: 'Largest argument' },
      { name: 'average(a, b, …)', desc: 'Arithmetic mean' },
    ],
  },
  {
    title: 'Number theory & bitwise',
    functions: [
      { name: 'mod(a, b)', desc: 'Modulo' },
      { name: 'gcd(a, b)', desc: 'Greatest common divisor' },
      { name: 'lcm(a, b)', desc: 'Least common multiple' },
      { name: 'bitand(a,b)', desc: 'Bitwise AND' },
      { name: 'bitor(a,b)', desc: 'Bitwise OR' },
      { name: 'bitxor(a,b)', desc: 'Bitwise XOR' },
      { name: 'bitnot(a)', desc: 'Bitwise NOT' },
      { name: 'bitshiftl(a,n)', desc: 'Left shift' },
      { name: 'bitshiftr(a,n)', desc: 'Right shift' },
      { name: 'baseconvert(s)', desc: 'Base conversion' },
    ],
  },
  {
    title: 'Complex helpers',
    blurb: 'A complex variable is the pair (Z_r, Z_i). Operators keep both parts in step automatically.',
    functions: [
      { name: 'Real(z)', desc: 'Real part' },
      { name: 'Imag(z)', desc: 'Imaginary part' },
      { name: 'Conj(z)', desc: 'Complex conjugate' },
      { name: 'Magnitude(z)', desc: 'Modulus |z|' },
      { name: 'Angle(z)', desc: 'Argument in radians' },
      { name: 'AngleDeg(z)', desc: 'Argument in degrees' },
      { name: 'Cis(theta)', desc: 'e^(jθ) = cosθ + j·sinθ' },
    ],
  },
  {
    title: 'Special functions',
    blurb: 'See also: Special & Statistical Functions topic.',
    functions: [
      { name: 'Gamma(x)', desc: 'Γ(x), Γ(n+1)=n!' },
      { name: 'LogGamma(x)', desc: 'ln Γ(x) (overflow-safe)' },
      { name: 'Digamma(x)', desc: 'ψ(x) = d/dx ln Γ(x)' },
      { name: 'Beta(a, b)', desc: 'Beta function' },
      { name: 'Erf(x)', desc: 'Error function' },
      { name: 'Erfc(x)', desc: 'Complementary error function' },
      { name: 'ErfInv(x)', desc: 'Inverse error function' },
      { name: 'BesselJ(n, x)', desc: 'Bessel 1st kind, order n' },
      { name: 'BesselY(n, x)', desc: 'Bessel 2nd kind, order n' },
      { name: 'BesselI(n, x)', desc: 'Modified Bessel 1st kind' },
      { name: 'BesselK(n, x)', desc: 'Modified Bessel 2nd kind' },
    ],
  },
  {
    title: 'Statistics & random',
    functions: [
      { name: 'Probability(x, μ, σ)', desc: 'Normal CDF at x', example: 'Probability(85,80,5) = 0.8413' },
      { name: 'Chi_Square(x, df)', desc: 'Chi-square CDF' },
      { name: 'Random(a, b)', desc: 'Uniform random in [a,b]' },
      { name: 'RandG(μ, σ)', desc: 'Gaussian random number' },
    ],
  },
  {
    title: 'Data Integration',
    blurb: 'Functions to pull data from external UI sources into the solver.',
    functions: [
      { name: 'ssheet(Sheet, Range)', desc: 'Reads data directly from a spreadsheet cell or range into a scalar, vector, or matrix. Sheet name is required.', example: 'data = ssheet(Sheet1, A1:B10)' },
    ],
  },
];

// ── CALL-able procedures (control systems & linear algebra) ─────────────────
// Source: backend EquationParser.flattenCallProc + Evaluator.evalCall.
export interface CallEntry {
  name: string;
  desc: string;
  signature: string;
  category: 'Model' | 'Analysis' | 'Design' | 'Digital' | 'Linear';
}

export const CALL_PROCEDURES: CallEntry[] = [
  { name: 'ss2tf', category: 'Model', desc: 'State space → transfer function', signature: 'CALL ss2tf(A, B, C, D : num, den)' },
  { name: 'tf2ss', category: 'Model', desc: 'Transfer function → state space', signature: 'CALL tf2ss(num, den : A, B, C, D)' },
  { name: 'zp2tf', category: 'Model', desc: 'Zero-pole-gain → transfer function', signature: 'CALL zp2tf(zr, zi, pr, pi, k : num, den)' },
  { name: 'tf2zp', category: 'Model', desc: 'Transfer function → zero-pole-gain', signature: 'CALL tf2zp(num, den : zr, zi, pr, pi, k)' },
  { name: 'series', category: 'Model', desc: 'Cascade two systems (G = G1·G2)', signature: 'CALL series(num1,den1,num2,den2 : num,den)' },
  { name: 'parallel', category: 'Model', desc: 'Parallel sum (G = G1 + G2)', signature: 'CALL parallel(num1,den1,num2,den2 : num,den)' },
  { name: 'feedback', category: 'Model', desc: 'Close a loop (T = G1/(1+G1·G2))', signature: 'CALL feedback(num1,den1,num2,den2 : num,den)' },
  { name: 'pade', category: 'Model', desc: 'Padé approximation of a dead time', signature: 'CALL pade(Td, order : num, den)' },
  { name: 'ss2ss', category: 'Model', desc: 'Similarity transform x = P·z', signature: 'CALL ss2ss(A,B,C,D,P : An,Bn,Cn,Dn)' },
  { name: 'pole', category: 'Analysis', desc: 'System poles', signature: 'CALL pole(num, den : pr, pi)' },
  { name: 'zero', category: 'Analysis', desc: 'System zeros', signature: 'CALL zero(num, den : zr, zi)' },
  { name: 'bode', category: 'Analysis', desc: 'Bode magnitude (dB) & phase (deg)', signature: 'CALL bode(num, den, omega : mag, phase)' },
  { name: 'nyquist', category: 'Analysis', desc: 'Nyquist real/imaginary parts', signature: 'CALL nyquist(num, den, omega : re, im)' },
  { name: 'nichols', category: 'Analysis', desc: 'Nichols magnitude/phase', signature: 'CALL nichols(num, den, omega : mag, phase)' },
  { name: 'margin', category: 'Analysis', desc: 'Gain & phase margins + crossovers', signature: 'CALL margin(num, den : gm, pm, w_cg, w_cp)' },
  { name: 'rlocus', category: 'Analysis', desc: 'Root-locus trajectories over K', signature: 'CALL rlocus(num, den : K, cpr, cpi)' },
  { name: 'routh', category: 'Analysis', desc: 'Routh-Hurwitz: # RHP poles', signature: 'CALL routh(den : nRHP, stable)' },
  { name: 'errorconst', category: 'Analysis', desc: 'Static error constants Kp, Kv, Ka', signature: 'CALL errorconst(num, den : Kp, Kv, Ka)' },
  { name: 'ctrb', category: 'Analysis', desc: 'Controllability matrix', signature: 'CALL ctrb(A, B : Co)' },
  { name: 'obsv', category: 'Analysis', desc: 'Observability matrix', signature: 'CALL obsv(A, C : Ob)' },
  { name: 'gram', category: 'Analysis', desc: "Controllability ('c') / observability ('o') gramian", signature: "CALL gram(A, M, 'c' : W)" },
  { name: 'rank', category: 'Analysis', desc: 'Numerical matrix rank (SVD)', signature: 'CALL rank(M : r)' },
  { name: 'step', category: 'Analysis', desc: 'Unit step response', signature: 'CALL step(num, den, t : y)' },
  { name: 'impulse', category: 'Analysis', desc: 'Impulse response', signature: 'CALL impulse(num, den, t : y)' },
  { name: 'lsim', category: 'Analysis', desc: 'Response to arbitrary input u(t)', signature: 'CALL lsim(num, den, u, t : y)' },
  { name: 'stepinfo', category: 'Analysis', desc: 'Step metrics (Tr, Tp, Ts, OS)', signature: 'CALL stepinfo(t, y : Tr, Tp, Ts, OS)' },
  { name: 'mason', category: 'Analysis', desc: "Mason's gain formula for a graph", signature: 'CALL mason(G, source, sink : T)' },
  { name: 'lqr', category: 'Design', desc: 'LQR optimal state-feedback gain', signature: 'CALL lqr(A, B, Q, R : K)' },
  { name: 'dlqr', category: 'Design', desc: 'Discrete-time LQR gain (DARE)', signature: 'CALL dlqr(A, B, Q, R : K)' },
  { name: 'place', category: 'Design', desc: 'Pole placement (Ackermann)', signature: 'CALL place(A, B, pr, pi : K)' },
  { name: 'acker', category: 'Design', desc: "Pole placement (Ackermann's formula)", signature: 'CALL acker(A, B, pr, pi : K)' },
  { name: 'lqe', category: 'Design', desc: 'Kalman estimator (LQE) gain', signature: 'CALL lqe(A, G, C, Q, R : L)' },
  { name: 'balreal', category: 'Design', desc: 'Balanced realization (model reduction)', signature: 'CALL balreal(A, B, C : Ab, Bb, Cb)' },
  { name: 'pidtune', category: 'Design', desc: 'PID auto-tune by loop shaping', signature: "CALL pidtune(num, den, 'PID', wc : Kp, Ki, Kd)" },
  { name: 'c2d', category: 'Digital', desc: 'Continuous → discrete (Tustin/ZOH)', signature: "CALL c2d(num, den, Ts, 'zoh' : numz, denz)" },
  { name: 'd2c', category: 'Digital', desc: 'Discrete → continuous (Tustin)', signature: "CALL d2c(numz, denz, Ts, 'tustin' : num, den)" },
  { name: 'residue', category: 'Linear', desc: 'Partial-fraction residues & poles', signature: 'CALL residue(num, den : rr, ri, pr, pi, k)' },
  { name: 'lyap', category: 'Linear', desc: "Continuous Lyapunov solve A·X+X·A'+Q=0", signature: 'CALL lyap(A, Q : X)' },
  { name: 'dlyap', category: 'Linear', desc: "Discrete Lyapunov (Stein) solve A·X·A'−X+Q=0", signature: 'CALL dlyap(A, Q : X)' },
  { name: 'dare', category: 'Linear', desc: 'Discrete algebraic Riccati solution', signature: 'CALL dare(A, B, Q, R : X)' },
  { name: 'Eigenvalues', category: 'Linear', desc: 'Matrix eigenvalues', signature: 'CALL Eigenvalues(A : lambda)' },
  { name: 'Eigen', category: 'Linear', desc: 'Eigenvalues & eigenvectors', signature: 'CALL Eigen(A : lambda, V)' },
  { name: 'LUDecompose', category: 'Linear', desc: 'LU decomposition', signature: 'CALL LUDecompose(A : L, U)' },
  { name: 'EulerRotate', category: 'Linear', desc: '3×3 rotation from Euler angles', signature: 'CALL EulerRotate(phi, theta, psi : R)' },
];

// ── Matrix / vector functions (matrix-returning) ────────────────────────────
export const MATRIX_FUNCTIONS: FuncEntry[] = [
  { name: 'SolveLinear(A, b)', desc: 'Solve A·x = b (same as A \\ b)' },
  { name: 'Inverse(A)', desc: 'Matrix inverse A⁻¹' },
  { name: 'Determinant(A)', desc: 'Determinant' },
  { name: 'Dot(a, b)', desc: 'Vector dot product' },
  { name: 'Cross(a, b)', desc: 'Cross product of 3-vectors' },
  { name: 'Norm(v)', desc: 'Euclidean length' },
  { name: 'Transpose(A)', desc: "Transpose (also A')" },
  { name: 'zeros(m, n)', desc: 'm×n zero matrix' },
  { name: 'ones(m, n)', desc: 'm×n ones matrix' },
  { name: 'eye(n) / identity(n)', desc: 'n×n identity' },
  { name: 'diag(v)', desc: 'Diagonal matrix from vector' },
  { name: 'linspace(a, b, n)', desc: 'n linearly spaced values' },
  { name: 'axpy(α, x, y)', desc: 'BLAS: αx + y' },
  { name: 'gemv(α, A, x, β, y)', desc: 'BLAS L2: αAx + βy' },
  { name: 'gemm(α, A, B, β, C)', desc: 'BLAS L3: αAB + βC' },
];

// ── Fluid property functions & indicators ───────────────────────────────────
// Source: props/PropertyFunctions OUTPUTS / INPUTS / HA_OUTPUTS / HA_INPUTS.
export const FLUID_PROPERTY_OUTPUTS: FuncEntry[] = [
  { name: 'Enthalpy', desc: 'Specific enthalpy', unit: 'J/kg' },
  { name: 'Entropy', desc: 'Specific entropy', unit: 'J/kg-K' },
  { name: 'Temperature', desc: 'Absolute temperature', unit: 'K' },
  { name: 'Pressure', desc: 'Absolute pressure', unit: 'Pa' },
  { name: 'Density', desc: 'Mass density', unit: 'kg/m³' },
  { name: 'Volume', desc: 'Specific volume', unit: 'm³/kg' },
  { name: 'IntEnergy', desc: 'Specific internal energy', unit: 'J/kg' },
  { name: 'Quality', desc: 'Vapour quality (0–1)' },
  { name: 'Cp / Specheat', desc: 'Specific heat Cp', unit: 'J/kg-K' },
  { name: 'Cv', desc: 'Specific heat Cv', unit: 'J/kg-K' },
  { name: 'Viscosity', desc: 'Dynamic viscosity', unit: 'Pa-s' },
  { name: 'Conductivity', desc: 'Thermal conductivity', unit: 'W/m-K' },
  { name: 'SoundSpeed', desc: 'Speed of sound', unit: 'm/s' },
  { name: 'CompressibilityFactor', desc: 'Z = Pv/(RT)' },
  { name: 'Prandtl', desc: 'Prandtl number' },
  { name: 'Gibbs', desc: 'Specific Gibbs free energy', unit: 'J/kg' },
];

export const FLUID_INPUT_INDICATORS: { key: string; meaning: string }[] = [
  { key: 'T', meaning: 'Temperature' },
  { key: 'P', meaning: 'Pressure' },
  { key: 'h', meaning: 'Enthalpy' },
  { key: 's', meaning: 'Entropy' },
  { key: 'u', meaning: 'Internal energy' },
  { key: 'x / q', meaning: 'Quality (0 sat. liquid, 1 sat. vapour)' },
  { key: 'v / d / rho', meaning: 'Specific volume / density' },
];

// ── Humid-air (AirH2O) — three coordinates, one must be P ───────────────────
export const AIRH2O_OUTPUTS: FuncEntry[] = [
  { name: 'HumRat', desc: 'Humidity ratio ω', unit: 'kg/kg dry air' },
  { name: 'RelHum', desc: 'Relative humidity φ', unit: '0–1' },
  { name: 'WetBulb', desc: 'Wet-bulb temperature', unit: 'K' },
  { name: 'DewPoint', desc: 'Dew-point temperature', unit: 'K' },
];
export const AIRH2O_INDICATORS: { key: string; meaning: string }[] = [
  { key: 'T', meaning: 'Dry-bulb temperature [K]' },
  { key: 'P', meaning: 'Total pressure [Pa] (required)' },
  { key: 'R / rh', meaning: 'Relative humidity [0–1]' },
  { key: 'W', meaning: 'Humidity ratio' },
  { key: 'B / twb', meaning: 'Wet-bulb temperature [K]' },
  { key: 'D / tdp', meaning: 'Dew-point temperature [K]' },
  { key: 'H', meaning: 'Specific enthalpy [J/kg dry air]' },
];

// ── Solid material functions & supported materials ──────────────────────────
// Source: props/SolidProperties functionNames() + DB map keys.
export const MATERIAL_FUNCTIONS: FuncEntry[] = [
  { name: 'k_(Mat[, T])', desc: 'Thermal conductivity', unit: 'W/m-K' },
  { name: 'c_(Mat[, T])', desc: 'Specific heat capacity', unit: 'J/kg-K' },
  { name: 'rho_(Mat)', desc: 'Density', unit: 'kg/m³' },
  { name: 'E_(Mat)', desc: "Young's modulus", unit: 'Pa' },
  { name: 'nu_(Mat)', desc: "Poisson's ratio" },
];

export const SOLID_MATERIALS: string[] = [
  'Aluminum', 'Copper', 'Steel', 'StainlessSteel', 'Iron', 'Brass', 'Bronze',
  'Gold', 'Silver', 'Lead', 'Nickel', 'Titanium', 'Tungsten', 'Zinc', 'Magnesium',
  'Concrete', 'Glass', 'Brick', 'Wood', 'Ice',
];

// ── Table accessor functions ────────────────────────────────────────────────
// Source: ast/Evaluator TABLE_FUNCTIONS + PARAMETRIC_ACCESSORS + OdeAccessors.NAMES.
export const TABLE_FUNCTIONS: FuncEntry[] = [
  { name: "Interpolate('t', x)", desc: 'Linear interpolation (same as t(x))' },
  { name: "Interpolate1('t', x)", desc: 'Cubic-spline interpolation' },
  { name: "Interpolate2D('t', x, y)", desc: 'Bilinear 2-D interpolation' },
  { name: "Lookup('t', row, col)", desc: 'Cell value by 1-based indices' },
  { name: "LookupRow('t', col, val)", desc: 'Row where col crosses val' },
  { name: 'NLookupRows(t)', desc: 'Number of data rows' },
  { name: "Differentiate('t', y, x, xv)", desc: 'Numerical dy/dx at xv' },
];

export const PARAMETRIC_ACCESSORS: FuncEntry[] = [
  { name: 'TableValue(run, col)', desc: 'A cell in the parametric table' },
  { name: 'TableRun#()', desc: 'Current run index (1-based)' },
  { name: 'NParametricRuns()', desc: 'Total configured runs' },
  { name: "TableSum('col')", desc: 'Sum of a column' },
  { name: "TableAvg('col')", desc: 'Average of a column' },
  { name: "TableMin('col')", desc: 'Minimum of a column' },
  { name: "TableMax('col')", desc: 'Maximum of a column' },
  { name: "TableStdDev('col')", desc: 'Standard deviation' },
  { name: "IntegralValue('y','x')", desc: 'Trapezoid integral of y vs x' },
];

export const ODE_ACCESSORS: FuncEntry[] = [
  { name: "FinalValue('col')", desc: 'Last value of an ODE column' },
  { name: "MaxValue('col')", desc: 'Peak value of an ODE column' },
  { name: "MinValue('col')", desc: 'Minimum value of an ODE column' },
  { name: "TimeAt('col', val)", desc: 'Time when col crosses val' },
  { name: "ODEValue('col', t)", desc: 'Value interpolated at time t' },
];

// ── Combustion / utility property functions ─────────────────────────────────
export const UTILITY_PROPERTY_FUNCS: FuncEntry[] = [
  { name: 'MolarMass(Fluid)', desc: 'Molar mass (kg/mol); accepts formulas', unit: 'kg/mol' },
  { name: "HeatingValue(Fuel, 'LHV'|'HHV')", desc: 'Heating value', unit: 'J/kg' },
  { name: 'StoichAFR(Fuel)', desc: 'Stoichiometric air-fuel ratio (mass)' },
  { name: 'P_sat(Fluid, T=t)', desc: 'Saturation pressure at T', unit: 'Pa' },
  { name: 'T_sat(Fluid, P=p)', desc: 'Saturation temperature at P', unit: 'K' },
  { name: 'Phase$(Fluid, T, P)', desc: "Phase string: 'liquid'|'gas'|'twophase'|…", unit: 'string' },
  { name: 'CompressibilityFactor(Fluid, T, P)', desc: 'Z = Pv/(RT)' },
  { name: 'StagnationTemp(T, V, cp)', desc: 'T0 = T + V²/(2cp)', unit: 'K' },
  { name: 'StagnationPres(P, T, T0, k)', desc: 'P0 = P(T0/T)^(k/(k-1))', unit: 'Pa' },
  { name: 'SurfaceTension(Fluid, T)', desc: 'Surface tension', unit: 'N/m' },
  { name: 'P_crit / T_crit / v_crit / T_triple', desc: 'Critical & triple-point constants' },
  { name: 'IsIdealGas(Fluid)', desc: '1 if ideal, else 0' },
  { name: 'viewfactor_perp / _plates / _disks', desc: 'Radiation view factors (Howell)' },
  { name: "heisler_temp('wall'|'cyl'|'sphere', Bi, Fo, x*)", desc: 'Heisler transient conduction' },
  { name: "heisler_q('wall'|'cyl'|'sphere', Bi, Fo)", desc: 'Heisler heat-fraction Q/Q0' },
];

// ── Built-in constants (for the live reference; also via /api/reference) ────
// Kept here as a fallback description map so the constants page reads well even
// when the backend is unreachable. Values are SI.
export const CONSTANT_DESCRIPTIONS: { name: string; value: string; unit: string; desc: string }[] = [
  { name: 'pi#', value: '3.14159265…', unit: '—', desc: 'Pi' },
  { name: 'e#', value: '2.71828183…', unit: '—', desc: 'Euler’s number' },
  { name: 'R#', value: '8.31446', unit: 'J/mol·K', desc: 'Universal gas constant' },
  { name: 'g#', value: '9.80665', unit: 'm/s²', desc: 'Standard gravity' },
  { name: 'Na#', value: '6.02214e23', unit: '1/mol', desc: 'Avogadro’s number' },
  { name: 'k#', value: '1.38065e-23', unit: 'J/K', desc: 'Boltzmann constant' },
  { name: 'h#', value: '6.62607e-34', unit: 'J·s', desc: 'Planck constant' },
  { name: 'c#', value: '2.99792e8', unit: 'm/s', desc: 'Speed of light' },
  { name: 'sigma#', value: '5.67037e-8', unit: 'W/m²·K⁴', desc: 'Stefan–Boltzmann constant' },
  { name: 'Gc#', value: '6.67430e-11', unit: 'N·m²/kg²', desc: 'Gravitational constant' },
  { name: 'qe#', value: '1.60218e-19', unit: 'C', desc: 'Elementary charge' },
];
