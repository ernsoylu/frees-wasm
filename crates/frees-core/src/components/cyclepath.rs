//! Thermodynamic state points and the cycle-path trace the property plots
//! overlay.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/api/CyclePathResolver.java`
//! (669 lines).
//!
//! # Two concerns, both pure functions of a solved system
//!
//! 1. **Fill missing properties.** Solver variables that name a fluid state
//!    (`T1`, `P_2`, `h[3]`, `Pw_1`) are grouped by their state index, and every
//!    property the state does not already carry is flashed from the two it
//!    does ([`resolve_missing_properties`]). The write-back keeps the
//!    document's own naming style, so a `Pw_1` circuit gains `hw_1`, not `h1`.
//! 2. **Interpolate the cycle.** Consecutive states are joined by a smooth
//!    process path — isobaric, isentropic, isothermal, isenthalpic, isochoric,
//!    or a straight line when none of those fit — so the frontend can draw the
//!    cycle on a T-s or P-h dome ([`generate_cycle_path`]). The path is
//!    **closed**: the last state is joined back to the first.
//!
//! # The COMPONENT layer plots as a cycle
//!
//! [`generate_cycle_path`] also recognises component stream members — `s1.P`,
//! `s2.h` — indexing the state by the *stream name's trailing digits*. That is
//! what makes a component-built Rankine cycle overlay the same dome a
//! hand-written `T1/P1/h1` document does. (Fill-missing deliberately does not:
//! stream members are solved by the expander, not back-filled.)
//!
//! # Wire shape
//!
//! [`generate_cycle_path`] returns `SolveResponse.cyclePath`, typed in
//! `web/src/api.ts` as `Record<string, number>[]`. Each point carries the keys
//! `T`, `P`, `h`, `s`, `v` — whichever of them the flash could resolve; the
//! renderer (`web/src/plots/figure.ts`) picks the two the axes need and drops
//! points that lack them, so a partially resolved point is safe.
//!
//! # Calling this from the boundary
//!
//! The `SolveController.resolveFillMissing` sequence, in this port's terms —
//! fill first, then plot the filled map, and stamp the injected variables'
//! units, which the unit checker cannot know because they were never in the
//! text:
//!
//! ```ignore
//! let added = cyclepath::resolve_missing_properties(
//!     &mut solution.values, &mut solution.display_names, source, None, &[]);
//! for name in &added {
//!     if let Some(unit) = cyclepath::si_unit_for_state_variable(name) {
//!         solution.inferred_units.entry(name.clone()).or_insert_with(|| unit.to_string());
//!     }
//! }
//! let cycle_path = cyclepath::generate_cycle_path(
//!     &solution.values, props::propfun::detect_fluid(source));
//! ```
//!
//! Both steps run **only when the request asked for fill-missing**; a plain
//! solve returns an empty `cyclePath` and injects nothing.
//!
//! # What this port's property backend can and cannot flash
//!
//! Every lookup goes through [`crate::props::propfun`], which in the browser is
//! the `(P,h)` split-table backend (decision D1), not CoolProp. It answers a
//! pair only when pressure is one of the two inputs (or the saturation pair
//! `(T, Q)`). Two of the six interpolation branches ask for something else:
//!
//! * **isothermal** — `(T, Smass)`, and
//! * **isochoric** — `(T, Dmass)`.
//!
//! Their points therefore come back carrying only the two properties the flash
//! was *given* (`T` and `s`, `T` and `v` respectively — [`get_flash_val`]
//! short-circuits an output that is already an input) instead of the full five.
//! That is a backend limitation, transcribed honestly rather than papered over
//! with an approximation: the plot draws a thinner line, never a wrong one. The
//! other four branches — isobaric, isentropic, isenthalpic and the linear
//! fallback — are fully served.
//!
//! # Deliberate divergence
//!
//! A state index that overflows a Java `int` (`t99999999999999 = 3`) makes the
//! Java `Integer.parseInt` throw out of the middle of a solve. The port skips
//! the variable instead: the name is not a state either way, and a display
//! layer must not be able to fail a solved document.

use std::collections::BTreeMap;

use crate::props::propfun;

/// CoolProp's canonical mass-basis parameter names, transcribed from the Java
/// constants of the same name.
const HMASS: &str = "Hmass";
const SMASS: &str = "Smass";
const DMASS: &str = "Dmass";

/// One entry of the Java `PROPERTY_ALIASES` map: a spelling a state variable
/// may start with, and the canonical property it means.
const PROPERTY_ALIASES: [(&str, &str); 17] = [
    ("t", "T"),
    ("drybulb", "T"),
    ("tdrybulb", "T"),
    ("p", "P"),
    ("pressure", "P"),
    ("v", "v"),
    ("volume", "v"),
    ("u", "u"),
    ("internalenergy", "u"),
    ("h", "h"),
    ("enthalpy", "h"),
    ("s", "s"),
    ("entropy", "s"),
    ("x", "x"),
    ("quality", "x"),
    ("rho", "rho"),
    ("density", "rho"),
];

/// [`PROPERTY_ALIASES`] keys, **longest first**, for leading-prefix matching of
/// declared state-table variables (so `rho…` beats `r…`).
///
/// The Java sorts the key set by descending length at class-init; because no
/// two keys of equal length can prefix the same string, that sort is a total
/// order for this use even though `Map.ofEntries` has no defined iteration
/// order. Written out here so the order is a fact of the source, not of a JVM
/// run.
const PROPERTY_PREFIXES: [&str; 17] = [
    "internalenergy",
    "enthalpy",
    "pressure",
    "tdrybulb",
    "density",
    "drybulb",
    "entropy",
    "quality",
    "volume",
    "rho",
    "h",
    "p",
    "s",
    "t",
    "u",
    "v",
    "x",
];

/// SI units of the canonical state properties this resolver fills in (quality
/// `x` is dimensionless and deliberately absent).
const PROPERTY_SI_UNITS: [(&str, &str); 7] = [
    ("T", "K"),
    ("P", "Pa"),
    ("v", "m^3/kg"),
    ("u", "J/kg"),
    ("h", "J/kg"),
    ("s", "J/kg-K"),
    ("rho", "kg/m^3"),
];

/// The property pairs a state is flashed from, most preferred first.
struct PropPair {
    key1: &'static str,
    val_key1: &'static str,
    key2: &'static str,
    val_key2: &'static str,
}

const fn pp(
    key1: &'static str,
    val_key1: &'static str,
    key2: &'static str,
    val_key2: &'static str,
) -> PropPair {
    PropPair {
        key1,
        val_key1,
        key2,
        val_key2,
    }
}

const PREFERRED_PAIRS: [PropPair; 12] = [
    pp("P", "P", "h", HMASS),
    pp("P", "P", "s", SMASS),
    pp("h", HMASS, "s", SMASS),
    pp("P", "P", "x", "Q"),
    pp("T", "T", "x", "Q"),
    pp("T", "T", "P", "P"),
    pp("T", "T", "s", SMASS),
    pp("T", "T", "h", HMASS),
    pp("P", "P", "v", DMASS),
    pp("T", "T", "v", DMASS),
    pp("P", "P", "rho", DMASS),
    pp("T", "T", "rho", DMASS),
];

/// The canonical outputs a flashed state writes back, with the CoolProp
/// parameter each is read as. The Java builds this with `Map.of`, whose
/// iteration order is unspecified — and irrelevant, because every entry writes
/// a different variable and reads nothing the others wrote.
const OUTPUTS: [(&str, &str); 6] = [
    ("T", "T"),
    ("P", "P"),
    ("h", HMASS),
    ("s", SMASS),
    ("u", "Umass"),
    ("rho", DMASS),
];

/// A `STATE TABLE name(vars…) FLUID = X END` block — the port of
/// `ast/StateTableDef.java`.
///
/// Declaring one makes property look-ups fluid-aware per circuit, so a Water
/// state 1 and an R134a state 1 never collide. Phase 8 taught the parser the
/// block — it lands in
/// [`Document::blocks`](crate::parser::Document::blocks) as a
/// [`StateTableDef`](crate::parser::blocks::StateTableDef), whose `name`,
/// `variables` and `fluid` are exactly this shape — but the fill-missing
/// callers do not thread it through yet and still pass an empty slice, which
/// makes [`resolve_missing_properties`] take the legacy global index
/// detection, exactly as the Java does for a document that declares no blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTableSpec {
    /// The circuit/table name (`WaterCircuit1`). Carried for fidelity with the
    /// AST record and the `stateTableDefs` wire shape; the resolver reads only
    /// the other two fields.
    pub name: String,
    /// The declared state-point variables, lowercase (`["pw1", "tw1"]`).
    pub variables: Vec<String>,
    /// The CoolProp fluid every state in this block uses; `None` when the block
    /// declared no `FLUID = …` line.
    pub fluid: Option<String>,
}

