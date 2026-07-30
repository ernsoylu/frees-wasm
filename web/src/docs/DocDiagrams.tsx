// Visual diagrams for documentation

// Common visual styling tokens
const THEME = {
  bg: 'transparent',
  gridColor: '#2C2E33',
  textColor: '#C1C2C5',
  textDimmed: '#909296',
  primary: '#228BE6',      // Blue
  primaryGrad: 'url(#blueGrad)',
  secondary: '#7950F2',    // Grape / Purple
  secondaryGrad: 'url(#purpleGrad)',
  accent: '#12B886',       // Teal / Green
  accentGrad: 'url(#tealGrad)',
  warning: '#FD7E14',      // Orange
  warningGrad: 'url(#orangeGrad)',
  danger: '#FA5252',       // Red
  dangerGrad: 'url(#redGrad)',
  strokeWidth: 2,
};

// Arrow markers specific to the Brayton cycle diagram. The shared color
// gradients (blueGrad/purpleGrad/tealGrad/orangeGrad/redGrad) are defined
// inside SolverPipelineDiagram / DegreesOfFreedomDiagram and reused here via
// their global SVG IDs — defining them again would collide and drift.
function BraytonDefs() {
  return (
    <defs>
      <marker id="bcArrow" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M 0 1.5 L 7 5 L 0 8.5 z" fill={THEME.textColor} />
      </marker>
      <marker id="bcHeatArrow" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M 0 1.5 L 7 5 L 0 8.5 z" fill={THEME.warning} />
      </marker>
      <marker id="bcWorkArrow" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M 0 1.5 L 7 5 L 0 8.5 z" fill={THEME.accent} />
      </marker>
    </defs>
  );
}

// One state-point card (marker + name/temperature/pressure info box).
function BraytonStateCard({ point, label, name, temp, pres }: {
  point: { x: number; y: number }; label: string; name: string; temp: string; pres: string;
}) {
  return (
    <g>
      <circle cx={point.x} cy={point.y} r="11" fill={THEME.accentGrad} stroke="#63E6BE" strokeWidth="1.5" />
      <text x={point.x} y={point.y + 4} fill="#FFF" fontWeight="700" fontSize="12" textAnchor="middle">{label}</text>
      <g transform={`translate(${point.x - 52}, 300)`}>
        <rect width="104" height="52" rx="5" fill="#25262B" stroke="#373A40" strokeWidth="1" />
        <text x="52" y="15" fill={THEME.accent} fontSize="10" fontWeight="700" textAnchor="middle">{name}</text>
        <text x="52" y="30" fill={THEME.textColor} fontSize="9.5" textAnchor="middle">{temp}</text>
        <text x="52" y="44" fill={THEME.textDimmed} fontSize="9.5" textAnchor="middle">{pres}</text>
      </g>
    </g>
  );
}

