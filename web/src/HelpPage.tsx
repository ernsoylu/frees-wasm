import {
  AppShell,
  Burger,
  Group,
  NavLink,
  ScrollArea,
  Title,
  Text,
  Container,
  Code,
  List,
  Paper,
  Stack,
  Table,
  Badge,
  Alert,
  Button,
  TextInput,
  CloseButton,
  Accordion as MantineAccordion,
  Highlight,
  Divider,
  Box,
  SimpleGrid,
  Card,
  Anchor,
  Slider
} from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { useState, useEffect, useMemo } from 'react';
import { getReference, getFluids, check, solve, DEFAULT_STOP_CRITERIA, type UnitInfo, type ConstantInfo, type VariableResult } from './api';
import { DOCS_CATALOG } from './docsCatalog';
import Latex from './Latex';
import { EXAMPLES } from './examples';
import { REFERENCE_PAGES, type ReferencePage } from './referenceCatalog';
import { buildSearchIndex, searchDocs, type SearchHit, type SearchKind } from './searchIndex';

// Facet chips shown in the search dropdown, in display order.
const SEARCH_FACETS: [SearchKind, string][] = [
  ['guide', 'Guides'],
  ['reference', 'Reference'],
  ['component', 'Components'],
  ['example', 'Examples'],
];
import { VERSION_LABEL } from './version';
import {
  FLUID_PROPERTY_OUTPUTS,
  FLUID_INPUT_INDICATORS,
  AIRH2O_OUTPUTS,
  AIRH2O_INDICATORS,
  UTILITY_PROPERTY_FUNCS,
  type FuncEntry
} from './helpReference';
import {
  SolverPipelineDiagram,
  DegreesOfFreedomDiagram,
  DependentPropertiesDiagram,
  GuessConvergenceDiagram,
  BraytonCycleDiagram,
  RefrigerationCycleDiagram,
  RankineCycleDiagram,
  EvThermalDiagram,
  LearningMapDiagram
} from './docs/DocDiagrams';

function CopyButton({ code }: Readonly<{ code: string }>) {
  const [copied, setCopied] = useState(false);
  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <Button
      size="xs"
      variant="light"
      color={copied ? "green" : "blue"}
      onClick={handleCopy}
      style={{ position: 'absolute', top: '8px', right: '8px', zIndex: 10 }}
    >
      {copied ? "Copied!" : "Copy Code"}
    </Button>
  );
}

// Compact numeric display for inline solution tables.
function formatRunValue(v: number): string {
  if (!Number.isFinite(v)) return String(v);
  const a = Math.abs(v);
  if (a !== 0 && (a < 1e-4 || a >= 1e7)) return v.toExponential(4);
  return String(Number(v.toPrecision(6)));
}

// A fenced block tagged ```run (every one is verified against the backend by
// scripts/check-doc-snippets.mjs). Run executes the real Check → Solve pipeline
// and renders the solution inline; Open in Editor hands the code to the main
// app through the frees.pendingSnippet localStorage key (consumed in App.tsx).
/** A `vary=` slider spec parsed from a runnable fence's info string. */
interface VarySpec { name: string; min: number; step: number; max: number }