/// The SI unit for a state-indexed property variable (`h1`, `rho[2]`, `T_3`,
/// …), or `None` when the name is not a recognisable state property.
///
/// Fill-missing injects these variables into the result **after** the solve, so
/// they never appear in the equation text the unit checker reads; callers stamp
/// their units from the property identity instead (otherwise the workspace
/// shows them unitless). Port of `siUnitForStateVariable`.
///
/// Note the deliberate asymmetry with the grouping code: this looks the base up
/// **without** stripping underscores, so `r_ho1` groups as a density but is not
/// unit-stamped. That is the Java behaviour, transcribed.
pub fn si_unit_for_state_variable(name: &str) -> Option<&'static str> {
    let (base, _) = match_state_var_index(name)?;
    let canonical = property_alias(&base.to_ascii_lowercase())?;
    property_si_unit(canonical)
}

/// Back-fills the missing fluid properties of every detected state point in
/// `values`, returning the **lowercase names it added** (a solved variable that
/// was already present is overwritten, not reported).
///
/// Port of `resolveMissingProperties`. `text` is the document, used only to
/// detect the default working fluid; `target_variables` restricts the
/// write-back to a requested set (`None` = fill everything), and `state_tables`
/// drives fluid-aware per-circuit grouping when the document declares any.
///
/// The Java returns a rebuilt `Result` and repeats the work for every
/// alternative solution; this port mutates the one map it is given, and the
/// caller repeats it per solution. Nothing happens at all when no property
/// backend is installed.
///
/// The added names are the port's answer to the Java controller's
/// `result != rawResult` set difference: pass each through
/// [`si_unit_for_state_variable`] to stamp its unit.
pub fn resolve_missing_properties(
    values: &mut BTreeMap<String, f64>,
    display_names: &mut BTreeMap<String, String>,
    text: &str,
    target_variables: Option<&[String]>,
    state_tables: &[StateTableSpec],
) -> Vec<String> {
    if !propfun::is_available() {
        return Vec::new();
    }
    // `detect_fluid` already answers "Water" for a document that names none,
    // which is the Java's explicit null-default.
    let default_fluid = propfun::detect_fluid(text);

    let mut sink = Sink {
        values,
        display_names,
        added: Vec::new(),
    };
    if state_tables.is_empty() {
        resolve_for_variables(&mut sink, default_fluid, target_variables);
    } else {
        // Each block's states resolve with that block's own fluid, so a Water
        // circuit's P1 and an R134a circuit's P1 never collide.
        for st in state_tables {
            let fluid = match st.fluid.as_deref() {
                Some(f) if !f.trim().is_empty() => f,
                _ => default_fluid,
            };
            resolve_block_states(&mut sink, &st.variables, fluid, target_variables);
        }
    }
    sink.added
}

/// The ordered list of flashed state points connecting every solved state, for
/// overlaying the cycle on a property plot.
///
/// Port of `generateCyclePath`. Returns an empty path when no property backend
/// is installed or fewer than two states are present. `values` is expected to
/// be the map **after** [`resolve_missing_properties`] has run, exactly as the
/// Java controller sequences the two.
pub fn generate_cycle_path(
    values: &BTreeMap<String, f64>,
    fluid: &str,
) -> Vec<BTreeMap<String, f64>> {
    let mut path = Vec::new();
    if !propfun::is_available() {
        return path;
    }
    let state_knowns = group_state_knowns(values);
    // A `BTreeMap` is already the Java's `Collections.sort(indices)`.
    let indices: Vec<i32> = state_knowns.keys().copied().collect();
    if indices.len() < 2 {
        return path;
    }
    let segments_count = indices.len();
    for (i, index) in indices.iter().enumerate() {
        let state_a = &state_knowns[index];
        let state_b = &state_knowns[&indices[(i + 1) % segments_count]];

        let segment_points = interpolate_process(state_a, state_b, fluid);
        if i > 0 && !segment_points.is_empty() {
            path.extend_from_slice(&segment_points[1..]);
        } else {
            path.extend(segment_points);
        }
    }
    path
}

// ---------------------------------------------------------------------------
// State detection
// ---------------------------------------------------------------------------

/// The canonical properties known for one state index. Keys are the canonical
/// names out of [`PROPERTY_ALIASES`] (`"T"`, `"P"`, `"v"`, `"u"`, `"h"`,
/// `"s"`, `"x"`, `"rho"`).
type Knowns = BTreeMap<&'static str, f64>;

/// How one state index spells its variables, so a computed property writes back
/// under the same naming: `%s` + `tag` + `tail` in the Java's format template
/// (`"%sw_1"` → `hw_1`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct StateStyle {
    /// The circuit tag between the property symbol and the index (the `w` of
    /// `Pw_1`); empty for the legacy global styles.
    tag: String,
    /// The index as it is written: `1`, `_1` or `[1]`.
    tail: String,
}

impl StateStyle {
    fn name_of(&self, prop: &str) -> String {
        format!("{prop}{}{}", self.tag, self.tail)
    }
}

#[derive(Debug, Default)]
struct StateData {
    known: BTreeMap<i32, Knowns>,
    style: BTreeMap<i32, StateStyle>,
}

/// The two maps a fill writes into, plus the record of what it added.
struct Sink<'a> {
    values: &'a mut BTreeMap<String, f64>,
    display_names: &'a mut BTreeMap<String, String>,
    added: Vec<String>,
}

/// Port of `parseStateVariables` + `parseAndPopulateState`.
fn parse_state_variables(values: &BTreeMap<String, f64>) -> StateData {
    let mut data = StateData::default();
    for (name, value) in values {
        parse_and_populate_state(name, *value, &mut data);
    }
    data
}

fn parse_and_populate_state(name: &str, value: f64, data: &mut StateData) {
    let Some((prop_name, digits)) = match_state_var_index(name) else {
        return;
    };
    let Some(index) = parse_index(digits) else {
        return;
    };
    let base: String = prop_name
        .chars()
        .filter(|c| *c != '_')
        .collect::<String>()
        .to_ascii_lowercase();
    let Some(canonical) = property_alias(&base) else {
        return;
    };
    data.known
        .entry(index)
        .or_default()
        .insert(canonical, value);

    data.style.entry(index).or_insert_with(|| {
        let tail = if name.contains('[') {
            format!("[{index}]")
        } else if name.contains('_') {
            format!("_{index}")
        } else {
            format!("{index}")
        };
        StateStyle {
            tag: String::new(),
            tail,
        }
    });
}

/// Groups every state the *cycle path* can plot, which is a superset of the
/// fill-missing grouping: it also accepts a COMPONENT-layer stream member
/// (`s1.p`, `s2.h`), indexed by the stream name's trailing digits.
///
/// Port of `groupStateKnowns`. A name that matches the state-variable pattern
/// but names no known property is dropped there and *not* retried as a stream
/// member — the Java `continue` is load-bearing.
fn group_state_knowns(values: &BTreeMap<String, f64>) -> BTreeMap<i32, Knowns> {
    let mut state_knowns: BTreeMap<i32, Knowns> = BTreeMap::new();
    for (name, value) in values {
        if let Some((prop_name, digits)) = match_state_var_index(name) {
            let Some(index) = parse_index(digits) else {
                continue;
            };
            let base: String = prop_name
                .chars()
                .filter(|c| *c != '_')
                .collect::<String>()
                .to_ascii_lowercase();
            if let Some(canonical) = property_alias(&base) {
                state_knowns
                    .entry(index)
                    .or_default()
                    .insert(canonical, *value);
            }
            continue;
        }
        // COMPONENT layer (§6): a stream member s1.p / s2.h plots as a cycle
        // state, indexed by the stream name's trailing digits.
        if let Some((_, digits, member)) = match_component_stream(name) {
            if let Some(canonical) = property_alias(&member.to_ascii_lowercase()) {
                let Some(index) = parse_index(digits) else {
                    continue;
                };
                state_knowns
                    .entry(index)
                    .or_default()
                    .insert(canonical, *value);
            }
        }
    }
    state_knowns
}