export function SolverPipelineDiagram() {
  return (
    <svg viewBox="0 0 800 240" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>
        <linearGradient id="blueGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#339AF0" />
          <stop offset="100%" stopColor="#1C7ED6" />
        </linearGradient>
        <linearGradient id="purpleGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#8F78F3" />
          <stop offset="100%" stopColor="#7048E8" />
        </linearGradient>
        <linearGradient id="tealGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#20C997" />
          <stop offset="100%" stopColor="#0CA678" />
        </linearGradient>
        <marker id="arrow" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 1.5 L 6 5 L 0 8.5 z" fill={THEME.textColor} />
        </marker>
        <marker id="loopArrow" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 1.5 L 6 5 L 0 8.5 z" fill={THEME.secondary} />
        </marker>
      </defs>

      {/* Grid Background */}
      <rect width="100%" height="100%" fill="none" />

      {/* Steps */}
      {/* 1. Equations Input */}
      <g transform="translate(15, 60)">
        <rect width="110" height="70" rx="8" fill={THEME.primaryGrad} stroke="#4DABF7" strokeWidth="1" />
        <text x="55" y="32" fill="#FFF" fontWeight="600" fontSize="13" textAnchor="middle">1. Code Editor</text>
        <text x="55" y="48" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">P*V = m*R*T</text>
        <text x="55" y="60" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Equation Input</text>
      </g>

      <line x1="125" y1="95" x2="150" y2="95" stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#arrow)" />

      {/* 2. AST Parser */}
      <g transform="translate(155, 60)">
        <rect width="110" height="70" rx="8" fill={THEME.primaryGrad} stroke="#4DABF7" strokeWidth="1" />
        <text x="55" y="32" fill="#FFF" fontWeight="600" fontSize="13" textAnchor="middle">2. ANTLR Parser</text>
        <text x="55" y="48" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Abstract Syntax</text>
        <text x="55" y="60" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Tree (AST)</text>
      </g>

      <line x1="265" y1="95" x2="290" y2="95" stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#arrow)" />

      {/* 3. Tarjan SCC Block Separation */}
      <g transform="translate(295, 60)">
        <rect width="120" height="70" rx="8" fill={THEME.secondaryGrad} stroke="#9775FA" strokeWidth="1" />
        <text x="60" y="32" fill="#FFF" fontWeight="600" fontSize="13" textAnchor="middle">3. Tarjan SCC</text>
        <text x="60" y="48" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Dependency Graph</text>
        <text x="60" y="60" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Equation Blocks</text>
      </g>

      <line x1="415" y1="95" x2="440" y2="95" stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#arrow)" />

      {/* 4. Newton-Raphson Solver */}
      <g transform="translate(445, 60)">
        <rect width="125" height="70" rx="8" fill={THEME.secondaryGrad} stroke="#9775FA" strokeWidth="1" />
        <text x="62.5" y="32" fill="#FFF" fontWeight="600" fontSize="13" textAnchor="middle">4. Newton Solver</text>
        <text x="62.5" y="48" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Iterative Jacobian</text>
        <text x="62.5" y="60" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Simultaneous Solve</text>
      </g>

      <line x1="570" y1="95" x2="595" y2="95" stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#arrow)" />

      {/* 5. DYNAMIC pass */}
      <g transform="translate(600, 60)">
        <rect width="110" height="70" rx="8" fill={THEME.accentGrad} stroke="#63E6BE" strokeWidth="1" />
        <text x="55" y="32" fill="#FFF" fontWeight="600" fontSize="13" textAnchor="middle">5. DYNAMIC Pass</text>
        <text x="55" y="48" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Numerical ODE</text>
        <text x="55" y="60" fill="#FFF" fontSize="10" opacity="0.85" textAnchor="middle">Time-series Plot</text>
      </g>

      <line x1="710" y1="95" x2="735" y2="95" stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#arrow)" />

      {/* 6. Results */}
      <circle cx="765" cy="95" r="25" fill="#10B981" stroke="#34D399" strokeWidth="1" />
      <text x="765" y="99" fill="#FFF" fontWeight="600" fontSize="12" textAnchor="middle">SOLVED</text>

      {/* Iterative feedback loop arrow */}
      {/* Path from DYNAMIC pass back to Newton solver if accessors require it */}
      <path d="M 655,130 C 655,185 507,185 507,135" fill="none" stroke={THEME.secondary} strokeWidth="2" strokeDasharray="4,4" markerEnd="url(#loopArrow)" />
      <text x="581" y="195" fill={THEME.secondary} fontSize="10" fontWeight="600" textAnchor="middle">Implicit Accessor Loop (FinalValue, MaxValue)</text>
    </svg>
  );
}

