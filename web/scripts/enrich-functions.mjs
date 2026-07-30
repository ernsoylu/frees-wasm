// Promote auto-generated baseline pages to curated rich pages by attaching a
// standard closed-form Mathematical Formulation + citation to each function. The
// formulas are the well-known textbook forms (also present in the FunctionRegistry
// descriptions); wrapper functions with no closed form (CoolProp properties, solid
// materials, CAS ops) are finalized without a fabricated math section.
//
// Operates on the existing baseline .md files: rewrites body + frontmatter
// (drops `generated: true`, fills `references`). Hand-authored pages and pages not
// listed here are left untouched.
//
// Run: node scripts/build-doc-manifest.mjs && node scripts/enrich-functions.mjs

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REF = path.join(__dirname, '../src/docs/reference');
const manifest = JSON.parse(fs.readFileSync(path.join(REF, 'function-manifest.json'), 'utf-8'));
const sigOf = {};
for (const f of [...manifest.functions, ...manifest.matrixFunctions]) sigOf[f.name.toLowerCase()] = f;
for (const p of manifest.callProcedures) sigOf[p.name.toLowerCase()] = p;

// Generic reference descriptors
const SPECFN_REF = 'NIST Digital Library of Mathematical Functions (dlmf.nist.gov)';
const LINALG_REF = 'Standard numerical linear-algebra references';
const THERMO_REF = 'Standard engineering-thermodynamics references (compressible flow)';
const COMPFLOW_REF = 'Standard compressible-flow references';
const TWOPHASE_REF = 'Standard two-phase flow and boiling/condensation references';
const HEATMASS_REF = 'Standard heat- and mass-transfer references';
const FLUIDS_REF = 'Standard fluid-mechanics references';
const STATS_REF = 'Standard engineering-statistics references';
const ISA76 = 'U.S. Standard Atmosphere, 1976 (NOAA/NASA/USAF)';
const COMBUST_REF = 'Standard combustion references';
const NUMERICS_REF = 'Standard numerical-methods references';