/// Parses a declared state-table variable as `<prop><tag><index>`: the longest
/// leading property symbol (`P`, `T`, `h`, …) is the property, any middle
/// characters are the circuit tag (the `w` of `Pw_1`), and the trailing digits
/// are the state index. The tag is preserved in the write-back style so
/// computed properties (`hw_1`) keep the same naming.
///
/// Port of `parseBlockState`.
fn parse_block_state(name: &str, value: f64, data: &mut StateData) {
    let lower = name.to_ascii_lowercase();
    let (prefix, index, tail) = if let Some((head, digits)) = match_bracket_state(&lower) {
        let Some(index) = parse_index(digits) else {
            return;
        };
        (head.to_string(), index, format!("[{index}]"))
    } else if let Some((head, underscore, digits)) = match_plain_state(&lower) {
        let Some(index) = parse_index(digits) else {
            return;
        };
        (head.to_string(), index, format!("{underscore}{index}"))
    } else {
        return;
    };

    let mut matched = None;
    for sym in PROPERTY_PREFIXES {
        if prefix.starts_with(sym) {
            matched = property_alias(sym).map(|canonical| (canonical, &prefix[sym.len()..]));
            break;
        }
    }
    let Some((canonical, tag)) = matched else {
        return;
    };
    data.known
        .entry(index)
        .or_default()
        .insert(canonical, value);
    data.style.entry(index).or_insert_with(|| StateStyle {
        tag: tag.to_string(),
        tail,
    });
}

/// `Integer.parseInt` on the digit run. The Java throws on overflow; see the
/// module docs for why the port skips the variable instead.
fn parse_index(digits: &str) -> Option<i32> {
    digits.parse::<i32>().ok()
}

fn property_alias(lower: &str) -> Option<&'static str> {
    PROPERTY_ALIASES
        .iter()
        .find(|(key, _)| *key == lower)
        .map(|(_, canonical)| *canonical)
}

fn property_si_unit(canonical: &str) -> Option<&'static str> {
    PROPERTY_SI_UNITS
        .iter()
        .find(|(prop, _)| *prop == canonical)
        .map(|(_, unit)| *unit)
}

// ---------------------------------------------------------------------------
// Pattern matching
//
// Hand-rolled ports of the four Java regexes; there is no regex crate in this
// workspace and these four are small, closed grammars. Java's `\d` is ASCII by
// default and `[a-zA-Z]` obviously is, so byte-wise ASCII matching is the exact
// semantics rather than an approximation.
// ---------------------------------------------------------------------------

/// `^([a-zA-Z][a-zA-Z_]*?)(_?)(\d+)$` — `(head, absorbed "_" or "", digits)`.
///
/// The reluctant head takes the shortest run for which the rest still matches,
/// and the optional `_` is greedy, so the *last* underscore before the trailing
/// digit run is the separator, not part of the head: `t__1` splits as
/// `("t_", "_", "1")` and `motor_temp_5` as `("motor_temp", "_", "5")`.
fn match_plain_state(name: &str) -> Option<(&str, &str, &str)> {
    let b = name.as_bytes();
    let mut t = b.len();
    while t > 0 && b[t - 1].is_ascii_digit() {
        t -= 1;
    }
    if t == b.len() || t == 0 {
        return None; // no trailing digits, or no head at all
    }
    if !is_ident_head(&b[..t]) {
        return None;
    }
    if t >= 2 && b[t - 1] == b'_' {
        Some((&name[..t - 1], &name[t - 1..t], &name[t..]))
    } else {
        Some((&name[..t], "", &name[t..]))
    }
}

/// `^([a-zA-Z][a-zA-Z_]*)\[(\d+)\]$` — `(head, digits)`.
fn match_bracket_state(name: &str) -> Option<(&str, &str)> {
    let b = name.as_bytes();
    if b.last() != Some(&b']') {
        return None;
    }
    let open = name.find('[')?;
    if !is_ident_head(&b[..open]) {
        return None;
    }
    let digits = &name[open + 1..b.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&name[..open], digits))
}

/// The Java `STATE_VAR_INDEX` alternation
/// `^([a-zA-Z][a-zA-Z_]*?)_?(\d+)$|^([a-zA-Z][a-zA-Z_]*)\[(\d+)\]$`, returning
/// the `(group 1 | group 3, group 2 | group 4)` selection every caller makes.
fn match_state_var_index(name: &str) -> Option<(&str, &str)> {
    if let Some((head, _, digits)) = match_plain_state(name) {
        return Some((head, digits));
    }
    match_bracket_state(name)
}

/// `^([a-zA-Z][a-zA-Z_]*?)(\d+)\.([a-zA-Z]+)$` — `(stream, digits, member)`.
fn match_component_stream(name: &str) -> Option<(&str, &str, &str)> {
    let dot = name.find('.')?;
    let member = &name[dot + 1..];
    if member.is_empty() || !member.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let head_part = &name[..dot];
    let b = head_part.as_bytes();
    let mut t = b.len();
    while t > 0 && b[t - 1].is_ascii_digit() {
        t -= 1;
    }
    if t == b.len() || t == 0 || !is_ident_head(&b[..t]) {
        return None;
    }
    Some((&head_part[..t], &head_part[t..], member))
}

/// `[a-zA-Z][a-zA-Z_]*` over a non-empty byte slice.
fn is_ident_head(bytes: &[u8]) -> bool {
    match bytes.first() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    bytes.iter().all(|c| c.is_ascii_alphabetic() || *c == b'_')
}

// ---------------------------------------------------------------------------
// Fill missing properties
// ---------------------------------------------------------------------------

/// Port of `resolveForVariables` — the legacy global index detection, used when
/// the document declares no `STATE TABLE` block.
fn resolve_for_variables(sink: &mut Sink<'_>, fluid: &str, targets: Option<&[String]>) {
    let data = parse_state_variables(sink.values);
    for (index, knowns) in &data.known {
        if knowns.len() < 2 {
            continue;
        }
        if let Some(style) = data.style.get(index) {
            solve_single_state(sink, knowns, style, fluid, targets);
        }
    }
}

/// Groups only this block's declared variables by state index and fills the
/// missing properties of each state with the block's fluid.
/// Port of `resolveBlockStates`.
fn resolve_block_states(
    sink: &mut Sink<'_>,
    declared_vars: &[String],
    fluid: &str,
    targets: Option<&[String]>,
) {
    let mut data = StateData::default();
    for var in declared_vars {
        // The Java map is case-insensitive; this one is keyed lowercase.
        if let Some(value) = sink.values.get(&var.to_ascii_lowercase()).copied() {
            parse_block_state(var, value, &mut data);
        }
    }
    for (index, knowns) in &data.known {
        if knowns.len() < 2 {
            continue;
        }
        if let Some(style) = data.style.get(index) {
            solve_single_state(sink, knowns, style, fluid, targets);
        }
    }
}

/// The inputs one flash is made with: the matched pair and its two values,
/// already converted to the CoolProp parameter's own basis (`v` → density).
struct FlashInputs<'a> {
    pair: &'a PropPair,
    value1: f64,
    value2: f64,
    fluid: &'a str,
}

/// Port of `solveSingleState`.
fn solve_single_state(
    sink: &mut Sink<'_>,
    knowns: &Knowns,
    style: &StateStyle,
    fluid: &str,
    targets: Option<&[String]>,
) {
    let Some(pair) = find_matched_pair(knowns) else {
        return;
    };
    let input1 = knowns[pair.key1];
    let input2 = knowns[pair.key2];
    let inputs = FlashInputs {
        pair,
        value1: if pair.key1 == "v" {
            1.0 / input1
        } else {
            input1
        },
        value2: if pair.key2 == "v" {
            1.0 / input2
        } else {
            input2
        },
        fluid,
    };

    let mut solved: Knowns = BTreeMap::new();
    for (canonical, output) in OUTPUTS {
        if knowns.contains_key(canonical) || should_skip_prop(canonical, style, targets) {
            continue;
        }
        let res = get_prop_or_nan(
            output,
            inputs.pair.val_key1,
            inputs.value1,
            inputs.pair.val_key2,
            inputs.value2,
            inputs.fluid,
        );
        if res.is_finite() {
            solved.insert(canonical, res);
        }
    }

    resolve_specific_volume(&mut solved, knowns, style, targets, &inputs);
    resolve_quality(&mut solved, knowns, style, targets, &inputs);

    populate_solved_properties(sink, &solved, style);
}