export function DegreesOfFreedomDiagram() {
  return (
    <svg viewBox="0 0 800 280" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>
        <linearGradient id="redGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#FF6B6B" />
          <stop offset="100%" stopColor="#E03131" />
        </linearGradient>
        <linearGradient id="greenGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#51CF66" />
          <stop offset="100%" stopColor="#2B8A3E" />
        </linearGradient>
        <linearGradient id="orangeGrad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#FF922B" />
          <stop offset="100%" stopColor="#D9480F" />
        </linearGradient>
      </defs>

      {/* 1. Underdetermined (DoF > 0) */}
      <g transform="translate(40, 20)">
        <rect width="210" height="150" rx="8" fill="#1A1B1E" stroke="#373A40" strokeWidth="1.5" />
        <text x="105" y="30" fill={THEME.warning} fontSize="13" fontWeight="bold" textAnchor="middle">Underdetermined (DoF &gt; 0)</text>
        <line x1="20" y1="105" x2="190" y2="125" stroke={THEME.textColor} strokeWidth="4" />
        <polygon points="105,75 85,110 125,110" fill={THEME.warningGrad} /> {/* Scale pivot */}
        {/* Scale pans */}
        <circle cx="20" cy="105" r="10" fill={THEME.textColor} /> {/* Unknowns (heavy) */}
        <circle cx="190" cy="125" r="10" fill={THEME.textColor} /> {/* Equations (light) */}
        <text x="20" y="88" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Variables (3)</text>
        <text x="190" y="108" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Equations (2)</text>
        <text x="105" y="142" fill={THEME.textDimmed} fontSize="10" textAnchor="middle">Too many unknowns.</text>
        <text x="105" y="156" fill="#FFF" fontSize="10" fontWeight="bold" textAnchor="middle">Action: Fix one variable</text>
      </g>

      {/* 2. Solvable (DoF = 0) */}
      <g transform="translate(295, 20)">
        <rect width="210" height="150" rx="8" fill="#1A1B1E" stroke="#373A40" strokeWidth="1.5" />
        <text x="105" y="30" fill={THEME.accent} fontSize="13" fontWeight="bold" textAnchor="middle">Exactly Determined (DoF = 0)</text>
        <line x1="20" y1="110" x2="190" y2="110" stroke={THEME.textColor} strokeWidth="4" />
        <polygon points="105,75 85,110 125,110" fill={THEME.accentGrad} /> {/* Scale pivot */}
        {/* Scale pans */}
        <circle cx="20" cy="110" r="10" fill={THEME.textColor} />
        <circle cx="190" cy="110" r="10" fill={THEME.textColor} />
        <text x="20" y="93" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Variables (2)</text>
        <text x="190" y="93" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Equations (2)</text>
        <text x="105" y="142" fill={THEME.textDimmed} fontSize="10" textAnchor="middle">Perfectly balanced.</text>
        <text x="105" y="156" fill={THEME.accent} fontSize="10" fontWeight="bold" textAnchor="middle">System Solvable!</text>
      </g>

      {/* 3. Overdetermined (DoF < 0) */}
      <g transform="translate(550, 20)">
        <rect width="210" height="150" rx="8" fill="#1A1B1E" stroke="#373A40" strokeWidth="1.5" />
        <text x="105" y="30" fill="#FA5252" fontSize="13" fontWeight="bold" textAnchor="middle">Overdetermined (DoF &lt; 0)</text>
        <line x1="20" y1="125" x2="190" y2="105" stroke={THEME.textColor} strokeWidth="4" />
        <polygon points="105,75 85,110 125,110" fill={THEME.dangerGrad} /> {/* Scale pivot */}
        {/* Scale pans */}
        <circle cx="20" cy="125" r="10" fill={THEME.textColor} /> {/* Unknowns (light) */}
        <circle cx="190" cy="105" r="10" fill={THEME.textColor} /> {/* Equations (heavy) */}
        <text x="20" y="108" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Variables (2)</text>
        <text x="190" y="88" fill={THEME.textColor} fontSize="9" textAnchor="middle" fontWeight="bold">Equations (3)</text>
        <text x="105" y="142" fill={THEME.textDimmed} fontSize="10" textAnchor="middle">Conflicting equations.</text>
        <text x="105" y="156" fill="#FFF" fontSize="10" fontWeight="bold" textAnchor="middle">Action: Delete an equation</text>
      </g>

      {/* Explanation Text */}
      <rect x="40" y="190" width="720" height="70" rx="6" fill="light-dark(#F8F9FA, #25262B)" stroke="light-dark(#E9ECEF, #373A40)" strokeWidth="1" />
      <text x="60" y="215" fill={THEME.textColor} fontSize="11" fontWeight="bold">DoF Calculation Equation: DoF = Variables - Equations</text>
      <text x="60" y="235" fill={THEME.textDimmed} fontSize="10.5">Use the compiler Check (F4) to query your degree of freedom count before running solves.</text>
      <text x="60" y="248" fill={THEME.textDimmed} fontSize="10.5">Setting variable bounds or guess values in the Variable Info sheet (Ctrl+I) helps Newton convergence.</text>
    </svg>
  );
}

export function DependentPropertiesDiagram() {
  return (
    <svg viewBox="0 0 800 340" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>
        <linearGradient id="domeGrad" x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stopColor="#4c6ef5" stopOpacity="0.15" />
          <stop offset="100%" stopColor="#4c6ef5" stopOpacity="0.02" />
        </linearGradient>
      </defs>

      {/* X and Y axes */}
      <line x1="60" y1="40" x2="60" y2="280" stroke={THEME.textColor} strokeWidth="1.5" />
      <line x1="60" y1="280" x2="750" y2="280" stroke={THEME.textColor} strokeWidth="1.5" />
      <text x="40" y="45" fill={THEME.textColor} fontSize="11" textAnchor="end" fontWeight="600">Temperature (T)</text>
      <text x="750" y="300" fill={THEME.textColor} fontSize="11" textAnchor="end" fontWeight="600">Specific Volume (v) or Enthalpy (h)</text>

      {/* Saturation Dome */}
      <path d="M 150,280 C 250,120 400,60 400,60 C 400,60 450,100 550,280" fill="url(#domeGrad)" stroke="#4C6EF5" strokeWidth="2.5" />
      <text x="400" y="50" fill="#4C6EF5" fontSize="10" textAnchor="middle" fontWeight="bold">Critical Point</text>
      <circle cx="400" cy="60" r="4" fill="#4C6EF5" />

      {/* Phase zones */}
      <text x="210" y="210" fill={THEME.textDimmed} fontSize="11" textAnchor="middle" fontStyle="italic">Compressed Liquid</text>
      <text x="400" y="160" fill="#4C6EF5" fontSize="12" textAnchor="middle" fontWeight="bold">Wet Two-Phase Region</text>
      <text x="590" y="210" fill={THEME.textDimmed} fontSize="11" textAnchor="middle" fontStyle="italic">Superheated Vapor</text>

      {/* Constant Pressure Isobar (e.g. 101.3 kPa) */}
      <path d="M 80,270 L 250,150 L 485,150 L 700,50" fill="none" stroke="#FD7E14" strokeWidth="2" />
      <text x="690" y="45" fill="#FD7E14" fontSize="10.5" fontWeight="bold">Isobar (P = const)</text>

      {/* Inside the Dome: Boiling flat region */}
      <circle cx="350" cy="150" r="4" fill="#E03131" />
      <text x="350" y="140" fill={THEME.textColor} fontSize="10" textAnchor="middle" fontWeight="600">Boiling State Point</text>
      <path d="M 350,150 L 350,280" stroke="#E03131" strokeWidth="1.5" strokeDasharray="3,3" />
      <text x="350" y="293" fill={THEME.textColor} fontSize="9.5" textAnchor="middle">v (known value)</text>

      <path d="M 350,150 L 60,150" stroke="#FA5252" strokeWidth="1.5" strokeDasharray="3,3" />
      <text x="50" y="154" fill="#FA5252" fontSize="9.5" textAnchor="end" fontWeight="bold">T_sat = 100°C</text>

      {/* Label explaining why T & P are dependent inside the dome */}
      <rect x="420" y="210" width="310" height="60" rx="4" fill="#25262B" stroke="#E03131" strokeWidth="1" />
      <text x="430" y="228" fill="#FF8787" fontSize="11" fontWeight="bold">⚠️ Boiling Fluid Gotcha:</text>
      <text x="430" y="245" fill={THEME.textColor} fontSize="10">Inside the dome, T determines P (and vice versa).</text>
      <text x="430" y="258" fill={THEME.textColor} fontSize="10">Pass quality (x) or enthalpy (h), NOT T and P.</text>
    </svg>
  );
}