// name -> { m: [katex lines], r: [refs] }.  `m` omitted ⇒ finalize without math.
const R = {
  // ── Elementary math ──
  abs: { m: ['|x| = \\begin{cases} x & x \\ge 0 \\\\ -x & x < 0 \\end{cases}'], r: [] },
  sign: { m: ['\\operatorname{sign}(x) = \\begin{cases} -1 & x<0 \\\\ 0 & x=0 \\\\ 1 & x>0 \\end{cases}'], r: [] },
  floor: { m: ['\\lfloor x \\rfloor = \\max\\{n \\in \\mathbb{Z} : n \\le x\\}'], r: [] },
  ceil: { m: ['\\lceil x \\rceil = \\min\\{n \\in \\mathbb{Z} : n \\ge x\\}'], r: [] },
  round: { m: ['\\operatorname{round}(x, d) = \\frac{\\lfloor x\\cdot 10^{d} + 0.5\\rfloor}{10^{d}}'], r: [] },
  trunc: { m: ['\\operatorname{trunc}(x) = \\operatorname{sign}(x)\\,\\lfloor |x| \\rfloor'], r: [] },
  mod: { m: ['\\operatorname{mod}(a,b) = a - b\\,\\lfloor a/b \\rfloor'], r: [] },
  factorial: { m: ['n! = \\prod_{k=1}^{n} k = n\\,(n-1)!', '\\quad n! = \\Gamma(n+1)'], r: [SPECFN_REF] },
  gcd: { m: ['\\gcd(a,b) = \\gcd(b,\\ a \\bmod b), \\qquad \\gcd(a,0)=a \\quad\\text{(Euclid)}'], r: [] },
  lcm: { m: ['\\operatorname{lcm}(a,b) = \\frac{|a\\,b|}{\\gcd(a,b)}'], r: [] },
  product: { m: ['\\prod_{i=\\text{lo}}^{\\text{hi}} \\text{term}(i)'], r: [] },
  tan: { m: ['\\tan(x) = \\frac{\\sin x}{\\cos x}, \\qquad x \\text{ in radians}'], r: [] },
  asin: { m: ['y = \\arcsin(x), \\qquad \\sin(y) = x,\\ \\ y \\in [-\\tfrac{\\pi}{2}, \\tfrac{\\pi}{2}]'], r: [] },
  acos: { m: ['y = \\arccos(x), \\qquad \\cos(y) = x,\\ \\ y \\in [0, \\pi]'], r: [] },
  atan: { m: ['y = \\arctan(x), \\qquad \\tan(y) = x'], r: [] },
  arcsin: { m: ['y = \\arcsin(x), \\qquad \\sin(y) = x,\\ \\ y \\in [-\\tfrac{\\pi}{2}, \\tfrac{\\pi}{2}]'], r: [] },
  arccos: { m: ['y = \\arccos(x), \\qquad \\cos(y) = x,\\ \\ y \\in [0, \\pi]'], r: [] },
  arctan: { m: ['y = \\arctan(x), \\qquad \\tan(y) = x'], r: [] },
  atan2: { m: ['\\operatorname{atan2}(y,x) = \\arg(x + jy) \\in (-\\pi, \\pi]'], r: [] },
  sinh: { m: ['\\sinh(x) = \\frac{e^{x} - e^{-x}}{2}'], r: [] },
  cosh: { m: ['\\cosh(x) = \\frac{e^{x} + e^{-x}}{2}'], r: [] },
  tanh: { m: ['\\tanh(x) = \\frac{\\sinh x}{\\cosh x} = \\frac{e^{x}-e^{-x}}{e^{x}+e^{-x}}'], r: [] },
  arcsinh: { m: ['\\operatorname{arcsinh}(x) = \\ln\\!\\big(x + \\sqrt{x^2+1}\\big)'], r: [] },
  arccosh: { m: ['\\operatorname{arccosh}(x) = \\ln\\!\\big(x + \\sqrt{x^2-1}\\big), \\quad x \\ge 1'], r: [] },
  arctanh: { m: ['\\operatorname{arctanh}(x) = \\tfrac12\\ln\\!\\frac{1+x}{1-x}, \\quad |x| < 1'], r: [] },
  log10: { m: ['y = \\log_{10}(x) = \\frac{\\ln x}{\\ln 10}, \\quad x > 0'], r: [] },
  bitand: { m: ['(a \\,\\&\\, b)\\ \\text{— bitwise AND of the integer operands}'], r: [] },
  bitor: { m: ['(a \\mathbin{|} b)\\ \\text{— bitwise OR of the integer operands}'], r: [] },
  bitxor: { m: ['(a \\oplus b)\\ \\text{— bitwise XOR of the integer operands}'], r: [] },
  bitnot: { m: ['(\\sim a) = -(a+1)\\ \\text{(two’s complement)}'], r: [] },
  bitshiftl: { m: ['a \\ll n = a\\cdot 2^{n}'], r: [] },
  bitshiftr: { m: ['a \\gg n = \\lfloor a / 2^{n} \\rfloor'], r: [] },
  arrayelmt: { m: ['\\operatorname{ArrayElmt}(\\{a_1,\\dots,a_n\\}, i) = a_i'], r: [] },
  baseconvert: { m: ['\\operatorname{baseconvert}(s) = \\text{numeric value of the based literal } s \\ (\\text{e.g. } \\mathtt{0xFF} \\to 255)'], r: [] },

  // ── Complex ──
  real: { m: ['\\Re(z) = \\Re(a + jb) = a'], r: [] },
  imag: { m: ['\\Im(z) = \\Im(a + jb) = b'], r: [] },
  conj: { m: ['\\bar z = \\overline{a + jb} = a - jb'], r: [] },
  magnitude: { m: ['|z| = \\sqrt{a^2 + b^2}'], r: [] },
  angle: { m: ['\\arg(z) = \\operatorname{atan2}(b, a)\\ \\ [\\text{rad}]'], r: [] },
  angledeg: { m: ['\\arg(z) = \\operatorname{atan2}(b, a)\\cdot\\tfrac{180}{\\pi}\\ \\ [\\text{deg}]'], r: [] },
  cis: { m: ['\\operatorname{cis}(\\theta) = e^{j\\theta} = \\cos\\theta + j\\sin\\theta'], r: [] },

  // ── Statistics ──
  mean: { m: ['\\bar x = \\frac{1}{n}\\sum_{i=1}^{n} x_i'], r: [STATS_REF] },
  average: { m: ['\\bar x = \\frac{1}{n}\\sum_{i=1}^{n} x_i'], r: [STATS_REF] },
  median: { m: ['\\text{the middle order statistic (mean of the two middle values if } n \\text{ even)}'], r: [STATS_REF] },
  sum: { m: ['\\sum_{i=1}^{n} x_i'], r: [] },
  std: { m: ['s = \\sqrt{\\frac{1}{n-1}\\sum_{i=1}^{n}(x_i - \\bar x)^2}'], r: [STATS_REF] },
  var: { m: ['s^2 = \\frac{1}{n-1}\\sum_{i=1}^{n}(x_i - \\bar x)^2'], r: [STATS_REF] },
  rms: { m: ['x_{\\text{rms}} = \\sqrt{\\frac{1}{n}\\sum_{i=1}^{n} x_i^2}'], r: [] },
  percentile: { m: ['P_p = \\text{value below which } p\\% \\text{ of the data fall (linear interpolation)}'], r: [STATS_REF] },
  probability: { m: ['\\Pr(X \\le x) = \\Phi\\!\\left(\\frac{x-\\mu}{\\sigma}\\right)'], r: [STATS_REF] },
  normalcdf: { m: ['\\Phi(x;\\mu,\\sigma) = \\tfrac12\\left[1 + \\operatorname{erf}\\!\\frac{x-\\mu}{\\sigma\\sqrt2}\\right]'], r: [STATS_REF] },
  normalpdf: { m: ['\\phi(x;\\mu,\\sigma) = \\frac{1}{\\sigma\\sqrt{2\\pi}}\\,e^{-(x-\\mu)^2/(2\\sigma^2)}'], r: [STATS_REF] },
  normalinvcdf: { m: ['x = \\Phi^{-1}(p;\\mu,\\sigma) = \\mu + \\sigma\\sqrt2\\,\\operatorname{erf}^{-1}(2p-1)'], r: [STATS_REF] },
  chi_square: { m: ['F(x; k) = \\frac{\\gamma(k/2,\\ x/2)}{\\Gamma(k/2)} \\quad\\text{(chi-square CDF, } k \\text{ d.o.f.)}'], r: [STATS_REF] },
  random: { m: ['X \\sim \\mathcal{U}(a, b), \\qquad X = a + (b-a)\\,U,\\ \\ U\\in[0,1)'], r: [] },
  randg: { m: ['X \\sim \\mathcal{N}(\\mu, \\sigma^2)'], r: [STATS_REF] },
  slope: { m: ['m = \\frac{\\sum (x_i-\\bar x)(y_i-\\bar y)}{\\sum (x_i-\\bar x)^2} \\quad\\text{(least squares)}'], r: [STATS_REF] },
  intercept: { m: ['b = \\bar y - m\\,\\bar x'], r: [STATS_REF] },
  r2: { m: ['R^2 = 1 - \\frac{\\sum (y_i - \\hat y_i)^2}{\\sum (y_i - \\bar y)^2}'], r: [STATS_REF] },

  // ── Matrix / linear algebra ──
  solvelinear: { m: ['A\\,x = b \\;\\Rightarrow\\; x = A^{-1}b \\quad\\text{(via } PA = LU\\text{, forward/back substitution)}'], r: [LINALG_REF] },
  inverse: { m: ['A\\,A^{-1} = A^{-1}A = I'], r: [LINALG_REF] },
  inv: { m: ['A\\,A^{-1} = A^{-1}A = I'], r: [LINALG_REF] },
  determinant: { m: ['\\det(A) = \\sum_{\\sigma} \\operatorname{sgn}(\\sigma)\\prod_i A_{i,\\sigma(i)} = \\pm\\prod_i U_{ii}'], r: [LINALG_REF] },
  det: { m: ['\\det(A) = \\pm\\prod_i U_{ii} \\quad\\text{(from } PA = LU\\text{)}'], r: [LINALG_REF] },
  trace: { m: ['\\operatorname{tr}(A) = \\sum_i A_{ii} = \\sum_i \\lambda_i'], r: [LINALG_REF] },
  transpose: { m: ['(A^\\top)_{ij} = A_{ji}'], r: [] },
  dot: { m: ['a \\cdot b = \\sum_i a_i b_i'], r: [] },
  cross: { m: ['a \\times b = (a_2 b_3 - a_3 b_2,\\ a_3 b_1 - a_1 b_3,\\ a_1 b_2 - a_2 b_1)'], r: [] },
  norm: { m: ['\\lVert v \\rVert_2 = \\sqrt{\\textstyle\\sum_i v_i^2}'], r: [LINALG_REF] },
  eig: { m: ['A v = \\lambda v, \\qquad \\det(A - \\lambda I) = 0'], r: [LINALG_REF] },
  eigvec: { m: ['A v_i = \\lambda_i v_i \\quad\\text{(columns are the eigenvectors)}'], r: [LINALG_REF] },
  rank: { m: ['\\operatorname{rank}(A) = \\#\\{\\sigma_i > \\text{tol}\\} \\quad\\text{(numerical, via SVD)}'], r: [LINALG_REF] },
  svd: { m: ['A = U\\,\\Sigma\\,V^\\top, \\qquad \\Sigma = \\operatorname{diag}(\\sigma_1 \\ge \\dots \\ge \\sigma_r > 0)'], r: [LINALG_REF] },
  qr: { m: ['A = Q\\,R, \\qquad Q^\\top Q = I,\\ R\\ \\text{upper triangular}'], r: [LINALG_REF] },
  cholesky: { m: ['A = L\\,L^\\top \\quad\\text{(} A \\text{ symmetric positive-definite)}'], r: [LINALG_REF] },
  matexp: { m: ['e^{A} = \\sum_{k=0}^{\\infty} \\frac{A^{k}}{k!}'], r: [LINALG_REF] },
  zeros: { m: ['Z_{ij} = 0 \\quad (m\\times n)'], r: [] },
  ones: { m: ['J_{ij} = 1 \\quad (m\\times n)'], r: [] },
  eye: { m: ['I_{ij} = \\delta_{ij} \\quad (n\\times n)'], r: [] },
  linspace: { m: ['x_k = a + (b-a)\\,\\frac{k-1}{n-1}, \\quad k = 1,\\dots,n'], r: [] },
  axpy: { m: ['y \\leftarrow \\alpha x + y \\quad\\text{(BLAS level 1)}'], r: [] },
  gemv: { m: ['y \\leftarrow \\alpha A x + \\beta y \\quad\\text{(BLAS level 2)}'], r: [] },
  gemm: { m: ['C \\leftarrow \\alpha A B + \\beta C \\quad\\text{(BLAS level 3)}'], r: [] },

  // ── Compressible flow ──
  rho0_rho: { m: ['\\frac{\\rho_0}{\\rho} = \\left(1 + \\tfrac{k-1}{2}M^2\\right)^{1/(k-1)}'], r: [THERMO_REF] },
  a_astar: { m: ['\\frac{A}{A^*} = \\frac{1}{M}\\left[\\frac{2}{k+1}\\left(1 + \\tfrac{k-1}{2}M^2\\right)\\right]^{(k+1)/[2(k-1)]}'], r: [THERMO_REF] },
  t2_t1_shock: { m: ['\\frac{T_2}{T_1} = \\frac{\\big[1 + \\tfrac{k-1}{2}M_1^2\\big]\\big[\\tfrac{2k}{k-1}M_1^2 - 1\\big]}{M_1^2\\,(k+1)^2/[2(k-1)]}'], r: [THERMO_REF] },
  rho2_rho1_shock: { m: ['\\frac{\\rho_2}{\\rho_1} = \\frac{(k+1)M_1^2}{2 + (k-1)M_1^2}'], r: [THERMO_REF] },
  prandtlmeyer: { m: ['\\nu(M) = \\sqrt{\\tfrac{k+1}{k-1}}\\,\\arctan\\!\\sqrt{\\tfrac{k-1}{k+1}(M^2-1)} - \\arctan\\!\\sqrt{M^2-1}'], r: [COMPFLOW_REF] },
  mach_prandtlmeyer: { m: ['\\text{solve } \\nu(M) = \\nu_{\\text{target}} \\text{ for } M \\quad (M \\ge 1)'], r: [COMPFLOW_REF] },
  machangle: { m: ['\\mu = \\arcsin\\!\\frac{1}{M}'], r: [COMPFLOW_REF] },
  theta_oblique: { m: ['\\tan\\theta = 2\\cot\\beta\\,\\frac{M_1^2\\sin^2\\beta - 1}{M_1^2(k + \\cos 2\\beta) + 2}'], r: [COMPFLOW_REF] },
  beta_oblique: { m: ['\\text{solve the } \\theta\\text{-}\\beta\\text{-}M \\text{ relation for the wave angle } \\beta \\ (\\text{weak/strong root})'], r: [COMPFLOW_REF] },
  rayleigh_t0_t0star: { m: ['\\frac{T_0}{T_0^*} = \\frac{(k+1)M^2\\,[2 + (k-1)M^2]}{(1 + kM^2)^2}'], r: [THERMO_REF] },
  rayleigh_t_tstar: { m: ['\\frac{T}{T^*} = \\left(\\frac{(k+1)M}{1 + kM^2}\\right)^2'], r: [THERMO_REF] },
  rayleigh_p_pstar: { m: ['\\frac{P}{P^*} = \\frac{k+1}{1 + kM^2}'], r: [THERMO_REF] },
  rayleigh_p0_p0star: { m: ['\\frac{P_0}{P_0^*} = \\frac{k+1}{1+kM^2}\\left[\\frac{2 + (k-1)M^2}{k+1}\\right]^{k/(k-1)}'], r: [THERMO_REF] },
  fanno_t_tstar: { m: ['\\frac{T}{T^*} = \\frac{k+1}{2 + (k-1)M^2}'], r: [THERMO_REF] },
  fanno_p_pstar: { m: ['\\frac{P}{P^*} = \\frac{1}{M}\\sqrt{\\frac{k+1}{2 + (k-1)M^2}}'], r: [THERMO_REF] },
  fanno_p0_p0star: { m: ['\\frac{P_0}{P_0^*} = \\frac{1}{M}\\left[\\frac{2 + (k-1)M^2}{k+1}\\right]^{(k+1)/[2(k-1)]}'], r: [THERMO_REF] },
  fanno_fld: { m: ['\\frac{4 f L^*}{D} = \\frac{1-M^2}{kM^2} + \\frac{k+1}{2k}\\ln\\frac{(k+1)M^2}{2 + (k-1)M^2}'], r: [THERMO_REF] },

  // ── Flow networks ──
  reynolds: { m: ['Re = \\frac{\\rho V D}{\\mu}'], r: [FLUIDS_REF] },
  minor_loss: { m: ['\\Delta P = K\\,\\tfrac12\\rho V^2'], r: [FLUIDS_REF] },
  friction_factor: { m: ['\\frac{1}{\\sqrt{f}} = -2\\log_{10}\\!\\left(\\frac{\\varepsilon/D}{3.7} + \\frac{2.51}{Re\\sqrt{f}}\\right) \\quad\\text{(Colebrook; } f = 64/Re \\text{ laminar)}'], r: [FLUIDS_REF] },

  // ── Pneumatics ──
  iso6358: { m: ['\\dot m = C\\,\\rho_0\\,P_{up}\\sqrt{\\tfrac{T_0}{T_{up}}}\\cdot\\begin{cases} 1 & P_{down}/P_{up} \\le b \\\\ \\sqrt{1 - \\big(\\tfrac{P_{down}/P_{up} - b}{1-b}\\big)^2} & \\text{else} \\end{cases}'], r: ['ISO 6358 — Pneumatic fluid power: flow-rate characteristics'] },

  // ── Atmosphere (ISA 1976) ──
  isa_t: { m: ['T(h) = T_b + L_b\\,(h - h_b) \\quad\\text{(layer lapse rate } L_b)'], r: [ISA76] },
  isa_p: { m: ['P(h) = P_b\\left(\\frac{T_b}{T_b + L_b(h-h_b)}\\right)^{g_0 M/(R L_b)} \\quad (L_b \\ne 0)'], r: [ISA76] },
  isa_rho: { m: ['\\rho(h) = \\frac{P(h)\\,M}{R\\,T(h)}'], r: [ISA76] },

  // ── Two-phase flow ──
  lm_phi2: { m: ['\\phi_l^2 = 1 + \\frac{C}{X} + \\frac{1}{X^2} \\quad\\text{(Chisholm)}'], r: [TWOPHASE_REF] },
  lm_martinelli_tt: { m: ['X_{tt} = \\left(\\frac{1-x}{x}\\right)^{0.9}\\left(\\frac{\\rho_g}{\\rho_l}\\right)^{0.5}\\left(\\frac{\\mu_l}{\\mu_g}\\right)^{0.1}'], r: [TWOPHASE_REF] },
  void_homogeneous: { m: ['\\alpha = \\frac{1}{1 + \\frac{1-x}{x}\\frac{\\rho_g}{\\rho_l}} \\quad\\text{(no slip)}'], r: [TWOPHASE_REF] },
  void_zivi: { m: ['\\alpha = \\frac{1}{1 + \\frac{1-x}{x}\\left(\\frac{\\rho_g}{\\rho_l}\\right)^{2/3}} \\quad\\text{(slip } S = (\\rho_l/\\rho_g)^{1/3})'], r: [TWOPHASE_REF] },
  void_rouhani: { m: ['\\alpha = \\frac{x}{\\rho_g}\\left[(1 + 0.12(1-x))\\left(\\frac{x}{\\rho_g} + \\frac{1-x}{\\rho_l}\\right) + \\frac{1.18(1-x)[g\\sigma(\\rho_l-\\rho_g)]^{0.25}}{G\\rho_l^{0.5}}\\right]^{-1}'], r: [TWOPHASE_REF] },
  friedel_phi2: { m: ['\\phi_{lo}^2 = E + \\frac{3.24\\,F H}{Fr^{0.045}We^{0.035}} \\quad\\text{(Friedel)}'], r: [TWOPHASE_REF] },
  momentum_flux: { m: ['\\left(\\frac{d P}{d z}\\right)_{\\text{acc}} = G^2\\frac{d}{dz}\\left[\\frac{x^2}{\\rho_g\\alpha} + \\frac{(1-x)^2}{\\rho_l(1-\\alpha)}\\right]'], r: [TWOPHASE_REF] },
  nu_dittus_boelter: { m: ['Nu = 0.023\\,Re^{0.8}\\,Pr^{n} \\quad (n = 0.4 \\text{ heating},\\ 0.3 \\text{ cooling})'], r: [HEATMASS_REF] },
  nu_gnielinski: { m: ['Nu = \\frac{(f/8)(Re-1000)Pr}{1 + 12.7\\sqrt{f/8}\\,(Pr^{2/3}-1)}'], r: [HEATMASS_REF] },
  chen_f: { m: ['F = \\big[1 + X_{tt}^{-1}\\big]^{0.736} \\text{-type convective enhancement (Chen)}'], r: [TWOPHASE_REF] },
  chen_s: { m: ['S = \\frac{1}{1 + 2.53\\times10^{-6}\\,Re_l^{1.17}} \\quad\\text{(nucleate suppression, Chen)}'], r: [TWOPHASE_REF] },
  nu_shah: { m: ['Nu_{TP} = Nu_l\\left(1 + \\frac{3.8}{Z^{0.95}}\\right), \\quad Z = (1/x - 1)^{0.8}p_r^{0.4} \\quad\\text{(Shah)}'], r: [TWOPHASE_REF] },
  nu_cavallini_zecchin: { m: ['Nu = 0.05\\,Re_{eq}^{0.8}\\,Pr_l^{0.33} \\quad\\text{(Cavallini–Zecchin condensation)}'], r: [TWOPHASE_REF] },
  zone_ramp: { m: ['r(L) = \\tanh\\!\\left(\\frac{L}{\\varepsilon}\\right) \\quad\\text{(smooth zone-collapse ramp)}'], r: [TWOPHASE_REF] },

  // ── Heat-transfer correlations / geometry ──
  dp_1phase: { m: ['\\Delta P = f\\,\\frac{L}{D_h}\\,\\frac{G^2}{2\\rho}, \\qquad G = \\dot m / A_{\\text{flow}} \\quad\\text{(Darcy)}'], r: [FLUIDS_REF] },
  dp_mueller_steinhagen: { m: ['\\frac{dP}{dz} = G_{ms}(1-x)^{1/3} + B\\,x^3, \\quad G_{ms} = A + 2(B-A)x \\quad\\text{(Müller-Steinhagen–Heck)}'], r: [TWOPHASE_REF] },
  dp_compact_core: { m: ['\\frac{\\Delta P}{P_1} = \\frac{G^2}{2\\rho_1 P_1}\\left[(1+\\sigma^2)\\!\\left(\\tfrac{\\rho_1}{\\rho_2}-1\\right) + f\\tfrac{A}{A_c}\\tfrac{\\rho_1}{\\rho_m}\\right] \\quad\\text{(compact-core)}'], r: ['Standard compact heat-exchanger references'] },
  dp_gravity: { m: ['\\Delta P_{\\text{grav}} = \\big[\\alpha\\rho_g + (1-\\alpha)\\rho_l\\big]\\,g\\,L\\sin\\theta'], r: [TWOPHASE_REF] },
  dp_2phase_avg: { m: ['\\Delta P = \\frac{1}{n}\\sum_{i=1}^{n} \\phi_l^2(x_i)\\,\\left(\\frac{dP}{dz}\\right)_{l,i} \\Delta z \\quad\\text{(quality-integrated)}'], r: [TWOPHASE_REF] },
  mass_flux: { m: ['G = \\frac{\\dot m}{A_{\\text{flow}}}'], r: [] },
  hx_dh: { m: ['D_h = \\frac{4\\,A_{\\text{flow}}\\,L}{A_{\\text{total}}}'], r: ['Standard compact heat-exchanger references'] },
  hx_sigma: { m: ['\\sigma = \\frac{A_{\\text{flow}}}{A_{\\text{frontal}}} \\quad\\text{(free-flow / contraction ratio)}'], r: ['Standard compact heat-exchanger references'] },
  nu_zukauskas: { m: ['Nu = C\\,Re_{\\max}^{m}\\,Pr^{0.36}\\,(Pr/Pr_w)^{1/4} \\quad\\text{(tube bank)}'], r: [HEATMASS_REF] },
  nu_tubebank: { m: ['Nu = C\\,Re_{\\max}^{m}\\,Pr^{0.36}\\,(Pr/Pr_w)^{1/4} \\quad (C, m \\text{ by arrangement/Re band})'], r: [HEATMASS_REF] },
  nu_colburn: { m: ['Nu = j\\,Re\\,Pr^{1/3} \\quad\\text{(Colburn } j\\text{-factor)}'], r: [HEATMASS_REF] },
  nu_churchill_chu: { m: ['Nu = \\left\\{0.60 + \\frac{0.387\\,Ra^{1/6}}{[1 + (0.559/Pr)^{9/16}]^{8/27}}\\right\\}^2 \\quad\\text{(Churchill–Chu)}'], r: [HEATMASS_REF] },
  nu_blend: { m: ['Nu = \\big(Nu_1^3 + Nu_2^3\\big)^{1/3} \\quad\\text{(free+forced cubic blend)}'], r: [HEATMASS_REF] },
  nu_hilpert: { m: ['Nu = C\\,Re^{m}\\,Pr^{1/3} \\quad\\text{(single cylinder, Hilpert)}'], r: [HEATMASS_REF] },
  nu_plate: { m: ['Nu = C(\\beta)\\,Re^{m}\\,Pr^{1/3} \\quad\\text{(chevron plate, angle } \\beta)'], r: ['Standard heat-exchanger design references'] },
  nu_gungor_winterton: { m: ['Nu = Nu_l\\big[1 + 3000\\,Bo^{0.86} + 1.12(x/(1-x))^{0.75}(\\rho_l/\\rho_g)^{0.41}\\big] \\quad\\text{(Gungor–Winterton)}'], r: [TWOPHASE_REF] },
  nu_traviss: { m: ['Nu = \\frac{Pr_l\\,Re_l^{0.9}\\,F(X_{tt})}{F_2} \\quad\\text{(Traviss condensation)}'], r: [TWOPHASE_REF] },
  j_fin: { m: ['j = St\\,Pr^{2/3} = C\\,Re^{m} \\quad\\text{(Colburn } j \\text{ for the fin surface)}'], r: ['Standard compact heat-exchanger references'] },
  f_fin: { m: ['f = C_f\\,Re^{m_f} \\quad\\text{(Fanning friction for the fin surface)}'], r: ['Standard compact heat-exchanger references'] },

  // ── Calculus ──
  gaussintegral: { m: ['\\int_a^b f(x)\\,dx \\approx \\frac{b-a}{2}\\sum_{i=1}^{n} w_i\\,f\\!\\left(\\tfrac{b-a}{2}\\xi_i + \\tfrac{a+b}{2}\\right) \\quad\\text{(Gauss–Legendre)}'], r: [NUMERICS_REF] },
  differentiate: { m: ['\\left.\\frac{dy}{dx}\\right|_{x_v} \\approx \\frac{y_{i+1}-y_{i-1}}{x_{i+1}-x_{i-1}} \\quad\\text{(central difference on the table)}'], r: [NUMERICS_REF] },
  integralvalue: { m: ['\\int y\\,dx \\approx \\sum_i \\tfrac{y_i + y_{i+1}}{2}(x_{i+1}-x_i) \\quad\\text{(trapezoidal)}'], r: [NUMERICS_REF] },
  uncertaintyof: { m: ['u(X) = \\text{user-supplied or RSS-propagated uncertainty of } X'], r: ['JCGM 100:2008 (GUM)'] },

  // ── Interpolation ──
  interpolate: { m: ['y = y_i + (y_{i+1}-y_i)\\frac{x - x_i}{x_{i+1} - x_i} \\quad\\text{(linear)}'], r: [NUMERICS_REF] },
  interpolate1: { m: ['\\text{piecewise cubic spline through the table knots (} C^2 \\text{ continuous)}'], r: [NUMERICS_REF] },
  lookup: { m: ['\\operatorname{Lookup}(t, r, c) = t_{r,c} \\quad\\text{(1-based cell)}'], r: [] },
  lookuprow: { m: ['\\text{row } r \\text{ where column } c \\text{ crosses } val \\text{ (interpolated)}'], r: [] },
  nlookuprows: { m: ['\\operatorname{NLookupRows}(t) = \\#\\text{rows}(t)'], r: [] },

  // ── Tables / ODE accessors ──
  tablevalue: { m: ['\\operatorname{TableValue}(r, c) = \\text{cell } (r, c) \\text{ of the parametric table}'], r: [] },
  'tablerun#': { m: ['\\text{current parametric run index (1-based)}'], r: [] },
  nparametricruns: { m: ['\\text{total number of configured parametric runs}'], r: [] },
  tablesum: { m: ['\\sum_{r} c_r'], r: [] },
  tablemin: { m: ['\\min_r c_r'], r: [] },
  tablemax: { m: ['\\max_r c_r'], r: [] },
  tablestddev: { m: ['s = \\sqrt{\\tfrac{1}{n-1}\\sum_r (c_r - \\bar c)^2}'], r: [] },
  minvalue: { m: ['\\min_{0 \\le i \\le N} \\text{col}(t_i)'], r: [] },
  odeavg: { m: ['\\frac{1}{N+1}\\sum_{i=0}^{N} \\text{col}(t_i)'], r: [] },
  odesum: { m: ['\\sum_{i=0}^{N} \\text{col}(t_i)'], r: [] },
  odestddev: { m: ['s = \\sqrt{\\tfrac{1}{N}\\sum_i (\\text{col}(t_i) - \\overline{\\text{col}})^2}'], r: [] },
  odemin: { m: ['\\min_i \\text{col}(t_i)'], r: [] },
  odemax: { m: ['\\max_i \\text{col}(t_i)'], r: [] },

  // ── Strings ──
  stringlen: { m: ['\\operatorname{StringLen}(s) = |s|'], r: [] },
  stringpos: { m: ['\\operatorname{StringPos}(s, t) = \\text{1-based index of } t \\text{ in } s,\\ 0 \\text{ if absent}'], r: [] },
  stringval: { m: ['\\operatorname{StringVal}(s) = \\text{numeric value parsed from } s'], r: [] },

  // ── Combustion ──
  adiabaticflametempeq: { m: ['H_{\\text{react}}(T_r) = H_{\\text{prod}}(T_{ad}) \\quad\\text{with equilibrium dissociation at } (T_{ad}, P)'], r: [COMBUST_REF] },
  eq_molefraction: { m: ['\\text{species mole fraction from chemical equilibrium } \\big(\\min G \\text{ at } T, P\\big)'], r: [COMBUST_REF] },
  mix_mw: { m: ['\\overline{M} = \\sum_i y_i M_i'], r: [COMBUST_REF] },
  mix_cp: { m: ['c_p = \\sum_i Y_i\\,c_{p,i}(T) \\quad\\text{(mass-weighted, NASA-7)}'], r: [COMBUST_REF] },
  mix_enthalpy: { m: ['h = \\sum_i Y_i\\,h_i(T) \\quad\\text{(NASA-7 polynomials)}'], r: [COMBUST_REF] },
  mix_entropy: { m: ['s = \\sum_i Y_i\\big[s_i(T) - R_i\\ln(y_i P/P_0)\\big]'], r: [COMBUST_REF] },
  mix_viscosity: { m: ['\\mu = \\sum_i \\frac{y_i \\mu_i}{\\sum_j y_j \\phi_{ij}} \\quad\\text{(Wilke)}'], r: [COMBUST_REF] },
  mix_conductivity: { m: ['\\lambda = \\sum_i \\frac{y_i \\lambda_i}{\\sum_j y_j \\phi_{ij}} \\quad\\text{(Wassiljewa/Wilke)}'], r: [COMBUST_REF] },
  wiebe: { m: ['x_b(\\theta) = 1 - \\exp\\!\\left[-a\\left(\\frac{\\theta-\\theta_0}{\\Delta\\theta}\\right)^{m+1}\\right]'], r: ['Standard internal-combustion-engine references'] },

  // ── EOS remainder ──
  eos_entropy: { m: ['s(T,P) = s^{\\text{ig}}(T,P) + (s - s^{\\text{ig}})_{T,P} \\quad\\text{(ideal-gas + EOS departure)}'], r: ['Standard chemical-engineering-thermodynamics references'] },
  eos_pressure: { m: ['P = \\frac{RT}{v-b} - \\frac{a\\,\\alpha(T)}{v(v+b) + b(v-b)} \\quad\\text{(PR; from } T, v)'], r: ['Standard cubic equation-of-state references'] },
};