fn find_matched_pair(knowns: &Knowns) -> Option<&'static PropPair> {
    PREFERRED_PAIRS
        .iter()
        .find(|pair| knowns.contains_key(pair.key1) && knowns.contains_key(pair.key2))
}

/// Resolves specific volume `v = 1/ρ` (reusing an already-solved density when
/// present). Port of `resolveSpecificVolume`.
fn resolve_specific_volume(
    solved: &mut Knowns,
    knowns: &Knowns,
    style: &StateStyle,
    targets: Option<&[String]>,
    inputs: &FlashInputs<'_>,
) {
    if knowns.contains_key("v") || should_skip_prop("v", style, targets) {
        return;
    }
    let res_dmass = match solved.get("rho") {
        Some(rho) => *rho,
        None => get_prop_or_nan(
            DMASS,
            inputs.pair.val_key1,
            inputs.value1,
            inputs.pair.val_key2,
            inputs.value2,
            inputs.fluid,
        ),
    };
    if res_dmass.is_finite() && res_dmass != 0.0 {
        solved.insert("v", 1.0 / res_dmass);
    }
}

/// Resolves vapour quality `x = Q`. Port of `resolveQuality`.
fn resolve_quality(
    solved: &mut Knowns,
    knowns: &Knowns,
    style: &StateStyle,
    targets: Option<&[String]>,
    inputs: &FlashInputs<'_>,
) {
    if knowns.contains_key("x") || should_skip_prop("x", style, targets) {
        return;
    }
    let res_q = get_prop_or_nan(
        "Q",
        inputs.pair.val_key1,
        inputs.value1,
        inputs.pair.val_key2,
        inputs.value2,
        inputs.fluid,
    );
    if res_q.is_finite() {
        solved.insert("x", res_q);
    }
}

/// Port of `shouldSkipProp`: with a target set in play, only the properties
/// that set names are written.
fn should_skip_prop(prop: &str, style: &StateStyle, targets: Option<&[String]>) -> bool {
    let Some(targets) = targets else {
        return false;
    };
    let var_name = style.name_of(&cased_prop(prop));
    !contains_ignore_case(targets, &var_name)
}

/// Port of the casing chain the Java repeats in `shouldSkipProp` and
/// `populateSolvedProperties`. Every branch is an identity on the canonical
/// property names this resolver uses; transcribed rather than dropped, because
/// it is the shape that decides the written variable's spelling.
fn cased_prop(prop: &str) -> String {
    match prop {
        "rho" => "rho".to_string(),
        "v" | "h" | "s" | "u" | "x" => prop.to_ascii_lowercase(),
        "T" | "P" => prop.to_ascii_uppercase(),
        other => other.to_string(),
    }
}

/// Port of `populateSolvedProperties`.
fn populate_solved_properties(sink: &mut Sink<'_>, solved: &Knowns, style: &StateStyle) {
    for (prop_name, value) in solved {
        let var_name = style.name_of(&cased_prop(prop_name));
        let key = var_name.to_ascii_lowercase();
        if sink.values.insert(key.clone(), *value).is_none() {
            sink.added.push(key.clone());
        }
        sink.display_names.insert(key, var_name);
    }
}

fn contains_ignore_case(list: &[String], target: &str) -> bool {
    list.iter().any(|s| s.eq_ignore_ascii_case(target))
}

/// Port of `getPropOrNaN`: an out-of-range or unsupported property request is
/// `NaN`, meaning "unavailable", never an error the caller must handle.
/// [`propfun::props_si_or_nan`] already collapses the failure the Java catches.
///
/// The Java also logs the swallowed exception at debug level; core has no
/// logging framework (and a wasm build has nowhere to send it), so the message
/// is dropped rather than faked. The refusal itself is never silent — it is
/// visible as a property the state simply does not gain.
fn get_prop_or_nan(
    output: &str,
    name1: &str,
    prop1: f64,
    name2: &str,
    prop2: f64,
    fluid: &str,
) -> f64 {
    propfun::props_si_or_nan(output, name1, prop1, name2, prop2, fluid)
}

// ---------------------------------------------------------------------------
// Process interpolation
// ---------------------------------------------------------------------------

/// The number of sub-intervals per process segment (31 points). Transcribed
/// from `interpolateProcess`.
const STEPS: usize = 30;

/// Port of `interpolateProcess`: the first process whose invariant both states
/// share wins, and an unrelated pair falls back to a straight line through
/// whatever properties both carry.
fn interpolate_process(
    state_a: &Knowns,
    state_b: &Knowns,
    fluid: &str,
) -> Vec<BTreeMap<String, f64>> {
    let p_a = state_a.get("P").copied();
    let p_b = state_b.get("P").copied();
    let t_a = state_a.get("T").copied();
    let t_b = state_b.get("T").copied();
    let s_a = state_a.get("s").copied();
    let s_b = state_b.get("s").copied();
    let h_a = state_a.get("h").copied();
    let h_b = state_b.get("h").copied();
    let v_a = state_a.get("v").copied();
    let v_b = state_b.get("v").copied();

    if let (Some(s_a), Some(s_b)) = (s_a, s_b) {
        if is_close(p_a, p_b) {
            return interpolate_isobaric(p_a.expect("is_close implies present"), s_a, s_b, fluid);
        }
    }
    if let (Some(p_a), Some(p_b)) = (p_a, p_b) {
        if is_close(s_a, s_b) {
            return interpolate_isentropic(s_a.expect("is_close implies present"), p_a, p_b, fluid);
        }
    }
    if let (Some(s_a), Some(s_b)) = (s_a, s_b) {
        if is_close(t_a, t_b) {
            return interpolate_isothermal(t_a.expect("is_close implies present"), s_a, s_b, fluid);
        }
    }
    if let (Some(p_a), Some(p_b)) = (p_a, p_b) {
        if is_close(h_a, h_b) {
            return interpolate_isenthalpic(
                h_a.expect("is_close implies present"),
                p_a,
                p_b,
                fluid,
            );
        }
    }
    if let (Some(t_a), Some(t_b)) = (t_a, t_b) {
        if is_close(v_a, v_b) {
            return interpolate_isochoric(v_a.expect("is_close implies present"), t_a, t_b, fluid);
        }
    }
    interpolate_default(state_a, state_b)
}

fn interpolate_isobaric(p: f64, s_a: f64, s_b: f64, fluid: &str) -> Vec<BTreeMap<String, f64>> {
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let s = s_a + u * (s_b - s_a);
            flash("P", p, SMASS, s, fluid, Some(s), Some(p))
        })
        .collect()
}

fn interpolate_isentropic(s: f64, p_a: f64, p_b: f64, fluid: &str) -> Vec<BTreeMap<String, f64>> {
    let log_pa = libm::log(p_a);
    let log_pb = libm::log(p_b);
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let p = libm::exp(log_pa + u * (log_pb - log_pa));
            flash("P", p, SMASS, s, fluid, Some(s), Some(p))
        })
        .collect()
}

fn interpolate_isothermal(t: f64, s_a: f64, s_b: f64, fluid: &str) -> Vec<BTreeMap<String, f64>> {
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let s = s_a + u * (s_b - s_a);
            flash("T", t, SMASS, s, fluid, Some(s), None)
        })
        .collect()
}

fn interpolate_isenthalpic(h: f64, p_a: f64, p_b: f64, fluid: &str) -> Vec<BTreeMap<String, f64>> {
    let log_pa = libm::log(p_a);
    let log_pb = libm::log(p_b);
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let p = libm::exp(log_pa + u * (log_pb - log_pa));
            flash("P", p, HMASS, h, fluid, None, Some(p))
        })
        .collect()
}

fn interpolate_isochoric(v: f64, t_a: f64, t_b: f64, fluid: &str) -> Vec<BTreeMap<String, f64>> {
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let t = t_a + u * (t_b - t_a);
            flash("T", t, DMASS, 1.0 / v, fluid, None, None)
        })
        .collect()
}