export function GuessConvergenceDiagram() {
  return (
    <svg viewBox="0 0 800 340" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>
        <marker id="tangentArrow" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="4" markerHeight="4" orient="auto-start-reverse">
          <path d="M 0 1.5 L 6 5 L 0 8.5 z" fill="#7950F2" />
        </marker>
      </defs>

      {/* Grid line axes */}
      <line x1="50" y1="280" x2="750" y2="280" stroke={THEME.textColor} strokeWidth="1.5" />
      <line x1="80" y1="30" x2="80" y2="300" stroke={THEME.textColor} strokeWidth="1.5" />
      <text x="70" y="35" fill={THEME.textColor} fontSize="11" textAnchor="end" fontWeight="600">Residual f(x)</text>
      <text x="750" y="295" fill={THEME.textColor} fontSize="11" textAnchor="end" fontWeight="600">Variable (x)</text>

      {/* Curve f(x) */}
      <path d="M 120,280 Q 250,20 400,20 T 650,220" fill="none" stroke="#228BE6" strokeWidth="3" />
      <text x="610" y="190" fill="#228BE6" fontSize="12" fontWeight="bold">Residual Curve f(x) = 0</text>

      {/* Root (f(x) = 0) */}
      <circle cx="160" cy="280" r="5" fill="#10B981" />
      <text x="160" y="295" fill="#10B981" fontSize="10.5" fontWeight="bold" textAnchor="middle">True Root (f(x)=0)</text>

      {/* Bad Guess Divergence Visual */}
      <g>
        <circle cx="400" cy="20" r="4" fill="#FA5252" />
        <text x="400" y="10" fill="#FA5252" fontSize="10.5" fontWeight="bold" textAnchor="middle">Flat Region (Derivative ~ 0)</text>
        <line x1="400" y1="20" x2="700" y2="20" stroke="#FA5252" strokeWidth="1.5" strokeDasharray="3,3" />
        <text x="710" y="24" fill="#FA5252" fontSize="9.5" fontWeight="bold">Diverges / Division by Zero!</text>
      </g>

      {/* Good Guess Iteration Visual */}
      <g>
        {/* Step 1: Guess x0 */}
        <circle cx="550" cy="115" r="4" fill="#7950F2" />
        <line x1="550" y1="115" x2="550" y2="280" stroke={THEME.textDimmed} strokeWidth="1" strokeDasharray="3,3" />
        <text x="550" y="295" fill={THEME.textColor} fontSize="10" textAnchor="middle">x0 (Good Guess)</text>

        {/* Tangent line at x0 */}
        <line x1="600" y1="171" x2="330" y2="-12" stroke="#7950F2" strokeWidth="1.5" markerEnd="url(#tangentArrow)" />
        <text x="495" y="80" fill="#7950F2" fontSize="9.5" fontWeight="bold" transform="rotate(31, 495, 80)">Tangent f'(x0)</text>

        {/* Step 2: Next iteration x1 */}
        <circle cx="340" cy="280" r="4" fill="#7950F2" />
        <text x="340" y="295" fill={THEME.textColor} fontSize="10" textAnchor="middle">x1</text>
        <path d="M 540,270 Q 440,250 350,275" fill="none" stroke="#7950F2" strokeWidth="1.5" strokeDasharray="2,2" />
      </g>

      {/* Explanatory notes */}
      <rect x="230" y="60" width="340" height="70" rx="4" fill="#25262B" stroke="#373A40" strokeWidth="1" />
      <text x="245" y="78" fill="#B197FC" fontSize="11" fontWeight="bold">Why Setup Bounds and Initial Guesses?</text>
      <text x="245" y="96" fill={THEME.textColor} fontSize="10">1. Bounds (e.g. x &gt; 0) prevent search in invalid domains.</text>
      <text x="245" y="110" fill={THEME.textColor} fontSize="10">2. Good guesses keep solver away from flat divergence regions.</text>
      <text x="245" y="124" fill={THEME.textColor} fontSize="10">3. Halves iterations and speeds up solving.</text>
    </svg>
  );
}

