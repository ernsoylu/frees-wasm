// Catalog for the top-bar Functions menu and the Ctrl+K command palette.
// Selecting an item inserts its snippet at the editor caret (see
// App.insertFunction). "$0" marks where the caret should land after insertion;
// if absent, the caret goes to the end. `description` and `usage` power the
// command-palette entries (explanation + a concrete sample call).
// Names mirror the backend's built-ins (Evaluator / parser / property service).
export interface FunctionItem {
  label: string
  snippet: string
  /** One-line explanation of what the function does. */
  description?: string
  /** A concrete, valid sample call. */
  usage?: string
}

export interface FunctionCategory {
  category: string
  items: FunctionItem[]
}

export const FUNCTION_CATEGORIES: FunctionCategory[] = [
  {
    category: 'Thermophysical Properties',
    items: [
      { label: 'Enthalpy', snippet: 'Enthalpy(Water, P=$0, T=)', description: 'Specific enthalpy of a fluid from any two independent properties.', usage: 'h = Enthalpy(Water, T=100 [C], P=101.325 [kPa])' },
      { label: 'Entropy', snippet: 'Entropy(Water, P=$0, T=)', description: 'Specific entropy of a fluid from any two independent properties.', usage: 's = Entropy(Water, P=101.325 [kPa], x=1)' },
      { label: 'IntEnergy', snippet: 'IntEnergy(Water, P=$0, T=)', description: 'Specific internal energy from any two independent properties.', usage: 'u = IntEnergy(Water, T=100 [C], x=0)' },
      { label: 'Temperature', snippet: 'Temperature(Water, P=$0, h=)', description: 'Temperature back-solved from any two independent properties.', usage: 'T = Temperature(Water, P=101.325 [kPa], h=2675000 [J/kg])' },
      { label: 'Pressure', snippet: 'Pressure(Water, T=$0, x=)', description: 'Pressure from any two independent properties.', usage: 'P = Pressure(Water, T=100 [C], x=0)' },
      { label: 'Density', snippet: 'Density(Water, P=$0, T=)', description: 'Density from any two independent properties.', usage: 'rho = Density(Air, T=25 [C], P=101.325 [kPa])' },
      { label: 'Volume', snippet: 'Volume(Water, P=$0, T=)', description: 'Specific volume from any two independent properties.', usage: 'v = Volume(Water, T=100 [C], x=1)' },
      { label: 'Quality', snippet: 'Quality(Water, P=$0, h=)', description: 'Vapor quality (0–1) inside the two-phase dome.', usage: 'x = Quality(Water, P=101.325 [kPa], h=2000000 [J/kg])' },
      { label: 'Cp', snippet: 'Cp(Water, P=$0, T=)', description: 'Specific heat at constant pressure.', usage: 'cp = Cp(Water, T=25 [C], P=101.325 [kPa])' },
      { label: 'Cv', snippet: 'Cv(Water, P=$0, T=)', description: 'Specific heat at constant volume.', usage: 'cv = Cv(Air, T=25 [C], P=101.325 [kPa])' },
      { label: 'SpecHeat', snippet: 'SpecHeat(Water, P=$0, T=)', description: 'Specific heat of an incompressible substance or ideal gas.', usage: 'c = SpecHeat(Water, T=25 [C], P=101.325 [kPa])' },
      { label: 'SoundSpeed', snippet: 'SoundSpeed(Water, P=$0, T=)', description: 'Speed of sound in the fluid.', usage: 'a = SoundSpeed(Air, T=25 [C], P=101.325 [kPa])' },
      { label: 'Viscosity', snippet: 'Viscosity(Water, P=$0, T=)', description: 'Dynamic (absolute) viscosity.', usage: 'mu = Viscosity(Water, T=25 [C], P=101.325 [kPa])' },
      { label: 'Conductivity', snippet: 'Conductivity(Water, P=$0, T=)', description: 'Thermal conductivity.', usage: 'k = Conductivity(Water, T=25 [C], P=101.325 [kPa])' },
      { label: 'P_sat', snippet: 'P_sat($0, T=)', description: 'Saturation pressure [Pa] at the given temperature.', usage: 'Ps = P_sat(Water, T=100 [C])   { ~101325 Pa }' },
      { label: 'T_sat', snippet: 'T_sat($0, P=)', description: 'Saturation temperature [K] at the given pressure.', usage: 'Ts = T_sat(Water, P=101.325 [kPa])   { 373.15 K }' },
      { label: 'Prandtl', snippet: 'Prandtl($0, T=, P=)', description: 'Prandtl number (dimensionless) of a fluid.', usage: 'Pr = Prandtl(Water, T=25 [C], P=101.325 [kPa])' },
      { label: 'CompressibilityFactor', snippet: 'CompressibilityFactor($0, T=, P=)', description: 'Compressibility factor Z = Pv/(RT), dimensionless.', usage: 'Z = CompressibilityFactor(CO2, T=300 [K], P=5 [MPa])' },
      { label: 'SurfaceTension', snippet: 'SurfaceTension($0, T=)', description: 'Surface tension [N/m] of a fluid at the given temperature.', usage: 'sigma = SurfaceTension(Water, T=25 [C])   { ~0.072 N/m }' },
      { label: 'P_crit', snippet: 'P_crit($0)', description: 'Critical pressure [Pa] of a fluid.', usage: 'Pc = P_crit(Water)   { 22.064 MPa }' },
      { label: 'T_crit', snippet: 'T_crit($0)', description: 'Critical temperature [K] of a fluid.', usage: 'Tc = T_crit(Water)   { 647.1 K }' },
      { label: 'v_crit', snippet: 'v_crit($0)', description: 'Critical specific volume [m³/kg] of a fluid.', usage: 'vc = v_crit(Water)' },
      { label: 'T_triple', snippet: 'T_triple($0)', description: 'Triple-point temperature [K] of a fluid.', usage: 'Tt = T_triple(Water)   { 273.16 K }' },
      { label: 'IsIdealGas', snippet: 'IsIdealGas($0)', description: 'Returns 1 if the fluid is treated as an ideal gas, 0 otherwise.', usage: 'chk = IsIdealGas(Air)   { 1 }' },
      { label: 'Phase$', snippet: 'Phase$($0, T=, P=)', description: "Phase of a fluid as a string: 'liquid', 'gas', 'twophase', 'supercritical'.", usage: "ph$ = Phase$(R134a, T=25 [C], P=100 [kPa])" },
      { label: 'Gibbs', snippet: 'Gibbs(Water, T=$0, P=)', description: 'Specific Gibbs free energy of a fluid.', usage: 'g = Gibbs(Water, T=300 [K], P=100 [kPa])' },
      { label: 'StagnationTemp', snippet: 'StagnationTemp(T=$0, V=, cp=)', description: 'Stagnation temperature T0 = T + V²/(2*cp) for compressible flow.', usage: 'T0 = StagnationTemp(300 [K], 200 [m/s], 1005 [J/kg-K])' },
      { label: 'StagnationPres', snippet: 'StagnationPres(P=$0, T=, T0=, k=)', description: 'Stagnation pressure P0 = P*(T0/T)^(k/(k-1)) for compressible flow.', usage: 'P0 = StagnationPres(100 [kPa], 300 [K], 319.9 [K], 1.4)' },
      { label: 'viewfactor_perp (perpendicular plates)', snippet: 'viewfactor_perp($0, w2, L)', description: 'Radiation view factor F_12 between two perpendicular rectangles sharing a common edge of length L; plate 1 extends w1 from the edge, plate 2 extends w2 (Howell C-14). Dimensionless.', usage: 'F = viewfactor_perp(2 [m], 1.5 [m], 3 [m])' },
      { label: 'viewfactor_plates (parallel plates)', snippet: 'viewfactor_plates($0, b, L)', description: 'Radiation view factor F_12 between two identical, directly opposed, aligned parallel rectangles of sides a by b separated by distance L (Howell C-11). Dimensionless.', usage: 'F = viewfactor_plates(2 [m], 2 [m], 1 [m])' },
      { label: 'viewfactor_disks (coaxial disks)', snippet: 'viewfactor_disks($0, r2, L)', description: 'Radiation view factor F_12 from coaxial parallel disk 1 (radius r1) to disk 2 (radius r2) separated by distance L (Howell C-41). Dimensionless.', usage: 'F = viewfactor_disks(0.5 [m], 1 [m], 0.4 [m])' },
      { label: 'heisler_temp (transient conduction)', snippet: "heisler_temp('$0', Bi, Fo, xstar)", description: "Dimensionless temperature theta* = (T - Tinf)/(Ti - Tinf) for 1-D transient conduction with internal gradients (Heisler one-term approximation, accurate for Fo >= 0.2). Geometry is 'wall', 'cylinder', or 'sphere'; xstar is the position (0 centre, 1 surface); Bi = h*Lc/k, Fo = alpha*t/Lc^2.", usage: "theta_c = heisler_temp('wall', Bi, Fo, 0)" },
      { label: 'heisler_q (transient heat removed)', snippet: "heisler_q('$0', Bi, Fo)", description: 'Fraction of the maximum possible heat transfer Q/Q0 for 1-D transient conduction (Heisler/Gröber). Geometry is wall/cylinder/sphere.', usage: "Q_ratio = heisler_q('sphere', Bi, Fo)" },
    ],
  },
  {
    category: 'Chemistry & Combustion',
    items: [
      { label: 'MolarMass', snippet: 'MolarMass($0)', description: 'Molar mass [kg/mol] of a fluid, ideal-gas species, or chemical formula. Formulas are case-sensitive; quote ones with parentheses.', usage: "M = MolarMass(C8H18)   { 0.11423 kg/mol }\nM = MolarMass('Ca(OH)2')" },
      { label: 'HeatingValue', snippet: "HeatingValue($0, 'LHV')", description: "Heating value [J/kg] of a hydrocarbon/alcohol fuel. 'LHV' references water vapour, 'HHV' liquid water.", usage: "LHV = HeatingValue(CH4, 'LHV')   { ~50 MJ/kg }" },
      { label: 'StoichAFR', snippet: 'StoichAFR($0)', description: 'Stoichiometric air-fuel ratio (mass basis) for CxHyOz combustion in air.', usage: 'AFR = StoichAFR(C8H18)   { ~15.0 }' },
    ],
  },
  {
    category: 'Solid & Material Properties',
    items: [
      { label: 'c_', snippet: 'c_($0)', description: 'Specific heat capacity [J/(kg·K)] of a common solid material. Add T=value (K) for a linear temperature correction on the well-characterised metals.', usage: 'c = c_(Steel)   { ~434 J/kg-K }   or   c = c_(Iron, T=400)' },
      { label: 'k_', snippet: 'k_($0)', description: 'Thermal conductivity [W/(m·K)] of a common solid material. Add T=value (K) for a linear temperature correction on the well-characterised metals.', usage: 'k = k_(Aluminum)   { ~237 W/m-K }   or   k = k_(Aluminum, T=500)' },
      { label: 'rho_', snippet: 'rho_($0)', description: 'Density [kg/m³] of a solid or liquid material.', usage: 'rho = rho_(Steel)   { ~7800 kg/m³ }' },
      { label: 'E_', snippet: 'E_($0)', description: "Young's modulus [Pa] of a structural material.", usage: "E = E_(Steel)   { ~200 GPa }" },
      { label: 'nu_', snippet: 'nu_($0)', description: "Poisson's ratio (dimensionless) of a structural material.", usage: 'nu = nu_(Steel)   { ~0.3 }' },
      { label: 'VolExpCoef', snippet: 'VolExpCoef($0)', description: 'Volumetric expansion coefficient [1/K] of a material.', usage: 'beta = VolExpCoef(Steel)' },
    ],
  },
  {
    category: 'Psychrometrics (AirH2O)',
    items: [
      { label: 'HumRat', snippet: 'HumRat(AirH2O, T=$0, P=, R=)', description: 'Humidity ratio of moist air (kg water / kg dry air).', usage: 'w = HumRat(AirH2O, T=25 [C], P=101.325 [kPa], R=0.5)' },
      { label: 'RelHum', snippet: 'RelHum(AirH2O, T=$0, P=, w=)', description: 'Relative humidity (0–1) of moist air.', usage: 'R = RelHum(AirH2O, T=25 [C], P=101.325 [kPa], w=0.01)' },
      { label: 'WetBulb', snippet: 'WetBulb(AirH2O, T=$0, P=, R=)', description: 'Wet-bulb temperature of moist air.', usage: 'Twb = WetBulb(AirH2O, T=25 [C], P=101.325 [kPa], R=0.5)' },
      { label: 'DewPoint', snippet: 'DewPoint(AirH2O, T=$0, P=, R=)', description: 'Dew-point temperature of moist air.', usage: 'Tdp = DewPoint(AirH2O, T=25 [C], P=101.325 [kPa], R=0.5)' },
    ],
  },
  {
    category: 'Math',
    items: [
      { label: 'sqrt', snippet: 'sqrt($0)', description: 'Square root.', usage: 'y = sqrt(2)' },
      { label: 'abs', snippet: 'abs($0)', description: 'Absolute value.', usage: 'y = abs(-3)' },
      { label: 'exp', snippet: 'exp($0)', description: 'Exponential, e raised to x.', usage: 'y = exp(1)   { 2.71828 }' },
      { label: 'ln', snippet: 'ln($0)', description: 'Natural logarithm (base e).', usage: 'y = ln(10)' },
      { label: 'log10', snippet: 'log10($0)', description: 'Base-10 logarithm.', usage: 'y = log10(1000)   { 3 }' },
      { label: 'min', snippet: 'min($0, )', description: 'Smallest of its arguments.', usage: 'y = min(3, 7, 2)   { 2 }' },
      { label: 'max', snippet: 'max($0, )', description: 'Largest of its arguments.', usage: 'y = max(3, 7, 2)   { 7 }' },
      { label: 'mod', snippet: 'mod($0, )', description: 'Remainder of a divided by b.', usage: 'y = mod(10, 3)   { 1 }' },
      { label: 'round', snippet: 'round($0, )', description: 'Round to a number of decimal places.', usage: 'y = round(3.14159, 2)   { 3.14 }' },
      { label: 'floor', snippet: 'floor($0)', description: 'Round down to the nearest integer.', usage: 'y = floor(2.7)   { 2 }' },
      { label: 'ceil', snippet: 'ceil($0)', description: 'Round up to the nearest integer.', usage: 'y = ceil(2.1)   { 3 }' },
      { label: 'trunc', snippet: 'trunc($0)', description: 'Discard the fractional part.', usage: 'y = trunc(2.9)   { 2 }' },
      { label: 'sign', snippet: 'sign($0)', description: 'Sign of x: −1, 0, or 1.', usage: 'y = sign(-15)   { -1 }' },
      { label: 'step', snippet: 'step($0)', description: 'Unit step: 1 if x ≥ 0, else 0.', usage: 'y = step(0.5)   { 1 }' },
      { label: 'factorial', snippet: 'factorial($0)', description: 'Factorial n! of a non-negative integer.', usage: 'y = factorial(5)   { 120 }' },
      { label: 'gcd', snippet: 'gcd($0, )', description: 'Greatest common divisor of two integers.', usage: 'y = gcd(48, 36)   { 12 }' },
      { label: 'lcm', snippet: 'lcm($0, )', description: 'Least common multiple of two integers.', usage: 'y = lcm(4, 6)   { 12 }' },
      { label: 'average', snippet: 'average($0, )', description: 'Arithmetic mean of its arguments.', usage: 'y = average(2, 4, 9)' },
      { label: 'Sum', snippet: 'Sum(i, 1, N, $0)', description: 'Sum of a term over an integer index range.', usage: 'y = Sum(i, 1, 4, i^2)   { 30 }' },
      { label: 'Product', snippet: 'Product(i, 1, N, $0)', description: 'Product of a term over an integer index range.', usage: 'y = Product(i, 1, 4, i)   { 24 }' },
      { label: 'Integral', snippet: 'Integral($0, t, a, b)', description: 'Numerical integral over a variable from a to b. When the integrand references its own result (starting from 0), frees solves it as a single-state ODE — the feedback pattern for ODEs.', usage: 'y = Integral(x^2, x, 0, 1)   { 0.3333 }' },
      { label: 'if (inline)', snippet: 'if($0, , )', description: 'Conditional selector: If(a, b, lt, eq, gt) returns lt/eq/gt by comparing a to b.', usage: 'k = If(T, 300, 1.2, 1.5, 1.8)' },
    ],
  },
  {
    category: 'Trigonometry',
    items: [
      { label: 'sin', snippet: 'sin($0)', description: 'Sine (argument in radians).', usage: 'y = sin(pi#/6)   { 0.5 }' },
      { label: 'cos', snippet: 'cos($0)', description: 'Cosine (argument in radians).', usage: 'y = cos(0)   { 1 }' },
      { label: 'tan', snippet: 'tan($0)', description: 'Tangent (argument in radians).', usage: 'y = tan(pi#/4)   { 1 }' },
      { label: 'arcsin', snippet: 'arcsin($0)', description: 'Inverse sine; returns radians.', usage: 'y = arcsin(0.5)' },
      { label: 'arccos', snippet: 'arccos($0)', description: 'Inverse cosine; returns radians.', usage: 'y = arccos(0.5)' },
      { label: 'arctan', snippet: 'arctan($0)', description: 'Inverse tangent; returns radians.', usage: 'y = arctan(1)' },
      { label: 'atan2', snippet: 'atan2($0, )', description: 'Two-argument arctangent atan2(y, x) — angle of (x, y) in the correct quadrant (radians).', usage: 'theta = atan2(1, -1)   { 2.356 rad }' },
      { label: 'sinh', snippet: 'sinh($0)', description: 'Hyperbolic sine.', usage: 'y = sinh(1.25)' },
      { label: 'cosh', snippet: 'cosh($0)', description: 'Hyperbolic cosine.', usage: 'y = cosh(1.25)' },
      { label: 'tanh', snippet: 'tanh($0)', description: 'Hyperbolic tangent.', usage: 'y = tanh(1.25)' },
      { label: 'arcsinh', snippet: 'arcsinh($0)', description: 'Inverse hyperbolic sine.', usage: 'y = arcsinh(2)' },
      { label: 'arccosh', snippet: 'arccosh($0)', description: 'Inverse hyperbolic cosine (argument ≥ 1).', usage: 'y = arccosh(1.5)' },
      { label: 'arctanh', snippet: 'arctanh($0)', description: 'Inverse hyperbolic tangent (argument in (−1, 1)).', usage: 'y = arctanh(0.5)' },
    ],
  },
  {
    category: 'Special Functions',
    items: [
      { label: 'Gamma', snippet: 'Gamma($0)', description: 'Gamma function Γ(x); Γ(n+1) = n!.', usage: 'y = Gamma(5)   { 24 }' },
      { label: 'LogGamma', snippet: 'LogGamma($0)', description: 'Natural log of the gamma function, ln Γ(x).', usage: 'y = LogGamma(10)' },
      { label: 'Digamma', snippet: 'Digamma($0)', description: 'Digamma ψ(x) = d/dx ln Γ(x).', usage: 'y = Digamma(3)' },
      { label: 'Beta', snippet: 'Beta($0, )', description: 'Beta function B(a, b) = Γ(a)·Γ(b)/Γ(a+b).', usage: 'y = Beta(2, 3)   { 0.0833 }' },
      { label: 'Erf', snippet: 'Erf($0)', description: 'Error function erf(x).', usage: 'y = Erf(1)   { 0.8427 }' },
      { label: 'Erfc', snippet: 'Erfc($0)', description: 'Complementary error function, 1 − erf(x).', usage: 'y = Erfc(1)   { 0.1573 }' },
      { label: 'ErfInv', snippet: 'ErfInv($0)', description: 'Inverse error function (argument in (−1, 1)).', usage: 'y = ErfInv(0.8427)   { ~1 }' },
      { label: 'Bessel_J', snippet: 'BesselJ($0, x)', description: 'Bessel function of the first kind, order n: J_n(x).', usage: 'y = BesselJ(0, 2.5)' },
      { label: 'Bessel_Y', snippet: 'BesselY($0, x)', description: 'Bessel function of the second kind, order n: Y_n(x) (x > 0).', usage: 'y = BesselY(0, 2.5)' },
      { label: 'Bessel_I', snippet: 'BesselI($0, x)', description: 'Modified Bessel function of the first kind, I_n(x).', usage: 'y = BesselI(0, 2.5)' },
      { label: 'Bessel_K', snippet: 'BesselK($0, x)', description: 'Modified Bessel function of the second kind, K_n(x) (x > 0).', usage: 'y = BesselK(1, 2.5)' },
      { label: 'Chi_Square', snippet: 'Chi_Square($0, df)', description: 'Cumulative chi-square distribution at x with df degrees of freedom.', usage: 'p = Chi_Square(5.99, 2)' },
      { label: 'Probability', snippet: 'Probability($0, mu, sigma)', description: 'Normal CDF: cumulative probability at x for mean mu and std dev sigma.', usage: 'p = Probability(85, 80, 5)   { 0.8413 }' },
    ],
  },
  {
    category: 'Random & Bitwise',
    items: [
      { label: 'Random', snippet: 'Random($0, )', description: 'Uniform random number in [a, b]; an optional 3rd argument seeds it.', usage: 'y = Random(0, 1)' },
      { label: 'RandG', snippet: 'RandG($0, sigma)', description: 'Gaussian random number with given mean and std dev; optional seed.', usage: 'y = RandG(0, 0.5)' },
      { label: 'BaseConvert', snippet: "BaseConvert('$0', 16, 10)", description: 'Convert a number written as a string from one base to another (2–36).', usage: "y = BaseConvert('FF', 16, 10)   { 255 }" },
      { label: 'BitAnd', snippet: 'BitAnd($0, )', description: 'Bitwise AND of two integers.', usage: 'y = BitAnd(12, 10)   { 8 }' },
      { label: 'BitOr', snippet: 'BitOr($0, )', description: 'Bitwise OR of two integers.', usage: 'y = BitOr(12, 10)   { 14 }' },
      { label: 'BitXor', snippet: 'BitXor($0, )', description: 'Bitwise XOR of two integers.', usage: 'y = BitXor(12, 10)   { 6 }' },
      { label: 'BitNot', snippet: 'BitNot($0)', description: 'Bitwise NOT of one integer.', usage: 'y = BitNot(0)' },
      { label: 'BitShiftL', snippet: 'BitShiftL($0, )', description: 'Left bit-shift a by n positions.', usage: 'y = BitShiftL(3, 4)   { 48 }' },
      { label: 'BitShiftR', snippet: 'BitShiftR($0, )', description: 'Right bit-shift a by n positions.', usage: 'y = BitShiftR(48, 2)   { 12 }' },
    ],
  },
  {
    category: 'Complex Numbers',
    items: [
      { label: 'Real', snippet: 'Real($0)', description: 'Real part of a complex value.', usage: 'a = Real(z)' },
      { label: 'Imag', snippet: 'Imag($0)', description: 'Imaginary part of a complex value.', usage: 'b = Imag(z)' },
      { label: 'Conj', snippet: 'Conj($0)', description: 'Complex conjugate.', usage: 'w = Conj(z)' },
      { label: 'Magnitude', snippet: 'Magnitude($0)', description: 'Magnitude (modulus) |z|.', usage: 'r = Magnitude(z)' },
      { label: 'Angle (rad)', snippet: 'Angle($0)', description: 'Argument (phase) of z in radians.', usage: 'phi = Angle(z)' },
      { label: 'AngleDeg', snippet: 'AngleDeg($0)', description: 'Argument (phase) of z in degrees.', usage: 'phi = AngleDeg(z)' },
      { label: 'Cis', snippet: 'Cis($0)', description: 'Unit complex number cos θ + i·sin θ.', usage: 'z = Cis(pi#/4)' },
    ],
  },
  {
    category: 'Matrix & Vector',
    items: [
      { label: 'SolveLinear', snippet: 'SolveLinear($0, b)', description: 'Solve the linear system A·x = b for the vector x.', usage: 'x[1:3] = SolveLinear(A[1:3,1:3], b[1:3])' },
      { label: 'Inverse', snippet: 'Inverse($0)', description: 'Inverse of a square matrix.', usage: 'Ai = Inverse(A)' },
      { label: 'Transpose', snippet: 'Transpose($0)', description: 'Transpose of a matrix or vector.', usage: 'At = Transpose(A)' },
      { label: 'Determinant', snippet: 'Determinant($0)', description: 'Determinant of a square matrix.', usage: 'd = Determinant(A)' },
      { label: 'Dot', snippet: 'Dot($0, )', description: 'Dot (inner) product of two vectors.', usage: 'd = Dot(a, b)' },
      { label: 'Cross', snippet: 'Cross($0, )', description: 'Cross product of two 3-vectors.', usage: 'c = Cross(a, b)' },
      { label: 'Norm', snippet: 'Norm($0)', description: 'Euclidean norm (length) of a vector.', usage: 'n = Norm(v)' },
      { label: 'Eigenvalues', snippet: 'Eigenvalues($0)', description: 'Eigenvalues of a square matrix; CALL Eigenvalues(A : re, im) returns a complex spectrum as real/imaginary parts.', usage: 'lambda = Eigenvalues(A)' },
      { label: 'Eigen', snippet: 'Eigen($0)', description: 'Eigenvalues and eigenvectors of a square matrix.', usage: 'Eigen(A)' },
      { label: 'LUDecompose', snippet: 'LUDecompose($0)', description: 'LU decomposition of a square matrix.', usage: 'LUDecompose(A)' },
      { label: 'zeros', snippet: 'zeros($0, )', description: 'Create an m×n zero matrix.', usage: 'A = zeros(3, 3)' },
      { label: 'ones', snippet: 'ones($0, )', description: 'Create an m×n all-ones matrix.', usage: 'A = ones(2, 4)' },
      { label: 'eye', snippet: 'eye($0)', description: 'Create an n×n identity matrix.', usage: 'I = eye(3)' },
      { label: 'identity', snippet: 'identity($0)', description: 'Alias for eye — n×n identity matrix.', usage: 'I = identity(4)' },
      { label: 'diag', snippet: 'diag($0)', description: 'Diagonal matrix from a vector, or extract diagonal of a matrix.', usage: 'D = diag(v[1:3])' },
      { label: 'linspace', snippet: 'linspace($0, , n)', description: 'Linearly spaced vector of n values from a to b.', usage: 'v[1:11] = linspace(0, 1, 11)' },
      { label: 'ArrayElmt', snippet: 'ArrayElmt($0[1:N], index)', description: 'Element of an array at a dynamic (expression) index.', usage: 'val = ArrayElmt(data[1:10], k)' },
      { label: 'axpy', snippet: 'axpy($0, x, y)', description: 'Compute alpha·x + y (BLAS-1 axpy).', usage: 'z[1:n] = axpy(2.5, x[1:n], y[1:n])' },
      { label: 'scal', snippet: 'scal($0, x)', description: 'Scale a vector by alpha: alpha·x (BLAS-1 scal).', usage: 'y[1:n] = scal(3, x[1:n])' },
      { label: 'asum', snippet: 'asum($0)', description: 'L1 norm (sum of absolute values) of a vector (BLAS-1 asum).', usage: 'n1 = asum(v[1:N])' },
      { label: 'nrm2', snippet: 'nrm2($0)', description: 'Euclidean L2 norm of a vector (BLAS-1 nrm2).', usage: 'n2 = nrm2(v[1:N])' },
      { label: 'copy', snippet: 'copy($0)', description: 'Return a symbolic copy of a vector.', usage: 'w[1:n] = copy(v[1:n])' },
      { label: 'gemv', snippet: 'gemv($0, A, x, beta, y)', description: 'Matrix-vector product: alpha·A·x + beta·y (BLAS-2 gemv).', usage: 'z[1:m] = gemv(1, A[1:m,1:n], x[1:n], 0, z[1:m])' },
      { label: 'ger', snippet: 'ger($0, x, y, A)', description: 'Outer product update: alpha·x·yᵀ + A (BLAS-2 ger).', usage: 'R[1:m,1:n] = ger(1, x[1:m], y[1:n], A[1:m,1:n])' },
      { label: 'gemm', snippet: 'gemm($0, A, B, beta, C)', description: 'Matrix-matrix product: alpha·A·B + beta·C (BLAS-3 gemm).', usage: 'D[1:m,1:n] = gemm(1, A[1:m,1:k], B[1:k,1:n], 0, D[1:m,1:n])' },
      { label: 'EulerRotate', snippet: 'CALL EulerRotate($0phi, theta, psi : R)', description: '3D rotation matrix from Euler angles φ, θ, ψ (ZYX convention, radians). Output R is a 3×3 matrix.', usage: 'CALL EulerRotate(phi, theta, psi : R[1:3,1:3])' },
    ],
  },
  {
    category: 'Strings',
    items: [
      { label: 'StringLen', snippet: 'StringLen($0)', description: 'Number of characters in a string.', usage: "n = StringLen('frees')   { 5 }" },
      { label: 'StringPos', snippet: 'StringPos($0, sub$)', description: 'Index of the first occurrence of sub$ in str$; 0 if not found.', usage: "i = StringPos('hello world', 'world')   { 7 }" },
      { label: 'StringVal', snippet: 'StringVal($0)', description: 'Convert a numeric string to a number.', usage: "v = StringVal('3.14')   { 3.14 }" },
    ],
  },
  {
    category: 'Lookup & Interpolation',
    items: [
      { label: 'Lookup', snippet: "Lookup('$0', row, col)", description: 'Return a single cell value from a TABLE by 1-based row and column indices (column 1 is the x axis, columns 2+ are the value/curve columns).', usage: "v = Lookup('fanCurve', 3, 2)" },
      { label: 'LookupRow', snippet: "LookupRow('$0', col, val)", description: 'Fractional 1-based row index where column col crosses val (linear).', usage: "r = LookupRow('myTable', 1, 25)" },
      { label: 'NLookupRows', snippet: "NLookupRows('$0')", description: 'Number of data rows in a TABLE.', usage: "n = NLookupRows('fanCurve')" },
      { label: 'Interpolate', snippet: "Interpolate('$0', x)", description: 'Piecewise-linear interpolation of a named TABLE at x (same as calling the table directly, table(x)).', usage: "h = Interpolate('steam', 250)" },
      { label: 'Interpolate1', snippet: "Interpolate1('$0', x)", description: 'Cubic-spline interpolation of a named TABLE at x (falls back to linear for fewer than 3 points).', usage: "h = Interpolate1('steam', 250)" },
      { label: 'Interpolate2D', snippet: "Interpolate2D('$0', x, y)", description: 'Bi-linear (2D) interpolation over a TABLE curve family — table(x) blended across the family parameter y (e.g. an engine/efficiency map). Equivalent to calling table(x, y).', usage: "z = Interpolate2D('map', rpm, load)" },
      { label: 'Differentiate', snippet: "Differentiate('$0', y_col, x_col, x_val)", description: 'Numerical derivative dy/dx at x_val from a TABLE (finite difference over the y_col vs x_col columns; column 1 is the x axis).', usage: "dhdx = Differentiate('steam', 2, 1, 250)" },
      { label: 'Differentiate1', snippet: "Differentiate1('$0', y_col, x_col, x_val)", description: 'Cubic-spline numerical derivative dy/dx at x_val.', usage: "dydx = Differentiate1('table', 2, 1, 5)" },
    ],
  },
  {
    category: 'Table Accessors',
    items: [
      { label: 'TableValue', snippet: 'TableValue($0, col)', description: 'Value of a cell in the current Parametric Table by row and column.', usage: 'v = TableValue(run#, 2)' },
      { label: 'TableRun#', snippet: 'TableRun#()', description: 'Current parametric run index (1-based).', usage: 'i = TableRun#()' },
      { label: 'NParametricRuns', snippet: 'NParametricRuns()', description: 'Total number of parametric runs in the active Parametric Table.', usage: 'n = NParametricRuns()' },
      { label: 'Table Sum', snippet: "TableSum('$0')", description: 'Sum of all values in a named Parametric Table column.', usage: "S = TableSum('Q_dot')" },
      { label: 'Table Avg', snippet: "TableAvg('$0')", description: 'Arithmetic mean of all values in a named column.', usage: "mu = TableAvg('T_out')" },
      { label: 'Table Min', snippet: "TableMin('$0')", description: 'Minimum value across all runs in a named column.', usage: "lo = TableMin('P')" },
      { label: 'Table Max', snippet: "TableMax('$0')", description: 'Maximum value across all runs in a named column.', usage: "hi = TableMax('P')" },
      { label: 'Table StdDev', snippet: "TableStdDev('$0')", description: 'Standard deviation of a named Parametric Table column.', usage: "sd = TableStdDev('h')" },
      { label: 'IntegralValue', snippet: "IntegralValue('$0', x_col)", description: 'Trapezoid-rule integral of a column with respect to x_col.', usage: "W = IntegralValue('Power', 'time')" },
    ],
  },
  {
    category: 'Conversion & Uncertainty',
    items: [
      { label: 'Convert', snippet: "Convert('$0', '')", description: 'Unit-conversion factor from one unit to another.', usage: "f = Convert('kJ', 'Btu')" },
      { label: 'ConvertTemp', snippet: 'ConvertTemp(C, F, $0)', description: 'Convert a temperature between scales (C, F, K, R).', usage: 'Tf = ConvertTemp(C, F, 100)   { 212 }' },
      { label: 'UncertaintyOf', snippet: 'UncertaintyOf($0)', description: 'Propagated uncertainty of a solved variable.', usage: 'u_T = UncertaintyOf(T)' },
    ],
  },
  {
    // Transient / ODE solving via the DYNAMIC ... END block. der(X) marks a
    // state; the accessor functions read solved columns back into the analytic
    // solution. Names mirror OdeAccessors in the backend.
    category: 'Dynamics (ODE)',
    items: [
      { label: 'der (state derivative)', snippet: 'der($0)', description: 'Mark a variable as an ODE state inside a DYNAMIC block: der(X) = rhs. Each state needs one initial condition X(0) = ….', usage: 'der(T) = -k * (T - T_inf)' },
      { label: 'ODEValue', snippet: "ODEValue('$0', t)", description: 'Value of a solved ODE column interpolated at time t.', usage: "v_t = ODEValue('v', 5)" },
      { label: 'FinalValue', snippet: "FinalValue('$0')", description: 'Last sampled value of a solved ODE column.', usage: "T_final = FinalValue('T')" },
      { label: 'MaxValue', snippet: "MaxValue('$0')", description: 'Maximum of a solved ODE column over the run.', usage: "T_peak = MaxValue('T')" },
      { label: 'MinValue', snippet: "MinValue('$0')", description: 'Minimum of a solved ODE column over the run.', usage: "y_min = MinValue('y')" },
      { label: 'TimeAt', snippet: "TimeAt('$0', value)", description: 'First time a solved ODE column crosses a given value.', usage: "t_apogee = TimeAt('v', 0)" },
      { label: 'ODEAvg', snippet: "ODEAvg('$0')", description: 'Time-series average of a solved ODE column.', usage: "T_avg = ODEAvg('T')" },
      { label: 'ODESum', snippet: "ODESum('$0')", description: 'Sum of the samples of a solved ODE column.', usage: "s = ODESum('rate')" },
      { label: 'ODEStdDev', snippet: "ODEStdDev('$0')", description: 'Standard deviation of a solved ODE column.', usage: "sigma = ODEStdDev('x')" },
    ],
  },
  {
    // Multi-line scaffolds for the structural blocks, so users don't have to
    // re-check the Help docs for the exact syntax.
    category: 'Blocks & Control Flow',
    items: [
      { label: 'DYNAMIC (ODE) block', snippet: 'DYNAMIC $0sys (method = ode45, t = 0 .. 10, points = 200)\n  der(x) = \n  x(0) = \nEND\n', description: 'Integrate a system of ODEs over time. Mark states with der(X) = rhs, give each an initial condition X(0) = …, then read results back with FinalValue / MaxValue / ODEValue.', usage: 'DYNAMIC sys (method = ode45, t = 0 .. 10) … der(x) = … … x(0) = … END' },
      { label: 'FUNCTION block', snippet: 'FUNCTION $0fname(x)\n  fname := \nEND\n', description: 'Define a reusable single-output function with a body of assignments (:=). Call it inline in any expression.', usage: 'FUNCTION f(x) … f := x^2 … END' },
      { label: 'FUNCTION (multi-output)', snippet: 'FUNCTION [$0out1, out2] = fname(x)\n  out1 := \n  out2 := \nEND\n', description: 'array-language-style function returning several outputs. Assign each output by name with := in the body, then call it with [a, b] = fname(x).', usage: 'FUNCTION [q, r] = DivMod(a, b) … q := … … r := … END\n[whole, rem] = DivMod(17, 5)' },
      { label: '[a, b] = f(x) (multi-call / destructuring)', snippet: '[$0a, b] = fname(x)', description: 'array-language-style destructuring. Works for user multi-output FUNCTIONs AND every built-in multi-output CALL function (e.g. [A, B, C, D] = tf2ss(num, den)). Use ~ to discard a slot, or omit trailing outputs you do not need.', usage: '[small, large] = Order(8, 3)\n[A, B, C, D] = tf2ss(num, den)\n[~, ~, V] = svd(M)\n[A, B] = tf2ss(num, den)   { C, D dropped }' },
      { label: 'PROCEDURE block', snippet: 'PROCEDURE $0pname(x : y)\n  y := \nEND\n', description: 'Define a procedure with inputs : outputs.', usage: 'PROCEDURE p(x : y) … END' },
      { label: 'MODULE block', snippet: 'MODULE $0mname(x : y)\n  y = \nEND\n', description: 'Define a module (reusable system of equations) with inputs : outputs.', usage: 'MODULE m(x : y) … END' },
      { label: 'TABLE (with units)', snippet: 'TABLE $0tname(x [unit]) [unit]\n  0   0\n  1   1\nEND\n', description: 'Define a lookup / interpolation table callable as a function.', usage: 'TABLE t(x [unit]) [unit] … END' },
      { label: 'PARAMETRIC table', snippet: 'PARAMETRIC $0sweep(x)\n  x = 0:1:10 | Linear\nEND\n', description: 'Declare a parametric sweep table in code.', usage: 'PARAMETRIC sweep(x) … END' },
      { label: 'PLOT block', snippet: "PLOT '$0'\n  kind = xy\n  x = \n  y = \nEND\n", description: 'Define a plot in code.', usage: "PLOT 'name' … END" },
      { label: 'STATE TABLE block', snippet: 'STATE TABLE $0Circuit1(P1, T1, h2)\n  FLUID = Water\nEND\n', description: 'Declare a fluid-aware state table: list the circuit’s state-point variables and the fluid (FLUID = ...) every state uses. Multiple blocks support multi-fluid / multi-circuit plants.', usage: 'STATE TABLE WaterCircuit(Pw_1, Pw_2, Tw1)  FLUID = Water  END' },
      { label: 'FOR loop', snippet: 'FOR i = 1 TO $0\n  \nEND\n', description: 'Generate equations over an integer index range.', usage: 'FOR i = 1 TO N … END' },
      { label: 'IF / THEN / ELSE (in FUNCTION)', snippet: 'IF $0 THEN\n  \nELSE\n  \nEND\n', description: 'Conditional branch inside a FUNCTION / PROCEDURE body.', usage: 'IF cond THEN … ELSE … END' },
      { label: 'REPEAT / UNTIL (in FUNCTION)', snippet: 'REPEAT\n  $0\nUNTIL ', description: 'Loop until a condition holds (inside a FUNCTION / PROCEDURE).', usage: 'REPEAT … UNTIL cond' },
      { label: 'WHILE / DO (in FUNCTION)', snippet: 'WHILE $0 DO\n  \nEND\n', description: 'While-loop (inside a FUNCTION / PROCEDURE).', usage: 'WHILE cond DO … END' },
    ],
  },
  {
    // Symbolic / CAS workflow: a SYMBOLIC variable turns an equation into an
    // identity (must hold for all values of that variable) that the CAS solves
    // for the remaining coefficients — e.g. Laplace partial-fraction residues.
    category: 'Control Systems (CAS)',
    items: [
      { label: 'SYMBOLIC (declare Laplace/symbolic var)', snippet: 'SYMBOLIC $0s', description: 'Declare an independent symbolic variable (e.g. the Laplace s). Any equation that contains it becomes an identity solved for the remaining coefficients, instead of solving for the symbolic variable itself.', usage: 'SYMBOLIC s' },
      { label: 'tf (transfer function)', snippet: 'tf([$0], [])', description: 'Transfer function num(s)/den(s) from coefficient arrays in descending powers ([1, 3, 2] = s^2 + 3s + 2). Use it on the left of a SYMBOLIC identity to decompose into partial fractions.', usage: 'tf([1, 3], [1, 3, 2])' },
      { label: 'Partial fractions (identity)', snippet: 'SYMBOLIC s\ntf([$0], []) = A/(s+1) + B/(s+2)', description: 'Decompose a transfer function into partial fractions: write the identity with named residues and frees solves for them (A, B appear in the Solution window).', usage: 'SYMBOLIC s\ntf([1, 3], [1, 3, 2]) = A/(s+1) + B/(s+2)' },
      { label: 'ss2tf (state-space to transfer function)', snippet: 'CALL ss2tf(A, B, C, D : num[1:$0], den[1:])', description: 'Convert state-space matrices A, B, C, D to transfer function coefficients num and den.', usage: 'CALL ss2tf(A, B, C, D : num[1:3], den[1:3])' },
      { label: 'tf2ss (transfer function to state-space)', snippet: 'CALL tf2ss(num, den : A[1:$0,1:], B[1:], C[1:], D)', description: 'Convert transfer function coefficients num and den to controllable canonical state-space matrices A, B, C, D.', usage: 'CALL tf2ss(num, den : A[1:2,1:2], B[1:2], C[1:2], D)' },
      { label: 'zp2tf (zero-pole-gain to transfer function)', snippet: 'CALL zp2tf(zr, zi, pr, pi, k : num[1:$0], den[1:])', description: 'Convert zero-pole-gain (zeros zr + j*zi, poles pr + j*pi, gain k) to transfer function coefficients num and den.', usage: 'CALL zp2tf(zr, zi, pr, pi, k : num[1:3], den[1:3])' },
      { label: 'tf2zp (transfer function to zero-pole-gain)', snippet: 'CALL tf2zp(num, den : zr[1:$0], zi[1:], pr[1:], pi[1:], k)', description: 'Convert transfer function coefficients num and den to zeros zr + j*zi, poles pr + j*pi, and gain k.', usage: 'CALL tf2zp(num, den : zr[1:1], zi[1:1], pr[1:2], pi[1:2], k)' },
      { label: 'series (series connection of systems)', snippet: 'CALL series(num1, den1, num2, den2 : num[1:$0], den[1:])', description: 'Connect two transfer functions in series (multiplies their transfer functions).', usage: 'CALL series(num1, den1, num2, den2 : num[1:3], den[1:3])' },
      { label: 'parallel (parallel connection of systems)', snippet: 'CALL parallel(num1, den1, num2, den2 : num[1:$0], den[1:])', description: 'Connect two transfer functions in parallel (adds their transfer functions).', usage: 'CALL parallel(num1, den1, num2, den2 : num[1:3], den[1:3])' },
      { label: 'feedback (feedback connection of systems)', snippet: 'CALL feedback(num1, den1, num2, den2, sign : num[1:$0], den[1:])', description: 'Connect two transfer functions in a feedback loop. sign is optional: 1.0 (default) for negative feedback, -1.0 for positive feedback.', usage: 'CALL feedback(num1, den1, num2, den2 : num[1:2], den[1:2])' },
      { label: 'pole (system poles)', snippet: 'CALL pole(num, den : pr[1:$0], pi[1:])', description: 'Compute system poles (real part pr, imaginary part pi) for a transfer function or state space matrix A.', usage: 'CALL pole(num, den : pr[1:2], pi[1:2]) or CALL pole(A : pr[1:2], pi[1:2])' },
      { label: 'zero (system zeros)', snippet: 'CALL zero(num, den : zr[1:$0], zi[1:])', description: 'Compute system zeros (real part zr, imaginary part zi) for a transfer function or state space system (A, B, C, D).', usage: 'CALL zero(num, den : zr[1:1], zi[1:1]) or CALL zero(A, B, C, D : zr[1:1], zi[1:1])' },
      { label: 'bode (Bode frequency response)', snippet: 'CALL bode(num, den, omega : mag[1:$0], phase[1:])', description: 'Compute Bode frequency response magnitude (dB) and unwrapped phase (deg) for a transfer function or state space system at given frequencies omega.', usage: 'CALL bode(num, den, omega : mag[1:50], phase[1:50])' },
      { label: 'nyquist (Nyquist frequency response)', snippet: 'CALL nyquist(num, den, omega : real[1:$0], imag[1:])', description: 'Compute Nyquist frequency response real and imaginary parts for a transfer function or state space system at given frequencies omega.', usage: 'CALL nyquist(num, den, omega : real[1:50], imag[1:50])' },
      { label: 'margin (gain and phase margins)', snippet: 'CALL margin(num, den : gm, pm, w_cg, w_cp)', description: 'Compute closed-loop gain margin (dB), phase margin (deg), and crossover frequencies (gain w_cg, phase w_cp) for a transfer function or state space system.', usage: 'CALL margin(num, den : gm, pm, w_cg, w_cp)' },
      { label: 'step (unit step response)', snippet: 'CALL step(num, den, t : y[1:$0])', description: 'Compute the unit step response y(t) for a transfer function or state space system at given time points t. Outputs are time/value arrays suitable for xy plotting.', usage: 'CALL step(num[1:3], den[1:3], t[1:100] : y[1:100])' },
      { label: 'impulse (impulse response)', snippet: 'CALL impulse(num, den, t : y[1:$0])', description: 'Compute the impulse response y(t) for a transfer function or state space system at given time points t. Outputs are time/value arrays suitable for xy plotting.', usage: 'CALL impulse(num[1:3], den[1:3], t[1:100] : y[1:100])' },
      { label: 'lsim (linear simulation)', snippet: 'CALL lsim(num, den, u, t : y[1:$0])', description: 'Simulate the output y(t) of a linear system driven by an arbitrary input signal u(t) at given time points t. Input u must be the same size as t.', usage: 'CALL lsim(num[1:3], den[1:3], u[1:100], t[1:100] : y[1:100])' },
      { label: 'lqr (LQR optimal gain)', snippet: 'CALL lqr(A, B, Q, R : K[1:$0])', description: 'Continuous-time LQR: optimal state-feedback gain K minimizing the quadratic cost with state weight Q and input weight R, solving the algebraic Riccati equation. Single-input: A and Q are n x n, B is an n-vector, R is a scalar, K is an n-vector.', usage: 'CALL lqr(A[1:2,1:2], B[1:2], Q[1:2,1:2], R : K[1:2])' },
      { label: 'dlqr (discrete LQR gain)', snippet: 'CALL dlqr(A, B, Q, R : K[1:$0,1:])', description: 'Discrete-time LQR: optimal state-feedback gain K for a discrete system, solving the discrete algebraic Riccati equation (DARE). A and Q are n x n, B is n x m, R is m x m, and K is m x n.', usage: 'CALL dlqr(A[1:2,1:2], B[1:2,1:1], Q[1:2,1:2], R : K[1:1,1:2])' },
      { label: 'place (pole placement)', snippet: 'CALL place(A, B, pr, pi : K[1:$0])', description: 'SISO pole placement (Ackermann): state-feedback gain K that moves the closed-loop poles of (A - B K) to the requested locations, given as real/imag arrays pr, pi (each length n).', usage: 'CALL place(A[1:2,1:2], B[1:2], pr[1:2], pi[1:2] : K[1:2])' },
      { label: 'acker (pole placement, Ackermann)', snippet: 'CALL acker(A, B, pr, pi : K[1:$0])', description: "SISO pole placement via Ackermann's formula (alias of place): state-feedback gain K placing the closed-loop poles of (A - B K) at the locations given by the real/imag arrays pr, pi (each length n).", usage: 'CALL acker(A[1:2,1:2], B[1:2], pr[1:2], pi[1:2] : K[1:2])' },
      { label: 'lqe (Kalman estimator gain)', snippet: 'CALL lqe(A, G, C, Q, R : L[1:$0,1:])', description: "Continuous-time Kalman estimator (LQE) gain L for the plant x' = A x + B u + G w, y = C x + v, with process-noise covariance Q and measurement-noise covariance R. Solves the filter Riccati equation; A is n x n, G is n x g, C is p x n, and L is n x p.", usage: 'CALL lqe(A[1:2,1:2], G[1:2,1:2], C[1:1,1:2], Q[1:2,1:2], R : L[1:2,1:1])' },
      { label: 'pidtune (auto-tune PID)', snippet: "CALL pidtune(num, den, '$0', wc : Kp, Ki, Kd)", description: "Auto-tune a P/PI/PID controller for plant num/den with gain crossover at frequency wc and a 60-degree phase-margin target. Type is a quoted 'P', 'PI', or 'PID'. Unused gains are returned as 0.", usage: "CALL pidtune(num[1:3], den[1:3], 'PID', wc : Kp, Ki, Kd)" },
      { label: 'ctrb (controllability matrix)', snippet: 'CALL ctrb(A, B : Ctrb[1:$0,1:])', description: 'Compute the controllability matrix Ctrb = [B, A*B, A^2*B, ..., A^(n-1)*B] for state-space matrices A (n x n) and B (n x m). Output Ctrb is n x (n*m).', usage: 'CALL ctrb(A[1:3,1:3], B[1:3] : Ctrb[1:3,1:3])' },
      { label: 'obsv (observability matrix)', snippet: 'CALL obsv(A, C : Obsv[1:$0,1:])', description: 'Compute the observability matrix Obsv = [C; C*A; C*A^2; ...; C*A^(n-1)] for state-space matrices A (n x n) and C (p x n). Output Obsv is (n*p) x n.', usage: 'CALL obsv(A[1:3,1:3], C[1:3] : Obsv[1:3,1:3])' },
      { label: 'gram (controllability/observability gramian)', snippet: "CALL gram(A, M, '$0' : W[1:,1:])", description: "System gramian via the Lyapunov equation. Type is a quoted 'c' (controllability gramian, with M = B) or 'o' (observability gramian, with M = C). Requires a stable A; the gramian W is n x n.", usage: "CALL gram(A[1:2,1:2], B[1:2,1:1], 'c' : W[1:2,1:2])" },
      { label: 'balreal (balanced realization)', snippet: 'CALL balreal(A, B, C : Ab[1:$0,1:], Bb[1:,1:], Cb[1:,1:])', description: 'Internally-balanced realization of a stable, minimal system (A, B, C): returns transformed matrices Ab, Bb, Cb whose controllability and observability gramians are equal and diagonal (the Hankel singular values). Useful for model reduction.', usage: 'CALL balreal(A[1:2,1:2], B[1:2,1:1], C[1:1,1:2] : Ab[1:2,1:2], Bb[1:2,1:1], Cb[1:1,1:2])' },
      { label: 'dare (discrete Riccati solution)', snippet: 'CALL dare(A, B, Q, R : X[1:$0,1:])', description: 'Solve the discrete-time algebraic Riccati equation (DARE), returning the stabilizing solution X (n x n). A and Q are n x n, B is n x m, R is m x m.', usage: 'CALL dare(A[1:2,1:2], B[1:2,1:1], Q[1:2,1:2], R : X[1:2,1:2])' },
      { label: 'lyap (continuous Lyapunov solve)', snippet: 'CALL lyap(A, Q : X[1:$0,1:])', description: "Solve the continuous-time Lyapunov equation A X + X A' + Q = 0 for X (n x n). Used for stability analysis and gramians.", usage: 'CALL lyap(A[1:2,1:2], Q[1:2,1:2] : X[1:2,1:2])' },
      { label: 'dlyap (discrete Lyapunov solve)', snippet: 'CALL dlyap(A, Q : X[1:$0,1:])', description: "Solve the discrete-time Lyapunov (Stein) equation A X A' - X + Q = 0 for X (n x n).", usage: 'CALL dlyap(A[1:2,1:2], Q[1:2,1:2] : X[1:2,1:2])' },
      { label: 'rank (matrix rank)', snippet: 'CALL rank(M : r)', description: 'Compute the numerical rank of matrix M using Singular Value Decomposition (SVD) tolerance comparison.', usage: 'CALL rank(M[1:3,1:3] : r)' },
      { label: 'ss2ss (state similarity transform)', snippet: 'CALL ss2ss(A, B, C, D, P : An[1:$0,1:], Bn[1:], Cn[1:], Dn)', description: 'Apply a similarity transformation matrix P to a state-space system (A, B, C, D) such that x = P * z, yielding transformed matrices An, Bn, Cn, Dn.', usage: 'CALL ss2ss(A[1:3,1:3], B[1:3], C[1:3], D, P[1:3,1:3] : An[1:3,1:3], Bn[1:3], Cn[1:3], Dn)' },
      { label: 'stepinfo (transient metrics)', snippet: 'CALL stepinfo(t, y : Tr, Tp, Ts, OS)', description: 'Extract transient response metrics (Rise Time Tr, Peak Time Tp, Settling Time Ts, and Percent Overshoot OS) from step response outputs y(t) at time points t.', usage: 'CALL stepinfo(t[1:100], y[1:100] : Tr, Tp, Ts, OS)' },
      { label: 'pade (time delay approximation)', snippet: 'CALL pade(Td, order : num_delay[1:$0], den_delay[1:])', description: 'Compute the numerator and denominator polynomial coefficients for a Padé approximation of time delay Td of a given order.', usage: 'CALL pade(Td, order : num_delay[1:3], den_delay[1:3])' },
      { label: 'rlocus (root locus trajectories)', snippet: 'CALL rlocus(num, den : K[1:$0], cpr[1:,1:], cpi[1:,1:])', description: 'Compute closed-loop pole trajectories (real parts cpr, imaginary parts cpi) by sweeping M gain steps K for a transfer function or state space system.', usage: 'CALL rlocus(num[1:3], den[1:3] : K[1:100], cpr[1:100,1:2], cpi[1:100,1:2])' },
      { label: 'routh (Routh-Hurwitz stability)', snippet: 'CALL routh(den : nRHP, stable)', description: 'Routh-Hurwitz test on a characteristic polynomial den (descending powers). Returns nRHP, the number of right-half-plane poles (sign changes in the first column), and stable (1 if nRHP=0, else 0). Handles the zero-in-first-column (epsilon) and row-of-zeros (auxiliary-polynomial) special cases.', usage: 'CALL routh(den[1:4] : nRHP, stable)' },
      { label: 'c2d (continuous to discrete)', snippet: "CALL c2d(num, den, Ts, '$0' : numz[1:], denz[1:])", description: "Discretize a continuous transfer function num/den at sample time Ts. Method is a quoted 'tustin' (bilinear, default) or 'zoh' (zero-order hold). num and den must be the same length; outputs numz/denz are that length in descending powers of z, normalized to a monic denominator.", usage: "CALL c2d(num[1:2], den[1:2], Ts, 'tustin' : numz[1:2], denz[1:2])" },
      { label: 'd2c (discrete to continuous)', snippet: "CALL d2c(numz, denz, Ts, 'tustin' : num[1:], den[1:])", description: "Convert a discrete transfer function numz/denz back to continuous time at sample time Ts using the inverse Tustin (bilinear) transform. Outputs num/den in descending powers of s, normalized to a monic denominator.", usage: "CALL d2c(numz[1:2], denz[1:2], Ts, 'tustin' : num[1:2], den[1:2])" },
      { label: 'residue (partial-fraction / inverse Laplace)', snippet: 'CALL residue(num, den : r_r[1:$0], r_i[1:], p_r[1:], p_i[1:], k)', description: 'Partial-fraction (Heaviside) expansion of num/den: residues (r_r, r_i), the matching poles (p_r, p_i), and the scalar direct term k, so that num/den = sum r_i/(s - p_i)^ord + k. The numeric inverse Laplace path — residues appear in the Solution window. Add a 6th output ord (CALL residue(num, den : r_r, r_i, p_r, p_i, ord, k)) to handle repeated poles, where ord is the power of each (s-p) term.', usage: 'CALL residue(num[1:1], den[1:3] : r_r[1:2], r_i[1:2], p_r[1:2], p_i[1:2], k)' },
      { label: 'nichols (Nichols chart data)', snippet: 'CALL nichols(num, den, omega : mag[1:$0], phase[1:])', description: 'Open-loop magnitude (dB) and unwrapped phase (deg) at frequencies omega, for a transfer function or state space system. Plot mag vs phase (xy kind) to form a Nichols chart.', usage: 'CALL nichols(num[1:2], den[1:2], omega[1:50] : mag[1:50], phase[1:50])' },
      { label: 'errorconst (steady-state error constants)', snippet: 'CALL errorconst(num, den : Kp, Kv, Ka)', description: 'Static error constants for an open-loop G(s)=num/den (in lowest terms): position Kp = lim G, velocity Kv = lim s*G, acceleration Ka = lim s^2*G as s->0. Constants that are infinite for the system type are returned as Infinity.', usage: 'CALL errorconst(num[1:3], den[1:3] : Kp, Kv, Ka)' },
      { label: 'mason (signal-flow graph gain)', snippet: 'CALL mason(G, source, sink : T)', description: "Overall transmittance of a scalar signal-flow graph by Mason's gain formula. G is a square node-gain matrix (G[i,j] = branch gain from node i to node j, 0 = no branch); source and sink are 1-based node numbers.", usage: 'CALL mason(G[1:4,1:4], 1, 4 : T)' },
    ],
  },
]