/// A straight line through every property both states carry — no property
/// backend involved. Port of `interpolateDefault`.
fn interpolate_default(state_a: &Knowns, state_b: &Knowns) -> Vec<BTreeMap<String, f64>> {
    const KEYS: [&str; 5] = ["T", "P", "v", "h", "s"];
    (0..=STEPS)
        .map(|i| {
            let u = i as f64 / STEPS as f64;
            let mut pt = BTreeMap::new();
            for key in KEYS {
                if let (Some(a), Some(b)) = (state_a.get(key), state_b.get(key)) {
                    pt.insert(key.to_string(), a + u * (b - a));
                }
            }
            pt
        })
        .collect()
}

/// Port of `getFlashVal`: an output that *is* one of the two inputs is returned
/// as given rather than round-tripped through the property backend.
fn get_flash_val(prop: &str, name1: &str, val1: f64, name2: &str, val2: f64, fluid: &str) -> f64 {
    if prop == name1 {
        return val1;
    }
    if prop == name2 {
        return val2;
    }
    propfun::props_si_or_nan(prop, name1, val1, name2, val2, fluid)
}

/// One point of a process path: whichever of `T`, `P`, `h`, `s`, `v` the flash
/// could resolve, with `P` and `s` falling back to the value the segment was
/// parameterised by. Port of `flash`.
///
/// The Java asks CoolProp for entropy under its short alias `S`; CoolProp
/// resolves that to `Smass`, and the tabulated backend here matches on
/// canonical parameter names only, so the port spells the same request
/// `Smass`. `getFlashVal`'s input short-circuit still fires, because the
/// segment's input pair is spelled the same way.
fn flash(
    name1: &str,
    val1: f64,
    name2: &str,
    val2: f64,
    fluid: &str,
    fallback_s: Option<f64>,
    fallback_p: Option<f64>,
) -> BTreeMap<String, f64> {
    let mut pt = BTreeMap::new();

    // Output properties we want: T, P, v, h, s
    let t_val = get_flash_val("T", name1, val1, name2, val2, fluid);
    let p_val = get_flash_val("P", name1, val1, name2, val2, fluid);
    let h_val = get_flash_val(HMASS, name1, val1, name2, val2, fluid);
    let s_val = get_flash_val(SMASS, name1, val1, name2, val2, fluid);
    let d_val = get_flash_val(DMASS, name1, val1, name2, val2, fluid);

    if t_val.is_finite() {
        pt.insert("T".to_string(), t_val);
    }
    if p_val.is_finite() {
        pt.insert("P".to_string(), p_val);
    } else if let Some(p) = fallback_p {
        pt.insert("P".to_string(), p);
    }

    if h_val.is_finite() {
        pt.insert("h".to_string(), h_val);
    }
    if s_val.is_finite() {
        pt.insert("s".to_string(), s_val);
    } else if let Some(s) = fallback_s {
        pt.insert("s".to_string(), s);
    }

    if d_val.is_finite() && d_val != 0.0 {
        pt.insert("v".to_string(), 1.0 / d_val);
    }

    pt
}