/**
 * Brayton cycle schematic for the "Simple Brayton Cycle, Cold-Air-Standard"
 * help-page example. Shows the four state points (1 → compressor → 2 →
 * combustor → 3 → turbine → 4 → exhaust/cool → 1), the five given inputs,
 * and the heat/work arrows. Purely illustrative — values are the textbook
 * reference numbers so the diagram doubles as a sanity check.
 */
export function BraytonCycleDiagram() {
  // Box geometry for the four components, laid out in a rectangle.
  const comp = { w: 130, h: 64 };
  const compY = 130;
  // State-point coordinates (centres of the connecting corners).
  const S1 = { x: 70,  y: 162 };
  const S2 = { x: 270, y: 162 };
  const S3 = { x: 470, y: 162 };
  const S4 = { x: 670, y: 162 };

  return (
    <svg viewBox="0 0 740 360" width="100%" height="auto"
         style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <BraytonDefs />

      {/* Title */}
      <text x="370" y="26" fill={THEME.textColor} fontSize="14" fontWeight="700" textAnchor="middle">
        Simple Brayton Cycle — Cold-Air-Standard (η_C = 80%, η_T = 85%)
      </text>

      {/* ---- Component boxes (top row) ---- */}
      {/* Compressor: 1 -> 2 */}
      <g transform={`translate(${S1.x + 35}, ${compY})`}>
        <rect width={comp.w} height={comp.h} rx="8" fill={THEME.secondaryGrad} stroke="#9775FA" strokeWidth="1" />
        <text x={comp.w / 2} y="26" fill="#FFF" fontWeight="700" fontSize="13" textAnchor="middle">Compressor</text>
        <text x={comp.w / 2} y="44" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">η_C = 0.80</text>
        <text x={comp.w / 2} y="56" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">P₁ → P₂ = r_p·P₁</text>
      </g>

      {/* Combustor: 2 -> 3 */}
      <g transform={`translate(${S2.x + 35}, ${compY})`}>
        <rect width={comp.w} height={comp.h} rx="8" fill={THEME.warningGrad} stroke="#FFA94D" strokeWidth="1" />
        <text x={comp.w / 2} y="26" fill="#FFF" fontWeight="700" fontSize="13" textAnchor="middle">Combustor</text>
        <text x={comp.w / 2} y="44" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">q_in (constant P)</text>
        <text x={comp.w / 2} y="56" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">T₂ → T₃</text>
      </g>

      {/* Turbine: 3 -> 4 */}
      <g transform={`translate(${S3.x + 35}, ${compY})`}>
        <rect width={comp.w} height={comp.h} rx="8" fill={THEME.primaryGrad} stroke="#4DABF7" strokeWidth="1" />
        <text x={comp.w / 2} y="26" fill="#FFF" fontWeight="700" fontSize="13" textAnchor="middle">Turbine</text>
        <text x={comp.w / 2} y="44" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">η_T = 0.85</text>
        <text x={comp.w / 2} y="56" fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">P₃ → P₄ = P₁</text>
      </g>

      {/* ---- Connecting flow arrows (top row) ---- */}
      <line x1={S2.x} y1={compY + comp.h / 2} x2={S2.x + 35} y2={compY + comp.h / 2}
            stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#bcArrow)" />
      <line x1={S3.x} y1={compY + comp.h / 2} x2={S3.x + 35} y2={compY + comp.h / 2}
            stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#bcArrow)" />

      {/* ---- Return path: 4 -> (exhaust/cool) -> 1 (bottom) ---- */}
      {/* Down from turbine exit */}
      <line x1={S4.x} y1={compY + comp.h} x2={S4.x} y2={250}
            stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} />
      {/* Exhaust / heat rejection label box */}
      <g transform={`translate(${S4.x - 90}, 250)`}>
        <rect width="180" height="40" rx="6" fill="#1A1B1E" stroke={THEME.danger} strokeWidth="1" />
        <text x="90" y="18" fill="#FF8787" fontSize="11" fontWeight="700" textAnchor="middle">q_out (exhaust)</text>
        <text x="90" y="32" fill={THEME.textDimmed} fontSize="9.5" textAnchor="middle">T₄ → T₁  at  P₁</text>
      </g>
      {/* Across the bottom back to state 1 */}
      <line x1={S4.x - 90} y1={270} x2={S1.x} y2={270}
            stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} />
      {/* Up into state 1 */}
      <line x1={S1.x} y1={270} x2={S1.x} y2={compY + comp.h}
            stroke={THEME.textColor} strokeWidth={THEME.strokeWidth} markerEnd="url(#bcArrow)" />

      {/* ---- State-point markers + labels ---- */}
      <BraytonStateCard point={S1} label="1" name="Inlet"     temp="T₁ = 300 K"     pres="P₁ = 100 kPa" />
      <BraytonStateCard point={S2} label="2" name="Comp. exit" temp="T₂a = 604.3 K" pres="P₂ = 800 kPa" />
      <BraytonStateCard point={S3} label="3" name="Turb. inlet" temp="T₃ = 1300 K"  pres="P₃ = 800 kPa" />
      <BraytonStateCard point={S4} label="4" name="Turb. exit"  temp="T₄a = 805.0 K" pres="P₄ = 100 kPa" />

      {/* ---- Input legend (the five given inputs) ---- */}
      <g transform="translate(20, 70)">
        <text x="0" y="0" fill={THEME.textDimmed} fontSize="10.5" fontWeight="700">Given inputs:</text>
        <text x="92"  y="0" fill={THEME.textColor} fontSize="10.5">rₚ = 8</text>
        <text x="158" y="0" fill={THEME.textColor} fontSize="10.5">k = 1.4</text>
        <text x="212" y="0" fill={THEME.textColor} fontSize="10.5">cₚ = 1.005 kJ/kg·K</text>
        <text x="348" y="0" fill={THEME.warning}   fontSize="10.5">η_C = 0.80</text>
        <text x="418" y="0" fill={THEME.primary}   fontSize="10.5">η_T = 0.85</text>
        <text x="486" y="0" fill={THEME.textDimmed} fontSize="10.5" fontStyle="italic">(cold-air-standard)</text>
      </g>

      {/* ---- Work / heat arrows ---- */}
      {/* Compressor work input (into compressor from above) */}
      <line x1={S1.x + 100} y1={compY - 18} x2={S1.x + 100} y2={compY}
            stroke={THEME.accent} strokeWidth="2.5" markerEnd="url(#bcWorkArrow)" />
      <text x={S1.x + 100} y={compY - 24} fill={THEME.accent} fontSize="10" fontWeight="700" textAnchor="middle">w_comp,in</text>

      {/* Turbine work output (out of turbine upward) */}
      <line x1={S3.x + 100} y1={compY} x2={S3.x + 100} y2={compY - 18}
            stroke={THEME.accent} strokeWidth="2.5" markerEnd="url(#bcWorkArrow)" />
      <text x={S3.x + 100} y={compY - 24} fill={THEME.accent} fontSize="10" fontWeight="700" textAnchor="middle">w_turb,out</text>

      {/* Heat-in arrow into combustor from above */}
      <line x1={S2.x + 100} y1={compY - 18} x2={S2.x + 100} y2={compY}
            stroke={THEME.warning} strokeWidth="2.5" markerEnd="url(#bcHeatArrow)" />
      <text x={S2.x + 100} y={compY - 24} fill={THEME.warning} fontSize="10" fontWeight="700" textAnchor="middle">q_in</text>
    </svg>
  );
}