// Block-construct keywords whose catalog snippets are scaffolds, not callable
// functions (so they are excluded from the bare callable-name list below).
const BLOCK_KEYWORDS = new Set([
  'FOR', 'TO', 'STEP', 'WHILE', 'DO', 'REPEAT', 'UNTIL', 'IF', 'THEN', 'ELSE',
  'END', 'FUNCTION', 'PROCEDURE', 'MODULE', 'CALL', 'PARAMETRIC', 'TABLE',
  'PLOT', 'DUPLICATE', 'AND', 'OR', 'NOT', 'DYNAMIC', 'STATE', 'EVENT', 'SYMBOLIC',
])

/**
 * Bare callable function names from the catalog: the callee of each CALL snippet
 * (e.g. `CALL lqr(...)` -> `lqr`), otherwise the snippet's leading identifier,
 * with block-construct scaffolds removed. Shared by the editor's autocomplete /
 * syntax highlighting and the REPL's Tab-completion so both stay in sync with
 * the Functions menu.
 */
export function catalogFunctionNames(): string[] {
  const names = FUNCTION_CATEGORIES.flatMap((c) => c.items)
    .map((it) => {
      const call = /^CALL\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(it.snippet)
      if (call) return call[1]
      return /^([A-Za-z_][A-Za-z0-9_]*\$?)/.exec(it.snippet)?.[1] ?? ''
    })
    .filter((name) => name && !BLOCK_KEYWORDS.has(name.toUpperCase()))
  return Array.from(new Set(names))
}