/// Within 1 % of each other, both present. Port of `isClose`.
///
/// `Math.max` propagates NaN where Rust's `f64::max` swallows it, so the
/// comparison is written out: a NaN state property must make the branch fall
/// through, not match.
fn is_close(a: Option<f64>, b: Option<f64>) -> bool {
    let (Some(a), Some(b)) = (a, b) else {
        return false;
    };
    let (x, y) = (a.abs(), b.abs());
    let max = if x.is_nan() || y.is_nan() {
        f64::NAN
    } else if x > y {
        x
    } else {
        y
    };
    if max == 0.0 {
        return true;
    }
    (a - b).abs() / max < 0.01
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), *v))
            .collect()
    }

    fn with_tables<T>(body: impl FnOnce() -> T) -> T {
        crate::props::propfun::test_with_builtin_tables(body)
    }

    // ── siUnitForStateVariable ───────────────────────────────────────────────

    /// Port of `CyclePathResolverUnitsTest`.
    #[test]
    fn maps_canonical_state_properties() {
        assert_eq!(si_unit_for_state_variable("T1"), Some("K"));
        assert_eq!(si_unit_for_state_variable("P_2"), Some("Pa"));
        assert_eq!(si_unit_for_state_variable("h1"), Some("J/kg"));
        assert_eq!(si_unit_for_state_variable("u2"), Some("J/kg"));
        assert_eq!(si_unit_for_state_variable("s[3]"), Some("J/kg-K"));
        assert_eq!(si_unit_for_state_variable("v1"), Some("m^3/kg"));
        assert_eq!(si_unit_for_state_variable("rho2"), Some("kg/m^3"));
        assert_eq!(si_unit_for_state_variable("enthalpy4"), Some("J/kg"));
    }

    #[test]
    fn quality_is_dimensionless() {
        assert_eq!(si_unit_for_state_variable("x1"), None);
    }

    #[test]
    fn non_state_names_are_not_stamped() {
        assert_eq!(si_unit_for_state_variable("mass1"), None);
        assert_eq!(si_unit_for_state_variable("h"), None);
        assert_eq!(si_unit_for_state_variable("eta"), None);
        assert_eq!(si_unit_for_state_variable("motor_temp_5"), None);
    }

    /// Every answer here was read off the Java `CyclePathResolver` itself, not
    /// inferred from the regex: the long aliases, both index styles, and the
    /// names that look like states but are not.
    #[test]
    fn the_unit_stamp_agrees_with_the_java_on_the_awkward_names() {
        let cases: [(&str, Option<&str>); 15] = [
            ("t007", Some("K")),
            ("P[07]", Some("Pa")),
            ("density9", Some("kg/m^3")),
            ("tdrybulb1", Some("K")),
            ("internalenergy3", Some("J/kg")),
            ("volume8", Some("m^3/kg")),
            ("quality2", None),
            ("t99999999999999", Some("K")),
            ("s1.p", None),
            ("T", None),
            ("1", None),
            ("_1", None),
            ("p1a", None),
            ("Pw_1", None),
            ("Tw1", None),
        ];
        for (name, expected) in cases {
            assert_eq!(si_unit_for_state_variable(name), expected, "for {name}");
        }
    }

    /// The base is looked up *without* stripping underscores here, but *with*
    /// stripping when grouping — so this name is a density for the cycle and
    /// not a state property for the unit stamp. Java behaviour, transcribed.
    #[test]
    fn the_unit_stamp_does_not_strip_underscores_from_the_base() {
        assert_eq!(si_unit_for_state_variable("r_ho1"), None);
        let mut data = StateData::default();
        parse_and_populate_state("r_ho1", 3.0, &mut data);
        assert_eq!(data.known[&1].get("rho"), Some(&3.0));
    }

    // ── pattern matching ─────────────────────────────────────────────────────

    #[test]
    fn plain_states_split_at_the_trailing_digit_run() {
        assert_eq!(match_plain_state("t1"), Some(("t", "", "1")));
        assert_eq!(match_plain_state("t_1"), Some(("t", "_", "1")));
        // The optional `_` is greedy, so it takes the last underscore.
        assert_eq!(match_plain_state("t__1"), Some(("t_", "_", "1")));
        assert_eq!(
            match_plain_state("motor_temp_5"),
            Some(("motor_temp", "_", "5"))
        );
        assert_eq!(match_plain_state("pw_12"), Some(("pw", "_", "12")));
        assert_eq!(match_plain_state("rho2"), Some(("rho", "", "2")));
        // Leading zeros stay in the digit run; the index is the parsed int.
        assert_eq!(match_plain_state("t007"), Some(("t", "", "007")));
    }

    #[test]
    fn plain_states_reject_what_the_java_regex_rejects() {
        assert_eq!(match_plain_state("eta"), None); // no digits
        assert_eq!(match_plain_state("123"), None); // no head
        assert_eq!(match_plain_state("_1"), None); // head must start with a letter
        assert_eq!(match_plain_state("1t2"), None);
        assert_eq!(match_plain_state("t1a"), None); // digits must end the name
        assert_eq!(match_plain_state("t.1"), None);
        assert_eq!(match_plain_state("t-1"), None);
        assert_eq!(match_plain_state(""), None);
    }

    #[test]
    fn bracket_states_match_the_second_alternative() {
        assert_eq!(match_bracket_state("h[3]"), Some(("h", "3")));
        assert_eq!(match_bracket_state("p_w[12]"), Some(("p_w", "12")));
        assert_eq!(match_bracket_state("h[]"), None);
        assert_eq!(match_bracket_state("h[3"), None);
        assert_eq!(match_bracket_state("[3]"), None);
        assert_eq!(match_bracket_state("h[3]x"), None);
        assert_eq!(match_bracket_state("h1[3]"), None); // head is letters/underscores only
    }

    #[test]
    fn component_streams_index_by_the_streams_trailing_digits() {
        assert_eq!(match_component_stream("s1.p"), Some(("s", "1", "p")));
        assert_eq!(
            match_component_stream("s12.mdot"),
            Some(("s", "12", "mdot"))
        );
        assert_eq!(
            match_component_stream("abc123.h"),
            Some(("abc", "123", "h"))
        );
        assert_eq!(match_component_stream("s_1.h"), Some(("s_", "1", "h")));
        assert_eq!(match_component_stream("chlr.out.h"), None); // member must be letters
        assert_eq!(match_component_stream("s.p"), None); // no index
        assert_eq!(match_component_stream("s1.h2"), None);
        assert_eq!(match_component_stream("s1a2.p"), None);
    }

    #[test]
    fn the_state_var_alternation_prefers_the_plain_form() {
        assert_eq!(match_state_var_index("t_1"), Some(("t", "1")));
        assert_eq!(match_state_var_index("s[3]"), Some(("s", "3")));
        assert_eq!(match_state_var_index("s1.p"), None);
    }

    // ── grouping ─────────────────────────────────────────────────────────────

    #[test]
    fn styles_are_first_seen_wins_per_index() {
        let mut data = StateData::default();
        parse_and_populate_state("p1", 1.0, &mut data);
        parse_and_populate_state("t_1", 2.0, &mut data);
        assert_eq!(data.style[&1].name_of("h"), "h1");

        let mut bracketed = StateData::default();
        parse_and_populate_state("p[1]", 1.0, &mut bracketed);
        assert_eq!(bracketed.style[&1].name_of("h"), "h[1]");
    }

    #[test]
    fn a_block_state_keeps_its_circuit_tag_in_the_write_back_style() {
        let mut data = StateData::default();
        parse_block_state("Pw_1", 1.0, &mut data);
        parse_block_state("Tw_1", 2.0, &mut data);
        assert_eq!(data.style[&1].name_of("h"), "hw_1");
        assert_eq!(data.known[&1].len(), 2);

        let mut plain = StateData::default();
        parse_block_state("Tw1", 400.0, &mut plain);
        assert_eq!(plain.style[&1].name_of("s"), "sw1");

        // The longest property prefix wins: `rho` beats `r`… (there is no `r`).
        let mut density = StateData::default();
        parse_block_state("rhoref[2]", 5.0, &mut density);
        assert_eq!(density.known[&2].get("rho"), Some(&5.0));
        assert_eq!(density.style[&2].name_of("h"), "href[2]");
    }

    #[test]
    fn non_state_variables_are_ignored_when_grouping() {
        let v = vars(&[
            ("zeta1", 5.0),
            ("eta", 0.8),
            ("P1", 101_325.0),
            ("s1", 1000.0),
        ]);
        let grouped = group_state_knowns(&v);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[&1].len(), 2);
    }

    #[test]
    fn component_stream_members_group_as_states() {
        let v = vars(&[
            ("s1.P", 10_000.0),
            ("s1.h", 191_800.0),
            ("s2.P", 8_000_000.0),
            ("s2.h", 200_000.0),
            ("s1.mdot", 1.0),
        ]);
        let grouped = group_state_knowns(&v);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[&1].len(), 2, "mdot is not a state property");
        assert_eq!(grouped[&1].get("P"), Some(&10_000.0));
    }

    #[test]
    fn an_index_that_overflows_an_int_is_skipped_not_fatal() {
        let v = vars(&[("t99999999999999", 300.0), ("p99999999999999", 1e5)]);
        assert!(group_state_knowns(&v).is_empty());
        assert_eq!(si_unit_for_state_variable("t99999999999999"), Some("K"));
    }

    // ── isClose ──────────────────────────────────────────────────────────────

    #[test]
    fn is_close_needs_both_values_and_ignores_nan() {
        assert!(is_close(Some(1.0), Some(1.005)));
        assert!(!is_close(Some(1.0), Some(1.02)));
        assert!(is_close(Some(0.0), Some(0.0)));
        assert!(!is_close(Some(1.0), None));
        assert!(!is_close(None, None));
        assert!(!is_close(Some(f64::NAN), Some(1.0)));
        assert!(!is_close(Some(1.0), Some(f64::NAN)));
    }

    // ── resolveMissingProperties ─────────────────────────────────────────────

    /// Relative agreement with the Java oracle. The measured table-vs-CoolProp
    /// error on these states is ≤ 6e-7 (see the values below, all captured from
    /// `CyclePathResolver` running against native CoolProp); 1e-5 leaves room
    /// without letting a real divergence through.
    fn assert_close_to_oracle(label: &str, actual: f64, oracle: f64) {
        let rel = (actual - oracle).abs() / oracle.abs();
        assert!(
            rel < 1e-5,
            "{label}: {actual} vs oracle {oracle} (rel {rel:e})"
        );
    }

    /// Port of `CyclePathResolverTest.backFillsThePropertiesOfANumberedState`,
    /// with the Java engine's own numbers for water at 400 K / 1 atm.
    #[test]
    fn back_fills_the_properties_of_a_numbered_state() {
        with_tables(|| {
            let mut v = vars(&[("T1", 400.0), ("P1", 101_325.0)]);
            let mut names = BTreeMap::new();
            let added =
                resolve_missing_properties(&mut v, &mut names, "T1 = 400\nP1 = 101325", None, &[]);

            for want in ["h1", "s1", "u1", "rho1", "v1"] {
                assert!(v.contains_key(want), "missing {want} in {:?}", v.keys());
                assert!(
                    added.contains(&want.to_string()),
                    "{want} not reported as added"
                );
            }
            // superheated steam at 400 K / 1 atm
            assert!(
                v["h1"] > 2.5e6,
                "steam enthalpy should be ~2.7 MJ/kg, got {}",
                v["h1"]
            );
            assert!((v["v1"] - 1.0 / v["rho1"]).abs() < 1e-9);
            assert_close_to_oracle("h1", v["h1"], 2_730_301.385_920_189_3);
            assert_close_to_oracle("s1", v["s1"], 7_496.202_152_375_406_5);
            assert_close_to_oracle("u1", v["u1"], 2_547_715.363_508_403_8);
            assert_close_to_oracle("rho1", v["rho1"], 0.554_943_903_490_498_7);
            assert_close_to_oracle("v1", v["v1"], 1.801_983_936_953_226);
            // The write-back records the display spelling of what it added.
            assert_eq!(names.get("h1").map(String::as_str), Some("h1"));
        });
    }

    /// A single-phase state still gets a quality written, because the flash
    /// returns a finite number and the Java stores every finite result.
    ///
    /// **The number itself is a backend divergence, not a logic one.** CoolProp
    /// answers `Q = -1` ("this state is not in the dome"); the `(P,h)` split
    /// table has no such sentinel and reports the linear extrapolation
    /// `(h-hf)/(hg-hf)` — 1.024 for this state. Filtering it here would be a
    /// silent deviation from the ported logic, so it is recorded instead.
    #[test]
    fn single_phase_quality_is_the_backends_answer_not_coolprops() {
        with_tables(|| {
            let mut v = vars(&[("T1", 400.0), ("P1", 101_325.0)]);
            let mut names = BTreeMap::new();
            resolve_missing_properties(&mut v, &mut names, "", None, &[]);
            let x = v["x1"];
            assert!(
                x > 1.0 && x < 1.1,
                "extrapolated superheated quality, got {x}"
            );
        });
    }

    /// Port of `CyclePathResolverTest.underscoreStyleStatesKeepTheirNamingInWriteBack`.
    #[test]
    fn underscore_style_states_keep_their_naming_in_write_back() {
        with_tables(|| {
            let mut v = vars(&[("T_1", 500.0), ("P_1", 200_000.0)]);
            let mut names = BTreeMap::new();
            resolve_missing_properties(&mut v, &mut names, "T_1 = 500\nP_1 = 200000", None, &[]);
            assert!(v.contains_key("h_1"), "{:?}", v.keys());
            assert!(v.contains_key("s_1"), "{:?}", v.keys());
        });
    }

    /// Port of `CyclePathResolverTest.targetVariablesRestrictWhichPropertiesAreAdded`.
    #[test]
    fn target_variables_restrict_which_properties_are_added() {
        with_tables(|| {
            let mut v = vars(&[("T1", 400.0), ("P1", 101_325.0)]);
            let mut names = BTreeMap::new();
            let targets = ["h1".to_string()];
            resolve_missing_properties(
                &mut v,
                &mut names,
                "T1 = 400\nP1 = 101325",
                Some(&targets),
                &[],
            );
            assert!(v.contains_key("h1"));
            assert!(
                !v.contains_key("s1"),
                "s1 was not requested and must not be added"
            );
            assert!(!v.contains_key("v1"));
            assert!(!v.contains_key("x1"));
        });
    }

    /// Port of `CyclePathResolverTest.stateTableBlocksResolveTaggedStatesWithTheirOwnFluid`.
    #[test]
    fn state_table_blocks_resolve_tagged_states_with_their_own_fluid() {
        with_tables(|| {
            let mut v = vars(&[("Tw1", 400.0), ("Pw1", 101_325.0)]);
            let mut names = BTreeMap::new();
            let tables = [StateTableSpec {
                name: "watercircuit".into(),
                variables: vec!["tw1".into(), "pw1".into()],
                fluid: Some("Water".into()),
            }];
            resolve_missing_properties(
                &mut v,
                &mut names,
                "Tw1 = 400\nPw1 = 101325",
                None,
                &tables,
            );
            // the circuit tag "w" is preserved in the computed property names
            assert!(v.contains_key("hw1"), "{:?}", v.keys());
            assert!(v.contains_key("sw1"), "{:?}", v.keys());
        });
    }

    /// A second block with its own fluid must not collide with the first.
    #[test]
    fn each_state_table_block_flashes_with_its_own_fluid() {
        with_tables(|| {
            let mut v = vars(&[
                ("Pw_1", 101_325.0),
                ("Tw_1", 400.0),
                ("Pref_1", 200_000.0),
                ("Tref_1", 280.0),
            ]);
            let mut names = BTreeMap::new();
            let tables = [
                StateTableSpec {
                    name: "waterloop".into(),
                    variables: vec!["pw_1".into(), "tw_1".into()],
                    fluid: Some("Water".into()),
                },
                StateTableSpec {
                    name: "refrigerantloop".into(),
                    variables: vec!["pref_1".into(), "tref_1".into()],
                    fluid: Some("R134a".into()),
                },
            ];
            resolve_missing_properties(&mut v, &mut names, "", None, &tables);
            assert!(v.contains_key("hw_1"), "{:?}", v.keys());
            assert!(v.contains_key("href_1"), "{:?}", v.keys());
            // Steam at 400 K is an order of magnitude above R134a vapour, and
            // both agree with the Java engine running the same two blocks.
            assert_close_to_oracle("hw_1", v["hw_1"], 2_730_301.385_920_189_3);
            assert_close_to_oracle("href_1", v["href_1"], 407_032.765_319_179_86);
            assert_close_to_oracle("sref_1", v["sref_1"], 1_786.504_238_566_198);
            assert_close_to_oracle("rhoref_1", v["rhoref_1"], 9.253_084_004_312_901);
        });
    }

    /// The bracket style writes back under brackets — `h[1]`, not `h1`. The
    /// added names are the Java's, for the same document.
    #[test]
    fn bracket_style_states_write_back_under_brackets() {
        with_tables(|| {
            let mut v = vars(&[("T[1]", 400.0), ("P[1]", 101_325.0)]);
            let mut names = BTreeMap::new();
            let mut added = resolve_missing_properties(&mut v, &mut names, "T[1] = 400", None, &[]);
            added.sort();
            assert_eq!(added, ["h[1]", "rho[1]", "s[1]", "u[1]", "v[1]", "x[1]"]);
            assert_close_to_oracle("h[1]", v["h[1]"], 2_730_301.385_920_189_3);
        });
    }

    /// The naming asymmetry, end to end: `r_ho1` groups as a density (the
    /// grouping strips underscores from the base) and `t__1` as a temperature
    /// (the reluctant head hands the last underscore to the separator), while
    /// the write-back style comes from the first variable in sorted order —
    /// `p1`, which carries neither bracket nor underscore. The Java adds
    /// exactly these five names for exactly this reason.
    #[test]
    fn underscore_heavy_names_group_but_write_back_in_the_first_seen_style() {
        with_tables(|| {
            let mut v = vars(&[("r_ho1", 0.5), ("t__1", 400.0), ("p1", 101_325.0)]);
            let mut names = BTreeMap::new();
            let mut added = resolve_missing_properties(&mut v, &mut names, "", None, &[]);
            added.sort();
            assert_eq!(added, ["h1", "s1", "u1", "v1", "x1"]);
            // {rho, T, P} matches the (T, P) pair, so the values are the plain
            // 400 K / 1 atm flash — the given rho is never used as an input.
            assert_close_to_oracle("h1", v["h1"], 2_730_301.385_920_189_3);
            assert_close_to_oracle("s1", v["s1"], 7_496.202_152_375_406_5);
        });
    }

    /// Port of `CyclePathResolverTest.passesThroughWhenNoStatePairIsComplete`.
    #[test]
    fn passes_through_when_no_state_pair_is_complete() {
        with_tables(|| {
            let mut v = vars(&[("T1", 400.0), ("zeta", 2.0)]);
            let mut names = BTreeMap::new();
            let added = resolve_missing_properties(&mut v, &mut names, "T1 = 400", None, &[]);
            assert!(!v.contains_key("h1"));
            assert!(added.is_empty());
        });
    }

    #[test]
    fn nothing_is_filled_without_a_property_backend() {
        crate::props::propfun::test_without_backend(|| {
            let mut v = vars(&[("T1", 400.0), ("P1", 101_325.0)]);
            let mut names = BTreeMap::new();
            let added = resolve_missing_properties(&mut v, &mut names, "", None, &[]);
            assert!(added.is_empty());
            assert_eq!(v.len(), 2);
            assert!(generate_cycle_path(&v, "Water").is_empty());
        });
    }

    #[test]
    fn a_known_property_is_never_recomputed() {
        with_tables(|| {
            // h1 is already solved; the fill must leave the solver's value alone.
            let mut v = vars(&[("P1", 101_325.0), ("h1", 2.7e6), ("T1", 400.0)]);
            let mut names = BTreeMap::new();
            let added = resolve_missing_properties(&mut v, &mut names, "", None, &[]);
            assert_eq!(v["h1"], 2.7e6);
            assert!(!added.contains(&"h1".to_string()));
            assert!(added.contains(&"s1".to_string()));
            // {P, T, h} matches (P, h) first, not (T, P): the entropy is the one
            // that belongs to the *given* enthalpy, which is the Java's answer
            // for this state and differs in the third digit from a (T, P) flash.
            assert_close_to_oracle("s1", v["s1"], 7_418.990_128_876_409_5);
            assert_close_to_oracle("rho1", v["rho1"], 0.577_843_445_791_960_8);
        });
    }

    // ── generateCyclePath ────────────────────────────────────────────────────

    /// Port of `CyclePathResolverTest.fewerThanTwoStatesProducesNoPath`.
    #[test]
    fn fewer_than_two_states_produces_no_path() {
        with_tables(|| {
            let v = vars(&[("P1", 101_325.0), ("s1", 1000.0)]);
            assert!(generate_cycle_path(&v, "Water").is_empty());
        });
    }

    /// Port of `CyclePathResolverTest.equalPressuresInterpolateAnIsobar`.
    #[test]
    fn equal_pressures_interpolate_an_isobar() {
        with_tables(|| {
            let v = vars(&[
                ("P1", 101_325.0),
                ("s1", 1000.0),
                ("P2", 101_325.0),
                ("s2", 3000.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            assert!(!path.is_empty());
            assert!(
                path.iter()
                    .all(|pt| pt.contains_key("P") || pt.contains_key("T")),
                "flashed points must carry plottable properties"
            );
            // Two states, both directions: 31 points then 30 more.
            assert_eq!(path.len(), 61);
            assert!((path[0]["P"] - 101_325.0).abs() < 1e-6);
            // Every point of a served branch carries all five properties, and
            // each matches what the Java produced against native CoolProp.
            assert_close_to_oracle("T[0]", path[0]["T"], 346.846_689_501_459_9);
            assert_close_to_oracle("h[0]", path[0]["h"], 308_616.064_583_961_4);
            assert_close_to_oracle("v[0]", path[0]["v"], 0.001_024_992_145_486_617_3);
            assert_eq!(path[0]["s"], 1000.0);
            assert_close_to_oracle("T[30]", path[30]["T"], 373.124_295_847_666_36);
            assert_close_to_oracle("h[30]", path[30]["h"], 1_050_786.712_729_426_8);
            // The closing segment returns to the first state.
            assert_eq!(path[60], path[0]);
        });
    }

    /// Port of `CyclePathResolverTest.equalEntropiesInterpolateAnIsentrope`.
    #[test]
    fn equal_entropies_interpolate_an_isentrope() {
        with_tables(|| {
            let v = vars(&[
                ("P1", 101_325.0),
                ("s1", 6000.0),
                ("P2", 1_000_000.0),
                ("s2", 6000.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            assert!(!path.is_empty());
            assert!(path.iter().all(|pt| pt.contains_key("s")));
            // The pressure sweep is geometric between the two states, and it is
            // pure `log`/`exp` arithmetic — no property backend in the way — so
            // it reproduces the Java's doubles exactly, not approximately.
            assert_eq!(path[0]["P"], 101_324.999_999_999_99);
            assert_eq!(path[1]["P"], 109_360.224_222_598_38);
            assert_eq!(path[15]["P"], 318_315.880_847_94);
            assert_eq!(path[30]["P"], 999_999.999_999_999_5);
            assert_eq!(path[31]["P"], 926_525.166_899_410_7);
            assert_close_to_oracle("T[15]", path[15]["T"], 408.704_680_429_142_1);
            assert_close_to_oracle("h[15]", path[15]["h"], 2_330_413.552_115_092_5);
        });
    }

    /// Port of `CyclePathResolverTest.equalEnthalpiesInterpolateAnIsenthalp`.
    #[test]
    fn equal_enthalpies_interpolate_an_isenthalp() {
        with_tables(|| {
            let v = vars(&[
                ("P1", 1_000_000.0),
                ("h1", 2_800_000.0),
                ("P2", 200_000.0),
                ("h2", 2_800_000.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            assert!(!path.is_empty());
            assert!(path.iter().all(|pt| pt.contains_key("h")));
            assert_eq!(path[1]["P"], 947_765.727_256_265_5);
            assert_eq!(path[30]["P"], 200_000.000_000_000_06);
            assert_close_to_oracle("T[0]", path[0]["T"], 461.763_466_563_679_8);
            assert_close_to_oracle("s[30]", path[30]["s"], 7_352.762_873_979_022_5);
        });
    }

    /// Port of `CyclePathResolverTest.equalTemperaturesInterpolateAnIsotherm`.
    /// The `(T, s)` pair is not tabulated by this port's backend, so the points
    /// carry the two properties the flash was *given* — see the module docs.
    /// The `T` and `s` it does carry are the segment's own parameterisation, so
    /// they are bit-identical to the Java's, and a T-s plot draws the same
    /// isotherm; only P/h/v are missing from the point.
    #[test]
    fn equal_temperatures_interpolate_an_isotherm() {
        with_tables(|| {
            let v = vars(&[("T1", 450.0), ("s1", 2000.0), ("T2", 450.0), ("s2", 5000.0)]);
            let path = generate_cycle_path(&v, "Water");
            assert_eq!(path.len(), 61);
            assert!(path
                .iter()
                .all(|pt| pt.contains_key("T") && pt.contains_key("s")));
            assert_eq!(path[0]["T"], 450.0);
            assert_eq!(path[1]["s"], 2100.0);
            assert_eq!(path[30]["s"], 5000.0);
        });
    }

    /// Port of `CyclePathResolverTest.equalSpecificVolumesInterpolateAnIsochor`.
    #[test]
    fn equal_specific_volumes_interpolate_an_isochor() {
        with_tables(|| {
            let v = vars(&[("T1", 420.0), ("v1", 0.5), ("T2", 520.0), ("v2", 0.5)]);
            let path = generate_cycle_path(&v, "Water");
            assert_eq!(path.len(), 61);
            // `(T, Dmass)` is likewise untabulated; T and v come through.
            assert!(path
                .iter()
                .all(|pt| pt.contains_key("T") && pt.contains_key("v")));
            assert!((path[0]["v"] - 0.5).abs() < 1e-12);
            assert_eq!(path[1]["T"], 423.333_333_333_333_3);
            assert_eq!(path[30]["T"], 520.0);
        });
    }

    /// Port of `CyclePathResolverTest.unrelatedStatesFallBackToLinearInterpolation`.
    #[test]
    fn unrelated_states_fall_back_to_linear_interpolation() {
        with_tables(|| {
            let v = vars(&[
                ("T1", 300.0),
                ("h1", 100_000.0),
                ("T2", 400.0),
                ("h2", 500_000.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            assert_eq!(path.len(), 61);
            // Linear interpolation carries the raw properties through and never
            // touches the property backend, so every number is the Java's.
            assert_eq!(path[0].keys().collect::<Vec<_>>(), vec!["T", "h"]);
            assert_eq!(path[0]["T"], 300.0);
            assert_eq!(path[1]["T"], 303.333_333_333_333_3);
            assert_eq!(path[1]["h"], 113_333.333_333_333_33);
            assert_eq!(path[STEPS]["T"], 400.0);
            assert_eq!(path[STEPS]["h"], 500_000.0);
            assert_eq!(path[31]["h"], 486_666.666_666_666_7);
        });
    }

    /// Port of `CyclePathResolverTest.nonStateVariablesAreIgnoredWhenGrouping`.
    #[test]
    fn only_one_real_state_produces_no_path() {
        with_tables(|| {
            let v = vars(&[
                ("zeta1", 5.0),
                ("eta", 0.8),
                ("P1", 101_325.0),
                ("s1", 1000.0),
            ]);
            assert!(generate_cycle_path(&v, "Water").is_empty());
        });
    }

    /// Port of `CyclePathComponentTest.componentRankineStreamsProduceACyclePath`
    /// with the streams' solved values standing in for the expander.
    #[test]
    fn component_rankine_streams_produce_a_cycle_path() {
        with_tables(|| {
            let v = vars(&[
                ("s1.P", 10_000.0),
                ("s1.h", 191_812.0),
                ("s2.P", 8_000_000.0),
                ("s2.h", 200_000.0),
                ("s3.P", 8_000_000.0),
                ("s3.h", 3_398_000.0),
                ("s4.P", 10_000.0),
                ("s4.h", 2_200_000.0),
                ("s1.mdot", 1.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            assert!(
                !path.is_empty(),
                "component streams should plot a cycle overlay"
            );
            assert!(
                path[0].contains_key("P") || path[0].contains_key("T"),
                "{:?}",
                path[0]
            );
            // Four states, closed: 31 + 3 * 30.
            assert_eq!(path.len(), 121);
            // Each leg's states share neither P, s, h nor v, so every segment is
            // the linear fallback — the same numbers the Java produced.
            assert_eq!(path[0]["P"], 10_000.0);
            assert_eq!(path[0]["h"], 191_812.0);
            assert_eq!(path[1]["P"], 276_333.333_333_333_3);
            assert_eq!(path[1]["h"], 192_084.933_333_333_32);
            assert_eq!(path[30]["P"], 8_000_000.0);
            assert_eq!(path[120], path[0], "the cycle closes");
        });
    }

    /// The path closes: the last segment runs from the highest index back to
    /// the lowest.
    #[test]
    fn the_path_closes_back_to_the_first_state() {
        with_tables(|| {
            let v = vars(&[
                ("P1", 101_325.0),
                ("s1", 1000.0),
                ("P2", 101_325.0),
                ("s2", 3000.0),
            ]);
            let path = generate_cycle_path(&v, "Water");
            let first = path.first().expect("non-empty");
            let last = path.last().expect("non-empty");
            assert!((first["s"] - 1000.0).abs() < 1.0);
            assert!((last["s"] - 1000.0).abs() < 1.0, "the cycle must close");
        });
    }
}