// ── Cookbook system schematics ───────────────────────────────────────────────
// Self-contained SVG block diagrams (solid theme colors, no shared gradient IDs)
// so they render correctly standalone on a Cookbook page.

const D_ARROW = (
  <marker id="ckArrow" viewBox="0 0 10 10" refX="7" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1.5 L 7 5 L 0 8.5 z" fill={THEME.textColor} />
  </marker>
);

function CkBox({ x, y, w = 140, h = 56, fill, title, sub }: Readonly<{ x: number; y: number; w?: number; h?: number; fill: string; title: string; sub?: string }>) {
  return (
    <g transform={`translate(${x}, ${y})`}>
      <rect width={w} height={h} rx="8" fill={fill} stroke="#FFFFFF22" strokeWidth="1" />
      <text x={w / 2} y={sub ? 24 : 32} fill="#FFF" fontWeight="700" fontSize="13" textAnchor="middle">{title}</text>
      {sub && <text x={w / 2} y={40} fill="#FFF" fontSize="10" opacity="0.9" textAnchor="middle">{sub}</text>}
    </g>
  );
}
const ckLine = (x1: number, y1: number, x2: number, y2: number, color = THEME.textColor, dash = false) => (
  <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={color} strokeWidth="2" strokeDasharray={dash ? '5 4' : undefined} markerEnd="url(#ckArrow)" />
);