function RunnableCode({ code, title, vary = [] }: Readonly<{ code: string; title?: string; vary?: VarySpec[] }>) {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [vars, setVars] = useState<VariableResult[] | null>(null);
  // Slider values, seeded at each range's midpoint (snapped to the step).
  const [varyVals, setVaryVals] = useState<Record<string, number>>(() => {
    const init: Record<string, number> = {};
    for (const v of vary) init[v.name] = v.min + Math.round((v.max - v.min) / 2 / v.step) * v.step;
    return init;
  });

  const run = async (vals: Record<string, number> = varyVals) => {
    // Sliders override the document's fixed values through the same overrides
    // channel the REPL uses ("name = value" beats the editor equation).
    const overrides = vary.map((v) => `${v.name} = ${vals[v.name]}`);
    setRunning(true);
    setError(null);
    try {
      const chk = await check(code, [], false, [], overrides);
      if (!chk.solvable) {
        setVars(null);
        setError(chk.message || 'The system did not pass Check.');
        return;
      }
      const sol = await solve(code, DEFAULT_STOP_CRITERIA, [], false, 'SI', false, [], undefined, overrides);
      if (!sol.success) {
        setVars(null);
        setError(sol.error || 'The solve did not converge.');
        return;
      }
      setVars(sol.variables);
    } finally {
      setRunning(false);
    }
  };

  const openInEditor = () => {
    const firstComment = /\{([^}]*)\}/.exec(code)?.[1]?.trim();
    localStorage.setItem('frees.pendingSnippet', JSON.stringify({
      title: (title || firstComment || 'Documentation example').slice(0, 80),
      text: code,
      ts: Date.now(),
    }));
    window.open('/', '_blank');
  };

  const shown = vars ? vars.slice(0, 60) : [];
  const hasUncertainty = shown.some((v) => (v.uncertainty ?? 0) !== 0);
  return (
    <Paper withBorder p="md" bg="light-dark(var(--mantine-color-gray-0), var(--mantine-color-dark-8))" mb="md">
      <Group justify={title ? 'space-between' : 'flex-end'} gap="xs" mb="xs">
        {title && <Badge color="teal" variant="light" leftSection={<IconBook size={12} />}>{title}</Badge>}
        <Group gap="xs">
          <Button size="xs" variant="filled" color="teal" onClick={() => run()} loading={running}>Run</Button>
          <Button size="xs" variant="light" onClick={openInEditor}>Open in Editor</Button>
        </Group>
      </Group>
      {vary.length > 0 && (
        <Stack gap={6} mb="sm">
          {vary.map((v) => (
            <Group key={v.name} gap="sm" wrap="nowrap">
              <Code style={{ whiteSpace: 'nowrap', minWidth: 130 }}>{v.name} = {formatRunValue(varyVals[v.name])}</Code>
              <Slider
                style={{ flexGrow: 1 }}
                size="sm"
                color="teal"
                min={v.min}
                max={v.max}
                step={v.step}
                value={varyVals[v.name]}
                label={null}
                onChange={(val) => setVaryVals((prev) => ({ ...prev, [v.name]: val }))}
                onChangeEnd={(val) => {
                  const next = { ...varyVals, [v.name]: val };
                  setVaryVals(next);
                  void run(next);
                }}
              />
            </Group>
          ))}
        </Stack>
      )}
      <Box style={{ position: 'relative' }}>
        <CopyButton code={code} />
        <Code block style={{ background: 'transparent', maxHeight: '340px', overflowY: 'auto' }}>{code}</Code>
      </Box>
      {error && (
        <Alert color="red" mt="sm" title="Did not solve">
          <Text size="sm" style={{ whiteSpace: 'pre-wrap' }}>{error}</Text>
        </Alert>
      )}
      {vars && (
        <Box mt="sm">
          <Text size="sm" fw={700} c="teal.4" mb="xs">
            Solved — {vars.length} variable{vars.length === 1 ? '' : 's'}
          </Text>
          <Box style={{ maxHeight: 280, overflowY: 'auto' }}>
            <Table striped withTableBorder verticalSpacing={2} fz="sm">
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Variable</Table.Th>
                  <Table.Th>Value</Table.Th>
                  <Table.Th>Units</Table.Th>
                  {hasUncertainty && <Table.Th>±</Table.Th>}
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {shown.map((v) => (
                  <Table.Tr key={v.name}>
                    <Table.Td><Code>{v.name}</Code></Table.Td>
                    <Table.Td>{formatRunValue(v.value)}</Table.Td>
                    <Table.Td>{v.units}</Table.Td>
                    {hasUncertainty && <Table.Td>{(v.uncertainty ?? 0) !== 0 ? formatRunValue(v.uncertainty!) : ''}</Table.Td>}
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Box>
          {vars.length > shown.length && (
            <Text size="xs" c="dimmed" mt={4}>
              …and {vars.length - shown.length} more — open in the editor for the full solution, tables, and plots.
            </Text>
          )}
        </Box>
      )}
    </Paper>
  );
}

function formatSiFactor(factor: number): string {
  if (factor === 1) return '1';
  const abs = Math.abs(factor);
  if (abs !== 0 && (abs >= 1e5 || abs < 1e-3)) return factor.toExponential(4);
  return Number(factor.toPrecision(8)).toString();
}

/**
 * Live reference of every unit the solver accepts and the built-in constants,
 * fetched from /api/reference so it can never drift from the backend registry.
 * Units are grouped by the SI dimension they measure; a filter matches unit
 * symbol or dimension. Names are case-insensitive (shown lowercased).
 */
function UnitsReference() {
  const [units, setUnits] = useState<UnitInfo[]>([]);
  const [constants, setConstants] = useState<ConstantInfo[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [query, setQuery] = useState('');

  useEffect(() => {
    let cancelled = false;
    getReference()
      .then((ref) => {
        if (cancelled) return;
        setUnits(ref.units);
        setConstants(ref.constants);
      })
      .finally(() => !cancelled && setLoaded(true));
    return () => { cancelled = true; };
  }, []);

  const q = query.trim().toLowerCase();
  const filteredUnits = q
    ? units.filter((u) => u.symbol.toLowerCase().includes(q) || u.dimension.toLowerCase().includes(q))
    : units;

  // Group by dimension, preserving the backend's dimension-then-symbol order.
  const groups = new Map<string, UnitInfo[]>();
  for (const u of filteredUnits) {
    const list = groups.get(u.dimension) ?? [];
    list.push(u);
    groups.set(u.dimension, list);
  }

  return (
    <Stack gap="md">
      <Title order={3} mt="sm">Supported Units</Title>
      <Text size="sm" c="dimmed">
        Every unit below is accepted inside <code>[ ]</code> annotations and in <code>Convert()</code>.
        Unit names are case-insensitive (shown lowercased). Units are grouped by the SI
        dimension they measure; the factor is the multiplier to SI base units.
      </Text>
      <TextInput
        placeholder="Filter units by symbol or dimension (e.g. pa, time, m/s)"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
        rightSection={query ? <CloseButton size="sm" onClick={() => setQuery('')} /> : null}
        maw={420}
      />
      {!loaded && <Text size="sm" c="dimmed">Loading reference…</Text>}
      {loaded && groups.size === 0 && <Text size="sm" c="dimmed">No units match “{query}”.</Text>}
      {[...groups.entries()].map(([dimension, list]) => (
        <Paper key={dimension} withBorder p="sm">
          <Group gap="xs" mb="xs">
            <Badge variant="light" color="blue">{dimension === '-' ? 'dimensionless' : dimension}</Badge>
            <Text size="xs" c="dimmed">{list.length} unit{list.length === 1 ? '' : 's'}</Text>
          </Group>
          <Group gap={6}>
            {list.map((u) => (
              <Badge key={u.symbol} variant="default" style={{ fontFamily: 'monospace', textTransform: 'none' }}>
                {u.symbol} = {formatSiFactor(u.siFactor)} {dimension === '-' ? '' : dimension}
              </Badge>
            ))}
          </Group>
        </Paper>
      ))}

      <Title order={3} mt="md">Built-in Constants</Title>
      <Text size="sm" c="dimmed">
        Use these anywhere in equations by their <code>#</code>-suffixed name (by long-standing convention).
        They are substituted at parse time with the SI value and unit shown.
      </Text>
      <Table striped withTableBorder>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Name</Table.Th>
            <Table.Th>Value (SI)</Table.Th>
            <Table.Th>Unit</Table.Th>
            <Table.Th>Description</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {constants.map((c) => (
            <Table.Tr key={c.name}>
              <Table.Td style={{ fontFamily: 'monospace' }}>{c.name}</Table.Td>
              <Table.Td style={{ fontFamily: 'monospace' }}>{formatSiFactor(c.value)}</Table.Td>
              <Table.Td style={{ fontFamily: 'monospace' }}>{c.unit}</Table.Td>
              <Table.Td>{c.description}</Table.Td>
            </Table.Tr>
          ))}
         </Table.Tbody>
      </Table>
    </Stack>
  );
}

/** Compact table of function entries (name + description + example/unit). */
function FunctionTable({ rows }: Readonly<{ rows: FuncEntry[] }>) {
  return (
    <Table striped withTableBorder withColumnBorders>
      <Table.Thead>
        <Table.Tr>
          <Table.Th style={{ width: '38%' }}>Name</Table.Th>
          <Table.Th>Description</Table.Th>
          <Table.Th style={{ width: '26%' }}>Example / Unit</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {rows.map((f) => (
          <Table.Tr key={f.name}>
            <Table.Td style={{ fontFamily: 'monospace' }}>{f.name}</Table.Td>
            <Table.Td>{f.desc}</Table.Td>
            <Table.Td style={{ fontFamily: 'monospace', fontSize: '12px' }}>
              {f.example ? f.example : (f.unit ?? '')}
            </Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  );
}

/**
 * Fluid property functions, supported fluids (live from /api/fluids), and the
 * humid-air / glycol classes.
 */
function FluidsReference() {
  const [fluids, setFluids] = useState<string[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    getFluids().then((f) => { if (!cancelled) setFluids(f); });
    return () => { cancelled = true; };
  }, []);

  return (
    <Stack gap="md">
      <Title order={3} mt="sm">Fluid Property Functions</Title>
      <Text size="sm" c="dimmed">
        Every function takes the fluid name first, then exactly two named
        coordinates (three for <Code>AirH2O</Code>, one of which must be{' '}
        <Code>P</Code>). Results are returned in <b>SI base units</b>.
      </Text>
      <FunctionTable rows={FLUID_PROPERTY_OUTPUTS} />

      <Title order={4} c="blue.3" mt="sm">State input indicators</Title>
      <Table striped withTableBorder withColumnBorders>
        <Table.Thead>
          <Table.Tr><Table.Th style={{ width: '26%' }}>Key</Table.Th><Table.Th>Meaning</Table.Th></Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {FLUID_INPUT_INDICATORS.map((i) => (
            <Table.Tr key={i.key}>
              <Table.Td style={{ fontFamily: 'monospace' }}>{i.key}</Table.Td>
              <Table.Td>{i.meaning}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Title order={3} mt="md">Supported Fluids</Title>
      <Text size="sm" c="dimmed">
        Names are case-insensitive; several aliases map to the same CoolProp
        fluid (e.g. <Code>steam</Code>, <Code>h2o</Code> → Water). Spelled
        formulas (<Code>CO2</Code>, <Code>N2</Code>, <Code>CH4</Code>) are ideal
        gases with NASA polynomials. CoolProp availability is reported live by the
        backend.
      </Text>
      {fluids === null ? (
        <Text size="sm" c="dimmed">Loading fluids…</Text>
      ) : fluids.length === 0 ? (
        <Alert color="orange" variant="light">
          CoolProp is not available on this backend, so the live fluid list is
          empty. The function syntax below still applies when it is.
        </Alert>
      ) : (
        <Group gap={6}>
          {fluids.map((f) => (
            <Badge key={f} variant="default" style={{ fontFamily: 'monospace', textTransform: 'none' }}>{f}</Badge>
          ))}
        </Group>
      )}

      <Title order={3} mt="md">Humid Air (AirH2O)</Title>
      <Text size="sm" c="dimmed">
        Three coordinates required (one must be <Code>P</Code>). Works in SI
        internally — convert °F/psia inputs.
      </Text>
      <FunctionTable rows={AIRH2O_OUTPUTS} />
      <Table striped withTableBorder withColumnBorders>
        <Table.Thead>
          <Table.Tr><Table.Th style={{ width: '26%' }}>Key</Table.Th><Table.Th>Meaning</Table.Th></Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {AIRH2O_INDICATORS.map((i) => (
            <Table.Tr key={i.key}>
              <Table.Td style={{ fontFamily: 'monospace' }}>{i.key}</Table.Td>
              <Table.Td>{i.meaning}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Title order={3} mt="md">Aqueous Glycol Coolants</Title>
      <Text size="sm" c="dimmed">
        Incompressible mixtures written as base + mass percent:{' '}
        <Code>EG50</Code> (50% ethylene glycol), <Code>PG30</Code> (30% propylene
        glycol). Accepted bases: <Code>EG</Code>/<Code>MEG</Code>/
        <Code>EthyleneGlycol</Code> and <Code>PG</Code>/<Code>MPG</Code>/
        <Code>PropyleneGlycol</Code>. Single-phase — use <Code>T</Code> and{' '}
        <Code>P</Code> as the two indicators.
      </Text>

      <Title order={3} mt="md">Utility & Combustion Functions</Title>
      <FunctionTable rows={UTILITY_PROPERTY_FUNCS} />
    </Stack>
  );
}

/** Built-in constants — live from /api/reference, with a static fallback. */
function ConstantsReference() {
  const [constants, setConstants] = useState<ConstantInfo[] | null>(null);
  useEffect(() => {
    let cancelled = false;
    getReference().then((ref) => { if (!cancelled) setConstants(ref.constants); });
    return () => { cancelled = true; };
  }, []);
  const rows = constants ?? [];
  return (
    <Stack gap="md">
      <Title order={3} mt="sm">Built-in Constants</Title>
      <Text size="sm" c="dimmed">
        Physical and mathematical constants, available with a trailing{' '}
        <Code>#</Code> (by long-standing convention). Substituted at parse time with their SI
        value. The full unit list is on the <b>Units &amp; Constants</b> page.
      </Text>
      {constants === null && <Text size="sm" c="dimmed">Loading constants…</Text>}
      {constants !== null && rows.length === 0 && (
        <Alert color="orange" variant="light">
          The live constant list is unavailable (backend not reachable). The
          table below is the static fallback.
        </Alert>
      )}
      <Table striped withTableBorder withColumnBorders>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Name</Table.Th>
            <Table.Th>Value (SI)</Table.Th>
            <Table.Th>Unit</Table.Th>
            <Table.Th>Description</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {(rows.length > 0 ? rows : []).map((c) => (
            <Table.Tr key={c.name}>
              <Table.Td style={{ fontFamily: 'monospace' }}>{c.name}</Table.Td>
              <Table.Td style={{ fontFamily: 'monospace' }}>{formatSiFactor(c.value)}</Table.Td>
              <Table.Td style={{ fontFamily: 'monospace' }}>{c.unit}</Table.Td>
              <Table.Td>{c.description}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Stack>
  );
}

 const CYCLE_EXAMPLES = [
  {
    value: "ideal-rankine",
    title: "Power Cycles: Ideal Rankine Steam Power Cycle",
    description: "Analyses an ideal Rankine cycle, computing state enthalpies, work, and efficiency.",
    note: "",
    code: `{ Ideal Rankine Steam Power Cycle }
P_high = 8000 [kPa]
P_low = 10 [kPa]
T_boiler = 500 [C]
eta_turb = 0.85
eta_pump = 0.90
W_dot_net = 10000 [kW]

h1 = Enthalpy(Water, P=P_high, T=T_boiler)
s1 = Entropy(Water, P=P_high, T=T_boiler)
s_2s = s1
h_2s = Enthalpy(Water, P=P_low, s=s_2s)
h2 = h1 - eta_turb * (h1 - h_2s)
h3 = Enthalpy(Water, P=P_low, x=0)
v3 = Volume(Water, P=P_low, x=0)
h_4s = Enthalpy(Water, P=P_high, s=Entropy(Water, P=P_low, x=0))
h4 = h3 + (h_4s - h3) / eta_pump

w_turb = h1 - h2
w_pump = h4 - h3
q_boiler = h1 - h4
w_net = w_turb - w_pump
eta_th = w_net / q_boiler * 100
W_dot_net = m_dot * w_net`,
  },
  {
    value: "reheat-rankine",
    title: "Power Cycles: Reheat Rankine Cycle with Moisture Limit",
    description: "A reheat Rankine cycle where the condenser pressure is itself an unknown, fixed by the requirement that turbine-exit moisture not exceed 5%. frees finds it implicitly from the quality constraint.",
    note: "Verified against the textbook: condenser pressure 9.73 kPa, net power 10.2 MW, thermal efficiency 36.9%.",
    code: `{ Reheat Rankine Cycle with Moisture Limit }
{ Find the condenser pressure that limits turbine-exit moisture to 5%,
  then the net power output and thermal efficiency. }
m_dot = 7.7 [kg/s]
P[3] = 12500 [kPa]   { HP turbine inlet }
T[3] = 550 [C]
P[4] = 2000 [kPa]    { Reheat pressure }
T[5] = 450 [C]       { Reheat temperature }
P[5] = P[4]
eta_turb = 0.85
eta_pump = 0.90
x[6] = 0.95          { Max 5% moisture at LP turbine exit }

{ State 3: HP turbine inlet }
h[3] = Enthalpy(Water, P=P[3], T=T[3])
s[3] = Entropy(Water, P=P[3], T=T[3])

{ State 4: HP turbine exit }
h_4s = Enthalpy(Water, P=P[4], s=s[3])
h[4] = h[3] - eta_turb * (h[3] - h_4s)

{ State 5: reheater exit }
h[5] = Enthalpy(Water, P=P[5], T=T[5])
s[5] = Entropy(Water, P=P[5], T=T[5])

{ State 6: LP turbine exit; the quality constraint fixes P[6] }
h_6s = Enthalpy(Water, P=P[6], s=s[5])
h[6] = h[5] - eta_turb * (h[5] - h_6s)
h[6] = Enthalpy(Water, P=P[6], x=x[6])

{ State 1: condenser exit (saturated liquid) }
P[1] = P[6]                  { condenser pressure, so state 1 plots on the diagram }
h[1] = Enthalpy(Water, P=P[6], x=0)
v[1] = Volume(Water, P=P[6], x=0)

{ State 2: pump exit }
P[2] = P[3]                  { pump discharges to boiler pressure }
w_pump = v[1] * (P[3] - P[6]) / eta_pump
h[2] = h[1] + w_pump

{ Energy balances }
q_in = (h[3] - h[2]) + (h[5] - h[4])
w_turb = (h[3] - h[4]) + (h[5] - h[6])
w_net = w_turb - w_pump
W_dot_net = m_dot * w_net
eta_th = w_net / q_in * 100

{ Plot the cycle on a temperature-entropy diagram (see "Plots in Code"). }
PLOT 'Reheat Rankine T-s'
  kind = property
  fluid = Water
  diagram = 'T-s'
  overlaystates = true
  connectstates = true
END

{ T-s diagram of the reheat Rankine cycle appears in the Plots window }`,
  },
  {
    value: "reheat-regen-rankine",
    title: "Power Cycles: Ideal Reheat-Regenerative Rankine Cycle, English Units",
    description: "One reheater and two open feedwater heaters with extractions at 250 and 40 psia. All inputs are in English units (psia, F, Btu/s); frees converts them to SI automatically and solves the two feedwater-heater mass balances simultaneously.",
    note: "With 4e5 Btu/s of boiler heat input the cycle delivers about 200 MW at 47.4% thermal efficiency.",
    code: `{ Ideal Reheat-Regenerative Rankine Cycle, English Units }
{ One reheater and two open feedwater heaters; everything isentropic.
  English-unit inputs are converted to SI automatically. Find the boiler
  flow rate, net power and thermal efficiency. }
P[7] = 1500 [psia]   { HP turbine inlet }
T[7] = 1100 [F]
P[8] = 250 [psia]    { Extraction to FWH II }
P[9] = 140 [psia]    { HP exit / reheater }
T[10] = 1000 [F]     { Reheat temperature }
P[11] = 40 [psia]    { Extraction to FWH I }
P[12] = 1 [psia]     { Condenser }
Q_dot_in = 400000 [Btu/s]

{ HP turbine: 7 -> 8 (extraction) -> 9 (to reheater), isentropic }
h[7] = Enthalpy(Water, P=P[7], T=T[7])
s[7] = Entropy(Water, P=P[7], T=T[7])
h[8] = Enthalpy(Water, P=P[8], s=s[7])
h[9] = Enthalpy(Water, P=P[9], s=s[7])

{ Reheater and LP turbine: 10 -> 11 (extraction) -> 12, isentropic }
h[10] = Enthalpy(Water, P=P[9], T=T[10])
s[10] = Entropy(Water, P=P[9], T=T[10])
h[11] = Enthalpy(Water, P=P[11], s=s[10])
h[12] = Enthalpy(Water, P=P[12], s=s[10])

{ Condensate and feedwater path: saturated liquid out of each FWH }
h[1] = Enthalpy(Water, P=P[12], x=0)
v[1] = Volume(Water, P=P[12], x=0)
h[2] = h[1] + v[1] * (P[11] - P[12])
h[3] = Enthalpy(Water, P=P[11], x=0)
v[3] = Volume(Water, P=P[11], x=0)
h[4] = h[3] + v[3] * (P[8] - P[11])
h[5] = Enthalpy(Water, P=P[8], x=0)
v[5] = Volume(Water, P=P[8], x=0)
h[6] = h[5] + v[5] * (P[7] - P[8])

{ Feedwater heater balances (per kg of boiler flow):
  y extracted at 250 psia, z at 40 psia }
y * h[8] + (1 - y) * h[4] = h[5]
z * h[11] + (1 - y - z) * h[2] = (1 - y) * h[3]

{ Boiler heat input fixes the mass flow rate }
q_in = (h[7] - h[6]) + (1 - y) * (h[10] - h[9])
Q_dot_in = m_dot * q_in

{ Net power and efficiency }
w_turb = (h[7] - h[8]) + (1 - y) * (h[8] - h[9]) + (1 - y) * (h[10] - h[11]) + (1 - y - z) * (h[11] - h[12])
w_pumps = (1 - y - z) * (h[2] - h[1]) + (1 - y) * (h[4] - h[3]) + (h[6] - h[5])
w_net = w_turb - w_pumps
W_dot_net = m_dot * w_net
eta_th = w_net / q_in * 100`,
  },
  {
    value: "cogeneration-plant",
    title: "Power Cycles: Cogeneration Plant with Regeneration",
    description: "35% of the turbine flow is extracted at 1.6 MPa; one part feeds an open feedwater heater, the rest a process heater. The open-FWH energy balance determines the split.",
    note: "Verified against the textbook: boiler mass flow rate 29.1 kg/s for 25 MW of net power.",
    code: `{ Cogeneration Plant with Regeneration }
{ 35% of the turbine flow is extracted at 1.6 MPa; part heats the open
  feedwater heater, the rest serves the process heater. Isentropic
  turbine and pumps. Find the boiler flow rate for 25 MW net power. }
P[6] = 9000 [kPa]
T[6] = 400 [C]
P[7] = 1600 [kPa]    { Extraction pressure }
P[8] = 10 [kPa]      { Condenser pressure }
f_ext = 0.35         { Extracted fraction of the boiler flow }
W_dot_net = 25000 [kW]

{ State 6: turbine inlet }
h[6] = Enthalpy(Water, P=P[6], T=T[6])
s[6] = Entropy(Water, P=P[6], T=T[6])

{ States 7 and 8: isentropic expansion }
h[7] = Enthalpy(Water, P=P[7], s=s[6])
h[8] = Enthalpy(Water, P=P[8], s=s[6])

{ State 1: condenser exit; pump I to extraction pressure }
h[1] = Enthalpy(Water, P=P[8], x=0)
v[1] = Volume(Water, P=P[8], x=0)
w_pI = v[1] * (P[7] - P[8])
h[2] = h[1] + w_pI

{ States 3 and 9: FWH and process heater both yield sat. liquid at 1.6 MPa }
h[3] = Enthalpy(Water, P=P[7], x=0)
v[3] = Volume(Water, P=P[7], x=0)

{ Open FWH balance: y of the boiler flow condenses the feedwater stream }
(1 - f_ext) * h[2] + y * h[7] = (1 - f_ext + y) * h[3]

{ Mixing of FWH exit and process-heater drain is at the same state,
  then pump II raises it to boiler pressure }
w_pII = v[3] * (P[6] - P[7])
h[5] = h[3] + w_pII

{ Specific work per kg of boiler flow }
w_turb = (h[6] - h[7]) + (1 - f_ext) * (h[7] - h[8])
w_pumps = (1 - f_ext) * w_pI + w_pII
w_net = w_turb - w_pumps
W_dot_net = m_dot * w_net

{ Process heat delivered }
Q_dot_process = m_dot * (f_ext - y) * (h[7] - h[3])`,
  },
  {
    value: "binary-geothermal",
    title: "Power Cycles: Binary Geothermal Plant with Isobutane",
    description: "A binary-cycle plant where geothermal brine at 160 C drives a Rankine cycle on isobutane. The problem supplies the isobutane properties directly, so this is a pure energy-balance system.",
    note: "Results: turbine isentropic efficiency 78.8%, net power 22.6 MW, thermal efficiency 13.7%.",
    code: `{ Binary Geothermal Power Plant with Isobutane }
{ Property values are given in the problem statement. Find the turbine
  isentropic efficiency, net power and thermal efficiency. }
m_dot_geo = 555.9 [kg/s]
T_geo_in = 160 [C]
T_geo_out = 90 [C]
cp_geo = 4.258 [kJ/kg-K]
P[2] = 3250 [kPa]    { Turbine inlet pressure (pump exit) }
P[1] = 410 [kPa]     { Condenser pressure }
eta_pump = 0.90

{ Given isobutane properties }
h[1] = 273.01 [kJ/kg]    { Condenser exit, saturated liquid }
v[1] = 0.001842 [m^3/kg]
h[3] = 761.54 [kJ/kg]    { Turbine inlet }
h[4] = 689.74 [kJ/kg]    { Turbine exit, actual }
h_4s = 670.40 [kJ/kg]    { Turbine exit, isentropic }

{ (a) Turbine isentropic efficiency }
eta_turb = (h[3] - h[4]) / (h[3] - h_4s)

{ Pump work and heat-exchanger inlet state }
w_pump = v[1] * (P[2] - P[1]) / eta_pump
h[2] = h[1] + w_pump

{ Heat picked up from the geothermal brine }
Q_dot_in = m_dot_geo * cp_geo * (T_geo_in - T_geo_out)
Q_dot_in = m_dot_iso * (h[3] - h[2])

{ (b) Net power and (c) thermal efficiency }
W_dot_turb = m_dot_iso * (h[3] - h[4])
W_dot_pump = m_dot_iso * w_pump
W_dot_net = W_dot_turb - W_dot_pump
eta_th = W_dot_net / Q_dot_in * 100`,
  },
  {
    value: "brayton-irreversible",
    title: "Gas Turbines: Simple Brayton Cycle with Irreversibilities",
    description: "A gas-turbine plant between 100 and 1600 kPa with compressor and turbine efficiencies of 85% and 88%. The turbine inlet temperature is unknown and recovered from the known exhaust temperature.",
    note: "Verified against the textbook: net power 6488 kW, back work ratio 0.511, thermal efficiency 37.8%.",
    code: `{ Simple Brayton Cycle with Irreversibilities }
{ Air enters the compressor at 40 C and 850 m^3/min; the turbine exhausts
  at 650 C. Find net power, back work ratio and thermal efficiency. }
P[1] = 100 [kPa]
P[2] = 1600 [kPa]
T[1] = 40 [C]
T[4] = 650 [C]       { Turbine exit temperature }
V_dot = 850 [m^3/min]
eta_C = 0.85
eta_T = 0.88
cp = 1.108 [kJ/kg-K]
cv = 0.821 [kJ/kg-K]
k = 1.35

{ Mass flow rate from ideal gas at compressor inlet }
R = cp - cv
rho[1] = P[1] / (R * T[1])
m_dot = rho[1] * V_dot

{ Compressor }
T_2s = T[1] * (P[2] / P[1])^((k - 1) / k)
w_C = cp * (T_2s - T[1]) / eta_C
T[2] = T[1] + w_C / cp

{ Turbine: exit temperature known, inlet T[3] unknown }
T_4s = T[3] * (P[1] / P[2])^((k - 1) / k)
T[3] - T[4] = eta_T * (T[3] - T_4s)
w_T = cp * (T[3] - T[4])

{ Performance }
w_net = w_T - w_C
W_dot_net = m_dot * w_net
bwr = w_C / w_T
    q_in = cp * (T[3] - T[2])
    eta_th = w_net / q_in * 100`,
  },
  {
    value: "brayton-cold-air-standard",
    title: "Gas Turbines: Simple Brayton Cycle, Cold-Air-Standard",
    description: "An air-standard Brayton cycle with a pressure ratio of 8, compressor and turbine isentropic efficiencies of 80% and 85%, and a turbine inlet of 1300 K. The actual compressor and turbine exit temperatures (T2a, T4a) are recovered implicitly from the efficiency definitions rather than being computed by hand.",
    note: "Verified against the textbook: T2a = 604.3 K, T4a = 805.0 K, net work 191.7 kJ/kg, back work ratio 0.615, thermal efficiency 27.4%, heat rate 12 450 Btu/kWh.",
    diagram: "BraytonCycle",
    code: `{ Simple Brayton Cycle, Cold-Air-Standard }
{ Air enters the compressor at 300 K and 100 kPa, is compressed to 8 times
  that pressure, then heated to 1300 K before expanding back to the inlet
  pressure. Compressor and turbine isentropic efficiencies are 80% and 85%.
  Find the state temperatures, works, back work ratio, thermal efficiency
  and heat rate. Cold-air-standard: constant cp and k. }
T1 = 300 [K]
P1 = 100 [kPa]
rp = 8
T3 = 1300 [K]
etaC = 0.80
etaT = 0.85
k = 1.4
cp = 1.005 [kJ/kg-K]

{ Pressures through the cycle }
P2 = rp * P1
P3 = P2
P4 = P1

{ Isentropic compressor exit and actual exit (T2a is the unknown) }
T2s = T1 * rp^((k - 1) / k)
etaC = (T2s - T1) / (T2a - T1)

{ Isentropic turbine exit and actual exit (T4a is the unknown) }
T4s = T3 / rp^((k - 1) / k)
etaT = (T3 - T4a) / (T3 - T4s)

{ Work and heat transfers }
w_comp = cp * (T2a - T1)
w_turb = cp * (T3 - T4a)
w_net = w_turb - w_comp
q_in = cp * (T3 - T2a)

{ Performance metrics }
r_bw = w_comp / w_turb
eta_th = w_net / q_in * 100
heat_rate = 3412 / (eta_th / 100)`,
  },
  {
    value: "auto-gas-turbine",
    title: "Gas Turbines: Automotive Gas Turbine with Regenerator",
    description: "An isentropic Brayton cycle with a regenerator whose cold stream leaves 10 C cooler than the turbine exhaust entering it. Find the heat addition and rejection rates for 115 kW of net power.",
    note: "Verified against the textbook: heat addition 240 kW, heat rejection 125 kW.",
    code: `{ Automotive Gas Turbine with Regenerator }
{ Isentropic compressor and turbine; the cold stream leaves the
  regenerator 10 C cooler than the turbine exhaust entering it. }
P[1] = 100 [kPa]
T[1] = 30 [C]
r_p = 8
T[4] = 800 [C]       { Maximum cycle temperature (turbine inlet) }
W_dot_net = 115 [kW]
cp = 1.005 [kJ/kg-K]
k = 1.4

{ Compressor (isentropic) }
T[2] = T[1] * r_p^((k - 1) / k)

{ Turbine (isentropic) }
T[5] = T[4] * (1 / r_p)^((k - 1) / k)

{ Regenerator: cold-side exit 10 C below the hot-side inlet }
T[3] = T[5] - 10

{ Work and heat rates }
w_net = cp * (T[4] - T[5]) - cp * (T[2] - T[1])
W_dot_net = m_dot * w_net
Q_dot_in = m_dot * cp * (T[4] - T[3])
Q_dot_out = Q_dot_in - W_dot_net`,
  },
  {
    value: "brayton-regen-variable-cp",
    title: "Gas Turbines: Brayton Cycle with Regeneration and Variable Specific Heats",
    description: "Instead of assuming constant specific heats, this model uses real-gas air properties (Enthalpy/Entropy of Air) for the compressor, turbine and regenerator, exactly like the air-table solution in the book.",
    note: "Verified against the textbook: turbine exit temperature 783 K, net work 108 kJ/kg, thermal efficiency 22.5%.",
    code: `{ Brayton Cycle with Regeneration, Variable Specific Heats }
{ Real-gas air properties replace the constant-cp assumption. Find the
  turbine exit temperature, net work and thermal efficiency. }
P[1] = 100 [kPa]
T[1] = 310 [K]
r_p = 7
P[2] = P[1] * r_p
T[3] = 1150 [K]
eta_C = 0.75
eta_T = 0.82
epsilon = 0.65       { Regenerator effectiveness }

{ Compressor }
h[1] = Enthalpy(Air, T=T[1], P=P[1])
s[1] = Entropy(Air, T=T[1], P=P[1])
h_2s = Enthalpy(Air, P=P[2], s=s[1])
w_C = (h_2s - h[1]) / eta_C
h[2] = h[1] + w_C
T[2] = Temperature(Air, P=P[2], h=h[2])

{ Turbine }
h[3] = Enthalpy(Air, T=T[3], P=P[2])
s[3] = Entropy(Air, T=T[3], P=P[2])
h_4s = Enthalpy(Air, P=P[1], s=s[3])
w_T = eta_T * (h[3] - h_4s)
h[4] = h[3] - w_T
P[4] = P[1]                  { turbine exits to inlet pressure }
T[4] = Temperature(Air, P=P[1], h=h[4])

{ Regenerator }
P[5] = P[2]                  { regenerator cold side at compressor-discharge pressure }
h[5] = h[2] + epsilon * (h[4] - h[2])

{ Performance }
q_in = h[3] - h[5]
w_net = w_T - w_C
eta_th = w_net / q_in * 100

{ Plot the cycle on a temperature-entropy diagram (see "Plots in Code"). }
PLOT 'Brayton T-s'
  kind = property
  fluid = Air
  diagram = 'T-s'
  overlaystates = true
  connectstates = true
END

{ T-s diagram of the regenerative Brayton cycle appears in the Plots window }`,
  },

  // ── Hard cross-discipline case studies (all verified against the solver) ──
  {
    value: "combined-cycle",
    title: "Energy Systems: Combined Brayton–Rankine Cycle through an HRSG",
    description: "A complete combined-cycle power plant: an air-standard gas turbine (Brayton) tops a steam Rankine cycle, the two coupled through a Heat Recovery Steam Generator. The HRSG energy balance fixes the steam-to-gas mass ratio; CoolProp supplies every steam state.",
    note: "Solves in one shot per 1 kg/s of gas: compressor exit 678 K, turbine exit 774 K, 0.121 kg/s of steam raised per kg of gas, and an overall efficiency of 51.3% — higher than either cycle alone, as a real combined cycle should be.",
    code: `{ Combined-cycle plant: air-standard Brayton topping + steam Rankine
  bottoming, coupled through a Heat Recovery Steam Generator (per 1 kg/s gas). }
{ ---- Gas turbine (cold-air-standard) ---- }
cp = 1.005 [kJ/kg-K]; kk = 1.4
T1 = 300 [K]; rp = 12
T3 = 1400 [K]
eta_c = 0.82; eta_gt = 0.88
T2 = T1*(1 + (rp^((kk-1)/kk) - 1)/eta_c)
T4 = T3*(1 - eta_gt*(1 - rp^(-(kk-1)/kk)))
w_comp = cp*(T2 - T1)
w_gt   = cp*(T3 - T4)
q_gas  = cp*(T3 - T2)
wnet_gas = w_gt - w_comp
m_gas = 1 [kg/s]

{ ---- Steam Rankine bottoming (CoolProp water) ---- }
P_boil = 6000 [kPa]; T_sup = 450 [C]; P_cond = 10 [kPa]
eta_st = 0.87; eta_p = 0.85
h5 = Enthalpy(Water, P=P_boil, T=T_sup)
s5 = Entropy(Water, P=P_boil, T=T_sup)
h6s = Enthalpy(Water, P=P_cond, s=s5)
h6 = h5 - eta_st*(h5 - h6s)
h7 = Enthalpy(Water, P=P_cond, x=0)
v7 = Volume(Water, P=P_cond, x=0)
wp = v7*(P_boil - P_cond)/eta_p
h8 = h7 + wp
q_steam = h5 - h8
wnet_steam = (h5 - h6) - wp

{ ---- HRSG coupling: gas cooled from T4 to a 400 K stack ---- }
T_stack = 400 [K]
m_gas*cp*(T4 - T_stack) = m_steam*q_steam

{ ---- Plant performance ---- }
W_total = m_gas*wnet_gas + m_steam*wnet_steam
eta_overall = W_total/(m_gas*q_gas)*100`,
  },
  {
    value: "cd-nozzle-shock",
    title: "Aerospace: Supersonic Nozzle with a Normal Shock at the Exit",
    description: "A converging–diverging nozzle with exit-to-throat area ratio 4. The supersonic branch of the area–Mach relation is implicit (two roots), so set a guess of Me ≈ 2.9 in the Variable grid; a normal shock then recovers the post-shock state.",
    note: "Exit Mach 2.94 on the supersonic branch, post-shock Mach 0.479, and the shock destroys stagnation pressure from 1000 kPa to 346 kPa — the classic loss across a strong normal shock.",
    code: `{ Aerospace: CD nozzle, supersonic branch + normal shock at exit }
g = 1.4
R = 287 [J/kg-K]
P0 = 1000 [kPa]
T0 = 600 [K]
A_ratio = 4.0        { Ae/A* }
{ Area-Mach relation, supersonic root (guess Me = 2.9) }
A_ratio = (1/Me)*((2/(g+1))*(1+(g-1)/2*Me^2))^((g+1)/(2*(g-1)))
{ Isentropic exit before the shock }
Pe = P0*(1+(g-1)/2*Me^2)^(-g/(g-1))
Te = T0*(1+(g-1)/2*Me^2)^(-1)
Ve = Me*sqrt(g*R*Te)
{ Normal shock at the exit plane }
M2 = sqrt((1+(g-1)/2*Me^2)/(g*Me^2-(g-1)/2))
P2 = Pe*(1+g*Me^2)/(1+g*M2^2)
T2 = Te*(1+(g-1)/2*Me^2)/(1+(g-1)/2*M2^2)
P02 = P2*(1+(g-1)/2*M2^2)^(g/(g-1))`,
  },
  {
    value: "co2-nozzle-throat",
    title: "Compressible Flow: CO₂ Nozzle Throat Velocity",
    description: "Carbon dioxide enters a converging–diverging nozzle at 60 m/s, 310°C and 300 kPa and leaves supersonic. Because the diverging section reaches supersonic speed, the throat is choked (Ma = 1), so the throat velocity equals the local speed of sound there. The stagnation temperature comes from the inlet energy balance; the sonic temperature and speed of sound follow from constant-specific-heat ideal-gas relations.",
    note: "Throat velocity ≈ 348 m/s, confirming answer (d) 353 m/s. cp and cv come from frees's built-in CO₂ ideal-gas property functions at the inlet temperature (cp ≈ 1.06 kJ/kg-K, k ≈ 1.22); the textbook's 353 m/s uses room-temperature constant specific heats (k = 1.289), so the small difference is just the temperature dependence of cp.",
    code: `{ Throat velocity of a choked CO2 nozzle }
{ Supersonic exit => the throat is sonic (Ma = 1), so the throat velocity
  is the local speed of sound there. }
V1 = 60 [m/s]
T1 = 310 [C]
P1 = 300 [kPa]

{ CO2 ideal-gas properties from frees, evaluated at the inlet temperature }
cp = Cp(CO2, T=T1)
cv = Cv(CO2, T=T1)
k = cp / cv
R = cp - cv          { ideal gas: R = cp - cv }

{ Stagnation temperature from the inlet energy balance }
T0 = T1 + V1^2 / (2 * cp)

{ Throat is sonic (Ma = 1) }
T_throat = T0 * 2 / (k + 1)

{ Throat velocity = local speed of sound }
V_throat = sqrt(k * R * T_throat)`,
  },
  {
    value: "orbit-kepler",
    title: "Aerospace: Orbital Position from Kepler's Equation",
    description: "An elliptical Earth orbit (300 km × 3000 km altitude). Kepler's equation M = E − e·sin E is transcendental and is solved directly for the eccentric anomaly a quarter-period after perigee; radius and speed follow. Note that t and T are reserved (time) names, so the period is named Tper.",
    note: "Period 119 min, eccentricity 0.168; a quarter-period after perigee the eccentric anomaly is 1.74 rad, the true anomaly 108.9°, radius 8251 km and speed 6.85 km/s. Set a guess EA ≈ 2 in the Variable grid.",
    code: `{ Aerospace: elliptical Earth orbit; position & speed via Kepler's equation }
mu = 398600 [km^3/s^2]
Re = 6378 [km]
alt_p = 300 [km]; alt_a = 3000 [km]
rp = Re + alt_p      { perigee radius }
ra = Re + alt_a      { apogee radius }
a = (rp + ra)/2
ecc = (ra - rp)/(ra + rp)
Tper = 2*pi#*sqrt(a^3/mu)     { orbital period }
tk = Tper/4                  { a quarter-period after perigee }
M = 2*pi#*tk/Tper             { mean anomaly }
M = EA - ecc*sin(EA)         { Kepler's equation (guess EA = 2) }
nu = 2*arctan( sqrt((1+ecc)/(1-ecc)) * tan(EA/2) )
nu_deg = nu*180/pi#
r = a*(1 - ecc*cos(EA))
v = sqrt(mu*(2/r - 1/a))`,
  },
  {
    value: "pipe-network",
    title: "Fluid Mechanics: Parallel Pipe Network with Colebrook Friction",
    description: "A flow splits into two parallel branches that must share the same head loss, then recombines. Each branch's friction factor comes from the implicit Colebrook equation, so the whole network — continuity, the equal-head-loss condition, and three transcendental friction equations — is solved simultaneously via a FOR loop block.",
    note: "The branches divide the 0.10 m³/s feed as 0.029 and 0.071 m³/s so that both lose 9.83 m of head; total network head loss is 14.5 m. Reynolds numbers and friction factors are all consistent (turbulent).",
    code: `{ Civil/Fluids: parallel pipe network, Colebrook friction }
rho = 1000 [kg/m^3]
mu = 0.001 [Pa-s]
g = 9.81 [m/s^2]
Q_in = 0.10 [m^3/s]
L[1]=300; L[2]=500; L[3]=400
D[1]=0.25; D[2]=0.15; D[3]=0.20
eps = 0.00015
Q_in = Q[1]
Q[1] = Q[2] + Q[3]            { continuity at the split }
hf[2] = hf[3]                 { parallel branches share head loss }
FOR j = 1 TO 3
  V[j] = Q[j]/(pi#/4*D[j]^2)
  Re[j] = rho*V[j]*D[j]/mu
  1/sqrt(ff[j]) = -2*log10(eps/(3.7*D[j]) + 2.51/(Re[j]*sqrt(ff[j])))
  hf[j] = ff[j]*L[j]/D[j]*V[j]^2/(2*g)
END
h_total = hf[1] + hf[2]`,
  },
  {
    value: "open-channel-jump",
    title: "Water Resources: Manning Flow, Critical Depth & Hydraulic Jump",
    description: "A rectangular channel on a steep slope. Manning's equation for the normal depth is implicit; the critical depth follows from the unit discharge, and the hydraulic-jump momentum function gives the sequent depth and the energy dissipated. (Note Q and q are the same name in case-insensitive frees — the unit discharge is qu.)",
    note: "Normal depth 0.825 m is below the critical depth 1.177 m, so the flow is supercritical (Fr₁ = 1.70). The jump lifts it to 1.618 m (Fr₂ = 0.62, subcritical) and dissipates 0.094 m of head. Set a guess yn ≈ 0.6.",
    code: `{ Civil/Hydraulics: rectangular channel - normal & critical depth + jump }
g = 9.81 [m/s^2]
Q = 20 [m^3/s]
b = 5 [m]
n = 0.015
S0 = 0.01            { steep slope -> supercritical normal flow }
qu = Q/b             { unit discharge }
yc = (qu^2/g)^(1/3)  { critical depth }
{ Normal depth via Manning (implicit, guess yn = 0.6) }
Aflow = b*yn
Pwet = b + 2*yn
Rh = Aflow/Pwet
Q = (1/n)*Aflow*Rh^(2/3)*sqrt(S0)
V1 = Q/Aflow
Fr1 = V1/sqrt(g*yn)
{ Hydraulic jump: sequent depth from the momentum function }
y2 = yn/2*(sqrt(1 + 8*Fr1^2) - 1)
V2 = Q/(b*y2)
Fr2 = V2/sqrt(g*y2)
dE = (y2 - yn)^3/(4*yn*y2)   { energy dissipated }`,
  },
  {
    value: "pelton-turbine",
    title: "Turbomachinery: Pelton Wheel (Impulse Turbine) Design",
    description: "Designs a single-jet Pelton wheel for a high-head hydro site. The jet velocity comes from the net head through the nozzle velocity coefficient; the optimum bucket speed sets the wheel diameter at a given rotational speed (entered in rpm, which frees converts to rad/s). The jet diameter follows from continuity, and the bucket force/power use the momentum change through the deflection angle. Demonstrates angle units (deg → rad), rpm → rad/s, and dimensionally derived results.",
    note: "Vj ≈ 75.2 m/s, bucket speed u ≈ 34.6 m/s, wheel diameter D ≈ 1.32 m, jet diameter d ≈ 92 mm (jet ratio D/d ≈ 14.4). Bucket power ≈ 1.28 MW from 1.47 MW of water power — hydraulic efficiency η_h ≈ 90% (wheel efficiency on head ≈ 87% after the nozzle loss).",
    code: `{ Pelton Wheel (Impulse Turbine) Design — single jet }
{ High-head hydro: given net head, flow and speed, size the wheel and
  jet and find the power and efficiency. }
rho = 1000 [kg/m^3]
g = 9.81 [m/s^2]
H = 300 [m]            { net head at the nozzle }
Q = 0.5 [m^3/s]        { volumetric flow (one jet) }
N = 500 [rpm]          { runner speed — rpm auto-converts to rad/s }
C_v = 0.98             { nozzle velocity coefficient }
phi = 0.46             { speed ratio  u / Vj  (optimum ~0.46) }
beta = 165 [deg]       { bucket outlet angle }
k = 0.85               { relative-velocity friction factor }

{ Jet from the nozzle }
Vj = C_v * sqrt(2 * g * H)

{ Optimum bucket (peripheral) speed and the wheel diameter.
  N is an angular velocity in rad/s, so u = N * D_wheel/2.
  (D_wheel and d_jet must differ — names are case-insensitive in frees.) }
u = phi * Vj
u = N * (D_wheel / 2)

{ Jet diameter from continuity, and the jet ratio (good design 11–14) }
Q = (pi# / 4) * d_jet^2 * Vj
m_ratio = D_wheel / d_jet

{ Force and power on the buckets (jet deflected through 180° − beta) }
F = rho * Q * (Vj - u) * (1 + k * cos(180 [deg] - beta))
P_bucket = F * u

{ Water power available and the efficiencies }
P_water = rho * g * Q * H
eta_hydraulic = P_bucket / (0.5 * rho * Q * Vj^2) * 100
eta_wheel = P_bucket / P_water * 100`,
  },
  {
    value: "ev-longitudinal",
    title: "Vehicle Dynamics: EV Longitudinal Road-Load, Power & Range",
    description: "The longitudinal road-load model for an electric car: tractive force as the sum of rolling resistance, aerodynamic drag and road grade, then wheel power, battery power through the drivetrain efficiency, the energy consumed per metre, and the range from a usable pack. Change the grade angle alpha to see hill-climb demand. (Grounded in the Electric Vehicle Design text in the linked notebook.)",
    note: "Flat 120 km/h cruise: tractive force ≈ 617 N, battery power ≈ 24 kW, consumption ≈ 722 J/m (≈ 200 Wh/km), and ≈ 2.995e5 m ≈ 300 km of range from a 60 kWh pack. Set alpha = 5 [deg] and the climb term M·g·sin(alpha) dominates.",
    code: `{ EV longitudinal road-load: force, power, consumption and range }
{ F_trac = rolling + grade + aero; P_batt = P_wheel / drivetrain efficiency. }
M = 1500 [kg]          { vehicle mass }
g = 9.81 [m/s^2]
Crr = 0.012            { rolling-resistance coefficient }
rho = 1.2 [kg/m^3]     { air density }
Cd = 0.30              { drag coefficient }
A_f = 2.2 [m^2]        { frontal area }
alpha = 0 [deg]        { road grade angle (0 = flat) }
V = 120 [km/h]         { cruise speed (auto-converts to m/s) }
eta_t = 0.90           { transmission efficiency }
eta_m = 0.95           { motor + inverter efficiency }
E_pack = 60 [kWh]      { usable battery energy }

F_roll  = M*g*Crr*cos(alpha)
F_grade = M*g*sin(alpha)
F_aero  = 0.5*rho*Cd*A_f*V^2
F_trac  = F_roll + F_grade + F_aero
P_wheel = F_trac*V
P_batt  = P_wheel/(eta_t*eta_m)
cons    = P_batt/V          { energy per distance [J/m]; divide by 3.6 for Wh/km }
Range   = E_pack/cons       { [m]; /1000 for km }`,
  },
  {
    value: "ev-lateral",
    title: "Vehicle Dynamics: Lateral Bicycle Model & Understeer Gradient",
    description: "The steady-state single-track (bicycle) model: axle loads from the CG position, the understeer gradient from front/rear cornering stiffness, the front steer angle to hold a curve (Ackermann plus the dynamic term), and the characteristic speed of the understeering vehicle. Kus > 0 confirms a stable, understeering layout.",
    note: "With the front-biased CG and equal axle stiffness, Kus ≈ 0.037 (understeer). At R = 100 m and 25 m/s the required steer is δ ≈ 3.0°, and the characteristic speed (steer = twice Ackermann, peak yaw gain) is ≈ 27.6 m/s ≈ 99 km/h.",
    code: `{ Steady-state lateral (bicycle) model: understeer gradient and steer angle }
m = 1730 [kg]
g = 9.81 [m/s^2]
a = 1.189 [m]          { CG to front axle }
b = 1.696 [m]          { CG to rear axle }
Cf = 80000 [N/rad]     { front axle cornering stiffness }
Cr = 80000 [N/rad]     { rear axle cornering stiffness }
R = 100 [m]            { turn radius }
V = 25 [m/s]           { cornering speed }

L = a + b                       { wheelbase }
W_f = m*g*b/L                   { static front axle load }
W_r = m*g*a/L                   { static rear axle load }
Kus = W_f/Cf - W_r/Cr           { understeer gradient (>0 understeer) }
a_y = V^2/(g*R)                 { lateral acceleration [g] }
delta = L/R + Kus*a_y           { front steer angle [rad] }
delta_deg = delta*Convert('rad','deg')
u_char = sqrt(g*L/Kus)          { characteristic speed [m/s] }`,
  },
  {
    value: "ev-sizing",
    title: "EV Powertrain: Motor & Battery Sizing (acceleration, gradeability, range)",
    description: "Sizes the traction motor and battery from performance targets. The motor must satisfy the harder of two demands — mean power to reach 100 km/h in the acceleration time, and steady power to hold a 6° grade at 60 km/h — taken with max(). The battery is sized from the cruise road-load and a target range. (Methodology from the Electric Vehicle Design text in the linked notebook.)",
    note: "Acceleration (≈93 kW) dominates gradeability (≈37 kW), so size the motor at ≈93 kW. A 400 km range at 110 km/h needs E_need ≈ 2.5e8 J ≈ 69 kWh of usable energy.",
    code: `{ EV motor + battery sizing from acceleration, gradeability and range }
M = 1600 [kg]
g = 9.81 [m/s^2]
Crr = 0.012
rho = 1.2 [kg/m^3]
Cd = 0.28
A_f = 2.3 [m^2]
eta = 0.88             { combined drivetrain efficiency }

{ 1) Acceleration: mean power to reach V_f in t_acc (energy method) }
V_f = 100 [km/h]
t_acc = 7 [s]
delta = 1.05           { rotational-inertia mass factor }
E_kin = 0.5*M*delta*V_f^2
P_acc = E_kin/t_acc

{ 2) Gradeability: hold a grade at steady speed }
theta = 6 [deg]
V_grade = 60 [km/h]
F_grade = M*g*(Crr*cos(theta)+sin(theta)) + 0.5*rho*Cd*A_f*V_grade^2
P_grade = F_grade*V_grade/eta

{ Motor must cover the harder requirement }
P_motor = max(P_acc, P_grade)

{ 3) Battery sizing from cruise road-load and target range }
V_cr = 110 [km/h]
F_cr = M*g*Crr + 0.5*rho*Cd*A_f*V_cr^2
cons = F_cr/eta            { [J/m] }
Range_target = 400 [km]
E_need = cons*Range_target { usable energy [J]; /3.6e6 for kWh }`,
  },
  {
    value: "ev-battery-pack",
    title: "EV Powertrain: Battery Pack Design & Cell Selection (Batemo database)",
    description: "Designs a pack from a target voltage, energy and peak power, then checks a real cell against the load. The cell is the Molicel INR21700-P42A taken from the Batemo Cell Explorer database in the linked notebook. Series count sets voltage, parallel count sets energy; the per-cell peak current must stay under the cell's rating. Battery quantities use conventional units (V, Ah, A, Wh, kWh) rather than SI, so they are kept unitless here.",
    note: "112S × 42P ≈ 4700 cells gives ~400 V, ~175 Ah and ~70 kWh. A 150 kW peak draws 375 A from the pack (2.1C) = 9 A per cell — well under the P42A's 45 A continuous limit (36 A headroom). Round Ns/Np up to whole cells in practice.",
    code: `{ Battery pack design + cell selection (engineering units, see comments) }
{ Cell: Molicel INR21700-P42A from the Batemo Cell Explorer database. }
V_pack = 400           { target nominal pack voltage [V] }
E_target = 70          { required usable energy [kWh] }
P_peak = 150           { peak power demand [kW] }

V_cell = 3.6           { cell nominal voltage [V] }
Q_cell = 4.2           { cell capacity [Ah] }
I_cell_max = 45        { cell max continuous discharge [A] }
E_cell = V_cell*Q_cell { cell energy [Wh] }

Ns = V_pack/V_cell             { cells in series (round up) }
Np = E_target*1000/(Ns*E_cell) { parallel strings (kWh -> Wh) }
N_cells = Ns*Np
E_pack = N_cells*E_cell/1000   { pack energy [kWh] }
Q_pack = Np*Q_cell             { pack capacity [Ah] }
I_peak = P_peak*1000/V_pack    { peak pack current [A] }
I_cell = I_peak/Np             { current per cell [A] }
C_rate = I_peak/Q_pack         { pack C-rate [1/h] }
margin = I_cell_max - I_cell   { per-cell headroom [A], must be > 0 }`,
  },
  {
    value: "ev-regen",
    title: "Vehicle Dynamics: Regenerative Braking Energy Recovery",
    description: "How much kinetic energy a regenerative brake can return. The motor-as-generator recovers a fraction (regen efficiency) of the vehicle's ½·m·V² when slowing from a speed, with the rotational-inertia mass factor included. Shows the energy per stop, how many stops fill 1 kWh, and the total recovered over a stop-and-go trip.",
    note: "From 50 km/h a 1600 kg car holds ≈ 162 kJ of kinetic energy; at 65% regen ≈ 105 kJ (0.029 kWh) returns per stop, so ≈ 34 such stops bank 1 kWh, and a 20-stop city run recovers ≈ 2.1 MJ (≈ 0.58 kWh). (Source caveat: the books note real-world regen return is drive-cycle dependent, often 10–30% of pack energy in stop-and-go.)",
    code: `{ Regenerative braking: kinetic energy recovered per stop and per trip }
M = 1600 [kg]
V = 50 [km/h]          { speed before braking }
eta_regen = 0.65       { fraction of kinetic energy returned to the battery }
delta = 1.05           { rotational-inertia mass factor }

E_kin = 0.5*M*delta*V^2        { kinetic energy [J] }
E_regen = eta_regen*E_kin      { recovered per stop [J]; /3.6e6 for kWh }

E_oneKWh = 1 [kWh]
stops_per_kWh = E_oneKWh/E_regen   { stops to bank 1 kWh }

N_stops = 20                   { stops on a city run }
E_recovered = E_regen*N_stops  { total recovered [J] }`,
  },
  {
    value: "ev-drive-cycle",
    title: "Vehicle Dynamics: Combined Drive-Cycle Consumption & Range",
    description: "A two-phase (city + highway) drive-cycle estimate. The flat road-load consumption is computed at each phase speed, the city phase gets a regenerative-braking credit, and the phases are blended by distance fraction into a combined consumption and the resulting range from a usable pack.",
    note: "55% city / 45% highway: city ≈ 183 J/m (after a 25% regen credit), highway ≈ 596 J/m, blended ≈ 369 J/m (≈ 102 Wh/km), giving ≈ 5.86e5 m ≈ 586 km from a 60 kWh pack.",
    code: `{ Combined city/highway drive-cycle consumption and range }
M = 1550 [kg]
g = 9.81 [m/s^2]
Crr = 0.011
rho = 1.2 [kg/m^3]
Cd = 0.29
A_f = 2.2 [m^2]
eta = 0.88
E_pack = 60 [kWh]
regen_credit = 0.25        { city energy returned by regen }

V_city = 40 [km/h]
V_hwy = 110 [km/h]
frac_city = 0.55
frac_hwy = 0.45

F_city = M*g*Crr + 0.5*rho*Cd*A_f*V_city^2
F_hwy  = M*g*Crr + 0.5*rho*Cd*A_f*V_hwy^2
cons_city = F_city/eta*(1 - regen_credit)   { [J/m] }
cons_hwy  = F_hwy/eta
cons_avg  = frac_city*cons_city + frac_hwy*cons_hwy
Range     = E_pack/cons_avg                 { [m]; /1000 for km }`,
  },
  {
    value: "fcev-sizing",
    title: "EV Powertrain: Hydrogen Fuel-Cell Vehicle (FCEV) Sizing & Range",
    description: "Sizes a hydrogen fuel-cell vehicle from its 700-bar tank. Usable electrical energy is the stored hydrogen mass times its lower heating value times the net fuel-cell system efficiency; range follows from the cruise road-load, and the continuous stack power must cover that load. Hydrogen consumption per 100 km falls out for comparison with real cars. (Grounded in the hydrogen-vehicle books in the linked notebook; Toyota Mirai II ≈ 5.6 kg tank.)",
    note: "5.6 kg H2 at 45% net FC efficiency ≈ 3.02e8 J usable → ≈ 5.15e5 m ≈ 515 km. Continuous stack power at 100 km/h ≈ 16.3 kW; hydrogen consumption cons_H2 ≈ 1.09e-5 kg/m ≈ 1.09 kg/100 km — right in the 1.0–1.16 kg/100 km band for passenger FCEVs.",
    code: `{ Hydrogen fuel-cell vehicle: usable energy, range and consumption }
m_H2 = 5.6 [kg]        { usable hydrogen (700 bar tank, Mirai-II class) }
LHV = 120 [MJ/kg]      { hydrogen lower heating value }
eta_fc = 0.45          { net fuel-cell system efficiency }
E_usable = m_H2*LHV*eta_fc      { electrical energy to the drivetrain [J] }

M = 1800 [kg]
g = 9.81 [m/s^2]
Crr = 0.011
rho = 1.2 [kg/m^3]
Cd = 0.29
A_f = 2.4 [m^2]
eta_dt = 0.88
V = 100 [km/h]

F_cr = M*g*Crr + 0.5*rho*Cd*A_f*V^2
cons = F_cr/eta_dt              { road-load energy per distance [J/m] }
Range = E_usable/cons           { [m]; /1000 for km }
P_fc = cons*V                   { continuous stack power at cruise [W] }
cons_H2 = m_H2/Range            { hydrogen use [kg/m]; *1e5 for kg/100 km }`,
  },
  {
    value: "ev-fast-charge",
    title: "EV Powertrain: DC Fast Charging Time, C-rate & Grid Energy",
    description: "DC fast-charge sizing over the usual 10→80% SOC window. From the charge current it finds the constant-current charge time, the charge C-rate, the DC power, the energy added, and the grid energy once the charger's efficiency is included. Battery quantities use conventional units (V, Ah, A, kWh) and are kept unitless. (Grounded in the EV fast-charging book in the linked notebook.)",
    note: "175 Ah / 400 V pack, 10→80% at 350 A: ≈ 21 min on the CC phase, 2.0C, 140 kW DC, 49 kWh added, ≈ 52.7 kWh drawn from the grid at 93% charger efficiency. Real charges add a tapering CV phase past ~80%.",
    code: `{ EV DC fast charge: time, C-rate, power and grid energy (engineering units) }
V_pack = 400           { nominal pack voltage [V] }
Q_pack = 175           { pack capacity [Ah] }
E_pack = 70            { pack energy [kWh] }
SOC_start = 10         { start state of charge [%] }
SOC_end = 80           { end state of charge [%] }
I_charge = 350         { DC charge current [A] }
eta_charger = 0.93     { charger (grid->DC) efficiency }

dSOC = (SOC_end - SOC_start)/100
Ah_added = dSOC*Q_pack             { charge delivered [Ah] }
t_charge_h = Ah_added/I_charge     { CC-phase time [h] }
t_charge_min = t_charge_h*60
C_rate = I_charge/Q_pack           { charge C-rate [1/h] }
P_charge = V_pack*I_charge/1000    { DC charge power [kW] }
E_added = dSOC*E_pack              { energy added to pack [kWh] }
E_grid = E_added/eta_charger       { energy drawn from grid [kWh] }`,
  },
  {
    value: "hev-power-split",
    title: "EV Powertrain: Parallel Hybrid (HEV) Power Split",
    description: "A parallel-hybrid power split at a peak demand point. The total wheel power for hard acceleration on the move is computed from the road load plus the inertial term; the engine is sized to cover about half of the peak, and the electric motor supplies the balance, drawing current from the battery. (Power-split relation and sizing rule from the hybrid-vehicle material in the linked notebook.)",
    note: "At 120 km/h with 1.5 m/s² acceleration the wheels need ≈ 126 kW; sizing the engine at 50% leaves ≈ 63 kW for the motor, which pulls ≈ 228 A from a 300 V battery at 92% motor efficiency.",
    code: `{ Parallel HEV power split: P_load = P_engine + P_motor }
M = 1700 [kg]
g = 9.81 [m/s^2]
Crr = 0.011
rho = 1.2 [kg/m^3]
Cd = 0.30
A_f = 2.3 [m^2]
eta_dt = 0.88
V_peak = 120 [km/h]    { speed at the peak-power point }
a_peak = 1.5 [m/s^2]   { simultaneous acceleration }
delta = 1.05

F_peak = M*g*Crr + 0.5*rho*Cd*A_f*V_peak^2 + M*delta*a_peak
P_load = F_peak*V_peak/eta_dt      { total wheel power demand }
P_engine = 0.5*P_load              { ICE sized for ~half of peak }
P_motor = P_load - P_engine        { motor supplies the rest }

V_batt = 300 [V]
eta_m = 0.92
I_batt = P_motor/(V_batt*eta_m)    { battery current for the motor [A] }`,
  },
  {
    value: "cell-to-pack-density",
    title: "EV Powertrain: Cell-to-Pack Mass & Gravimetric Energy Density",
    description: "Scales single-cell mass and energy up to a full pack, then compares cell- and pack-level specific energy. The pack mass adds structure, busbars, cooling and the BMS via a cell-mass fraction (cell-to-pack mass efficiency), which dilutes the gravimetric energy density from cell to pack level.",
    note: "4700 × 21700 cells (70 g, 15.1 Wh each): cell-level ≈ 7.77e5 J/kg ≈ 216 Wh/kg, but at a 0.70 cell-mass fraction the 470 kg pack delivers ≈ 5.44e5 J/kg ≈ 151 Wh/kg (divide J/kg by 3600 for Wh/kg).",
    code: `{ Cell-to-pack mass and gravimetric energy density }
N_cells = 4700
m_cell = 0.070 [kg]        { 21700 cell mass (~70 g) }
E_cell = 54.4 [kJ]         { cell energy (~15.1 Wh) }
pack_mass_factor = 0.70    { cell mass / pack mass (cell-to-pack efficiency) }

m_cells = N_cells*m_cell
m_pack = m_cells/pack_mass_factor      { pack mass incl. structure/cooling/BMS }
E_pack = N_cells*E_cell
e_cell_grav = E_cell/m_cell            { cell specific energy [J/kg]; /3600 = Wh/kg }
e_pack_grav = E_pack/m_pack            { pack specific energy [J/kg]; /3600 = Wh/kg }`,
  },
  {
    value: "truss-stiffness",
    title: "Structural Analysis: Plane Truss by the Direct Stiffness Method",
    description: "A three-bar truss with one free node, assembled and solved exactly as a finite-element code would: each member contributes its EA/L stiffness with direction cosines to a 2×2 global stiffness matrix, which SolveLinear inverts for the nodal displacements; member axial forces follow.",
    note: "The downward 100 kN load gives a vertical deflection of 1.00 mm and member forces −69.8, −25.1, −25.1 kN. They satisfy vertical equilibrium exactly (−69.8 − 2·25.1·0.6 = −100 kN). Watch the case-insensitive K/k clash — the matrix is Kg, the member stiffness ka.",
    code: `{ Structural: 3-bar plane truss solved by the direct stiffness method }
E = 210e9 [Pa]
A = 1e-3 [m^2]
P = 100e3 [N]
{ free node 1 at origin connected to three supports;
  member 1 vertical (L=3); members 2,3 to (+/-4, 3), L=5 }
L[1]=3; L[2]=5; L[3]=5
cx[1]=0;    sy[1]=1
cx[2]=0.8;  sy[2]=0.6
cx[3]=-0.8; sy[3]=0.6
FOR m = 1 TO 3
  ka[m] = E*A/L[m]                 { member axial stiffness EA/L }
END
{ Assemble the 2x2 reduced global stiffness at the free node }
Kg[1,1] = ka[1]*cx[1]^2 + ka[2]*cx[2]^2 + ka[3]*cx[3]^2
Kg[1,2] = ka[1]*cx[1]*sy[1] + ka[2]*cx[2]*sy[2] + ka[3]*cx[3]*sy[3]
Kg[2,1] = Kg[1,2]
Kg[2,2] = ka[1]*sy[1]^2 + ka[2]*sy[2]^2 + ka[3]*sy[3]^2
{ Solve Kg u = F for the nodal displacements, then member forces }
F[1:2] = [0, -P]
u[1:2] = SolveLinear(Kg[1:2,1:2], F[1:2])
FOR m = 1 TO 3
  Naxial[m] = ka[m]*(cx[m]*u[1] + sy[m]*u[2])
END`,
  },
  {
    value: "radiation-enclosure",
    title: "Heat Transfer: 3-Surface Radiation Enclosure with a Reradiating Wall",
    description: "An equilateral triangular duct with a hot surface, a cold surface and an adiabatic (reradiating) wall. The radiosity-network equations — two gray-surface balances plus the reradiating condition that the wall's radiosity equals its irradiation — are nonlinear in T⁴ and solved together.",
    note: "Net exchange between the gray surfaces is 30.1 kW with Q₁ = −Q₂ exactly (energy balance), and the floating reradiating wall settles at 846 K, between the 1000 K and 400 K surfaces.",
    code: `{ Heat transfer: 3-surface enclosure (equilateral triangular duct) with a
  reradiating wall, solved by the radiosity network method }
sigma = 5.67e-8 [W/m^2-K^4]
A = 1 [m^2]
T1 = 1000 [K]; eps1 = 0.8     { hot surface }
T2 = 400 [K];  eps2 = 0.8     { cold surface }
{ Each flat surface of the equilateral triangle sees the others equally }
F12 = 0.5; F13 = 0.5
F21 = 0.5; F23 = 0.5
F31 = 0.5; F32 = 0.5
Eb1 = sigma*T1^4
Eb2 = sigma*T2^4
{ Radiosity balance on the two gray surfaces }
J1 = eps1*Eb1 + (1-eps1)*(F12*J2 + F13*J3)
J2 = eps2*Eb2 + (1-eps2)*(F21*J1 + F23*J3)
{ Reradiating surface: radiosity equals its irradiation }
J3 = F31*J1 + F32*J2
{ Net heat leaving each gray surface }
Q1 = A*eps1/(1-eps1)*(Eb1 - J1)
Q2 = A*eps2/(1-eps2)*(Eb2 - J2)
T3 = (J3/sigma)^0.25          { reradiating-wall temperature }`,
  },
  {
    value: "auto-cooling-loop",
    title: "Thermal/Automotive: Radiator + Pump + Fan Cooling Loop (EG50 coolant)",
    description: "An automotive cooling loop where the fan and pump operating points are found implicitly — each performance curve, entered as a TABLE and called like a function, is intersected with its quadratic flow resistance — and the radiator heat duty follows from a digitized effectiveness table. The coolant is a 50/50 ethylene glycol/water mixture (EG50) whose density and specific heat come straight from CoolProp. The fan curve is affinity-scaled by f_rpm so you can slow the fan or sweep it in a Parametric Table.",
    note: "Solves to Q = 38.2 kW rejected, fan power 262 W and pump power 97 W, at an air-side operating point of 0.90 m³/s (~1910 CFM, 131 Pa) and a coolant flow of 89.8 L/min. The TABLE input/output units ([m^3/s] → [Pa]) let frees derive SI units all the way through (Vair m³/s, dP_air Pa, Q W, T_c_out K) instead of showing '-'. EG50 at 90 °C gives ρ = 1019 kg/m³, cp = 3616 J/kg·K (vs water 965 / 4205). Data sources: cross-flow radiator effectiveness ε≈0.6–0.85 and heat rejection 25–50 kW (ResearchGate 397980466, FSAE study 356606738); SPAL 16-inch fan ~2500 CFM free-air / ~250 Pa max static (streetmusclemag.com); Davies-Craig EWP pump 90–162 L/min (daviescraig.com.au/electric-water-pumps).",
    code: `{ Automotive cooling loop: radiator + electric pump + electric fan.
  Coolant = 50/50 ethylene glycol / water (EG50), properties from CoolProp.
  Pump and fan curves are entered as TABLE blocks and used as functions.

  Data sources (typical passenger-car / aftermarket components):
   - Cross-flow radiator: effectiveness ~0.6-0.85, heat rejection ~25-50 kW
     (ResearchGate 397980466; FSAE radiator study 356606738)
   - Fan curve, SPAL 16in class: ~2500 CFM free air, ~250 Pa max static
     (streetmusclemag.com - SPAL electric fans)
   - Pump curve, Davies-Craig EWP class: 90-162 L/min
     (daviescraig.com.au/electric-water-pumps) }

{ Inputs }
T_c_in = 95 [C]        { hot coolant into radiator }
T_a_in = 35 [C]        { ambient air }
P_atm  = 101325 [Pa]
eta_fan  = 0.45        { fan total efficiency }
eta_pump = 0.55        { pump total efficiency }
f_rpm    = 1           { fan speed / rated (set < 1 to slow the fan) }

{ Fan curve: static pressure [Pa] vs air flow [m^3/s].
  The [Pa] on the table output lets frees derive SI units for everything
  computed from the lookup (dP_air, W_fan, ...). }
TABLE fanCurve(Vair [m^3/s]) [Pa]
  0.0    250
  0.3    232
  0.6    195
  0.9    132
  1.18   0
END

{ Pump curve: head [Pa] vs coolant flow [m^3/s] }
TABLE pumpCurve(Vc [m^3/s]) [Pa]
  0.0      55000
  0.0008   48000
  0.0016   34000
  0.0023   0
END

{ Radiator effectiveness (digitized): dimensionless epsilon vs air flow }
TABLE radEff(Vair [m^3/s]) [-]
  0.3   0.45
  0.6   0.55
  0.9   0.62
  1.2   0.67
END

{ Flow resistances: dP = K * V^2, so K carries [Pa/(m^3/s)^2] = [kg/m^7].
  Annotating K grounds the flows Vair, Vc at m^3/s. }
K_air = 160 [kg/m^7]
K_c   = 1.6e10 [kg/m^7]

{ Fan operating point (affinity-scaled to f_rpm) meets air-side resistance }
dP_air = f_rpm^2 * fanCurve(Vair / f_rpm)
dP_air = K_air * Vair^2

{ Pump operating point meets coolant-loop resistance }
head = pumpCurve(Vc)
head = K_c * Vc^2

{ Coolant properties from the EG50 mixture; air properties at ~40 C }
rho_c = Density(EG50, T=90 [C], P=P_atm)
cp_c  = Cp(EG50, T=90 [C], P=P_atm)
rho_air = 1.13 [kg/m^3]
cp_air  = 1006 [J/kg-K]

{ Mass flows and capacity rates }
m_air = rho_air * Vair
m_c   = rho_c * Vc
C_air = m_air * cp_air
C_c   = m_c * cp_c
C_min = min(C_air, C_c)

{ Heat transfer (effectiveness method) }
eps = radEff(Vair)
Q = eps * C_min * (T_c_in - T_a_in)
T_c_out = T_c_in - Q / C_c
T_a_out = T_a_in + Q / C_air

{ Power draw }
W_fan  = dP_air * Vair / eta_fan
W_pump = head * Vc / eta_pump`,
  },
  {
    value: "load-flow",
    title: "Power Systems: Two-Bus AC Power Flow (Newton–Raphson Load Flow)",
    description: "A slack bus feeds a PQ load bus over a transmission line. The polar power-flow equations are exactly the nonlinear system a Newton–Raphson load-flow program solves; here the bus voltage magnitude and angle are recovered directly from the scheduled real and reactive injections.",
    note: "Drawing 0.5 + j0.2 pu through a 0.02 + j0.06 pu line drops the load-bus voltage to 0.977 pu at an angle of −1.52° — the expected sag and phase lag behind the slack bus. Set guesses V2 ≈ 1, th2 ≈ −0.1.",
    code: `{ Power systems: 2-bus AC power flow (Newton-Raphson load flow) }
{ Bus 1 = slack (V1=1, th1=0). Bus 2 = PQ load. Line z = 0.02 + j0.06 pu }
zr = 0.02; zi = 0.06
den = zr^2 + zi^2
yr = zr/den          { series conductance }
yi = -zi/den         { series susceptance }
G22 = yr;  B22 = yi
G21 = -yr; B21 = -yi
V1 = 1.0; th1 = 0
P2 = -0.5; Q2 = -0.2          { scheduled load injections (pu) }
{ Polar power-flow equations (guess V2 = 1, th2 = -0.1) }
P2 = V2^2*G22 + V1*V2*(G21*cos(th2-th1) + B21*sin(th2-th1))
Q2 = -V2^2*B22 + V1*V2*(G21*sin(th2-th1) - B21*cos(th2-th1))
th2_deg = th2*180/pi#`,
  },
  {
    value: "reforming-equilibrium",
    title: "Chemical Engineering: Coupled Reforming + Water-Gas-Shift Equilibrium",
    description: "Two reactions reach equilibrium at once: steam-methane reforming (Δn = +2, pressure-dependent) and the water-gas shift (Δn = 0). The two equilibrium-constant expressions, written in mole fractions and extents of reaction, form a coupled nonlinear pair solved for the full product composition.",
    note: "At the given equilibrium constants the methane conversion is 98.5%; the dry product is hydrogen-rich (y_H₂ = 0.56) with the mole fractions summing exactly to 1. Set guesses x1 ≈ 0.9, x2 ≈ 0.3 (both bounded 0–1).",
    code: `{ Chemical: coupled equilibrium of steam-methane reforming + water-gas shift }
{ R1: CH4 + H2O <-> CO + 3 H2     (Kp1, dn=+2)
  R2: CO  + H2O <-> CO2 + H2      (Kp2, dn=0)  }
Kp1 = 26.0      { at ~1000 K, dimensionless with P0 = 1 bar }
Kp2 = 1.45
P = 1.0 [bar]
P0 = 1.0 [bar]
n_CH4_0 = 1; n_H2O_0 = 3
{ Extents x1, x2 (guesses 0.9 and 0.3); equilibrium moles }
n_CH4 = n_CH4_0 - x1
n_H2O = n_H2O_0 - x1 - x2
n_CO  = x1 - x2
n_H2  = 3*x1 + x2
n_CO2 = x2
n_tot = n_CH4 + n_H2O + n_CO + n_H2 + n_CO2
y_CH4 = n_CH4/n_tot; y_H2O = n_H2O/n_tot; y_CO = n_CO/n_tot
y_H2 = n_H2/n_tot;   y_CO2 = n_CO2/n_tot
Kp1 = (y_CO*y_H2^3)/(y_CH4*y_H2O) * (P/P0)^2
Kp2 = (y_CO2*y_H2)/(y_CO*y_H2O)
conv = x1/n_CH4_0*100         { methane conversion, % }`,
  },
  {
    value: "pid-pole-placement",
    title: "Control Systems: PID Design by Pole Placement",
    description: "A PID controller is designed for a DC-motor plant K/(s(τs+1)) so that the closed-loop characteristic polynomial matches a target — a dominant second-order pair (ζ, ωₙ) plus a fast real pole. Matching the three coefficients gives three linear equations for the proportional, integral and derivative gains.",
    note: "For ζ = 0.7, ωₙ = 10 rad/s and a pole at −50, the gains are Kc = 200, Ki = 1250, Kd = 15.5; the dominant pair gives 4.6% overshoot and a 0.57 s settling time. Each matched coefficient checks out exactly.",
    code: `{ Controls: PID design by pole placement for plant G(s)=K/(s(tau s+1)) }
Kp_plant = 2.0      { plant DC gain }
tau = 0.5 [s]
{ Desired poles: a complex pair (zeta, wn) + a fast real pole at -p }
zeta = 0.7
wn = 10 [rad/s]
p = 50 [rad/s]
{ Closed-loop char. poly  s^3 + ((1+Kp_plant*Kd)/tau) s^2
     + (Kp_plant*Kc/tau) s + Kp_plant*Ki/tau
   matched to (s+p)(s^2 + 2 zeta wn s + wn^2) }
(1 + Kp_plant*Kd)/tau = p + 2*zeta*wn
Kp_plant*Kc/tau       = wn^2 + 2*zeta*wn*p
Kp_plant*Ki/tau       = p*wn^2
ts = 4/(zeta*wn)                              { settling time }
Mp = exp(-pi#*zeta/sqrt(1-zeta^2))*100         { percent overshoot }`,
  },
  {
    value: "partial-fraction-real",
    title: "Control Systems: Partial-Fraction Expansion (Real Poles)",
    description: "Inverse Laplace transform by partial fractions. Declaring s as SYMBOLIC turns the next equation into an identity that must hold for all s; frees matches coefficients and solves for the residues, which appear in the Solution window like any other variable. tf(num, den) builds the rational function from descending-power coefficient arrays.",
    note: "32/[s(s+4)(s+8)] expands to K1/s + K2/(s+4) + K3/(s+8) with K1 = 1, K2 = −2, K3 = 1 — i.e. the time response y(t) = 1 − 2e^(−4t) + e^(−8t).",
    code: `{ Partial-fraction expansion of Y(s) = 32 / [s(s+4)(s+8)] }
{ den = s(s+4)(s+8) = s^3 + 12 s^2 + 32 s  ->  [1, 12, 32, 0] }
SYMBOLIC s
tf([32], [1, 12, 32, 0]) = K1/s + K2/(s+4) + K3/(s+8)`,
  },
  {
    value: "partial-fraction-complex",
    title: "Control Systems: Partial Fractions with a Complex Pole Pair",
    description: "When the denominator has a complex (oscillatory) root pair, the corresponding partial-fraction term keeps a first-order numerator (K2·s + K3) over the quadratic factor. The SYMBOLIC identity solves for all three residues at once.",
    note: "3/[s(s²+2s+5)] = K1/s + (K2·s + K3)/(s²+2s+5) gives K1 = 0.6, K2 = −0.6, K3 = −1.2.",
    code: `{ Partial fractions with a complex pole pair }
{ den = s(s^2 + 2s + 5) = s^3 + 2 s^2 + 5 s  ->  [1, 2, 5, 0] }
SYMBOLIC s
tf([3], [1, 2, 5, 0]) = K1/s + (K2*s + K3)/(s^2 + 2*s + 5)`,
  },
  {
    value: "tf-to-ss",
    title: "Control Systems: Transfer Function to State Space",
    description: "ss/tf models are plain array and matrix variables — a transfer function is its num/den coefficient arrays (descending powers). CALL tf2ss returns a controllable-canonical realization (A, B, C, D); any phase-variable form is a similarity transform of it.",
    note: "(s²+7s+2)/(s³+9s²+26s+24) realizes as A = [[−9,−26,−24],[1,0,0],[0,1,0]], B = [1,0,0], C = [1,7,2], D = 0. Converting it straight back with ss2tf recovers the original coefficients exactly.",
    code: `{ Transfer function -> state space (controllable canonical form) }
num = [0, 1, 7, 2]
den = [1, 9, 26, 24]
CALL tf2ss(num[1:4], den[1:4] : A[1:3,1:3], B[1:3], C[1:3], D)`,
  },
  {
    value: "ss-to-tf",
    title: "Control Systems: State Space to Transfer Function",
    description: "The inverse conversion: build the A, B, C, D matrices entry by entry, then CALL ss2tf for the equivalent transfer-function coefficient arrays. The solver registers the output array shapes so num and den are usable downstream as bare names.",
    note: "The phase-variable system below converts to T(s) = 10(s²+3s+2)/(s³+3s²+2s+1), i.e. num = [0, 10, 30, 20] and den = [1, 3, 2, 1].",
    code: `{ State space -> transfer function }
A[1,1]=0; A[1,2]=1; A[1,3]=0
A[2,1]=0; A[2,2]=0; A[2,3]=1
A[3,1]=-1; A[3,2]=-2; A[3,3]=-3
B[1]=10; B[2]=0; B[3]=0
C[1]=1; C[2]=0; C[3]=0
D=0
CALL ss2tf(A[1:3,1:3], B[1:3], C[1:3], D : num[1:4], den[1:4])`,
  },
  {
    value: "first-order-poles-zeros",
    title: "Control Systems: Poles & Zeros of a First-Order System",
    description: "pole and zero return the real/imaginary parts of a system's poles and zeros; tf2zp additionally factors the system into zero–pole–gain form. Poles in the left half-plane mean a stable system.",
    note: "G(s) = (s+2)/(s+5) has a single zero at s = −2 and a single pole at s = −5; tf2zp returns the same factors with gain k = 1.",
    code: `{ Poles, zeros and zero-pole-gain factoring of G(s) = (s+2)/(s+5) }
num = [1, 2]
den = [1, 5]
[pr, pi] = pole(num[1:2], den[1:2])
CALL zero(num[1:2], den[1:2] : zr[1:1], zi[1:1])
CALL tf2zp(num[1:2], den[1:2] : zzr[1:1], zzi[1:1], ppr[1:1], ppi[1:1], k)`,
  },
  {
    value: "underdamped-poles",
    title: "Control Systems: Complex Poles of an Underdamped System",
    description: "A second-order system with a complex-conjugate pole pair. The real part sets the decay rate and the imaginary part the damped oscillation frequency.",
    note: "G(s) = 200/(s²+10s+200) has poles at s = −5 ± j13.23 (natural frequency ≈ 14.1 rad/s, damping ratio ≈ 0.354).",
    code: `{ Complex poles of an underdamped second-order system }
num = [0, 0, 200]
den = [1, 10, 200]
CALL pole(num[1:3], den[1:3] : pr[1:2], pi[1:2])`,
  },
  {
    value: "thirdorder-poles-zeros",
    title: "Control Systems: Poles & Zeros of a Third-Order System",
    description: "pole and zero handle mixed real-and-complex root sets. Here a real pole, a complex pole pair, and a single finite zero are all extracted from one transfer function.",
    note: "G(s) = (s+2)/[(s+3)(s²+2s+2)] returns poles at s = −3 and s = −1 ± j1, with a zero at s = −2.",
    code: `{ Poles and zeros of a third-order system with a complex pole pair }
{ den = (s+3)(s^2+2s+2) = s^3 + 5 s^2 + 8 s + 6 }
num = [0, 0, 1, 2]
den = [1, 5, 8, 6]
[pr, pi] = pole(num[1:4], den[1:4])
CALL zero(num[1:4], den[1:4] : zr[1:1], zi[1:1])`,
  },
  {
    value: "zpk-feedback",
    title: "Control Systems: Zero-Pole-Gain to TF and Unity Feedback",
    description: "Build a forward-path transfer function from its zeros, poles and gain with zp2tf, then close the loop with unity feedback. feedback forms T(s) = G/(1 + G·H); the result's array length is L1 + L2 − 1.",
    note: "Forward loop G(s) = (s+3)(s+4)/[(s+1)(s+2)] with unity feedback gives T(s) = (s²+7s+12)/(2s²+10s+14), whose closed-loop poles are s = −2.5 ± j0.866.",
    code: `{ Zero-pole-gain -> transfer function, then close with unity feedback }
zr = [-3, -4]
zi = [0, 0]
pr = [-1, -2]
pi = [0, 0]
k = 1
[numG, denG] = zp2tf(zr[1:2], zi[1:2], pr[1:2], pi[1:2], k)
numH = [1]
denH = [1]
[numT, denT] = feedback(numG[1:3], denG[1:3], numH[1:1], denH[1:1])
CALL pole(numT[1:3], denT[1:3] : tpr[1:2], tpi[1:2])`,
  },
  {
    value: "parallel-interconnect",
    title: "Control Systems: Parallel Interconnection of Blocks",
    description: "parallel adds two transfer functions, G(s) = G1(s) + G2(s) (series multiplies them, feedback closes a loop). Here the three first-order terms of a partial-fraction expansion are recombined to reconstruct the original system.",
    note: "Adding 1/s, −2/(s+4) and 1/(s+8) reproduces 32/[s³+12s²+32s] — exactly the transfer function whose partial fractions were 1, −2, 1, closing the loop on the first example.",
    code: `{ Parallel sum of partial-fraction terms reconstructs the original G(s) }
n1 = [0, 1]
d1 = [1, 0]
n2 = [0, -2]
d2 = [1, 4]
[n12, d12] = parallel(n1[1:2], d1[1:2], n2[1:2], d2[1:2])
n3 = [0, 1]
d3 = [1, 8]
CALL parallel(n12[1:3], d12[1:3], n3[1:2], d3[1:2] : nsum[1:4], dsum[1:4])`,
  },
  {
    value: "second-order-step",
    title: "Control Systems: Second-Order Step Response & Metrics",
    description: "step integrates the unit step response on a supplied time vector (it routes through the same ODE engine as DYNAMIC blocks, so y plots directly with the xy plot kind). Alongside it the closed-form second-order metrics — natural frequency, damping ratio, peak time, percent overshoot and settling time — are computed from the coefficients.",
    note: "G(s) = 100/(s²+15s+100): ωn = 10 rad/s, ζ = 0.75, peak time Tp = 0.475 s, percent overshoot %OS = 2.84%, settling time Ts = 0.533 s.",
    code: `{ Second-order step response and the standard performance metrics }
num = [0, 0, 100]
den = [1, 15, 100]
wn = sqrt(100)
zeta = 15 / (2*wn)
Tp = pi# / (wn*sqrt(1 - zeta^2))
OS = 100 * exp(-zeta*pi#/sqrt(1 - zeta^2))
Ts = 4 / (zeta*wn)
t = 0:0.02:1
N = 51
[y] = step(num[1:3], den[1:3], t[1:N])

PLOT 'Step Response'
  kind = xy
  x = t
  y = y
  xlabel = Time [s]
  ylabel = Amplitude
END`,
  },
  {
    value: "impulse-response",
    title: "Control Systems: Impulse & Step Response",
    description: "impulse returns the impulse response y(t) = C·e^(At)·B on the supplied time grid; here it is plotted next to the step response of the same plant for comparison.",
    note: "450/[(s+5)(s+20)] = 450/(s²+25s+100) is overdamped — both responses settle smoothly with no overshoot. The step response saturates at the DC gain G(0) = 450/100 = 4.5, not 1.",
    code: `{ Impulse and step response of an overdamped second-order system }
num = [0, 0, 450]
den = [1, 25, 100]
t = 0:0.02:1
N = 51
[y_imp] = impulse(num[1:3], den[1:3], t[1:N])
[y_step] = step(num[1:3], den[1:3], t[1:N])

PLOT 'Impulse vs Step'
  kind = xy
  x = t
  y = y_imp
  xlabel = Time [s]
  ylabel = Amplitude
END`,
  },
  {
    value: "forced-response-lsim",
    title: "Control Systems: Forced Response to an Arbitrary Input (lsim)",
    description: "lsim simulates the response to any input signal u(t), linearly interpolated between samples (u and t must be the same length). Here the underdamped second-order system is driven by a unit ramp.",
    note: "Driving 100/(s²+15s+100) (ωn = 10, ζ = 0.75) with a unit ramp u(t) = t; the output tracks the ramp with the expected lag.",
    code: `{ Forced (lsim) response to a unit-ramp input }
num = [0, 0, 100]
den = [1, 15, 100]
t = 0:0.02:1
N = 51
u = 0:0.02:1
[y] = lsim(num[1:3], den[1:3], u[1:N], t[1:N])

PLOT 'Ramp Response'
  kind = xy
  x = t
  y = y
  xlabel = Time [s]
  ylabel = Output
END`,
  },
  {
    value: "bode-response",
    title: "Control Systems: Bode Frequency Response",
    description: "bode evaluates magnitude (dB) and unwrapped phase (degrees) at a vector of frequencies; the bode plot kind draws the stacked magnitude/phase diagram. Build a logarithmic frequency sweep with the start:count:end | Log range form.",
    note: "G(s) = (s+3)/[(s+2)(s²+2s+25)] has DC gain 3/50, so the low-frequency magnitude sits at 20·log10(0.06) = −24.44 dB, with break frequencies near 2, 3 and 5 rad/s.",
    code: `{ Bode response of a first-over-second-order system }
num = [0, 0, 1, 3]
den = [1, 4, 29, 50]
G0 = 3/50
dB0 = 20*log10(G0)        { low-frequency magnitude, dB }
Nw = 50
omega = 0.1:50:100 | Log
[mag, phase] = bode(num[1:4], den[1:4], omega[1:Nw])

PLOT 'Bode Diagram'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END`,
  },
  {
    value: "gain-phase-margin",
    title: "Control Systems: Gain & Phase Margins",
    description: "margin returns the gain margin (dB), phase margin (degrees) and the two crossover frequencies for an open-loop transfer function. The loop here is an antenna-azimuth position servo built up with series from a power amplifier and a motor/load block, scaled by a preamplifier gain.",
    note: "With preamplifier gain 20 the open loop has a 32.4 dB gain margin and a 43.9° phase margin (gain crossover 1.72 rad/s, phase crossover 13.1 rad/s) — a comfortably stable design.",
    code: `{ Gain and phase margins of an antenna-azimuth position servo }
Kpre = 20
num_pa = [0, 100*Kpre*0.2083]   { preamp * power amp * motor gain }
den_pa = [1, 100]
num_mo = [0, 0, 1]
den_mo = [1, 1.71, 0]
[num_ol, den_ol] = series(num_pa[1:2], den_pa[1:2], num_mo[1:3], den_mo[1:3])
[gm, pm, w_cg, w_cp] = margin(num_ol[1:4], den_ol[1:4])
Nw = 50
omega = 0.01:50:100 | Log
[mag, phase] = bode(num_ol[1:4], den_ol[1:4], omega[1:Nw])

PLOT 'Open-Loop Bode'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END`,
  },
  {
    value: "nyquist-stability",
    title: "Control Systems: Nyquist Diagram & Stability",
    description: "nyquist evaluates the real and imaginary parts of the open-loop frequency response; the nyquist plot kind draws the polar plot with the −1 + j0 critical point marked. margin quantifies how far the locus stays from that point.",
    note: "G(s) = 50/[s(s+3)(s+6)] has a 10.2 dB gain margin and a 35.0° phase margin — a stable but moderately damped loop.",
    code: `{ Nyquist diagram and stability margins of a type-1 loop }
num = [0, 0, 0, 50]
den = [1, 9, 18, 0]
Nw = 50
omega = 0.1:50:100 | Log
[re, im] = nyquist(num[1:4], den[1:4], omega[1:Nw])
[gm, pm, w_cg, w_cp] = margin(num[1:4], den[1:4])

PLOT 'Nyquist Diagram'
  kind = nyquist
  real = re
  imag = im
END`,
  },
  {
    value: "closed-loop-position-servo",
    title: "Control Systems: Closed-Loop Position Servo (series + feedback)",
    description: "A complete block-diagram reduction: cascade a power amplifier and a motor/load block with series, then close the loop with unity feedback. pole confirms the closed-loop stability and step shows the transient.",
    note: "Antenna-azimuth plant (preamp 50 · 100/(s+100) · 0.2083/[s(s+1.71)]) under unity feedback has stable closed-loop poles at −100.1 and −0.80 ± j3.12, giving a lightly damped step response.",
    code: `{ Closed-loop position servo: cascade then feed back }
Kpre = 50
num_pa = [0, 100*Kpre]
den_pa = [1, 100]
num_mo = [0, 0, 0.2083]
den_mo = [1, 1.71, 0]
[num_g, den_g] = series(num_pa[1:2], den_pa[1:2], num_mo[1:3], den_mo[1:3])
num_h = [1]
den_h = [1]
[num_cl, den_cl] = feedback(num_g[1:4], den_g[1:4], num_h[1:1], den_h[1:1])
[cpr, cpi] = pole(num_cl[1:4], den_cl[1:4])
t = 0:0.05:3
N = 61
[y] = step(num_cl[1:4], den_cl[1:4], t[1:N])

PLOT 'Closed-Loop Step'
  kind = xy
  x = t
  y = y
  xlabel = Time [s]
  ylabel = Position
END`,
  },
  {
    value: "pid-auto-tune",
    title: "Control Systems: PID Auto-Tuning by Loop Shaping",
    description: "pidtune sizes a P, PI or PID controller (quoted type) so the open loop crosses over at the target frequency wc with a 60° phase-margin objective. The tuned controller is then assembled as (Kd·s² + Kp·s + Ki)/s, cascaded with the plant via series, and verified with margin.",
    note: "For plant (s+8)/[(s+3)(s+6)(s+10)] at wc = 5 rad/s the gains are Kp ≈ 48.3, Ki ≈ 195.3, Kd ≈ 2.98; the closed verification confirms a 60.0° phase margin at exactly 5 rad/s.",
    code: `{ PID auto-tuning for plant G(s) = (s+8) / [(s+3)(s+6)(s+10)] }
num = [0, 0, 1, 8]
den = [1, 19, 108, 180]
wc = 5 [rad/s]
[Kp, Ki, Kd] = pidtune(num[1:4], den[1:4], 'PID', wc)
{ Assemble C(s) = Kp + Ki/s + Kd s = (Kd s^2 + Kp s + Ki)/s and check the loop }
num_c = [Kd, Kp, Ki]
den_c = [0, 1, 0]
[num_ol, den_ol] = series(num_c[1:3], den_c[1:3], num[1:4], den[1:4])
CALL margin(num_ol[1:6], den_ol[1:6] : gm, pm, w_cg, w_cp)`,
  },
  {
    value: "pole-placement",
    title: "Control Systems: Pole Placement by State Feedback",
    description: "place computes the state-feedback gain K (Ackermann's formula) that relocates the eigenvalues of A − B·K to the requested locations, supplied as real/imaginary arrays (complex poles in conjugate pairs). The closed-loop matrix is rebuilt and pole confirms the placement.",
    note: "Relocating the phase-variable plant (den s³+9s²+26s+24) to poles −10 and −2 ± j2 gives K = [56, 22, 5]; the closed-loop A − B·K poles land exactly on the targets.",
    code: `{ State-feedback pole placement via Ackermann's formula }
A[1,1]=0; A[1,2]=1; A[1,3]=0
A[2,1]=0; A[2,2]=0; A[2,3]=1
A[3,1]=-24; A[3,2]=-26; A[3,3]=-9
B[1]=0; B[2]=0; B[3]=1
{ Desired closed-loop poles: -10 and -2 +/- j2 }
des_pr = [-2, -2, -10]
des_pi = [2, -2, 0]
[K] = place(A[1:3,1:3], B[1:3], des_pr[1:3], des_pi[1:3])
{ Verify: rebuild Acl = A - B K and read its poles }
Acl[1,1]=A[1,1]-B[1]*K[1]; Acl[1,2]=A[1,2]-B[1]*K[2]; Acl[1,3]=A[1,3]-B[1]*K[3]
Acl[2,1]=A[2,1]-B[2]*K[1]; Acl[2,2]=A[2,2]-B[2]*K[2]; Acl[2,3]=A[2,3]-B[2]*K[3]
Acl[3,1]=A[3,1]-B[3]*K[1]; Acl[3,2]=A[3,2]-B[3]*K[2]; Acl[3,3]=A[3,3]-B[3]*K[3]
CALL pole(Acl[1:3,1:3] : ppr[1:3], ppi[1:3])`,
  },
  {
    value: "lqr-regulator",
    title: "Control Systems: LQR Optimal State-Feedback Regulator",
    description: "lqr returns the state-feedback gain K that minimizes ∫(x'Qx + u'Ru) dt, solving the algebraic Riccati equation via the matrix sign function of the Hamiltonian. The closed-loop A − B·K is guaranteed stable.",
    note: "With Q = I and R = 1 on a third-order plant the optimal gain is K ≈ [1.019, −0.101, −0.191]; the closed-loop poles −9.95, −2.23 and −1.01 are all in the left half-plane.",
    code: `{ LQR optimal regulator: minimize the quadratic cost integral }
A[1,1]=0; A[1,2]=1; A[1,3]=0
A[2,1]=0; A[2,2]=0; A[2,3]=1
A[3,1]=-1; A[3,2]=-2; A[3,3]=-3
B[1]=10; B[2]=0; B[3]=0
Q[1,1]=1; Q[1,2]=0; Q[1,3]=0
Q[2,1]=0; Q[2,2]=1; Q[2,3]=0
Q[3,1]=0; Q[3,2]=0; Q[3,3]=1
R = 1
[K] = lqr(A[1:3,1:3], B[1:3], Q[1:3,1:3], R)
{ Verify the closed-loop poles are stable }
Acl[1,1]=A[1,1]-B[1]*K[1]; Acl[1,2]=A[1,2]-B[1]*K[2]; Acl[1,3]=A[1,3]-B[1]*K[3]
Acl[2,1]=A[2,1]-B[2]*K[1]; Acl[2,2]=A[2,2]-B[2]*K[2]; Acl[2,3]=A[2,3]-B[2]*K[3]
Acl[3,1]=A[3,1]-B[3]*K[1]; Acl[3,2]=A[3,2]-B[3]*K[2]; Acl[3,3]=A[3,3]-B[3]*K[3]
CALL pole(Acl[1:3,1:3] : ppr[1:3], ppi[1:3])`,
  },
  {
    value: "control-analysis-report",
    title: "Control Systems: End-to-End Analysis Report",
    description: "A single document that walks through a complete plant analysis — poles/zeros, stability margins, Bode, Nyquist and step response — with the narrative in comments and each figure declared by a named PLOT block. Solve, then open the Plots window.",
    note: "Analyzes G(s) = (s+3)/[(s+2)(s²+2s+25)]: all poles are left-half-plane (stable), and the lightly damped pole pair produces a pronounced, slowly decaying step response.",
    code: `{ Control System Analysis Report

  This document analyzes the plant G(s) end to end: poles and zeros, gain and
  phase margins, frequency response (Bode and Nyquist), and the unit step
  response. Press Solve (F2), then open the Plots window — each PLOT block
  below declares a named figure. }

{ 1. Plant model — the numerator and denominator coefficients (descending
  powers of s) define a third-order plant with one real zero, one real pole,
  and a lightly damped second-order pole pair. }
num = [0, 0, 1, 3]
den = [1, 4, 29, 50]

{ 2. Poles, zeros and stability margins — all three poles lie in the left
  half-plane, so the open-loop plant is stable. }
[pr, pi] = pole(num[1:4], den[1:4])
CALL zero(num[1:4], den[1:4] : zr[1:1], zi[1:1])
[gm, pm, w_cg, w_cp] = margin(num[1:4], den[1:4])

{ 3. Frequency response — sweep 50 logarithmically spaced frequencies, then
  evaluate the Bode and Nyquist responses. }
Nw = 50
omega = 0.1:50:100 | Log
[mag, phase] = bode(num[1:4], den[1:4], omega[1:Nw])
[re, im] = nyquist(num[1:4], den[1:4], omega[1:Nw])

{ 4. Time-domain step response — the lightly damped pole pair produces a
  pronounced, slowly decaying oscillation over 6 seconds. }
Nt = 121
t = 0:0.05:6
[y] = step(num[1:4], den[1:4], t[1:Nt])

PLOT 'Pole-Zero Map'
  kind = polezero
  pr = pr
  pi = pi
  zr = zr
  zi = zi
END

PLOT 'Bode Diagram'
  kind = bode
  omega = omega
  mag = mag
  phase = phase
END

PLOT 'Nyquist Diagram'
  kind = nyquist
  real = re
  imag = im
END

PLOT 'Step Response'
  kind = xy
  x = t
  y = y
  xlabel = Time [s]
  ylabel = Amplitude
END`,
  },
  {
    value: "inhour-equation",
    title: "Nuclear Engineering: Stable Reactor Period from the Inhour Equation",
    description: "A step reactivity insertion in a reactor with six delayed-neutron groups. The inhour equation relating reactivity to the stable period is transcendental with all six groups contributing; the β and λ data are entered as arrays and summed.",
    note: "Inserting 0.0025 Δk/k (well below the 0.0065 delayed-neutron fraction) gives a stable, positive reactor period of about 10.9 s — a controllable transient. Above β the period would collapse to prompt-critical. Set a guess Tper ≈ 30 s.",
    code: `{ Nuclear: stable reactor period from the 6-group inhour equation (U-235) }
Lambda = 2e-5 [s]            { prompt neutron generation time }
rho = 0.0025                 { inserted reactivity (dk/k) }
beta[1:6] = [0.000215, 0.001424, 0.001274, 0.002568, 0.000748, 0.000273]
lam[1:6]  = [0.0124, 0.0305, 0.111, 0.301, 1.14, 3.01]
{ Stable period Tper (guess ~30 s); contributions of the 6 groups }
FOR i = 1 TO 6
  term[i] = beta[i]/(1 + lam[i]*Tper)
END
rho = Lambda/Tper + sum(term[1:6])
beta_tot = sum(beta[1:6])`,
  },
  {
    value: "paris-fatigue",
    title: "Materials Engineering: Fatigue Life by Integrating the Paris Law",
    description: "An edge-cracked plate under cyclic stress. The critical crack length comes from the fracture toughness; the cycles to failure are the Paris-law crack-growth rate integrated from the initial to the critical crack length — a definite integral with the unknown (critical length) as its upper limit.",
    note: "The crack grows from 0.5 mm to a critical 10.15 mm, giving a fatigue life of about 161,000 cycles. frees evaluates the crack-growth integral directly with the critical length solved from the toughness in the same system.",
    code: `{ Materials: fatigue life of an edge-cracked plate via the Paris law
  integrated from initial to critical crack length (consistent MPa, m units) }
C = 6.9e-12; m = 3.0       { Paris constants (m/cycle, MPa*sqrt(m)) }
Y = 1.12                   { edge-crack geometry factor }
K_IC = 60                  { fracture toughness, MPa*sqrt(m) }
sig_max = 300              { max stress, MPa }
dsig = 200                 { stress range, MPa }
a_i = 0.0005               { initial crack length, m }
a_c = (K_IC/(sig_max*Y))^2/pi#      { critical crack length }
{ Cycles to failure = integral of da / (C (dsig Y sqrt(pi# a))^m) }
N_f = Integral(1/(C*(dsig*Y*sqrt(pi#*a))^m), a, a_i, a_c)`,
  },
  {
    value: "ammonia-refrigeration",
    title: "HVAC & Refrigeration: Ammonia Refrigeration Cycle COP",
    description: "An ammonia (R-717) chiller with a flooded evaporator operates between suction pressure 38.5 psia (with 20°F superheat) and discharge pressure 229 psia. The COP is computed from states' enthalpies.",
    note: "Results: COP = 3.9, cooling load = 10,250 Btu/min, compressor power = 2,594 Btu/min.",
    code: `{ Problem 1: Ammonia Refrigeration Cycle COP }
P_suction = 38.5 [psia]
superheat = 20 [F]
P_discharge = 229 [psia]
m_dot = 22 [lb/min]

h1 = 627.0 [Btu/lbm]      { Enthalpy entering compressor }
h2 = 745.0 [Btu/lbm]      { Enthalpy leaving compressor }
h3 = 161.1 [Btu/lbm]      { Saturated liquid leaving condenser }

COP = (h1 - h3) / (h2 - h1)
Q_dot_cool = m_dot * (h1 - h3)
W_dot_comp = m_dot * (h2 - h1)`,
  },
  {
    value: "face-bypass-control",
    title: "HVAC & Refrigeration: Face and Bypass Control Load",
    description: "Face and bypass control maintains room air at 80°F db/50% rh. A mixed stream of outdoor air and return air is cooled by a chilled-water coil. The total refrigeration load is determined from psychrometric property functions.",
    note: "Results: mixed enthalpy = 32.8 Btu/lb, supply enthalpy = 23.8 Btu/lb, total load = 67.9 tons of refrigeration.",
    code: `{ Problem 2: Face and Bypass Control Load }
V_dot_supply = 20000 [cfm]
V_dot_oa = 5000 [cfm]
T_room = 80 [F]
rh_room = 0.50
T_oa_db = 90 [F]
T_oa_wb = 74 [F]
T_coil_out_db = 58 [F]
T_coil_out_wb = 56 [F]
P_atm = 14.696 [psia]

v_supply = 13.25 [ft^3/lb] { Specific volume of supply air }
m_dot_supply = V_dot_supply * 60 / v_supply

f_oa = V_dot_oa / V_dot_supply
f_return = 1 - f_oa

h_room = Enthalpy(AirH2O, T=T_room, R=rh_room, P=P_atm)
h_oa = Enthalpy(AirH2O, T=T_oa_db, B=T_oa_wb, P=P_atm)
h_mix = f_oa * h_oa + f_return * h_room
h_supply = Enthalpy(AirH2O, T=T_coil_out_db, B=T_coil_out_wb, P=P_atm)

Q_dot_coil_btu = m_dot_supply * (h_mix - h_supply)
Q_dot_coil_tons = Q_dot_coil_btu / 12000`,
  },
  {
    value: "solar-heat-gain",
    title: "HVAC & Refrigeration: Solar Heat Gain Through Windows",
    description: "Calculates total heat gain through windows on North, East, and West faces of a building using U-value, Cooling Load Temperature Differences (CLTD), and Solar Heat Gain Factors (SHGF).",
    note: "Results: total heat gain = 21,720 Btu/hr.",
    code: `{ Problem 4: Solar Heat Gain Through Windows }
U_value = 1.1 [Btu/hr-ft^2-F]
T_in = 75 [F]
T_out = 95 [F]
A_window = 40 [ft^2]

SHGF_North = 47 [Btu/hr-ft^2]
SHGF_East = 215 [Btu/hr-ft^2]
SHGF_West = 215 [Btu/hr-ft^2]

Q_North = A_window * SHGF_North + U_value * A_window * (T_out - T_in)
Q_East = A_window * SHGF_East + U_value * A_window * (T_out - T_in)
Q_West = A_window * SHGF_West + U_value * A_window * (T_out - T_in)

Q_total = Q_North + Q_East + Q_West`,
  },
  {
    value: "enthalpy-wheel",
    title: "HVAC & Refrigeration: Enthalpy Wheel Heat Recovery",
    description: "Finds the dry-bulb temperature of tempered air leaving an 80% effective sensible/latent heat recovery enthalpy wheel.",
    note: "Results: leaving dry-bulb temperature = 79°F.",
    code: `{ Problem 5: Enthalpy Wheel Heat Recovery }
V_dot_oa = 1500 [cfm]
T_oa_db = 95 [F]
T_oa_wb = 78 [F]
T_room_db = 75 [F]
rh_room = 0.50
effectiveness = 0.80

T_tempered_db = T_oa_db - (T_oa_db - T_room_db) * effectiveness`,
  },
  {
    value: "run-around-cycle",
    title: "HVAC & Refrigeration: Run-Around Water Cycle Balance",
    description: "Determines leaving air temperature from a cooling coil coupled to a run-around loop water cycle under steady-state energy balance.",
    note: "Results: heat transfer rate = 75,000 Btu/hr, air temp difference = 13.6°F, leaving air temperature = 61.4°F (or 52°F depending on heating balancing).",
    code: `{ Problem 6: Run-Around Water Cycle }
gpm = 15 [gpm]
delta_T_water = 10 [F]
T_air_in = 75 [F]
cfm = 5000 [cfm]

{ Heat transfer rate }
Q = 500 * gpm * delta_T_water
Q = 1.1 * cfm * delta_T_air
T_air_out = T_air_in - delta_T_air`,
  },
  {
    value: "latent-heat-freezing",
    title: "HVAC & Refrigeration: Specific Heat of Freezing",
    description: "Calculates the cooling required to cool 10,000 lbs of frozen chicken from its freezing point (27°F) to storage temperature (-10°F).",
    note: "Results: cooling required = 136,900 Btu.",
    code: `{ Problem 7: Specific & Latent Heat of Freezing }
mass = 10000 [lb]
T_freeze = 27 [F]
T_storage = -10 [F]
Cp_below = 0.37 [Btu/lb-F]

Q_required = mass * Cp_below * (T_freeze - T_storage)`,
  },


  {
    value: "psychrometric-balancing",
    title: "HVAC & Refrigeration: Psychrometric Room Balancing",
    description: "Calculates entering and leaving air conditions for a cooling coil serving a space with both sensible and latent heat gains under outdoor ventilation requirements.",
    note: "Results: Mixed Air Temp = 80.7°F db / 66.2°F wb, Leaving Air Temp = 55.0°F db / 51.1°F wb.",
    code: `{ Problem 3: Psychrometric Room Balancing }
{ AirH2O properties use SI internally: convert °F→K and psia→Pa,
  then divide enthalpy by 2326 to work in Btu/lb throughout. }
Q_sensible = 90000         { Btu/hr }
Q_latent = 40000           { Btu/hr }
V_dot_supply = 3600        { cfm }
T_supply_db = 55           { °F }
T_room_db = 78             { °F }
rh_room = 0.45
T_oa_db = 92               { °F }
T_oa_wb = 76               { °F }
V_dot_oa = 700             { cfm }
P_atm = 14.696 * 6894.76   { psia → Pa }

V_dot_return = V_dot_supply - V_dot_oa
f_oa = V_dot_oa / V_dot_supply
f_return = V_dot_return / V_dot_supply

{ Convert to Kelvin for CoolProp }
T_room_K = (T_room_db - 32) * 5/9 + 273.15
T_oa_K   = (T_oa_db   - 32) * 5/9 + 273.15
T_oa_wb_K = (T_oa_wb  - 32) * 5/9 + 273.15

{ Mixed air condition (MAT) }
T_entering_db = f_oa * T_oa_db + f_return * T_room_db
T_entering_K  = (T_entering_db - 32) * 5/9 + 273.15

h_room = Enthalpy(AirH2O, T=T_room_K, R=rh_room,  P=P_atm) / 2326
h_oa   = Enthalpy(AirH2O, T=T_oa_K,   B=T_oa_wb_K, P=P_atm) / 2326
h_mix  = f_oa * h_oa + f_return * h_room

T_entering_wb_K = WetBulb(AirH2O, T=T_entering_K, H=h_mix * 2326, P=P_atm)
T_entering_wb   = (T_entering_wb_K - 273.15) * 9/5 + 32

{ Room total load: sensible + latent + ventilation load }
Q_total = Q_sensible + Q_latent + V_dot_oa * 4.5 * (h_oa - h_room)
h_leaving = h_mix - Q_total / (4.5 * V_dot_supply)

T_supply_K    = (T_supply_db - 32) * 5/9 + 273.15
T_leaving_wb_K = WetBulb(AirH2O, T=T_supply_K, H=h_leaving * 2326, P=P_atm)
T_leaving_wb   = (T_leaving_wb_K - 273.15) * 9/5 + 32`,
  },
  {
    value: "pumping-friction-head",
    title: "HVAC & Refrigeration: Pumping and Friction Head",
    description: "Computes the operating head of a water pump overcoming static elevation and pipe friction (Darcy-Weisbach friction equation) under pressurized inlet conditions.",
    note: "Results: velocity = 8.54 ft/s, friction head = 9.3 ft, total head required = 169.3 ft, inlet head = 46.2 ft, pump head = 123.1 ft.",
    code: `{ Problem 8: Pumping and Friction Head }
{ All quantities are in English units (ft, gpm, fps) but kept unitless
  to avoid placing unit annotations on computed expressions. }
T_water = 90
gpm = 26000
f_factor = 0.01
L_eq = 2425
z_elev = 160
P_inlet = 20
OD = 36
t_wall = 0.375

ID = (OD - 2 * t_wall) / 12
V = (gpm * 0.1337 / 60) / (pi# / 4 * ID^2)
h_friction = f_factor * (L_eq / ID) * (V^2 / 64.4)
h_total = z_elev + h_friction

h_inlet = P_inlet * 2.31
h_pump = h_total - h_inlet`,
  },
  {
    value: "air-supply-wetbulb",
    title: "HVAC & Refrigeration: Air Supply Wet-Bulb Determination",
    description: "Finds the wet-bulb temperature of the supply air to maintain a room at 75°F db/63°F wb under sensible load and Sensible Heat Factor.",
    note: "Results: total heat load = 250,000 Btu/hr, supply wet-bulb temperature = 55.0°F — enthalpies are on CoolProp's datum, which differs from the standard psychrometric chart by a constant that cancels in the load equation's difference.",
    code: `{ Problem 9: Air Supply Wet-Bulb Determination }
{ AirH2O needs SI: convert °F→K and psia→Pa; Enthalpy returns J/kg,
  divide by 2326 to get Btu/lb; multiply back by 2326 for H= input. }
T_room_db = 75             { °F }
T_room_wb = 63             { °F }
T_supply_db = 58           { °F }
Q_sensible = 200000        { Btu/hr }
SHF = 0.80
V_dot_supply = 10700       { cfm }
P_atm = 14.696 * 6894.76   { psia → Pa }

T_room_db_K   = (T_room_db   - 32) * 5/9 + 273.15
T_room_wb_K   = (T_room_wb   - 32) * 5/9 + 273.15
T_supply_db_K = (T_supply_db - 32) * 5/9 + 273.15

h_room = Enthalpy(AirH2O, T=T_room_db_K, B=T_room_wb_K, P=P_atm) / 2326
Q_total = Q_sensible / SHF

{ Total heat equation — h_supply solved implicitly }
Q_total = 4.5 * V_dot_supply * (h_room - h_supply)
T_supply_wb_K = WetBulb(AirH2O, T=T_supply_db_K, H=h_supply * 2326, P=P_atm)
T_supply_wb   = (T_supply_wb_K - 273.15) * 9/5 + 32`,
  },
  {
    value: "multistage-food-freezing",
    title: "HVAC & Refrigeration: Multi-Stage Food Freezing",
    description: "Calculates total refrigeration required to cool lean ham from 40°F to 28°F, freeze it, and then subcool it to 0°F.",
    note: "Results: cooling above = 99,600 Btu, freezing = 980,000 Btu, cooling below = 148,400 Btu, total = 1.228 x 10^6 Btu.",
    code: `{ Problem 10: Multi-Stage Food Freezing }
mass = 10000 [lb]
T_in = 40 [F]
T_freeze = 28 [F]
T_out = 0 [F]
Cp_above = 0.83 [Btu/lb-F]
Cp_below = 0.53 [Btu/lb-F]
L_fusion = 98 [Btu/lb]

Q_sensible_above = mass * Cp_above * (T_in - T_freeze)
Q_latent_freeze = mass * L_fusion
Q_sensible_below = mass * Cp_below * (T_freeze - T_out)

Q_total_btu = Q_sensible_above + Q_latent_freeze + Q_sensible_below
Q_total_millions = Q_total_btu / 1e6`,
  },
  {
    value: "siyavula-correlation",
    title: "Statistics: Linear Correlation (Siyavula Grade 12 Table & Curve Fit)",
    description: "Defines a bivariate dataset from Siyavula Grade 12 Statistics using an inline TABLE block. When compiled, the table is registered as an internal table and can be selected as the data source in the Curve Fit tool (function icon in the left rail) to find the regression line and correlation coefficient.",
    note: "How-To: 1. Paste this code and press Check (F4). 2. Open the Curve Fit tool on the left rail. 3. Select 'siyavula_data' from the Table dropdown. 4. Select 'x' as independent, 'y' as dependent column. 5. Choose the Linear template and click Fit. R² = 0.9921 yields |r| = sqrt(0.9921) = 0.9961, and since the slope is negative, r = -0.9961.",
    code: `{ Statistics: Linear Correlation (Siyavula Grade 12) }
{ This example defines the Siyavula Grade 12 Statistics bivariate dataset
  using an inline TABLE block. Once compiled (F4), the table is registered
  in the app's internal tables and can be loaded into the Curve Fit engine. }

TABLE siyavula_data(x)
   58   -100
  -81    195
  -94    210
   67   -126
  -13      9
   52   -102
 -100    228
  -11     40
   44    -96
  -54    131
END

{ A dummy equation using the table function to ensure it compiles }
y_test = siyavula_data(0)`,
  },
  // ── Differential Equations (ODE) — 20 worked DYNAMIC problems ──────────────
  // Sourced from standard reference manuals and classic
  // numerical-methods texts, and verified in the backend OdeProblemLibraryTest against closed-form
  // answers. They span every solver: fixed ode1–ode5, adaptive ode45/ode23, and
  // stiff ode23s/ode15s.
  {
    value: "ode-decay",
    title: "Differential Equations (ODE): Radioactive Decay — ode1 (Euler)",
    description: "First-order decay dc/dt = -k·c. Verify: c(20) = 100·e^(-0.175·20) ≈ 3.02.",
    note: "Fixed-step Euler (ode1) — lowest order; many points keep it accurate.",
    code: `{ Radioactive decay — verify c(20) = 100 e^(-0.175*20) = 3.02 }
k = 0.175
DYNAMIC decay (method = ode1, t = 0 .. 20, points = 800)
  der(c) = -k * c
  c(0) = 100
END`,
  },
  {
    value: "ode-rc",
    title: "Differential Equations (ODE): RC Circuit Charging — ode2 (Heun)",
    description: "Capacitor charging de/dt = (es-e)/(RC). Verify: e(0.006) = 10(1-e^(-6)) ≈ 9.975 V.",
    note: "",
    code: `{ RC charging — verify e -> es = 10 V; e(0.006) = 9.975 V }
R = 1000
C = 1e-6
es = 10
DYNAMIC rc (method = ode2, t = 0 .. 0.006, points = 400)
  der(e) = (es - e) / (R * C)
  e(0) = 0
END`,
  },
  {
    value: "ode-reaction",
    title: "Differential Equations (ODE): First-Order Reaction A→B — ode3",
    description: "cA = e^(-k·t), cB = 1 - e^(-k·t). Verify: cA(5) ≈ 0.030, cB(5) ≈ 0.970.",
    note: "",
    code: `{ Irreversible reaction A -> B; cA + cB is conserved = 1 }
k = 0.7
DYNAMIC reaction (method = ode3, t = 0 .. 5, points = 300)
  der(cA) = -k * cA
  der(cB) =  k * cA
  cA(0) = 1
  cB(0) = 0
END`,
  },
  {
    value: "ode-rl",
    title: "Differential Equations (ODE): RL Circuit Decay — ode4 (RK4)",
    description: "L·di/dt + R·i = 0 → i = 0.5·e^(-1.5·t). Verify: i(2) = 0.5·e^(-3) ≈ 0.0249 A.",
    note: "Classic fourth-order Runge–Kutta.",
    code: `{ RL natural response — verify i(2) = 0.5 e^(-3) = 0.0249 A }
L = 1
R = 1.5
DYNAMIC rl (method = ode4, t = 0 .. 2, points = 200)
  der(i) = -(R / L) * i
  i(0) = 0.5
END`,
  },
  {
    value: "ode-cooling",
    title: "Differential Equations (ODE): Newton Cooling — ode5",
    description: "T = Ta + (T0-Ta)·e^(-k·t). Verify: T(30) = 20 + 70·e^(-3) ≈ 23.48 °C.",
    note: "Time is named 'time' because a state T collides with a time variable t.",
    code: `{ Newton cooling — verify T(30) = 20 + 70 e^(-3) = 23.48 }
k = 0.1
Ta = 20
DYNAMIC cooling (method = ode5, time = 0 .. 30, points = 200)
  der(T) = -k * (T - Ta)
  T(0) = 90
END`,
  },
  {
    value: "ode-parachutist",
    title: "Differential Equations (ODE): Falling Parachutist (linear drag) — ode45",
    description: "dv/dt = g - (c/m)·v. Verify: terminal v = g·m/c = 53.44 m/s.",
    note: "Adaptive Dormand–Prince 5(4) — the default method.",
    code: `{ Falling parachutist — verify terminal velocity g*m/c = 53.44 m/s }
g = 9.81
m = 68.1
c = 12.5
DYNAMIC fall (method = ode45, t = 0 .. 30, points = 600, rtol = 1e-8)
  der(v) = g - (c / m) * v
  v(0) = 0
END`,
  },
  {
    value: "ode-tank",
    title: "Differential Equations (ODE): Draining Tank (Torricelli) — ode23",
    description: "dy/dt = -k·√y → y = (√3 - 0.03·t)². Verify: y(20) = (√3-0.6)² ≈ 1.281 m.",
    note: "Adaptive Bogacki–Shampine 3(2).",
    code: `{ Torricelli draining — verify y(20) = (sqrt(3) - 0.6)^2 = 1.281 m }
k = 0.06
DYNAMIC tank (method = ode23, t = 0 .. 20, points = 200, rtol = 1e-8)
  der(y) = -k * sqrt(y)
  y(0) = 3
END`,
  },
  {
    value: "ode-logistic",
    title: "Differential Equations (ODE): Logistic Population Growth — ode45",
    description: "dp/dt = r(1-p/K)p → p = K/(1+3.6967·e^(-r·t)). Verify: p(100) ≈ 9414 million.",
    note: "",
    code: `{ Logistic growth — verify p(100) = 12000/(1 + 3.6967 e^(-2.6)) = 9414 }
r = 0.026
K = 12000
DYNAMIC logistic (method = ode45, t = 0 .. 100, points = 200, rtol = 1e-9)
  der(p) = r * (1 - p / K) * p
  p(0) = 2555
END`,
  },
  {
    value: "ode-quadratic-drag",
    title: "Differential Equations (ODE): Terminal Velocity (quadratic drag) — ode45",
    description: "dv/dt = g - (c/m)·v² → v = v_t·tanh(t·√(gc/m)), v_t = √(g·m/c) ≈ 52.41 m/s.",
    note: "",
    code: `{ Quadratic drag — verify terminal velocity sqrt(g*m/c) = 52.41 m/s }
g = 9.81
m = 70
c = 0.25
DYNAMIC fall2 (method = ode45, t = 0 .. 20, points = 300, rtol = 1e-9)
  der(v) = g - (c / m) * v * v
  v(0) = 0
END`,
  },
  {
    value: "ode-msd",
    title: "Differential Equations (ODE): Mass-Spring-Damper (underdamped) — ode45",
    description: "m·x'' + c·x' + k·x = 0 (two states). Verify: x(5) ≈ 0.066 m (decaying sinusoid).",
    note: "A true two-state ODE; plot x vs time, or v vs x (phase portrait).",
    code: `{ Underdamped vibration — m=20, c=5, k=20. Verify x(5) = 0.066 m }
m = 20
c = 5
k = 20
DYNAMIC vib (method = ode45, t = 0 .. 15, points = 600, rtol = 1e-9)
  der(x) = v
  der(v) = -(c/m) * v - (k/m) * x
  x(0) = 1
  v(0) = 0
END`,
  },
  {
    value: "ode-shm",
    title: "Differential Equations (ODE): Undamped Oscillator (energy check) — ode4",
    description: "x'' + ω²x = 0 → x = cos(ωt). Verify: x(2π) = 1 and energy E = 2 is conserved.",
    note: "Energy E = ½v² + ½ω²x² is an algebraic auxiliary column you can plot.",
    code: `{ Simple harmonic motion — verify x(2pi)=1 and E stays 2 }
w = 2
DYNAMIC shm (method = ode4, t = 0 .. 6.283185307, points = 400)
  der(x) = v
  der(v) = -w*w * x
  E = 0.5 * v*v + 0.5 * w*w * x*x
  x(0) = 1
  v(0) = 0
END`,
  },
  {
    value: "ode-pendulum",
    title: "Differential Equations (ODE): Simple Pendulum (small angle) — ode5",
    description: "θ'' + (g/l)θ = 0 → θ = (π/4)cos(√(g/l)·t). Verify: θ(1) ≈ -0.506 rad.",
    note: "",
    code: `{ Small-angle pendulum — g=32.2 ft/s^2, l=2 ft. Verify theta(1) = -0.506 rad }
g = 32.2
l = 2
DYNAMIC pend (method = ode5, t = 0 .. 3, points = 600)
  der(theta) = omega
  der(omega) = -(g/l) * theta
  theta(0) = pi# / 4
  omega(0) = 0
END`,
  },
  {
    value: "ode-rlc",
    title: "Differential Equations (ODE): Series RLC Circuit (underdamped) — ode23",
    description: "L·q'' + R·q' + q/C = 0. Verify: q(0.05) ≈ 0.153 C (decaying oscillation).",
    note: "Capacitance is named Cap because C is conventionally a constant elsewhere.",
    code: `{ Series RLC — L=5, R=280, C=1e-4. Verify q(0.05) = 0.153 C }
L = 5
R = 280
Cap = 1e-4
DYNAMIC rlc (method = ode23, t = 0 .. 0.2, points = 400, rtol = 1e-9)
  der(q) = i
  der(i) = -(R/L) * i - (1/(L*Cap)) * q
  q(0) = 1
  i(0) = 0
END`,
  },
  {
    value: "ode-lotka",
    title: "Differential Equations (ODE): Lotka–Volterra Predator–Prey — ode45",
    description: "Coupled nonlinear system; the invariant V = d·x - c·ln x + b·y - a·ln y stays constant.",
    note: "No closed-form time solution — verified via the conserved invariant V (an aux column).",
    code: `{ Predator-prey — V = d*x - c*ln(x) + b*y - a*ln(y) is conserved.
  Plot y vs x for the closed phase-plane orbit. }
a = 1.2
b = 0.6
cc = 0.8
d = 0.3
DYNAMIC lv (method = ode45, t = 0 .. 30, points = 600, rtol = 1e-9)
  der(x) = a*x - b*x*y
  der(y) = -cc*y + d*x*y
  V = d*x - cc*ln(x) + b*y - a*ln(y)
  x(0) = 2
  y(0) = 1
END`,
  },
  {
    value: "ode-tanks",
    title: "Differential Equations (ODE): Coupled Mixing Tanks — ode23",
    description: "Two tanks exchanging fluid. Verify: both approach the mean (5), total stays 10.",
    note: "",
    code: `{ Coupled mixing — verify c1,c2 -> 5 and total = c1+c2 stays 10 }
kk = 0.5
DYNAMIC tanks (method = ode23, t = 0 .. 20, points = 200, rtol = 1e-8)
  der(c1) = kk * (c2 - c1)
  der(c2) = kk * (c1 - c2)
  total = c1 + c2
  c1(0) = 10
  c2(0) = 0
END`,
  },
  {
    value: "ode-orbit",
    title: "Differential Equations (ODE): Two-Body Circular Orbit — ode45",
    description: "Four-state gravitational orbit (r=1, μ=1, v=1). Verify: returns to (1,0) at t=2π.",
    note: "Plot y vs x for the circular trajectory; r is an aux column that stays 1.",
    code: `{ Circular orbit — verify x(2pi)=1, y(2pi)=0, radius r stays 1 }
mu = 1
DYNAMIC orbit (method = ode45, t = 0 .. 6.283185307, points = 400, rtol = 1e-10)
  r = sqrt(x*x + y*y)
  der(x) = vx
  der(y) = vy
  der(vx) = -mu * x / r^3
  der(vy) = -mu * y / r^3
  x(0) = 1
  y(0) = 0
  vx(0) = 0
  vy(0) = 1
END`,
  },
  {
    value: "ode-stiff-linear",
    title: "Differential Equations (ODE): Classic Stiff Linear ODE — ode15s",
    description: "y' = -1000y + 3000 - 2000·e^(-t) → y = 3 - 0.998·e^(-1000t) - 2.002·e^(-t). Verify: y(0.4) ≈ 1.658.",
    note: "Stiff: explicit methods need tiny steps; ode15s (implicit BDF) handles it easily.",
    code: `{ Stiff linear ODE — verify y(0.4) = 3 - 2.002 e^(-0.4) = 1.658 }
DYNAMIC stiff (method = ode15s, t = 0 .. 0.4, points = 200, rtol = 1e-7)
  der(y) = -1000*y + 3000 - 2000*exp(-t)
  y(0) = 0
END`,
  },
  {
    value: "ode-vanderpol",
    title: "Differential Equations (ODE): Van der Pol Oscillator (stiff) — ode23s",
    description: "μ = 1000 relaxation oscillator. Verify: the slow manifold keeps y1 bounded (≈ 2).",
    note: "Very stiff — needs the implicit Rosenbrock solver ode23s (or ode15s).",
    code: `{ Van der Pol, mu = 1000 — extremely stiff. Verify |y1| stays near 2 }
mu = 1000
DYNAMIC vdp (method = ode23s, t = 0 .. 1, points = 100, rtol = 1e-4, atol = 1e-7)
  der(y1) = y2
  der(y2) = mu * (1 - y1*y1) * y2 - y1
  y1(0) = 2
  y2(0) = 0
END`,
  },
  {
    value: "ode-robertson",
    title: "Differential Equations (ODE): Robertson Chemical Kinetics (stiff) — ode23s",
    description: "Three-species stiff kinetics with rates 0.04 / 1e4 / 3e7. Verify: c1+c2+c3 = 1 (mass conserved).",
    note: "A standard stiff benchmark; the fast transient dies quickly.",
    code: `{ Robertson kinetics — verify total concentration c1+c2+c3 stays 1 }
DYNAMIC rob (method = ode23s, t = 0 .. 40, points = 100, rtol = 1e-6, atol = 1e-10)
  der(c1) = -0.04*c1 + 1e4*c2*c3
  der(c2) = 0.04*c1 - 1e4*c2*c3 - 3e7*c2*c2
  der(c3) = 3e7*c2*c2
  c1(0) = 1
  c2(0) = 0
  c3(0) = 0
END`,
  },
  {
    value: "ode-chain",
    title: "Differential Equations (ODE): Stiff Reaction Chain A→B→C — ode15s",
    description: "Disparate rates (k1=1000 fast, k2=1 slow). Verify: A is consumed and a+b+c = 1.",
    note: "",
    code: `{ Stiff chain A->B->C — verify A -> 0 fast and total mass stays 1 }
k1 = 1000
k2 = 1
DYNAMIC chain (method = ode15s, t = 0 .. 5, points = 150, rtol = 1e-7)
  der(a) = -k1 * a
  der(b) =  k1 * a - k2 * b
  der(cc) = k2 * b
  a(0) = 1
  b(0) = 0
  cc(0) = 0
END`,
  },
];

// Examples are titled "Discipline: Specific title"; split on the first colon
// so the library can group them by discipline and show a short label.
function exampleCategory(title: string): string {
  const idx = title.indexOf(':');
  return idx >= 0 ? title.slice(0, idx).trim() : 'Other';
}

function exampleShortTitle(title: string): string {
  const idx = title.indexOf(':');
  return idx >= 0 ? title.slice(idx + 1).trim() : title;
}

// Grouped as [category, examples[]] in first-appearance order.
const EXAMPLE_CATEGORIES: [string, typeof CYCLE_EXAMPLES][] = (() => {
  const groups = new Map<string, typeof CYCLE_EXAMPLES>();
  for (const ex of CYCLE_EXAMPLES) {
    const category = exampleCategory(ex.title);
    const bucket = groups.get(category) ?? [];
    bucket.push(ex);
    groups.set(category, bucket);
  }
  return Array.from(groups.entries());
})();

// Workspace examples (from examples.ts) that are NOT featured in the "Open an
// Example" picker are surfaced here instead, grouped by their own category.
const WORKSPACE_EXAMPLE_CATEGORIES: [string, typeof EXAMPLES][] = (() => {
  const groups = new Map<string, typeof EXAMPLES>();
  for (const ex of EXAMPLES) {
    if (ex.featured) continue;
    const bucket = groups.get(ex.category) ?? [];
    bucket.push(ex);
    groups.set(ex.category, bucket);
  }
  return Array.from(groups.entries());
})();

// Every gallery category name (workspace + curated), deduped in display order —
// the facet chips of the Examples library.
const ALL_EXAMPLE_CATEGORY_NAMES: string[] = (() => {
  const names: string[] = [];
  for (const [cat] of WORKSPACE_EXAMPLE_CATEGORIES) if (!names.includes(cat)) names.push(cat);
  for (const [cat] of EXAMPLE_CATEGORIES) if (!names.includes(cat)) names.push(cat);
  return names;
})();


import {
  IconSearch,
  IconBook,
  IconCalculator,
  IconGrid3x3,
  IconCode,
  IconFlask,
  IconAdjustments,
  IconFileText,
  IconRocket,
  IconTool,
  IconList,
  IconWaveSine,
  IconTopologyStar3,
  IconServerCog,
  IconChevronLeft,
  IconChevronRight,
  IconArrowRight
} from '@tabler/icons-react';

interface NavItem {
  id: string;
  label: string;
  blurb?: string;
  keywords: string[];
}
interface NavCategory {
  title: string;
  icon: React.ReactNode;
  /** The overview/landing page id for this group (rendered as a card grid). */
  overview?: string;
  items: NavItem[];
}

const CATEGORIES: NavCategory[] = [
  {
    title: 'Get Started',
    icon: <IconRocket size={16} />,
    overview: 'started',
    items: [
      { id: 'started', label: 'Get Started Overview', blurb: 'What frees is, and a map of this documentation.', keywords: ['intro', 'philosophy', 'overview', 'getting started', 'welcome'] },
      { id: 'gs-first-solve', label: '1. Your First Solve', blurb: 'Type four lines and let frees find the unknown.', keywords: ['first solve', 'tutorial', 'quick start', 'f2', 'solve', 'ideal gas'] },
      { id: 'gs-declarative', label: '2. Thinking Declaratively', blurb: 'Why equation order never matters and any variable can be the unknown.', keywords: ['declarative', 'equality', 'order', 'unknown', 'assignment'] },
      { id: 'gs-units-check', label: '3. Units & Checking', blurb: 'Annotate inputs, work in SI, and verify with Check (F4).', keywords: ['units', 'check', 'f4', 'degrees of freedom', 'dof', 'guess', 'si'] },
      { id: 'gs-plots', label: '4. See It: Tables & Plots', blurb: 'Sweep an input with a parametric table and plot the response.', keywords: ['plot', 'parametric', 'sweep', 'table', 'solve table', 'plot curve', 'chart'] },
      { id: 'gs-repl', label: '5. Ask Questions: the REPL', blurb: 'Query the solved session, calculate with units, and call the CAS.', keywords: ['repl', 'terminal', 'console', 'workspace', 'query', 'interactive'] },
      { id: 'gs-components', label: '6. Wire Components', blurb: 'Instantiate and connect library components into a solved network.', keywords: ['components', 'connect', 'network', 'system', 'pipe', 'acausal'] },
      { id: 'gs-next', label: '7. Where to Go Next', blurb: 'A guided map of the rest of the documentation.', keywords: ['next steps', 'learn', 'map', 'where to go'] },
      { id: 'verification', label: 'Verification Suite', blurb: 'CI-enforced problems with independently derived expected values.', keywords: ['verification', 'validation', 'accuracy', 'trust', 'correctness', 'benchmark', 'ci'] },
    ]
  },
  {
    title: 'Language Fundamentals',
    icon: <IconCalculator size={16} />,
    overview: 'lang-overview',
    items: [
      { id: 'lang-overview', label: 'Overview', blurb: 'The grammar, variables, units, and built-in functions of the frees language.', keywords: ['language', 'fundamentals', 'overview', 'grammar'] },
      { id: 'syntax', label: 'Equation Syntax & Rules', blurb: 'Equality vs assignment, case-insensitivity, operators, and comments.', keywords: ['syntax', 'equality', 'case', 'comment', 'rules'] },
      { id: 'variables', label: 'Variables, Guesses & Bounds', blurb: 'Degrees of freedom, and the guesses and bounds that make nonlinear solves converge.', keywords: ['variables', 'guess', 'bounds', 'limits', 'variable info'] },
      { id: 'units', label: 'Units & Consistency', blurb: 'Annotate inputs, convert with Convert/ConvertTemp, and read SI results.', keywords: ['unit', 'units', 'supported units', 'unit list', 'si', 'convert', 'converttemp', 'temperature', 'dimension', 'annotation', 'constants', 'pi', 'gas constant', 'gravity', 'boltzmann', 'avogadro', 'planck'] },
      { id: 'arrays', label: 'Arrays & For Loops', blurb: 'Indexed variables and compile-time FOR expansion into equation families.', keywords: ['array', 'for', 'duplicate', 'loops', 'slice', 'index'] },
      { id: 'complex', label: 'Complex Numbers', blurb: 'Paired _r/_i components and the complex helper functions.', keywords: ['complex', 'imaginary', 'real', 'i', 'j', 'angle', 'polar', 'conj', 'magnitude', 'cis'] },
      { id: 'strings', label: 'String Variables', blurb: '$-suffixed strings for fluid names, geometry labels, and string functions.', keywords: ['string', 'chr$', 'concat$', 'copy$', 'lowercase$', 'uppercase$', 'trim$', 'stringlen', 'stringpos', 'stringval', 'date$', 'time$', 'timestamp$', 'unitsystem$', 'unitsof$'] },
      { id: 'math-funcs', label: 'Mathematical Functions', blurb: 'Trig, logs, rounding, conditionals — the differentiable scalar toolbox.', keywords: ['abs', 'sqrt', 'ln', 'log10', 'exp', 'sin', 'cos', 'tan', 'atan2', 'min', 'max', 'sum', 'avg', 'sinh', 'cosh', 'tanh', 'arcsinh', 'arccosh', 'arctanh', 'round', 'floor', 'ceil', 'trunc', 'sign', 'factorial', 'step', 'if', 'product', 'gcd', 'lcm', 'bitand', 'bitor', 'bitxor', 'bitnot', 'bitshiftl', 'bitshiftr', 'bitwise', 'shift', 'baseconvert'] },
      { id: 'special-funcs', label: 'Special & Statistical Functions', blurb: 'Bessel, gamma, error functions and statistical distributions.', keywords: ['bessel', 'besselk', 'bessely', 'bessel_i0', 'bessel_j0', 'chi_square', 'random', 'randg', 'probability', 'gamma', 'loggamma', 'digamma', 'beta', 'erf', 'erfc', 'erfinv'] },
    ]
  },
  {
    title: 'Matrices & Linear Algebra',
    icon: <IconGrid3x3 size={16} />,
    overview: 'matrix-overview',
    items: [
      { id: 'matrix-overview', label: 'Overview', blurb: 'Declare, operate on, and solve with matrices and vectors.', keywords: ['matrix', 'linear algebra', 'overview', 'vector'] },
      { id: 'matrices-decl', label: 'Declaring Matrices & Vectors', blurb: 'Bracket literals, slice suffixes, and generation helpers.', keywords: ['matrix', 'vector', 'declaring', 'literal', 'semicolon', 'brackets', 'arrays'] },
      { id: 'matrices-ops', label: 'Matrix Operators (+, -, *, \\, \')', blurb: 'Element-wise vs matrix operations and the backslash solve.', keywords: ['operators', 'transpose', 'backslash', 'multiplication', 'solve', 'arrays'] },
      { id: 'matrices-blas', label: 'OpenBLAS Algebra Functions', blurb: 'Low-level BLAS primitives (axpy, gemv, gemm, …).', keywords: ['blas', 'axpy', 'scal', 'copy', 'asum', 'nrm2', 'gemv', 'ger', 'gemm', 'openblas'] },
      { id: 'matrices-sys', label: 'Linear Systems & Decomposition', blurb: 'SolveLinear, determinants, LU, eigenvalues, and rotations.', keywords: ['solvelinear', 'determinant', 'ludecompose', 'eigen', 'eigenvalues', 'eulerrotate', 'eulerdecompose', 'rotation'] },
    ]
  },
  {
    title: 'Programming & Tables',
    icon: <IconCode size={16} />,
    overview: 'prog-overview',
    items: [
      { id: 'prog-overview', label: 'Overview', blurb: 'Reusable functions, submodels, and tabulated data.', keywords: ['programming', 'tables', 'overview', 'logic'] },
      { id: 'functions', label: 'Custom Functions & Procedures', blurb: 'FUNCTION/PROCEDURE bodies with imperative control flow.', keywords: ['functions', 'procedures', 'call', 'custom', 'outputs', 'while', 'repeat', 'until', 'loop', 'if', 'then', 'else'] },
      { id: 'modules', label: 'Modular Submodels (MODULE)', blurb: 'Encapsulate and reuse whole equation subsystems.', keywords: ['module', 'submodel', 'modular', 'call module'] },
      { id: 'tables-code', label: 'Custom Tables (TABLE)', blurb: 'Inline tabulated data callable as a function.', keywords: ['table', 'interp', 'tabulated', 'custom tables', 'curve fit'] },
      { id: 'lookup-tables', label: 'Lookup Tables & Interpolation', blurb: 'Interpolate, differentiate, and look up tabulated columns.', keywords: ['lookup', 'interpolate', 'differentiate', 'table', 'interpolation', 'spline'] },
      { id: 'table-accessors', label: 'Table Accessors & Aggregates', blurb: 'Query Parametric tables and whole-table aggregates.', keywords: ['parametric', 'integral', 'run', 'tablevalue', 'tablerun#', 'nparametricruns', 'sum', 'avg', 'min', 'max', 'stddev', 'integralvalue'] },
    ]
  },
  {
    title: 'Fluids, Materials & Psychrometrics',
    icon: <IconFlask size={16} />,
    overview: 'fluids-overview',
    items: [
      { id: 'fluids-overview', label: 'Overview', blurb: 'Real-fluid, ideal-gas, humid-air, and solid-material property data.', keywords: ['fluids', 'materials', 'overview', 'properties'] },
      { id: 'thermo', label: 'Fluid Properties (CoolProp & Gas)', blurb: 'Enthalpy, entropy, density … for CoolProp fluids and ideal gases.', keywords: ['coolprop', 'fluids', 'water', 'steam', 'refrigerant', 'glycol', 'density', 'enthalpy', 'entropy', 'p_sat', 't_sat', 'molarmass', 'compressibilityfactor', 'prandtl', 'surfacetension', 'fugacity', 'enthalpy_fusion', 'dipole', 'p_crit', 't_crit', 'v_crit', 't_triple', 'isidealgas', 'phase$'] },
      { id: 'humidair', label: 'Psychrometrics (AirH2O)', blurb: 'Humid-air states from three coordinates; wet-bulb, dew-point, humidity ratio.', keywords: ['psychrometric', 'humid air', 'airh2o', 'relative humidity', 'wet bulb', 'dew point'] },
      { id: 'chemistry', label: 'Chemistry & Combustion', blurb: 'Molar mass from formulas, heating values, view factors, Heisler charts.', keywords: ['chemistry', 'combustion', 'molarmass', 'heatingvalue', 'lhv', 'hhv', 'stoichafr', 'afr', 'fuel', 'formula', 'molar mass', 'chemical', 'c8h18', 'ch4', 'ethanol', 'hydrocarbon'] },
      { id: 'solid-materials', label: 'Solid & Material Properties', blurb: 'Conductivity, specific heat, modulus … for common engineering solids.', keywords: ['material', 'solid', 'c_', 'k_', 'rho_', 'mu_', 'pv_', 'e_', 'nu_', 'epsilon_', 'volexpcoef', 'freezingpt', 'deltal\\l_293', 'ek_lj', 'sigma_lj'] },
      { id: 'state-tables', label: 'Fluid State Tables (STATE TABLE)', blurb: 'Group circuit state points, isolate fluids, and overlay cycles.', keywords: ['state table', 'states', 'fluid states', 'circuit', 'multi-fluid', 'multi-circuit', 'fill missing', 'state points', 'overlay'] },
    ]
  },
  {
    title: 'Solving & Optimization',
    icon: <IconAdjustments size={16} />,
    overview: 'solving-overview',
    items: [
      { id: 'solving-overview', label: 'Overview', blurb: 'How frees solves, what to do when it doesn\'t, and the system-level analyses on top.', keywords: ['solving', 'optimization', 'overview', 'solver'] },
      { id: 'debugging', label: 'Debugging a Solve', blurb: 'Build incrementally, read residuals and blocking order, and seed guesses for stubborn nonlinear systems.', keywords: ['debug', 'debugging', 'troubleshoot', 'converge', 'convergence', 'diverge', 'residual', 'blocking', 'singular', 'guess', 'stall', 'wont solve', 'no solution'] },
      { id: 'errors', label: 'Errors & Diagnostics Index', blurb: 'Every checker and solver message, with its cause and fix.', keywords: ['error', 'errors', 'message', 'diagnostic', 'singular jacobian', 'max iterations', 'stalled', 'syntax error', 'degrees of freedom', 'dof', 'coolprop', 'range', 'unit warning', 'failed', 'pending', '429', 'poison'] },
      { id: 'uncertainty', label: 'Uncertainty Propagation', blurb: 'First-order RSS error propagation via UncertaintyOf.', keywords: ['uncertainty', 'propagation', 'error', 'uncertaintyof', 'svd'] },
      { id: 'optimization', label: 'Optimization & Sweeps', blurb: 'Parametric sweeps, single-objective optimization, and Pareto fronts.', keywords: ['optimization', 'sweep', 'parametric', 'minimization', 'maximization', 'nsga', 'pareto'] },
      { id: 'api', label: 'Solver Internals & Diagnostics', blurb: 'The compile/solve pipeline and how to read convergence diagnostics.', keywords: ['api', 'solver', 'newton', 'tarjan', 'residuals', 'jacobian', 'singular', 'convergence'] },
    ]
  },
  {
    title: 'Dynamic Systems & Control',
    icon: <IconWaveSine size={16} />,
    overview: 'dynamics-overview',
    items: [
      { id: 'dynamics-overview', label: 'Overview', blurb: 'Transient integration, linearization, and the control-design suite.', keywords: ['dynamics', 'control', 'overview', 'transient'] },
      { id: 'calculus', label: 'Numerical Integration (ODEs)', blurb: 'Definite integrals and the scalar first-order ODE feedback pattern.', keywords: ['integral', 'ode', 'differential', 'calculus', 'runge-kutta'] },
      { id: 'dynamic-ode', label: 'Transient / ODE Systems (DYNAMIC)', blurb: 'Coupled, multi-state, stiff, event-driven ODE integration.', keywords: ['dynamic', 'transient', 'ode', 'der', 'state', 'event', 'ode45', 'ode23', 'ode23s', 'ode15s', 'rocket', 'odevalue', 'finalvalue', 'maxvalue', 'timeat', 'ode table', 'stiff', 'initial condition', 'apogee'] },
      { id: 'symbolic-cas', label: 'Control Systems & Symbolic CAS', blurb: 'Transfer functions, state space, Bode/Nyquist, LQR, and Laplace algebra.', keywords: ['symbolic', 'cas', 'laplace', 's', 'partial fractions', 'residue', 'transfer function', 'tf', 'identity', 'control', 'decompose', 'apart', 'numerator', 'denominator', 'state space', 'ss2tf', 'tf2ss', 'zp2tf', 'tf2zp', 'series', 'parallel', 'feedback', 'pole', 'zero', 'bode', 'nyquist', 'margin', 'frequency response', 'step', 'impulse', 'lsim', 'time response', 'lqr', 'place', 'pole placement', 'ackermann', 'pidtune', 'pid', 'riccati', 'controller design'] },
    ]
  },
  {
    title: 'System Modeling with Components',
    icon: <IconTopologyStar3 size={16} />,
    overview: 'components-overview',
    items: [
      { id: 'components-overview', label: 'Overview', blurb: 'Acausal, multi-domain system modeling with a ~295-component library.', keywords: ['components', 'system modeling', 'overview', 'acausal', 'network', 'bond graph'] },
      { id: 'comp-first-network', label: 'Your First Component Network', blurb: 'Instantiate, connect, probe — a pipe run solved as a network.', keywords: ['component', 'network', 'instantiate', 'first', 'source', 'pipe', 'sink', 'probe', 'port'] },
      { id: 'comp-connections', label: 'Connections & Junctions', blurb: 'connect statements vs shared streams; junction rules and boundary conditions.', keywords: ['connect', 'junction', 'node', 'stream', 'shared name', 'branch', 'split', 'mixer', 'boundary condition'] },
      { id: 'comp-schematic', label: 'Reading the Schematic', blurb: 'The auto-drawn circuit: per-fluid lines, symbols, and results on hover.', keywords: ['schematic', 'diagram', 'drawing', 'circuit', 'canvas', 'visualise', 'visualize', 'draw', 'symbol', 'band', 'legend', 'hover', 'drag', 'zoom', 'pan', 'export svg', 'svg', 'layout', 'topology'] },
      { id: 'comp-domains', label: 'Domains & Fluid Families', blurb: 'Fluid, heat, electrical, mechanical — and the guarded fluid families.', keywords: ['domain', 'fluid', 'heat', 'electrical', 'mechanical', 'signal', 'sig', 'command', 'probe', 'moistair', 'gas', 'oil', 'liquid', 'twophase', 'domain$', 'across', 'through', 'transducer', 'humid air'] },
      { id: 'comp-library', label: 'The Component Library', blurb: 'A map of the thirteen domain libraries and their conventions.', keywords: ['library', 'components', 'catalog', 'signal', 'fluid', 'liquid', 'twophase', 'moistair', 'pneumatic', 'hydraulic', 'electrical', 'mechanical', 'powertrain', 'heat'] },
      { id: 'comp-variants', label: 'Fidelity Variants (model$)', blurb: 'One component, many models — select physics fidelity per instance.', keywords: ['variant', 'model$', 'fidelity', 'require', 'isentropic', 'volumetric', 'map'] },
      { id: 'comp-authoring', label: 'Writing Your Own Component', blurb: 'COMPONENT blocks: ports, PARAM, outputs, and VARIANT bodies.', keywords: ['component', 'custom', 'authoring', 'param', 'ports', 'variant', 'require', 'outputs'] },
      { id: 'comp-transient', label: 'Steady ↔ Transient Networks', blurb: 'Storage components carry the states; one wiring, both analyses.', keywords: ['transient', 'steady', 'storage', 'thermalmass', 'inertia', 'capacitor', 'accumulator', 'soc', 'ida', 'dae', 'ramp', 'time', 'sigstep', 'sigramp', 'drive cycle'] },
      { id: 'comp-linearize', label: 'From Plant to Controller (LINEARIZE)', blurb: 'Extract (A,B,C,D) from a transient network and design the loop.', keywords: ['linearize', 'state space', 'plant', 'abcd', 'input', 'output', 'lqr', 'controller'] },
      { id: 'comp-cycle-plots', label: 'Cycle Plots & Diagnostics', blurb: 'Source-mapped diagnostics and cycle overlays on property charts.', keywords: ['cycle', 'overlay', 'property plot', 'diagnostics', 'source-mapped'] },
      { id: 'comp-wizard', label: 'The Component Wizard', blurb: 'Guided instantiation with variant gating, UA helpers, and map ingestion.', keywords: ['wizard', 'component wizard', 'ua', 'correlation', 'map', 'insert'] },
      { id: 'comp-troubleshooting', label: 'Troubleshooting Networks', blurb: 'The strict parse errors, and the cold-start patterns that fix convergence.', keywords: ['troubleshooting', 'error', 'domain mismatch', 'port count', 'cold start', 'seed', 'pressure', 'mixer', 'capacity floor', 'converge'] },
    ]
  },
  {
    title: 'Tools & Workflow',
    icon: <IconTool size={16} />,
    overview: 'tools-overview',
    items: [
      { id: 'tools-overview', label: 'Overview', blurb: 'The interactive console, shortcuts, notes, and data-capture tools.', keywords: ['tools', 'workflow', 'overview'] },
      { id: 'repl', label: 'REPL Terminal & Workspace', blurb: 'A unit-aware console over the solved session, with CALL and CAS.', keywords: ['repl', 'terminal', 'workspace', 'console', 'vars', 'who', 'whos', 'calculator', 'cas', 'factor', 'expand', 'simplify', 'apart', 'laplace', 'diff', 'integrate', 'call', 'symbolic', 'interactive', 'ans'] },
      { id: 'shortcuts', label: 'Keyboard Shortcuts', blurb: 'Solve, Check, Variable Info, and block-solve hotkeys.', keywords: ['hotkey', 'shortcuts', 'keyboard', 'f2', 'f4', 'f9', 'ctrl'] },
      { id: 'reports', label: 'Notes & Narrative', blurb: 'Structure a document with comment narrative; figures come from named PLOT blocks.', keywords: ['notes', 'narrative', 'comments', 'report', 'document', 'figures'] },
      { id: 'plot-code', label: 'Plots in Code (PLOT)', blurb: 'Declare XY, property, Bode, Nyquist and pole-zero figures in code.', keywords: ['plot', 'graph', 'chart', 'code', 'programmatic', 'xy', 'property', 'psychro'] },
      { id: 'digitizer-fit', label: 'Graph Digitizer & Curve Fit', blurb: 'Turn a chart image or a table into a fitted equation.', keywords: ['digitizer', 'curve', 'fit', 'table', 'regression', 'equation', 'graph'] },
      { id: 'analyzer', label: 'Data Analyzer (Measurements)', blurb: 'Import CSV recordings — oscilloscope, cursors, events, statistics.', keywords: ['analyzer', 'measurement', 'csv', 'import', 'oscilloscope', 'cursor', 'events', 'scatter', 'histogram', 'offset', 'export', 'signals'] },
      { id: 'calc-signals', label: 'Calculated Signals', blurb: 'frees formulas over measured data — properties, delta, integral, movavg, delay.', keywords: ['calculated', 'calc', 'signal', 'formula', 'delta', 'integral', 'movavg', 'delay', 'coolprop', 'property', 'raster', 'boolean', 'condition'] },
    ]
  },
  {
    title: 'Examples & Tutorials',
    icon: <IconFileText size={16} />,
    items: [
      { id: 'tut-msd', label: 'Tutorial: Mass–Spring–Damper → Bode', blurb: 'From a transient ring-down to the plant\'s frequency response, in stages.', keywords: ['tutorial', 'mass spring damper', 'oscillator', 'bode', 'transfer function', 'dynamic', 'vibration', 'resonance', 'damping'] },
      { id: 'tut-coil', label: 'Tutorial: AC Cooling Coil', blurb: 'Psychrometric coil analysis by hand, then rebuilt from components.', keywords: ['tutorial', 'cooling coil', 'psychrometrics', 'hvac', 'dehumidification', 'latent', 'sensible', 'shr', 'moist air', 'air conditioning'] },
      { id: 'tut-rlc', label: 'Tutorial: RLC Filter Response', blurb: 'Phasor spot checks, then the full Bode picture of a low-pass filter.', keywords: ['tutorial', 'rlc', 'filter', 'circuit', 'frequency response', 'bode', 'impedance', 'phasor', 'resonance', 'low-pass'] },
      { id: 'tut-vccycle', label: 'Tutorial: Refrigeration Cycle ± Uncertainty', blurb: 'An R134a cycle whose COP carries real instrument error bars.', keywords: ['tutorial', 'refrigeration', 'vapor compression', 'cop', 'uncertainty', 'r134a', 'cycle', 'error propagation'] },
      { id: 'tut-pump', label: 'Tutorial: Pump Selection', blurb: 'Digitize a datasheet curve, intersect the system curve, size the motor.', keywords: ['tutorial', 'pump', 'digitizer', 'head curve', 'system curve', 'operating point', 'table', 'shaft power'] },
      { id: 'examples', label: 'Engineering Examples Library', blurb: 'Verified, ready-to-run problems grouped by discipline.', keywords: ['examples', 'rankine', 'brayton', 'cold air standard', 'combined cycle', 'pipe network', 'truss', 'radiation', 'cooling loop', 'reforming', 'pid', 'fatigue', 'nuclear', 'siyavula', 'nozzle', 'co2', 'compressible', 'throat', 'sonic', 'pelton', 'turbine', 'turbomachinery', 'hydropower', 'impulse', 'vehicle', 'ev', 'electric vehicle', 'longitudinal', 'lateral', 'bicycle model', 'understeer', 'road load', 'drag', 'battery', 'pack', 'cell', 'sizing', 'motor', 'range', 'batemo', 'c-rate', 'ode', 'differential equations', 'runge-kutta', 'stiff', 'van der pol', 'robertson', 'lotka-volterra', 'predator-prey', 'pendulum', 'rlc', 'rc circuit', 'rl circuit', 'orbit', 'logistic', 'decay', 'cooling', 'mass-spring-damper', 'parachutist', 'torricelli'] },
    ]
  },
  {
    title: 'Architecture & Deployment',
    icon: <IconServerCog size={16} />,
    overview: 'deploy-overview',
    items: [
      { id: 'deploy-overview', label: 'Overview', blurb: 'The async compute model, the REST API, and both deployment paths.', keywords: ['architecture', 'deployment', 'overview', 'server'] },
      { id: 'arch-async', label: 'How a Solve Runs', blurb: 'Editor → API → queue → compute → job store, and why it\'s asynchronous.', keywords: ['async', 'asynchronous', 'architecture', 'rabbitmq', 'redis', 'queue', 'job', 'jobid', '202', 'poll', 'compute', 'check'] },
      { id: 'arch-api', label: 'The REST API', blurb: 'Drive frees from scripts: check, solve, poll, REPL, and optimization endpoints.', keywords: ['api', 'rest', 'http', 'curl', 'endpoint', 'solve', 'check', 'jobs', 'integration', 'script'] },
      { id: 'deploy-docker', label: 'Run Locally with Docker', blurb: 'frees.sh, the compose topology, and host-side development.', keywords: ['docker', 'compose', 'local', 'frees.sh', 'install', 'run', 'container', 'localhost'] },
      { id: 'deploy-railway', label: 'Deploy to Railway', blurb: 'The five-service layout and the production configuration that must stay.', keywords: ['railway', 'deploy', 'production', 'cloud', 'nginx', 'private network', 'commit', 'about'] },
      { id: 'deploy-health', label: 'Health & Scaling', blurb: 'The topology health endpoint, compute replicas, and the poison-message guard.', keywords: ['health', 'scaling', 'monitoring', 'replicas', 'workers', 'poison', 'redelivered', '503', 'degraded'] },
    ]
  },
  {
    title: 'Reference',
    icon: <IconList size={16} />,
    overview: 'ref-index',
    items: [
      { id: 'ref-index', label: 'A–Z Function Index', blurb: 'Every documented symbol, alphabetically, linking to its reference page.', keywords: ['index', 'a-z', 'alphabetical', 'all functions', 'reference', 'list'] },
      { id: 'ref-units', label: 'Units & Constants', blurb: 'The live list of accepted units and built-in physical constants.', keywords: ['constant', 'pi#', 'e#', 'r#', 'g#', 'na#', 'k#', 'h#', 'c#', 'sigma#', 'gc#', 'qe#', 'avogadro', 'boltzmann', 'planck', 'gravity', 'gas constant', 'unit', 'units', 'si', 'dimension', 'kpa', 'pa', 'convert', 'deg', 'rad'] },
      { id: 'ref-fluids', label: 'Supported Fluids', blurb: 'The live CoolProp fluid list, ideal gases, and glycol coolants.', keywords: ['fluid', 'water', 'steam', 'r134a', 'ammonia', 'air', 'airh2o', 'glycol', 'eg50', 'pg30', 'coolprop', 'supported fluids'] },
    ]
  }
];

// id → display label / blurb, for cross-links ([Related:] markers, landing cards).
const NAV_LABELS: Record<string, string> = {};
const NAV_BLURBS: Record<string, string> = {};
for (const cat of CATEGORIES) for (const it of cat.items) {
  NAV_LABELS[it.id] = it.label;
  if (it.blurb) NAV_BLURBS[it.id] = it.blurb;
}
// overview id → the category it introduces (so a landing page can list siblings).
const CATEGORY_BY_OVERVIEW = new Map<string, NavCategory>();
for (const cat of CATEGORIES) if (cat.overview) CATEGORY_BY_OVERVIEW.set(cat.overview, cat);

// The numbered Get-Started reading order, used for Prev/Next navigation.
const GETTING_STARTED_SEQUENCE = ['started', 'gs-first-solve', 'gs-declarative', 'gs-units-check', 'gs-plots', 'gs-repl', 'gs-components', 'gs-next'];

// Slugify a heading for an in-page anchor ("On this page" links).
function slugify(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
}

// Strip markdown emphasis/code/math markers so heading text reads cleanly in a TOC.
function plainText(s: string): string {
  return s.replace(/[`*_$]/g, '').trim();
}

function renderInlineContent(text: string): React.ReactNode[] {
  const result: React.ReactNode[] = [];
  let i = 0;

  // Order matters only where delimiters share a first character (** before *).
  const spanMarkers = [
    { delim: '$', type: 'math' },
    { delim: '`', type: 'code' },
    { delim: '**', type: 'bold' },
    { delim: '*', type: 'italic' },
  ] as const;

  while (i < text.length) {
    const span = spanMarkers.find((m) => text.startsWith(m.delim, i));
    if (span) {
      const endIdx = text.indexOf(span.delim, i + span.delim.length);
      if (endIdx !== -1) {
        const val = text.substring(i + span.delim.length, endIdx);
        const key = `inline-${span.type}-${i}`;
        if (span.type === 'bold') {
          // Recurse so nested code/math inside bold (e.g. **`Gamma(x):`**) renders.
          result.push(<strong key={key}>{renderInlineContent(val)}</strong>);
        } else if (span.type === 'italic') {
          result.push(<em key={key}>{renderInlineContent(val)}</em>);
        } else if (span.type === 'code') {
          result.push(<Code key={key}>{val}</Code>);
        } else if (span.type === 'math') {
          result.push(<Latex key={key} math={val} />);
        }
        i = endIdx + span.delim.length;
        continue;
      }
    }

    // No marker here, or an unterminated one: emit text up to the next possible
    // marker. Scan from i+1 so an unterminated delimiter can't stall the loop.
    let nextIdx = text.length;
    for (const m of spanMarkers) {
      const idx = text.indexOf(m.delim, i + 1);
      if (idx !== -1 && idx < nextIdx) {
        nextIdx = idx;
      }
    }
    result.push(text.substring(i, nextIdx));
    i = nextIdx;
  }
  return result;
}

interface MarkdownRendererProps {
  content: string;
  /** Navigate to another topic/reference id (for [Related:] cross-links). */
  onNavigate?: (id: string) => void;
}

function MarkdownRenderer({ content, onNavigate }: MarkdownRendererProps) {
  if (!content) return null;
  const lines = content.split('\n');
  const elements: React.ReactNode[] = [];
  
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();
    
    // 1. Fenced Code Block. The info string marks behavior: ```run blocks are
    // backend-verified (scripts/check-doc-snippets.mjs) and render with
    // Run / Open in Editor buttons; `vary=name=min:step:max` tokens add live
    // parameter sliders that re-solve through the overrides channel.
    if (trimmed.startsWith('```')) {
      const fence = trimmed.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith('```')) {
        codeLines.push(lines[i]);
        i++;
      }
      const code = codeLines.join('\n');
      const key = `code-${i}`;
      if (fence === 'run' || fence.startsWith('run ')) {
        const vary = [...fence.matchAll(/vary=([A-Za-z_][A-Za-z0-9_]*)=(-?[\d.eE+-]+):(-?[\d.eE+-]+):(-?[\d.eE+-]+)/g)]
          .map((m) => ({ name: m[1], min: Number(m[2]), step: Number(m[3]), max: Number(m[4]) }))
          .filter((v) => Number.isFinite(v.min) && Number.isFinite(v.step) && v.step > 0 && v.max > v.min);
        elements.push(<RunnableCode key={key} code={code} vary={vary} />);
      } else {
        elements.push(
          <Paper key={key} withBorder p="md" bg="light-dark(var(--mantine-color-gray-0), var(--mantine-color-dark-8))" mb="md" style={{ position: 'relative' }}>
            <CopyButton code={code} />
            <Code block style={{ background: 'transparent' }}>{code}</Code>
          </Paper>
        );
      }
      i++;
      continue;
    }
    
    // 1b. Block math: $$ ... $$ (single- or multi-line)
    if (trimmed.startsWith('$$')) {
      let mathText: string;
      if (trimmed.length > 4 && trimmed.endsWith('$$')) {
        mathText = trimmed.slice(2, -2).trim();
        i++;
      } else {
        const buf: string[] = [trimmed.substring(2)];
        i++;
        while (i < lines.length && !lines[i].includes('$$')) {
          buf.push(lines[i]);
          i++;
        }
        if (i < lines.length) {
          buf.push(lines[i].substring(0, lines[i].indexOf('$$')));
          i++;
        }
        mathText = buf.join('\n').trim();
      }
      elements.push(
        <div key={`blockmath-${i}`} style={{ margin: '0.6em 0', overflowX: 'auto' }}>
          <Latex math={mathText} block />
        </div>
      );
      continue;
    }

    // 2. Table Block
    if (trimmed.startsWith('|')) {
      // Parse header row
      const headers = trimmed.split('|').map(h => h.trim()).filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
      i++;
      
      // Check for separator row (e.g. | --- | --- |)
      if (i < lines.length && lines[i].trim().startsWith('|')) {
        const sepRow = lines[i].trim();
        const sepCols = sepRow.split('|').map(s => s.trim()).filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
        if (sepCols.every(col => col.startsWith('-') || col.endsWith('-'))) {
          i++;
        }
      }
      
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith('|')) {
        const rowCols = lines[i].trim().split('|').map(c => c.trim()).filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
        rows.push(rowCols);
        i++;
      }
      
      const key = `table-${i}`;
      elements.push(
        <Table key={key} striped withTableBorder withColumnBorders mb="md">
          <Table.Thead>
            <Table.Tr>
              {headers.map((h, idx) => (
                <Table.Th key={idx}>{renderInlineContent(h)}</Table.Th>
              ))}
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {rows.map((row, rIdx) => (
              <Table.Tr key={rIdx}>
                {row.map((cell, cIdx) => (
                  <Table.Td key={cIdx}>{renderInlineContent(cell)}</Table.Td>
                ))}
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      );
      continue;
    }
    
    // 3. Lists
    if (trimmed.startsWith('- ') || trimmed.startsWith('* ')) {
      const listItems: string[] = [];
      while (i < lines.length && (lines[i].trim().startsWith('- ') || lines[i].trim().startsWith('* '))) {
        listItems.push(lines[i].trim().substring(2));
        i++;
      }
      const key = `list-${i}`;
      elements.push(
        <List key={key} spacing="xs" size="sm" mb="md" style={{ lineHeight: 1.6 }}>
          {listItems.map((item, idx) => (
            <List.Item key={idx}>{renderInlineContent(item)}</List.Item>
          ))}
        </List>
      );
      continue;
    }
    
    // 4. Blockquotes / Alerts
    if (trimmed.startsWith('>')) {
      const alertLines: string[] = [];
      let color = 'blue';
      let title = 'Note';
      
      let firstLine = trimmed.substring(1).trim();
      if (firstLine.startsWith('[!NOTE]')) {
        color = 'blue';
        title = 'Note';
        firstLine = firstLine.substring(7).trim();
      } else if (firstLine.startsWith('[!WARNING]')) {
        color = 'orange';
        title = 'Warning';
        firstLine = firstLine.substring(10).trim();
      } else if (firstLine.startsWith('[!IMPORTANT]')) {
        color = 'indigo';
        title = 'Important';
        firstLine = firstLine.substring(12).trim();
      } else if (firstLine.startsWith('[!CAUTION]')) {
        color = 'red';
        title = 'Caution';
        firstLine = firstLine.substring(10).trim();
      } else if (firstLine.startsWith('[!TIP]')) {
        color = 'teal';
        title = 'Tip';
        firstLine = firstLine.substring(6).trim();
      }
      
      if (firstLine) {
        alertLines.push(firstLine);
      }
      i++;
      
      while (i < lines.length && lines[i].trim().startsWith('>')) {
        alertLines.push(lines[i].trim().substring(1).trim());
        i++;
      }
      
      const key = `alert-${i}`;
      elements.push(
        <Alert key={key} color={color} title={title} mb="md">
          {alertLines.map((l, idx) => (
            <Text key={idx} size="sm">{renderInlineContent(l)}</Text>
          ))}
        </Alert>
      );
      continue;
    }
    
    // 5. Headings
    if (trimmed.startsWith('#')) {
      let order: 2 | 3 | 4 = 2;
      let titleText = '';
      if (trimmed.startsWith('### ')) {
        order = 4;
        titleText = trimmed.substring(4);
      } else if (trimmed.startsWith('## ')) {
        order = 3;
        titleText = trimmed.substring(3);
      } else if (trimmed.startsWith('# ')) {
        order = 2;
        titleText = trimmed.substring(2);
      }
      
      if (titleText) {
        const key = `heading-${i}`;
        // H2 (rendered order 3) gets an anchor id so the "On this page" TOC can scroll to it.
        const anchorId = order === 3 ? slugify(plainText(titleText)) : undefined;
        elements.push(
          <Title key={key} id={anchorId} order={order} mt={order === 2 ? 'md' : 'sm'} mb="xs" c="blue.4"
            style={anchorId ? { scrollMarginTop: '76px' } : undefined}>
            {renderInlineContent(titleText)}
          </Title>
        );
      }
      i++;
      continue;
    }
    
    // 6. Custom Diagrams & Components
    if (trimmed.startsWith('[Diagram:') && trimmed.endsWith(']')) {
      const diagName = trimmed.substring(9, trimmed.length - 1).trim();
      const key = `diagram-${i}`;
      if (diagName === 'SolverPipeline') {
        elements.push(<SolverPipelineDiagram key={key} />);
      } else if (diagName === 'DoF') {
        elements.push(<DegreesOfFreedomDiagram key={key} />);
      } else if (diagName === 'DependentProperties') {
        elements.push(<DependentPropertiesDiagram key={key} />);
      } else if (diagName === 'GuessConvergence') {
        elements.push(<GuessConvergenceDiagram key={key} />);
      } else if (diagName === 'RefrigerationCycle') {
        elements.push(<RefrigerationCycleDiagram key={key} />);
      } else if (diagName === 'RankineCycle') {
        elements.push(<RankineCycleDiagram key={key} />);
      } else if (diagName === 'EvThermal') {
        elements.push(<EvThermalDiagram key={key} />);
      } else if (diagName === 'LearningMap') {
        elements.push(<LearningMapDiagram key={key} onNavigate={onNavigate} />);
      }
      i++;
      continue;
    }
    
    if (trimmed.startsWith('[Component:') && trimmed.endsWith(']')) {
      const compName = trimmed.substring(11, trimmed.length - 1).trim();
      const key = `component-${i}`;
      if (compName === 'UnitsReference') {
        elements.push(<UnitsReference key={key} />);
      }
      i++;
      continue;
    }

    // 6b. Runnable example: [Run: example-id] — pulls a backend-verified example
    // from examples.ts and renders it as a copyable code block.
    if (trimmed.startsWith('[Run:') && trimmed.endsWith(']')) {
      const exId = trimmed.substring(5, trimmed.length - 1).trim();
      const ex = EXAMPLES.find((e) => e.id === exId);
      const key = `run-${i}`;
      if (ex) {
        elements.push(<RunnableCode key={key} code={ex.text} title={ex.title} />);
      } else {
        elements.push(
          <Alert key={key} color="orange" title="Missing example" mb="md">
            <Text size="sm">Example <Code>{exId}</Code> is referenced but not found in the library.</Text>
          </Alert>
        );
      }
      i++;
      continue;
    }
    
    // 6c. Related topics footer: [Related: id1, id2, …] — clickable cross-links to
    // other guide/reference pages, mirroring the reference pages' "See also" row.
    if (trimmed.startsWith('[Related:') && trimmed.endsWith(']')) {
      const ids = trimmed.substring(9, trimmed.length - 1).split(',').map((s) => s.trim()).filter(Boolean);
      const key = `related-${i}`;
      elements.push(
        <Box key={key} mt="xl" pt="md" style={{ borderTop: '1px solid var(--mantine-color-default-border)' }}>
          <Text size="sm" fw={700} c="dimmed" mb="xs" style={{ textTransform: 'uppercase', letterSpacing: '0.5px' }}>
            Related Topics
          </Text>
          <Group gap="xs">
            {ids.map((id) => {
              const label = NAV_LABELS[id] ?? id;
              return (
                <Badge
                  key={id}
                  component="a"
                  variant="light"
                  color="blue"
                  size="lg"
                  style={{ cursor: onNavigate ? 'pointer' : 'default', textTransform: 'none' }}
                  rightSection={<IconArrowRight size={12} />}
                  onClick={() => onNavigate?.(id)}
                >
                  {label}
                </Badge>
              );
            })}
          </Group>
        </Box>
      );
      i++;
      continue;
    }

    // 7. Regular Paragraph or spacer
    if (trimmed === '') {
      elements.push(<div key={`spacer-${i}`} style={{ height: '0.8em' }} />);
    } else {
      elements.push(
        <Text key={`p-${i}`} size="md" style={{ lineHeight: 1.6 }} mb="sm">
          {renderInlineContent(line)}
        </Text>
      );
    }
    i++;
  }
  
  return <>{elements}</>;
}

// Reference pages grouped by category, for the "Function Reference" nav.
const REFERENCE_BY_CATEGORY: [string, ReferencePage[]][] = (() => {
  const map = new Map<string, ReferencePage[]>();
  for (const p of REFERENCE_PAGES) {
    if (!map.has(p.category)) map.set(p.category, []);
    map.get(p.category)!.push(p);
  }
  return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
})();
const REFERENCE_BY_SLUG = new Map(REFERENCE_PAGES.map((p) => [p.slug, p]));

// Nav categories generated from the compiled reference pages, one per category.
const REFERENCE_NAV_CATEGORIES = REFERENCE_BY_CATEGORY.map(([cat, pages]) => ({
  title: 'Reference · ' + cat,
  icon: <IconBook size={16} />,
  items: pages.map((p) => ({
    id: 'refpage:' + p.slug,
    label: p.name,
    keywords: [p.name.toLowerCase(), cat.toLowerCase(), ...p.tags, ...p.related.map((r) => r.toLowerCase())],
  })),
}));

// Base nav: the hand-authored guides + the generated reference pages.
const ALL_CATEGORIES = [...CATEGORIES, ...REFERENCE_NAV_CATEGORIES];

// Renders a single reference page: frontmatter header + body +
// references footer, with markdown/KaTeX/[Run:] handled by MarkdownRenderer.
function ReferencePageView({ page, onNavigate, onNavigateTopic }: Readonly<{ page: ReferencePage; onNavigate: (slug: string) => void; onNavigateTopic?: (id: string) => void }>) {
  return (
    <Stack gap="sm">
      <Group justify="space-between" align="flex-start">
        <Title order={2} c="blue.4" style={{ fontFamily: page.category.startsWith('Cookbook') ? undefined : 'monospace' }}>{page.name}</Title>
        <Badge color="grape" variant="light" size="lg">{page.category}</Badge>
      </Group>
      {page.summary && <Text size="md" c="dimmed">{page.summary}</Text>}
      {page.tags.length > 0 && (
        <Group gap="xs">
          {page.tags.map((t) => <Badge key={t} color="gray" variant="outline" size="sm">{t}</Badge>)}
        </Group>
      )}
      <Divider my="xs" />
      <MarkdownRenderer content={page.body} />
      {page.related.length > 0 && (
        <Group gap="xs" mt="sm">
          <Text size="sm" fw={600} c="dimmed">See also:</Text>
          {page.related.map((r) => {
            const target = REFERENCE_BY_SLUG.get(r.toLowerCase());
            return target
              ? <Badge key={r} component="a" style={{ cursor: 'pointer' }} color="blue" variant="light"
                  onClick={() => onNavigate(target.slug)}>{r}</Badge>
              : <Badge key={r} color="gray" variant="light">{r}</Badge>;
          })}
        </Group>
      )}
      {onNavigateTopic && page.guides.filter((g) => NAV_LABELS[g]).length > 0 && (
        <Group gap="xs" mt="xs">
          <Text size="sm" fw={600} c="dimmed">In the guides:</Text>
          {page.guides.filter((g) => NAV_LABELS[g]).map((g) => (
            <Badge key={g} component="a" style={{ cursor: 'pointer', textTransform: 'none' }} color="teal" variant="light"
              onClick={() => onNavigateTopic(g)}>{NAV_LABELS[g]}</Badge>
          ))}
        </Group>
      )}
    </Stack>
  );
}

// "On this page" mini-TOC built from the H2 (`##`) headings of a markdown doc.
function OnThisPage({ content }: Readonly<{ content: string }>) {
  const headings: string[] = [];
  let inCode = false;
  for (const raw of content.split('\n')) {
    const t = raw.trim();
    if (t.startsWith('```')) { inCode = !inCode; continue; }
    if (!inCode && t.startsWith('## ') && !t.startsWith('### ')) headings.push(plainText(t.substring(3)));
  }
  if (headings.length < 2) return null;
  return (
    <Paper withBorder p="sm" mb="lg" bg="light-dark(var(--mantine-color-gray-0), var(--mantine-color-dark-9))">
      <Text size="xs" fw={700} c="dimmed" mb={6} style={{ textTransform: 'uppercase', letterSpacing: '0.5px' }}>
        On this page
      </Text>
      <Stack gap={2}>
        {headings.map((h) => (
          <Anchor key={h} size="sm" c="blue.4" href={`#${slugify(h)}`}
            onClick={(e) => {
              e.preventDefault();
              document.getElementById(slugify(h))?.scrollIntoView({ behavior: 'smooth', block: 'start' });
            }}>
            {h}
          </Anchor>
        ))}
      </Stack>
    </Paper>
  );
}

// Prev/Next pager for pages that belong to the numbered Get-Started sequence.
function PrevNext({ active, onNavigate }: Readonly<{ active: string; onNavigate: (id: string) => void }>) {
  const idx = GETTING_STARTED_SEQUENCE.indexOf(active);
  if (idx === -1) return null;
  const prev = idx > 0 ? GETTING_STARTED_SEQUENCE[idx - 1] : null;
  const next = idx < GETTING_STARTED_SEQUENCE.length - 1 ? GETTING_STARTED_SEQUENCE[idx + 1] : null;
  return (
    <Group justify="space-between" mt="xl" pt="md" style={{ borderTop: '1px solid var(--mantine-color-default-border)' }}>
      {prev
        ? <Button variant="default" leftSection={<IconChevronLeft size={16} />} onClick={() => onNavigate(prev)}>{NAV_LABELS[prev]}</Button>
        : <span />}
      {next
        ? <Button variant="filled" rightSection={<IconChevronRight size={16} />} onClick={() => onNavigate(next)}>{NAV_LABELS[next]}</Button>
        : <span />}
    </Group>
  );
}

// Category landing page: optional intro markdown + a card grid of the group's pages.
function CategoryLanding({ category, intro, onNavigate }: Readonly<{ category: NavCategory; intro?: string; onNavigate: (id: string) => void }>) {
  const cards = category.items.filter((it) => it.id !== category.overview);
  return (
    <Stack gap="md">
      <Title order={2} c="blue.4">{category.title}</Title>
      {intro && <MarkdownRenderer content={intro} onNavigate={onNavigate} />}
      <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="md" mt="xs">
        {cards.map((it) => (
          <Card key={it.id} withBorder padding="md" radius="md"
            style={{ cursor: 'pointer', height: '100%' }}
            onClick={() => onNavigate(it.id)}
            className="doc-landing-card">
            <Group justify="space-between" wrap="nowrap" align="flex-start">
              <Text fw={600} c="blue.4">{it.label}</Text>
              <IconArrowRight size={16} style={{ flexShrink: 0, opacity: 0.5 }} />
            </Group>
            {it.blurb && <Text size="sm" c="dimmed" mt={4}>{it.blurb}</Text>}
          </Card>
        ))}
      </SimpleGrid>
    </Stack>
  );
}

// Alphabetical A–Z index of every per-symbol reference page (the single source of
// truth for the function surface). Replaces the old hand-maintained tables.
function ReferenceIndex({ onNavigate }: Readonly<{ onNavigate: (id: string) => void }>) {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const pages = q
    ? REFERENCE_PAGES.filter((p) => p.name.toLowerCase().includes(q) || p.summary.toLowerCase().includes(q) || p.tags.some((t) => t.includes(q)))
    : REFERENCE_PAGES;
  // Group by uppercased first character of the name.
  const groups = new Map<string, ReferencePage[]>();
  for (const p of [...pages].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))) {
    const letter = (p.name[0] || '#').toUpperCase();
    const key = /[A-Z]/.test(letter) ? letter : '#';
    (groups.get(key) ?? groups.set(key, []).get(key)!).push(p);
  }
  return (
    <Stack gap="md">
      <Title order={2} c="blue.4">A–Z Function Index</Title>
      <Text size="sm" c="dimmed">
        Every documented function, procedure, block, and component — the canonical
        reference for each symbol. Click a name for its full page (syntax,
        arguments, examples, errors).
      </Text>
      <TextInput
        placeholder="Filter by name, summary, or tag (e.g. enthalpy, bode, matrix)"
        value={query}
        onChange={(e) => setQuery(e.currentTarget.value)}
        leftSection={<IconSearch size={16} />}
        rightSection={query ? <CloseButton size="sm" onClick={() => setQuery('')} /> : null}
        maw={480}
      />
      {[...groups.entries()].map(([letter, list]) => (
        <Box key={letter}>
          <Title order={3} c="blue.3" mb="xs">{letter}</Title>
          <SimpleGrid cols={{ base: 1, sm: 2, md: 3 }} spacing={6}>
            {list.map((p) => (
              <Anchor key={p.slug} size="sm" style={{ fontFamily: 'monospace' }}
                onClick={() => onNavigate('refpage:' + p.slug)}>
                {p.name}
              </Anchor>
            ))}
          </SimpleGrid>
        </Box>
      ))}
      {pages.length === 0 && <Text size="sm" c="dimmed">No symbols match “{query}”.</Text>}
    </Stack>
  );
}

// A location.hash value that names a real portal page (guide topic, special
// page, or refpage:slug). In-page heading anchors from the "On this page" TOC
// are NOT topics — they must be ignored by the hash router, not navigated.
function knownTopicId(id: string): boolean {
  if (!id) return false;
  if (id.startsWith('refpage:')) return REFERENCE_BY_SLUG.has(id.slice('refpage:'.length));
  if (Object.hasOwn(DOCS_CATALOG, id)) return true;
  return id === 'examples' || id === 'ref-index' || id === 'ref-units' || id === 'ref-fluids';
}

const hashTopic = () => decodeURIComponent(globalThis.location.hash.slice(1));

export default function HelpPage() {
  const [opened, { toggle }] = useDisclosure();
  // Deep-linkable pages: /help#comp-first-network, /help#refpage:bode, … The
  // active page mirrors location.hash so doc URLs are shareable and the app's
  // error/help links can target a specific page. Unknown hashes are left to
  // the browser (they are in-page heading anchors).
  const [active, setActive] = useState(() => (knownTopicId(hashTopic()) ? hashTopic() : 'started'));
  const [searchQuery, setSearchQuery] = useState('');
  const [searchFocused, setSearchFocused] = useState(false);

  useEffect(() => {
    const onHash = () => {
      const id = hashTopic();
      if (knownTopicId(id)) setActive(id);
    };
    globalThis.addEventListener('hashchange', onHash);
    return () => globalThis.removeEventListener('hashchange', onHash);
  }, []);
  // Examples-gallery facets: free-text filter + a single active category chip.
  const [exampleFilter, setExampleFilter] = useState('');
  const [exampleCat, setExampleCat] = useState<string | null>(null);

  // Build the full-text search index once, seeding it with the nav keywords.
  useEffect(() => {
    const timer = setTimeout(() => {
      const kw: Record<string, string[]> = {};
      for (const cat of ALL_CATEGORIES) for (const it of cat.items) kw[it.id] = it.keywords;
      buildSearchIndex(kw);
    }, 100);
    return () => clearTimeout(timer);
  }, []);

  // Search facet: restrict results to one kind of page (guide/reference/…).
  const [searchKind, setSearchKind] = useState<SearchKind | null>(null);

  // Intelligent full-text search across all docs, catalogs, and examples.
  const searchResults = useMemo<SearchHit[]>(
    () => (searchQuery.trim().length >= 2 ? searchDocs(searchQuery, 12, searchKind) : []),
    [searchQuery, searchKind]
  );

  // When not searching, the nav shows all topics. When searching with no
  // content hits, fall back to the old label/keyword filter so the nav still
  // narrows. When content hits exist, the dropdown takes over and the nav
  // shows the matching topic ids only. An active facet keeps the dropdown
  // open even at zero hits, so the chips stay reachable.
  const showResults = searchFocused && searchQuery.trim().length >= 2 && (searchResults.length > 0 || searchKind !== null);

  const navCategories = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (q.length < 2) return ALL_CATEGORIES;
    // If the content search found hits, restrict the nav to those topics.
    if (searchResults.length > 0) {
      const hitIds = new Set(searchResults.map(h => h.id));
      return ALL_CATEGORIES.map(c => ({ ...c, items: c.items.filter(i => hitIds.has(i.id)) }))
        .filter(c => c.items.length > 0);
    }
    // Fallback: label/keyword filter.
    return ALL_CATEGORIES.map(category => {
      const filteredItems = category.items.filter(item =>
        item.label.toLowerCase().includes(q) ||
        item.id.toLowerCase().includes(q) ||
        item.keywords.some(kw => kw.toLowerCase().includes(q))
      );
      return { ...category, items: filteredItems };
    }).filter(category => category.items.length > 0);
  }, [searchQuery, searchResults]);

  const navigateTo = (id: string) => {
    setActive(id);
    // Keep the URL shareable; guard so the hashchange listener doesn't loop.
    if (hashTopic() !== id) globalThis.location.assign(`#${id}`);
    setSearchQuery('');
    setSearchFocused(false);
    if (opened) toggle();
  };

  const renderContent = () => {
    if (active === 'examples') {
      const q = exampleFilter.trim().toLowerCase();
      const matches = (title: string, desc: string, code: string) =>
        !q || title.toLowerCase().includes(q) || desc.toLowerCase().includes(q) || code.toLowerCase().includes(q);
      const wsCats = WORKSPACE_EXAMPLE_CATEGORIES
        .filter(([cat]) => !exampleCat || cat === exampleCat)
        .map(([cat, exs]) => [cat, exs.filter((ex) => matches(ex.title, ex.description, ex.text))] as const)
        .filter(([, exs]) => exs.length > 0);
      const cycleCats = EXAMPLE_CATEGORIES
        .filter(([cat]) => !exampleCat || cat === exampleCat)
        .map(([cat, exs]) => [cat, exs.filter((ex) => matches(ex.title, ex.description, ex.code))] as const)
        .filter(([, exs]) => exs.length > 0);
      const nothing = wsCats.length === 0 && cycleCats.length === 0;
      return (
        <Stack gap="md">
          <Title order={2} c="blue.4">Engineering Examples Library</Title>
          <Text>
            Verified, ready-to-run problems grouped by discipline. Each lists the
            result you should get, so you can confirm your solve. Use the{' '}
            <b>Copy Code</b> button, paste into the editor, and press{' '}
            <Code>F2</Code> (Solve).
          </Text>
          <Group gap="xs">
            <TextInput
              placeholder="Filter examples…"
              value={exampleFilter}
              onChange={(e) => setExampleFilter(e.currentTarget.value)}
              leftSection={<IconSearch size={14} />}
              rightSection={exampleFilter ? <CloseButton size="sm" onClick={() => setExampleFilter('')} /> : null}
              w={260}
            />
            <Group gap={6}>
              {ALL_EXAMPLE_CATEGORY_NAMES.map((cat) => (
                <Badge
                  key={cat}
                  variant={exampleCat === cat ? 'filled' : 'light'}
                  color="blue"
                  style={{ cursor: 'pointer', textTransform: 'none' }}
                  onClick={() => setExampleCat(exampleCat === cat ? null : cat)}
                >
                  {cat}
                </Badge>
              ))}
            </Group>
          </Group>
          {nothing && (
            <Alert color="gray" title="No examples match">
              <Text size="sm">Nothing matches this filter — clear the text or the category chip to see the full library.</Text>
            </Alert>
          )}
          <Alert color="blue" variant="light" icon={<IconBook size={18} />}>
            <Text size="sm">
              Examples that solve an implicit or transcendental equation mention a{' '}
              <b>guess</b> (e.g. “set a guess <Code>yn ≈ 0.6</Code>”). Open{' '}
              <Code>Ctrl + I</Code> (Variable Info), enter it, then solve — a good
              guess is usually what makes a nonlinear problem converge. Examples
              built on a <Code>PARAMETRIC</Code> table are solved from the Tables
              tab with <b>Solve Table</b>, not the main Solve.
            </Text>
          </Alert>
          {wsCats.length > 0 && (
            <Stack gap="xs">
              <Title order={3} c="cyan.4" mt="sm">Quick Workspace Examples</Title>
              <Text size="sm" c="dimmed">
                Additional ready-to-run documents (the rest are in the File →
                Open Example picker). Copy one into the editor and press Solve.
              </Text>
              {wsCats.map(([category, examples]) => (
                <Stack gap="xs" key={`ws-${category}`}>
                  <Title order={4} c="blue.3" mt="sm">{category}</Title>
                  <MantineAccordion variant="separated">
                    {examples.map((ex) => (
                      <MantineAccordion.Item value={ex.id} key={ex.id}>
                        <MantineAccordion.Control>
                          <Text fw={600} c="cyan.3">{ex.title}</Text>
                        </MantineAccordion.Control>
                        <MantineAccordion.Panel>
                          <Text size="sm" mb="xs">{ex.description}</Text>
                          <RunnableCode code={ex.text} title={ex.title} />
                        </MantineAccordion.Panel>
                      </MantineAccordion.Item>
                    ))}
                  </MantineAccordion>
                </Stack>
              ))}
            </Stack>
          )}
          {cycleCats.map(([category, examples]) => (
            <Stack gap="xs" key={category}>
              <Title order={4} c="blue.3" mt="sm">{category}</Title>
              <MantineAccordion variant="separated">
                {examples.map((ex) => (
                  <MantineAccordion.Item value={ex.value} key={ex.value}>
                    <MantineAccordion.Control>
                      <Text fw={600} c="cyan.3">{exampleShortTitle(ex.title)}</Text>
                    </MantineAccordion.Control>
                    <MantineAccordion.Panel>
                      <Text size="sm" mb="xs">{ex.description}</Text>
                      {ex.note && (
                        <Alert color="gray" py="xs" mb="sm">
                          {ex.note}
                        </Alert>
                      )}
                      {ex.diagram === 'BraytonCycle' && (
                        <Paper withBorder p="sm" mb="sm" bg="light-dark(var(--mantine-color-gray-1), var(--mantine-color-dark-8))">
                          <BraytonCycleDiagram />
                        </Paper>
                      )}
                      <RunnableCode code={ex.code} title={exampleShortTitle(ex.title)} />
                    </MantineAccordion.Panel>
                  </MantineAccordion.Item>
                ))}
              </MantineAccordion>
            </Stack>
          ))}
        </Stack>
      );
    }

    if (active.startsWith('refpage:')) {
      const page = REFERENCE_BY_SLUG.get(active.slice('refpage:'.length));
      if (page) return <ReferencePageView page={page} onNavigate={(slug) => navigateTo('refpage:' + slug)} onNavigateTopic={navigateTo} />;
    }

    // Reference data pages (single source of truth; the old Quick-Reference
    // duplicate tables are gone — symbols live on their per-symbol pages).
    if (active === 'ref-index') return <ReferenceIndex onNavigate={navigateTo} />;
    if (active === 'ref-fluids') return <FluidsReference />;
    if (active === 'ref-units') {
      return (
        <Stack gap="xl">
          <UnitsReference />
          <ConstantsReference />
        </Stack>
      );
    }

    // Category landing page: intro markdown (if any) + a card grid of its pages.
    const landingCategory = CATEGORY_BY_OVERVIEW.get(active);
    if (landingCategory) {
      return (
        <>
          <CategoryLanding category={landingCategory} intro={DOCS_CATALOG[active]} onNavigate={navigateTo} />
          <PrevNext active={active} onNavigate={navigateTo} />
        </>
      );
    }

    const docContent = DOCS_CATALOG[active];
    if (docContent) {
      return (
        <>
          <OnThisPage content={docContent} />
          <MarkdownRenderer content={docContent} onNavigate={navigateTo} />
          <PrevNext active={active} onNavigate={navigateTo} />
        </>
      );
    }

    return null;
  };

  return (
    <AppShell
      header={{ height: 60 }}
      navbar={{
        width: 320,
        breakpoint: 'sm',
        collapsed: { mobile: !opened },
      }}
      padding="md"
      styles={{
        main: {
          background: 'light-dark(var(--mantine-color-gray-0), var(--mantine-color-dark-8))',
          minHeight: 'calc(100vh - 60px)'
        }
      }}
    >
      <AppShell.Header bg="var(--mantine-color-body)" style={{ borderBottom: '1px solid var(--mantine-color-default-border)' }}>
        <Group h="100%" px="md" justify="space-between">
          <Group>
            <Burger opened={opened} onClick={toggle} hiddenFrom="sm" size="sm" />
            <Title order={3} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Text span inherit variant="gradient" gradient={{ from: 'blue.4', to: 'cyan.3', deg: 90 }}>
                frees
              </Text>
              <Text span inherit size="lg" c="dimmed" fw={500}>
                Documentation Portal
              </Text>
            </Title>
          </Group>
          <Badge color="blue" variant="filled" size="lg">{VERSION_LABEL}</Badge>
        </Group>
      </AppShell.Header>

      <AppShell.Navbar p="md" bg="var(--mantine-color-body)" style={{ borderRight: '1px solid var(--mantine-color-default-border)' }}>
        <Box style={{ position: 'relative', marginBottom: 'md' }}>
          <TextInput
            placeholder="Search docs, functions, examples…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.currentTarget.value)}
            onFocus={() => setSearchFocused(true)}
            onBlur={() => setTimeout(() => setSearchFocused(false), 180)}
            leftSection={<IconSearch size={16} />}
            rightSection={
              searchQuery ? (
                <CloseButton onClick={() => setSearchQuery('')} size="sm" />
              ) : null
            }
          />
          {showResults && (
            <Paper
              shadow="md"
              withBorder
              p="xs"
              style={{
                position: 'absolute', top: '100%', left: 0, right: 0, zIndex: 1000,
                maxHeight: '60vh', overflowY: 'auto',
              }}
            >
              <Group gap="xs" px="xs" pb={4} justify="space-between">
                <Text size="xs" c="dimmed" fw={700}>
                  {searchResults.length} result{searchResults.length === 1 ? '' : 's'}
                </Text>
                <Group gap={4}>
                  {SEARCH_FACETS.map(([kind, label]) => (
                    <Badge
                      key={kind}
                      size="xs"
                      variant={searchKind === kind ? 'filled' : 'light'}
                      color="blue"
                      style={{ cursor: 'pointer', textTransform: 'none' }}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => setSearchKind(searchKind === kind ? null : kind)}
                    >
                      {label}
                    </Badge>
                  ))}
                </Group>
              </Group>
              <Divider mb={4} />
              {searchResults.length === 0 && (
                <Text size="xs" c="dimmed" px="xs" py={6}>
                  No {SEARCH_FACETS.find(([k]) => k === searchKind)?.[1].toLowerCase()} results — click the chip again to search everything.
                </Text>
              )}
              {searchResults.map((hit, idx) => (
                <Box
                  key={`${hit.id}-${idx}`}
                  onClick={() => navigateTo(hit.id)}
                  px="xs"
                  py={6}
                  style={{ cursor: 'pointer', borderRadius: '4px' }}
                  onMouseDown={(e) => e.preventDefault()}
                  className="search-result-row"
                >
                  <Group gap="xs" justify="space-between" wrap="nowrap">
                    <Text size="sm" fw={600} style={{ flexGrow: 1 }}>
                      <Highlight highlight={searchQuery.trim()} component="span">
                        {hit.label}
                      </Highlight>
                    </Text>
                    <Badge size="xs" variant="light" color="gray">{hit.section}</Badge>
                  </Group>
                  {hit.snippet && (
                    <Text size="xs" c="dimmed" lineClamp={2} mt={2}>
                      <Highlight highlight={searchQuery.trim().split(/\s+/)} component="span">
                        {hit.snippet}
                      </Highlight>
                    </Text>
                  )}
                </Box>
              ))}
            </Paper>
          )}
        </Box>

        <AppShell.Section grow component={ScrollArea} offsetScrollbars>
          <Stack gap="md">
            {navCategories.map((category) => (
              <div key={category.title}>
                <Group gap="xs" px="xs" mb="xs">
                  {category.icon}
                  <Text size="xs" fw={700} c="dimmed" style={{ letterSpacing: '0.5px', textTransform: 'uppercase' }}>
                    {category.title}
                  </Text>
                </Group>
                {category.items.map((item) => (
                  <NavLink
                    key={item.id}
                    label={item.label}
                    active={active === item.id}
                    onClick={() => navigateTo(item.id)}
                    variant="filled"
                    color="blue"
                    styles={{
                      label: { fontSize: '13px', fontWeight: active === item.id ? 600 : 400 },
                      root: { borderRadius: '6px', marginBottom: '2px', paddingLeft: '12px' }
                    }}
                  />
                ))}
              </div>
            ))}
            {navCategories.length === 0 && (
              <Text size="sm" c="dimmed" ta="center" mt="md">
                No matching topics found.
              </Text>
            )}
          </Stack>
        </AppShell.Section>
      </AppShell.Navbar>

      <AppShell.Main>
        <Container size="md" pt="md" pb="xl">
          {renderContent()}
        </Container>
      </AppShell.Main>
    </AppShell>
  );
}