// Wrapper functions (no closed form) — finalize without a fabricated math section.
const WRAPPERS = new Set([
  ...manifest.propertyFunctions.map((p) => p.name.toLowerCase()),
  ...manifest.materials.functions.map((f) => f.toLowerCase()),
  ...manifest.replCasOps.map((o) => o.toLowerCase()),
]);

// Walk baseline pages and rewrite the ones we can enrich/finalize.
let enriched = 0, finalized = 0;
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
  const p = path.join(d, e.name);
  if (e.isDirectory()) { if (e.name !== 'components') walk(p); return; }
  if (!e.name.endsWith('.md') || e.name.startsWith('_')) return;
  let src = fs.readFileSync(p, 'utf-8');
  if (!/^generated:\s*true/m.test(src)) return; // hand-authored — leave alone
  const name = (src.match(/^name:\s*(.+)$/m) || [])[1]?.trim();
  if (!name) return;
  const key = name.toLowerCase();
  const rich = R[key];
  const wrapper = WRAPPERS.has(key);
  if (!rich && !wrapper) return; // not in scope for this pass

  // Strip frontmatter `generated: true`.
  src = src.replace(/^generated:\s*true\s*\n/m, '');
  // Replace the auto-generated note with a finalized lead (curated).
  src = src.replace(/^> \*\*Auto-generated\*\*[^\n]*\n/m,
    wrapper
      ? '> Real-fluid/material/symbolic operation — see the inputs and references below.\n'
      : '');
  // Fill references frontmatter if we have refs and it is empty.
  if (rich && rich.r.length) {
    src = src.replace(/^references:\s*\[\]\s*$/m, 'references:\n' + rich.r.map((x) => `  - "${x.replace(/\*/g, '')}"`).join('\n'));
  }

  if (rich) {
    const mathBlock = ['## Mathematical Formulation', '', '$$ ' + rich.m.join(' $$\n\n$$ ') + ' $$', ''];
    const refBlock = rich.r.length ? ['## References', '', ...rich.r.map((x, i) => `${i + 1}. ${x}.`), ''] : [];
    // Insert math after the Description section (before Input Arguments / Examples / References).
    const lines = src.split('\n');
    let insAt = lines.findIndex((l, i) => i > 0 && /^## (Input Arguments|Examples|Output Arguments|References)/.test(l));
    if (insAt === -1) insAt = lines.length;
    lines.splice(insAt, 0, ...mathBlock);
    src = lines.join('\n');
    // Append references at the end if not already present.
    if (refBlock.length && !/^## References/m.test(src)) src = src.replace(/\s*$/, '\n\n' + refBlock.join('\n'));
    enriched++;
  } else {
    finalized++;
  }
  fs.writeFileSync(p, src);
});
walk(REF);
console.log(`enrich-functions: ${enriched} functions enriched with math, ${finalized} wrappers finalized.`);