/** Closed four-component thermodynamic loop (refrigeration / Rankine). */
function FourBoxLoop({ title, boxes, heatIn, heatOut, work }: Readonly<{
  title: string;
  boxes: [string, string, string, string];   // top-left, top-right, bottom-right, bottom-left
  heatIn: string; heatOut: string; work: string;
}>) {
  const w = 150, h = 58;
  const TL = { x: 80, y: 60 }, TR = { x: 410, y: 60 }, BR = { x: 410, y: 215 }, BL = { x: 80, y: 215 };
  return (
    <svg viewBox="0 0 640 320" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>{D_ARROW}</defs>
      <text x="320" y="28" fill={THEME.textColor} fontSize="14" fontWeight="700" textAnchor="middle">{title}</text>
      <CkBox x={TL.x} y={TL.y} w={w} h={h} fill={THEME.secondary} title={boxes[0]} />
      <CkBox x={TR.x} y={TR.y} w={w} h={h} fill={THEME.danger} title={boxes[1]} />
      <CkBox x={BR.x} y={BR.y} w={w} h={h} fill={THEME.warning} title={boxes[2]} />
      <CkBox x={BL.x} y={BL.y} w={w} h={h} fill={THEME.accent} title={boxes[3]} />
      {/* clockwise loop */}
      {ckLine(TL.x + w, TL.y + h / 2, TR.x, TR.y + h / 2)}
      {ckLine(TR.x + w / 2, TR.y + h, BR.x + w / 2, BR.y)}
      {ckLine(BR.x, BR.y + h / 2, BL.x + w, BL.y + h / 2)}
      {ckLine(BL.x + w / 2, BL.y, TL.x + w / 2, TL.y + h)}
      {/* energy labels */}
      <text x={TL.x + w / 2} y={TL.y - 8} fill={THEME.secondary} fontSize="11" fontWeight="700" textAnchor="middle">{work}</text>
      <text x={TR.x + w / 2} y={TR.y - 8} fill="#FF8787" fontSize="11" fontWeight="700" textAnchor="middle">{heatOut}</text>
      <text x={BL.x + w / 2} y={BL.y + h + 18} fill="#63E6BE" fontSize="11" fontWeight="700" textAnchor="middle">{heatIn}</text>
    </svg>
  );
}

export function RefrigerationCycleDiagram() {
  return <FourBoxLoop title="Vapor-Compression Refrigeration Cycle"
    boxes={['Compressor', 'Condenser', 'Expansion Valve', 'Evaporator']}
    work="w_c (in)" heatOut="q_H → ambient" heatIn="q_L ← cold space" />;
}

export function RankineCycleDiagram() {
  return <FourBoxLoop title="Rankine Steam Power Cycle"
    boxes={['Boiler', 'Turbine', 'Condenser', 'Pump']}
    work="w_pump (in)" heatOut="q_out (turbine work →)" heatIn="q_in ← furnace" />;
}

/** EV thermal-management: coolant loop + refrigerant loop coupled by a chiller. */
export function EvThermalDiagram() {
  const w = 120, h = 50;
  const C = THEME.accent;   // coolant (teal)
  const R = THEME.secondary; // refrigerant (purple)
  return (
    <svg viewBox="0 0 700 380" width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>{D_ARROW}</defs>
      <text x="350" y="26" fill={THEME.textColor} fontSize="14" fontWeight="700" textAnchor="middle">EV Thermal-Management System</text>
      <text x="165" y="48" fill={C} fontSize="11" fontWeight="700" textAnchor="middle">Coolant loop (EG50)</text>
      <text x="535" y="48" fill={R} fontSize="11" fontWeight="700" textAnchor="middle">Refrigerant loop (R1234yf)</text>

      {/* Coolant loop (left) */}
      <CkBox x={105} y={70}  w={w} h={h} fill={THEME.danger} title="Radiator" sub="→ ambient" />
      <CkBox x={40}  y={170} w={w} h={h} fill={C} title="Battery" />
      <CkBox x={170} y={170} w={w} h={h} fill={C} title="Motor" />
      <CkBox x={105} y={280} w={w} h={h} fill={THEME.primary} title="Pump" />
      {/* pump -> split -> battery & motor -> radiator -> pump */}
      {ckLine(135, 280, 100, 220, C)}
      {ckLine(195, 280, 230, 220, C)}
      {ckLine(100, 170, 150, 120, C)}
      {ckLine(230, 170, 180, 120, C)}
      {ckLine(165, 280, 165, 280, C)}
      {ckLine(110, 95, 60, 95, C)}
      <line x1={60} y1={95} x2={60} y2={305} stroke={C} strokeWidth="2" />
      {ckLine(60, 305, 105, 305, C)}

      {/* Refrigerant loop (right) */}
      <CkBox x={530} y={70}  w={w} h={h} fill={THEME.danger} title="Condenser" sub="→ ambient" />
      <CkBox x={530} y={280} w={w} h={h} fill={R} title="Compressor" sub="w_c (in)" />
      <CkBox x={400} y={280} w={w} h={h} fill={THEME.warning} title="Exp. Valve" />
      <CkBox x={400} y={170} w={w} h={h} fill={R} title="Chiller" sub="evaporator" />
      {/* compressor -> condenser -> valve -> chiller -> compressor */}
      {ckLine(590, 280, 590, 126, R)}
      {ckLine(530, 95, 460, 130, R)}
      <line x1={460} y1={95} x2={460} y2={95} stroke={R} strokeWidth="2" />
      {ckLine(460, 280, 460, 220, R)}
      {ckLine(460, 170, 460, 130, R)}
      {ckLine(460, 220, 460, 280, R)}
      {ckLine(520, 305, 460, 305, R)}
      <line x1={400} y1={305} x2={400} y2={305} stroke={R} strokeWidth="2" />
      {ckLine(530, 305, 520, 305, R)}

      {/* cross-domain bridge: battery coolant <-> chiller refrigerant */}
      {ckLine(160, 195, 400, 195, THEME.danger, true)}
      <text x="280" y="185" fill="#FF8787" fontSize="10.5" fontWeight="700" textAnchor="middle">Q̇  (cross-domain wall)</text>
    </svg>
  );
}

// ── Learning map ──────────────────────────────────────────────────────────────
// The "Where to Go Next" prerequisite map: four stages, every node clickable
// (navigates to that section's landing page). Pure SVG in the DocDiagrams
// style — no graph library.

const LEARNING_MAP: { title: string; fill: string; stroke: string; nodes: { id: string; label: string }[] }[] = [
  {
    title: 'Start here', fill: '#0CA678', stroke: '#63E6BE',
    nodes: [{ id: 'started', label: 'Get Started (1–7)' }],
  },
  {
    title: 'Foundations', fill: '#1C7ED6', stroke: '#4DABF7',
    nodes: [
      { id: 'lang-overview', label: 'Language Fundamentals' },
      { id: 'matrix-overview', label: 'Matrices & Linear Algebra' },
      { id: 'prog-overview', label: 'Programming & Tables' },
      { id: 'fluids-overview', label: 'Fluids & Materials' },
    ],
  },
  {
    title: 'Applied modeling', fill: '#7048E8', stroke: '#9775FA',
    nodes: [
      { id: 'solving-overview', label: 'Solving & Optimization' },
      { id: 'dynamics-overview', label: 'Dynamics & Control' },
      { id: 'components-overview', label: 'System Modeling' },
    ],
  },
  {
    title: 'Practice & ship', fill: '#0CA678', stroke: '#63E6BE',
    nodes: [
      { id: 'tools-overview', label: 'Tools & Workflow' },
      { id: 'tut-msd', label: 'Guided Tutorials' },
      { id: 'deploy-overview', label: 'Architecture & Deployment' },
    ],
  },
];

export function LearningMapDiagram({ onNavigate }: { onNavigate?: (id: string) => void }) {
  const colW = 168;
  const colGap = 38;
  const nodeH = 40;
  const nodeGap = 12;
  const topPad = 34;
  const maxNodes = Math.max(...LEARNING_MAP.map((c) => c.nodes.length));
  const height = topPad + maxNodes * (nodeH + nodeGap) + 8;
  const width = LEARNING_MAP.length * colW + (LEARNING_MAP.length - 1) * colGap + 30;
  const colX = (i: number) => 15 + i * (colW + colGap);
  const colMidY = (i: number) => topPad + (LEARNING_MAP[i].nodes.length * (nodeH + nodeGap) - nodeGap) / 2;
  return (
    <svg viewBox={`0 0 ${width} ${height}`} width="100%" height="auto" style={{ background: THEME.bg, fontFamily: 'system-ui, sans-serif' }}>
      <defs>
        <marker id="lmArrow" viewBox="0 0 10 10" refX="6" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 1.5 L 6 5 L 0 8.5 z" fill={THEME.textDimmed} />
        </marker>
      </defs>
      {LEARNING_MAP.map((col, i) => (
        <g key={col.title} transform={`translate(${colX(i)}, 0)`}>
          <text x={colW / 2} y={18} fill={THEME.textDimmed} fontSize="11" fontWeight="700" textAnchor="middle" style={{ letterSpacing: '0.5px', textTransform: 'uppercase' }}>
            {col.title}
          </text>
          {col.nodes.map((n, j) => (
            <g
              key={n.id}
              transform={`translate(0, ${topPad + j * (nodeH + nodeGap)})`}
              onClick={() => onNavigate?.(n.id)}
              style={{ cursor: onNavigate ? 'pointer' : 'default' }}
            >
              <rect width={colW} height={nodeH} rx="8" fill={col.fill} stroke={col.stroke} strokeWidth="1" />
              <text x={colW / 2} y={nodeH / 2 + 4} fill="#FFF" fontWeight="600" fontSize="11.5" textAnchor="middle">
                {n.label}
              </text>
            </g>
          ))}
        </g>
      ))}
      {LEARNING_MAP.slice(0, -1).map((_, i) => (
        <line
          key={`arrow-${LEARNING_MAP[i].title}`}
          x1={colX(i) + colW + 4}
          y1={colMidY(i)}
          x2={colX(i + 1) - 6}
          y2={colMidY(i + 1)}
          stroke={THEME.textDimmed}
          strokeWidth={THEME.strokeWidth}
          markerEnd="url(#lmArrow)"
        />
      ))}
    </svg>
  );
}
