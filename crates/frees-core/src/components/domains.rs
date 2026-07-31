//! The four physical domains, node classification and the junction rules.
//!
//! Port of the domain logic in
//! `../frEES/backend/core/src/main/java/com/frees/backend/parser/ComponentExpander.java`
//! — `nodeDomain`, `acrossMembers`, `acrossMembersForNode`, `checkSingleDomain`,
//! `checkFluidConnectorType`, and the supporting `buildStreamDomainMap`,
//! `nodeFluidType`, `nodeHasMember`, `portFluid`, `isFluidStream`,
//! `CANONICAL_UNITS`/`memberUnits`, `kirchhoffBalance`, `massConservation`,
//! `equality` and `portDirection`.
//!
//! # What this module is
//!
//! This is the physics core of the component layer, and it is **pure**: every
//! function here takes plain data (stream names, member names, resolved string
//! parameters) and returns a classification, a member list, an [`Equation`] or a
//! hard [`FreesError::Parse`]. Nothing here reaches into the solver — the
//! component layer is a parser/expander, and these rules only decide *which
//! scalar equations a `connect(...)` node emits* (see `components/mod.rs`).
//!
//! It deliberately does **not** depend on `components/def.rs`. The Java operates
//! on a `ResolvedInstance`, but the only things the domain rules actually read
//! off one are a port→stream binding and the resolved string parameters, so
//! those are passed as ordered slices. That keeps this module testable on its
//! own and makes the wiring to `def.rs` a one-line adaptation.
//!
//! # The rules, in one place
//!
//! A **node** (one `connect(...)`, or the set of ports that share a stream name)
//! belongs to exactly one domain, and each domain is an `(across, flow)` pair
//! with its own junction rule:
//!
//! | Domain | across (equal at the node) | flow (through) | junction rule |
//! |---|---|---|---|
//! | [`Domain::Fluid`] | `P`, `h` | `ṁ` | pass-through / signed Σ at a branch |
//! | [`Domain::Heat`] | `T` | `Q̇` | `ΣQ̇ = 0` |
//! | [`Domain::Electrical`] | `V` | `I` | `ΣI = 0` |
//! | [`Domain::Mechanical`] | `ω` | `τ` | `Στ = 0` |
//! | [`Domain::Translational`] | `v` | `F` | `ΣF = 0` |
//! | [`Domain::Signal`] | `sig` | — | causal broadcast, no flow |
//!
//! **The through-variable wins.** A moist-air stream carries `T` *and* `ṁ`; it
//! is fluid, not heat. A source that carries only an across variable (a thermal
//! source's `T`, a ground's `V`, a mechanical ground's `ω`) is still placed in
//! its domain by the across fallback — see [`endpoint_domain`].
//!
//! Two guards make a wrong network a **hard parse error**, never a silent
//! mis-solve: [`check_single_domain`] (one bond-graph domain per node) and
//! [`check_fluid_connector_type`] (the fluid-family connector types that share
//! the `(P, ṁ, h)` bond are still not interchangeable).
//!
//! # Deliberate deviations from the Java, and why
//!
//! * `streamMembers` / `streamDomain` are `LinkedHashMap<String, HashSet<…>>`
//!   in Java, so [`member_units`] there iterates a `HashSet` and its output
//!   order is unspecified. [`StreamMembers`] and [`ConnectorTypes`] use ordered
//!   maps, making the same output deterministic. No classification depends on
//!   set order (every rule is a `contains` test or a fixed-order scan), so this
//!   changes nothing but reproducibility.
//! * [`kirchhoff_balance`] on an empty node yields `0 = 0` where the Java would
//!   dereference a null sum. Unreachable either way: `expandConnects` rejects a
//!   `connect(...)` with fewer than two endpoints before it gets here.
//!
//! Everything else — including the exact wording of every rejection — is
//! transcribed.
//!
//! # What the caller must pass
//!
//! Two conventions the Java pipeline establishes and every rejection here
//! depends on — both confirmed by replaying documents through the Java engine:
//!
//! * **`refs` are already lowercase.** `ConnectDecl::ports` stores endpoints
//!   lowercased, so `connect(SUP.out, TNK.port)` is rejected as
//!   `connect(sup.out, tnk.port)`. These functions echo whatever they are
//!   given; they never re-case.
//! * **`source_text` is not.** The declaration's own text keeps the user's
//!   casing but loses the spaces between arguments
//!   (`connect(SRC.out,E1.in,E2.node)`), which is why [`mass_conservation`]
//!   takes it separately instead of rebuilding it from `refs`.
//!
//! Every message asserted in this module's tests was checked against the real
//! Java engine via `tools/golden-dumper/run.sh`, not transcribed on faith.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::{BinOp, Equation, Expr};
use crate::diag::{FreesError, Result};

// ── The domains ─────────────────────────────────────────────────────────────

/// The physical domain of a connection node, with its node rule.
///
/// Port of the private `ComponentExpander.Domain` enum. The variant order
/// matches the Java so [`Domain::as_str`] reproduces
/// `Domain.name().toLowerCase(Locale.ROOT)` — the spelling that appears in the
/// [`check_single_domain`] rejection and in the schematic payload's
/// `Connection.domain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    /// `P`/`ṁ` with the convective enthalpy `h` riding along.
    Fluid,
    /// `T`/`Q̇`.
    Heat,
    /// `V`/`I`.
    Electrical,
    /// Mechanical rotational: `ω`/`τ`.
    Mechanical,
    /// Mechanical translational: `v`/`F`.
    Translational,
    /// The causal signal domain: a value, no flow.
    Signal,
}

impl Domain {
    /// The lowercase domain name used in diagnostics and the schematic payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Domain::Fluid => "fluid",
            Domain::Heat => "heat",
            Domain::Electrical => "electrical",
            Domain::Mechanical => "mechanical",
            Domain::Translational => "translational",
            Domain::Signal => "signal",
        }
    }

    /// The across (equal-at-the-node) members for this domain.
    ///
    /// Port of `acrossMembers`. Fluid equates pressure **and** enthalpy: `h` is
    /// the convective across variable, equal at a split or a pass-through and
    /// flow-weighted only inside an explicit mixer component's own body.
    ///
    /// For a *node* — where the moist-air `w`, the blend `z` and the species
    /// riders may join the list — use [`across_members_for_node`].
    pub fn across_members(self) -> &'static [&'static str] {
        match self {
            Domain::Fluid => &["p", "h"],
            Domain::Heat => &["t"],
            Domain::Electrical => &["v"],
            Domain::Mechanical => &["w"],
            Domain::Translational => &["vel"],
            Domain::Signal => &["sig"],
        }
    }

    /// The through (flow) member for this domain, or `None` for the across-only
    /// signal domain.
    pub fn flow_member(self) -> Option<&'static str> {
        match self {
            Domain::Fluid => Some("mdot"),
            Domain::Heat => Some("qdot"),
            Domain::Electrical => Some("i"),
            Domain::Mechanical => Some("tau"),
            Domain::Translational => Some("f"),
            Domain::Signal => None,
        }
    }

    /// How the flow variable is conserved at a node of this domain.
    ///
    /// Encodes the `switch (dom)` at the end of `expandConnects`.
    pub fn junction_rule(self) -> JunctionRule {
        match self {
            Domain::Fluid => JunctionRule::FluidMass,
            Domain::Heat => JunctionRule::Kirchhoff("qdot"),
            Domain::Electrical => JunctionRule::Kirchhoff("i"),
            Domain::Mechanical => JunctionRule::Kirchhoff("tau"),
            Domain::Translational => JunctionRule::Kirchhoff("f"),
            Domain::Signal => JunctionRule::Broadcast,
        }
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The conservation law a node applies to its flow variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JunctionRule {
    /// Fluid `ṁ`: it *passes through*. A two-endpoint node is a plain equality;
    /// a 3+-port branch is the signed `Σ(ṁ_out) = Σ(ṁ_in)` of
    /// [`mass_conservation`]; a loop-closing `connect` emits nothing at all (the
    /// balance is already implied by the rest of the loop).
    FluidMass,
    /// `Σ(flow) = 0` over every endpoint, each flow signed into its component —
    /// `ΣQ̇=0`, `ΣI=0`, `Στ=0`, `ΣF=0`. Carries the flow member's name.
    Kirchhoff(&'static str),
    /// Causal broadcast: the across equality only. One writer, any number of
    /// readers, no flow member to conserve.
    Broadcast,
}

/// The reserved string parameter that names a fluid-family connector type.
pub const DOMAIN_PARAM: &str = "domain$";

/// The connector type a component gets when it declares no [`DOMAIN_PARAM`].
pub const DEFAULT_CONNECTOR_TYPE: &str = "fluid";

/// Conserved-scalar riders carried across fluid nodes when present (E3): the
/// single-species fraction, the named combustion species, and the
/// oil-circulation fraction.
///
/// Transcribed from `ComponentExpander.SPECIES_RIDERS`; the order is the order
/// they are appended to a node's across members.
pub const SPECIES_RIDERS: [&str; 6] = ["y", "yo2", "yco2", "yh2o", "yn2", "oc"];

// ── Stream member sets ──────────────────────────────────────────────────────

/// Per stream, the set of port members the attached component bodies reference
/// — `{p, h, mdot}` for a fluid stream, `{t, qdot}` for a heat stream.
///
/// Port of `ComponentExpander.streamMembers`. This is the only input the domain
/// classifier has: a stream *is* the members its components name on it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamMembers {
    by_stream: BTreeMap<String, BTreeSet<String>>,
}

impl StreamMembers {
    pub fn new() -> StreamMembers {
        StreamMembers::default()
    }

    /// Records that `stream` carries `member`.
    pub fn add(&mut self, stream: &str, member: &str) {
        self.by_stream
            .entry(stream.to_string())
            .or_default()
            .insert(member.to_string());
    }

    /// The members recorded for `stream`, or `None` if it has never been seen.
    pub fn members(&self, stream: &str) -> Option<&BTreeSet<String>> {
        self.by_stream.get(stream)
    }

    /// Whether `stream` is known to carry `member`.
    pub fn has(&self, stream: &str, member: &str) -> bool {
        self.by_stream
            .get(stream)
            .is_some_and(|m| m.contains(member))
    }

    /// Whether `stream` carries enough to be classified — the
    /// `m == null || m.isEmpty()` skip in `checkSingleDomain`.
    pub fn is_classifiable(&self, stream: &str) -> bool {
        self.by_stream.get(stream).is_some_and(|m| !m.is_empty())
    }

    /// Every stream that carries at least one member, in name order.
    pub fn streams(&self) -> impl Iterator<Item = &str> {
        self.by_stream.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_stream.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_stream.is_empty()
    }

    /// Records the port members one component-body equation references, given
    /// that instance's port→stream binding.
    ///
    /// Port of `buildStreamMembers`; call it once per equation of every
    /// instance's *effective* (variant-selected) body.
    pub fn collect_from_equation(&mut self, eq: &Equation, ports: &[(String, String)]) {
        self.collect_from_expr(&eq.lhs, ports);
        self.collect_from_expr(&eq.rhs, ports);
    }

    /// Port of `collectMembers`.
    ///
    /// Only `Expr::Var` is inspected, and only the text before its **first**
    /// dot is treated as a port: `in.h` records `h` on the stream bound to
    /// `in`, while `in.a.b` records nothing (the Java's
    /// `member.indexOf('.') < 0` guard). An `ArrayAccess`'s *name* is
    /// deliberately not examined — the Java walks its indices only.
    pub fn collect_from_expr(&mut self, e: &Expr, ports: &[(String, String)]) {
        match e {
            Expr::Var(name) => {
                let Some(dot) = name.find('.') else { return };
                let (port, member) = (&name[..dot], &name[dot + 1..]);
                if let Some(stream) = port_stream(ports, port) {
                    if !member.contains('.') {
                        let stream = stream.to_string();
                        self.add(&stream, member);
                    }
                }
            }
            Expr::Neg(inner) | Expr::Not(inner) => self.collect_from_expr(inner, ports),
            Expr::BinOp { left, right, .. }
            | Expr::Compare { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
            } => {
                self.collect_from_expr(left, ports);
                self.collect_from_expr(right, ports);
            }
            Expr::Call { args, .. } => {
                for a in args {
                    self.collect_from_expr(a, ports);
                }
            }
            Expr::ArrayAccess { indices, .. } => {
                for i in indices {
                    self.collect_from_expr(i, ports);
                }
            }
            Expr::ArrayLiteral(elements) => {
                for el in elements {
                    self.collect_from_expr(el, ports);
                }
            }
            Expr::Num { .. } | Expr::Str(_) => {}
        }
    }
}

/// Looks up a port's bound stream in a declaration-ordered port map.
///
/// The binding is a slice of pairs rather than a map because **port order is
/// load-bearing** elsewhere in the expander (positional binding, the
/// two-port pass-through seed, and the order in which
/// [`build_connector_types`] reports a conflict).
pub fn port_stream<'a>(ports: &'a [(String, String)], port: &str) -> Option<&'a str> {
    ports
        .iter()
        .find(|(p, _)| p == port)
        .map(|(_, s)| s.as_str())
}

/// Whether a stream is a fluid stream — i.e. its components reference `mdot`.
///
/// Port of `isFluidStream`. Note this is *not* the same test as
/// `endpoint_domain(...) == Domain::Fluid`: a stream with no members at all
/// classifies as fluid by the default fallback but is not a fluid stream here.
pub fn is_fluid_stream(stream: &str, members: &StreamMembers) -> bool {
    members.has(stream, "mdot")
}

// ── Node classification ─────────────────────────────────────────────────────

/// Classifies a `connect` node's domain from the members its streams carry.
///
/// Port of `nodeDomain`. The union of every endpoint's members is tested in a
/// fixed priority order, so **the through-variable wins**: `sig` ⇒ signal,
/// `mdot` ⇒ fluid, `qdot` ⇒ heat, `i` ⇒ electrical, `tau` ⇒ mechanical,
/// `f` ⇒ translational. Only then do the across variables act as a fallback for
/// source/ground-only nodes: `t` ⇒ heat, `v` ⇒ electrical, `w` ⇒ mechanical,
/// `vel` ⇒ translational.
///
/// Fluid is the default, so a node whose streams carry nothing recognisable —
/// including a bare stream no component has referenced — is fluid.
pub fn node_domain<S: AsRef<str>>(streams: &[S], members: &StreamMembers) -> Domain {
    let mut union: BTreeSet<&str> = BTreeSet::new();
    for st in streams {
        if let Some(m) = members.members(st.as_ref()) {
            union.extend(m.iter().map(String::as_str));
        }
    }
    // Transcribed in the Java's exact order: the through-variable tests first,
    // then the across-variable fallbacks, then the fluid default.
    if union.contains("sig") {
        return Domain::Signal;
    }
    if union.contains("mdot") {
        return Domain::Fluid;
    }
    if union.contains("qdot") {
        return Domain::Heat;
    }
    if union.contains("i") {
        return Domain::Electrical;
    }
    if union.contains("tau") {
        return Domain::Mechanical;
    }
    if union.contains("f") {
        return Domain::Translational;
    }
    if union.contains("t") {
        return Domain::Heat; // a heat node carrying only temperatures (sources)
    }
    if union.contains("v") {
        return Domain::Electrical; // an electrical node carrying only potentials
    }
    if union.contains("w") {
        return Domain::Mechanical; // only speeds (grounds/sources)
    }
    if union.contains("vel") {
        return Domain::Translational; // only velocities
    }
    Domain::Fluid
}

/// The domain of a **single** connect endpoint — `nodeDomain(List.of(stream))`.
///
/// This is the classification [`check_single_domain`] applies per endpoint, and
/// the one [`build_connector_types`] uses to decide which streams are
/// fluid-class. Because it runs on one stream at a time, a source that carries
/// only an across variable is still placed in its own domain instead of being
/// absorbed into a neighbour's.
pub fn endpoint_domain(stream: &str, members: &StreamMembers) -> Domain {
    node_domain(&[stream], members)
}

/// Whether any stream at the node carries the given port member.
///
/// Port of `nodeHasMember`.
pub fn node_has_member<S: AsRef<str>>(
    streams: &[S],
    member: &str,
    members: &StreamMembers,
) -> bool {
    streams.iter().any(|st| members.has(st.as_ref(), member))
}

// ── Fluid connector types (the `domain$` rider) ─────────────────────────────

/// Stream → fluid connector type (`fluid` / `liquid` / `twophase` / `gas` /
/// `oil` / `moistair`), from each component's reserved `domain$` parameter.
///
/// Port of `ComponentExpander.streamDomain`. Only fluid-class streams are
/// tagged; it separates the pneumatic, hydraulic, two-phase, humid-air and
/// thermofluid lines that all share the same `(P, ṁ, h)` bond, so a wrong
/// cross-connection is a hard error rather than a silently-solved nonsense
/// network.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectorTypes {
    by_stream: BTreeMap<String, String>,
}

impl ConnectorTypes {
    pub fn new() -> ConnectorTypes {
        ConnectorTypes::default()
    }

    /// The connector type tagged on `stream`, or `None` outside the fluid
    /// family (heat / electrical / mechanical streams carry none).
    pub fn get(&self, stream: &str) -> Option<&str> {
        self.by_stream.get(stream).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_stream.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_stream.is_empty()
    }

    /// Tags every fluid-class stream of one instance with its connector type,
    /// rejecting two conflicting types bound to the *same* stream.
    ///
    /// Port of the inner loop of `buildStreamDomainMap`. Two ports of
    /// conflicting type on one stream is shared-name misuse and fails
    /// immediately; a `connect`-bound conflict is caught per node instead, by
    /// [`check_fluid_connector_type`].
    ///
    /// `ports` must be in port-declaration order — the Java reports the first
    /// conflict in that order, and the message names both types.
    pub fn tag_instance(
        &mut self,
        connector_type: &str,
        ports: &[(String, String)],
        members: &StreamMembers,
    ) -> Result<()> {
        for (_, stream) in ports {
            if endpoint_domain(stream, members) != Domain::Fluid {
                continue; // only fluid-class ports carry a fluid connector type
            }
            match self.by_stream.get(stream.as_str()) {
                Some(prev) if prev != connector_type => {
                    return Err(FreesError::parse(format!(
                        "Incompatible fluid connector types on stream '{stream}': \
                         '{prev}' and '{connector_type}' bound to the same stream. \
                         Pneumatic ('gas'), hydraulic ('oil') and thermofluid ('fluid') \
                         lines are different connector types and cannot share a port."
                    )));
                }
                Some(_) => {}
                None => {
                    self.by_stream
                        .insert(stream.clone(), connector_type.to_string());
                }
            }
        }
        Ok(())
    }
}

/// The connector-typing view of one resolved component instance.
///
/// Everything [`build_connector_types`] needs off a `ResolvedInstance`, and
/// nothing more — so this module stays independent of `components/def.rs`.
#[derive(Debug, Clone, Copy)]
pub struct InstanceView<'a> {
    /// Resolved string parameters in **declaration order**, each name including
    /// its trailing `$` (`fluid$`, `domain$`, `model$`, `map$`, …).
    pub string_params: &'a [(String, String)],
    /// Port name → bound stream name, in **port-declaration order**.
    pub ports: &'a [(String, String)],
}

/// An instance's fluid connector type: its `domain$` parameter, defaulting to
/// `fluid`.
///
/// Port of `stringParams.getOrDefault("domain$", "fluid")`.
pub fn instance_connector_type(string_params: &[(String, String)]) -> &str {
    string_params
        .iter()
        .find(|(k, _)| k == DOMAIN_PARAM)
        .map_or(DEFAULT_CONNECTOR_TYPE, |(_, v)| v.as_str())
}

/// Tags every stream in the network with its fluid connector type.
///
/// Port of `buildStreamDomainMap`. `instances` must be in declaration order.
pub fn build_connector_types(
    instances: &[InstanceView<'_>],
    members: &StreamMembers,
) -> Result<ConnectorTypes> {
    let mut types = ConnectorTypes::new();
    for inst in instances {
        let dom = instance_connector_type(inst.string_params);
        types.tag_instance(dom, inst.ports, members)?;
    }
    Ok(types)
}

/// The fluid connector type of a node, from its first tagged stream.
///
/// Port of `nodeFluidType`. Defaults to `fluid`; the node is verified
/// homogeneous by [`check_fluid_connector_type`] before this is trusted.
pub fn node_fluid_type<'a, S: AsRef<str>>(streams: &[S], types: &'a ConnectorTypes) -> &'a str {
    for st in streams {
        if let Some(d) = types.get(st.as_ref()) {
            return d;
        }
    }
    DEFAULT_CONNECTOR_TYPE
}

/// The fluid a given port draws on, from a component's resolved string
/// parameters.
///
/// Port of `portFluid`. An exact `fluid$` parameter applies to every port;
/// otherwise a string parameter whose base name (the name minus its trailing
/// `$`) is `fluid` or is a **prefix of the port name** applies — so a two-fluid
/// heat exchanger's `hot$` covers `hot_in` and `hot_out` while `cold$` covers
/// the cold side.
///
/// This is the guard that keeps the reserved `domain$` from being mistaken for
/// a fluid name: `domain$`'s base is `domain`, which is neither `fluid` nor a
/// prefix of any port name in the library, so a `PARAM domain$ = moistair`
/// component reports **no** fluid. The same holds for `model$`, `map$`, `arr$`
/// and every other non-fluid string parameter.
///
/// `string_params` must be in declaration order: the Java returns the *first*
/// match.
pub fn port_fluid<'a>(port: &str, string_params: &'a [(String, String)]) -> Option<&'a str> {
    if let Some((_, v)) = string_params.iter().find(|(k, _)| k == "fluid$") {
        return Some(v.as_str());
    }
    for (key, value) in string_params {
        // Strip the trailing '$'. (The Java's `substring(0, len-1)` would throw
        // on an empty key; a string parameter always has the sigil.)
        let base = match key.char_indices().next_back() {
            Some((idx, _)) => &key[..idx],
            None => key.as_str(),
        };
        if base == "fluid" || port.starts_with(base) {
            return Some(value.as_str());
        }
    }
    None
}

// ── The two hard guards ─────────────────────────────────────────────────────

/// Rejects a `connect` node that mixes physical (bond-graph) domains.
///
/// Port of `checkSingleDomain`. Every endpoint is classified **individually**
/// via [`endpoint_domain`] — so the through-variable wins and a source carrying
/// only an across variable is still placed — and all endpoints must agree.
/// Connecting a heat port to an electrical port, or a fluid line to a
/// mechanical shaft, is a hard error rather than a silently mis-solved network.
/// A moist-air stream carrying both `T` and `ṁ` classifies as fluid, so it is
/// unaffected.
///
/// Endpoints whose stream carries no members yet are skipped, not guessed at.
///
/// `streams[i]` must be the stream `refs[i]` resolves to.
pub fn check_single_domain<S: AsRef<str>, R: AsRef<str>>(
    streams: &[S],
    refs: &[R],
    members: &StreamMembers,
) -> Result<()> {
    let mut first: Option<(Domain, &str)> = None;
    for (stream, reference) in streams.iter().zip(refs.iter()) {
        let (stream, reference) = (stream.as_ref(), reference.as_ref());
        if !members.is_classifiable(stream) {
            continue; // a bare stream with no members yet — nothing to classify
        }
        let d = endpoint_domain(stream, members);
        match first {
            None => first = Some((d, reference)),
            Some((f, _)) if f == d => {}
            Some((f, first_ref)) => {
                return Err(FreesError::parse(format!(
                    "connect({}): cannot connect a {} port ({}) to a {} port ({}) — \
                     different physical domains. Couple domains through a transducer \
                     component (a motor, pump, heating resistor, …), not a direct connect.",
                    join_refs(refs),
                    f,
                    first_ref,
                    d,
                    reference
                )));
            }
        }
    }
    Ok(())
}

/// Rejects a fluid `connect` node whose endpoints are different fluid connector
/// types — a pneumatic (`gas`) line tied to a hydraulic (`oil`) or thermofluid
/// (`fluid`) line.
///
/// Port of `checkFluidConnectorType`. All the fluid-family types share the same
/// `(P, ṁ, h)` bond algebra but model incompatible working fluids, so the
/// mistake is caught here instead of silently solving. Streams with no
/// connector tag (everything outside the fluid domain) are skipped.
///
/// Call this only for a node [`node_domain`] classified [`Domain::Fluid`], as
/// `expandConnects` does.
pub fn check_fluid_connector_type<S: AsRef<str>, R: AsRef<str>>(
    streams: &[S],
    refs: &[R],
    types: &ConnectorTypes,
) -> Result<()> {
    let mut found: Option<(&str, &str)> = None;
    for (stream, reference) in streams.iter().zip(refs.iter()) {
        let reference = reference.as_ref();
        let Some(d) = types.get(stream.as_ref()) else {
            continue;
        };
        match found {
            None => found = Some((d, reference)),
            Some((f, _)) if f == d => {}
            Some((f, found_ref)) => {
                return Err(FreesError::parse(format!(
                    "connect({}): cannot connect a '{}' line ({}) to a '{}' line ({}). \
                     Thermofluid ('fluid'), liquid ('liquid'), two-phase ('twophase'), \
                     pneumatic ('gas'), hydraulic ('oil') and humid-air ('moistair') are \
                     incompatible fluid connector types; couple them only through a heat \
                     exchanger.",
                    join_refs(refs),
                    f,
                    found_ref,
                    d,
                    reference
                )));
            }
        }
    }
    Ok(())
}

fn join_refs<R: AsRef<str>>(refs: &[R]) -> String {
    refs.iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Across members, riders included ─────────────────────────────────────────

/// The across members for a specific node: [`Domain::across_members`] plus the
/// conserved-scalar riders the node's streams actually carry.
///
/// Port of `acrossMembersForNode`. Only a fluid node gains riders, in this
/// order:
///
/// 1. `w` — the humidity ratio, when the node's connector type is `moistair`.
///    Humid air works on the basis `(P, ṁ_da, h, W)`: `Σṁ_da = 0` conserves dry
///    air, and `W` is a second conserved mass (water). It is **equal across a
///    pass-through connect**, and flow-weighted only inside an explicit mixing
///    component's own body — that asymmetry is what makes humid air distinct
///    from a single-phase fluid.
/// 2. `z` — the opt-in zeotropic-blend composition, when some stream carries it.
///    Pure and azeotropic refrigerants carry no `z`, so their connector stays a
///    plain `(P, ṁ, h)` bond.
/// 3. The [`SPECIES_RIDERS`] present at the node — the gas-mixture `.y` species
///    fraction, the named combustion species `yo2`/`yco2`/`yh2o`/`yn2`, and the
///    oil-circulation fraction `oc`. They ride exactly like `W` and `z`.
pub fn across_members_for_node<S: AsRef<str>>(
    domain: Domain,
    streams: &[S],
    members: &StreamMembers,
    types: &ConnectorTypes,
) -> Vec<&'static str> {
    let base = domain.across_members();
    if domain != Domain::Fluid {
        return base.to_vec();
    }
    let mut ext = base.to_vec();
    if node_fluid_type(streams, types) == "moistair" {
        ext.push("w"); // humidity-ratio rider (water mass) — moist-air domain
    }
    if node_has_member(streams, "z", members) {
        ext.push("z");
    }
    for sp in SPECIES_RIDERS {
        if node_has_member(streams, sp, members) {
            ext.push(sp);
        }
    }
    ext
}

// ── Canonical member units ──────────────────────────────────────────────────

/// The SI unit of a *canonical* (stored, solver-unknown) stream member.
///
/// Port of `CANONICAL_UNITS`. Derived properties of a fluid stream
/// (temperature, entropy, …) are absent: they are rewritten to property calls
/// that already carry units. Nothing else ever grounds a stream member's
/// dimension, so without this a stream's own unknowns would show up
/// dimensionless.
///
/// `w` deliberately appears in two domains with different meanings — a
/// moist-air humidity ratio (dimensionless) and a mechanical angular speed —
/// which is exactly why the lookup is keyed by domain. The signal domain has no
/// table at all: a `sig` value carries whatever the control law gives it.
pub fn canonical_unit(domain: Domain, member: &str) -> Option<&'static str> {
    let table: &[(&str, &str)] = match domain {
        Domain::Fluid => &[("p", "Pa"), ("mdot", "kg/s"), ("h", "J/kg"), ("w", "-")],
        Domain::Heat => &[("t", "K"), ("qdot", "W")],
        Domain::Electrical => &[("v", "V"), ("i", "A")],
        Domain::Mechanical => &[("w", "rad/s"), ("tau", "N-m")],
        Domain::Translational => &[("vel", "m/s"), ("f", "N")],
        Domain::Signal => return None,
    };
    table
        .iter()
        .find(|(m, _)| *m == member)
        .map(|(_, unit)| *unit)
}

/// SI units of every stream's canonical members, keyed by the flat solver
/// variable name (`s2$p`, `s2$h`, `s2$mdot`).
///
/// Port of `memberUnits`. The unit is chosen by the stream's own domain (the
/// through-variable wins, per [`endpoint_domain`]), so a moist-air stream's `w`
/// is a humidity ratio while a mechanical stream's `w` is an angular speed.
pub fn member_units(members: &StreamMembers) -> BTreeMap<String, &'static str> {
    let mut out = BTreeMap::new();
    for stream in members.streams() {
        let domain = endpoint_domain(stream, members);
        let Some(set) = members.members(stream) else {
            continue;
        };
        for member in set {
            if let Some(unit) = canonical_unit(domain, member) {
                out.insert(format!("{stream}${member}").to_ascii_lowercase(), unit);
            }
        }
    }
    out
}

// ── The junction equations ──────────────────────────────────────────────────

/// The flat solver-variable name of a stream member: `s2` + `h` → `s2$h`.
pub fn stream_member_name(stream: &str, member: &str) -> String {
    format!("{stream}${member}")
}

/// A stream member as a solver variable.
///
/// Registering the `s2.h` display name for the mangled `s2$h` is the expander's
/// job (it owns the display-name map); this stays pure.
pub fn stream_member(stream: &str, member: &str) -> Expr {
    Expr::var(stream_member_name(stream, member))
}

/// One across equality across a node edge: `a.member = b.member`.
///
/// Port of `equality`. `prefix` is the `"CONNECT a, b: "` source-text prefix the
/// expander builds, so diagnostics stay component-named.
pub fn equality(stream_a: &str, member: &str, stream_b: &str, prefix: &str) -> Equation {
    Equation::new(
        stream_member(stream_a, member),
        stream_member(stream_b, member),
        format!("{prefix}{stream_a}.{member} = {stream_b}.{member}"),
    )
}

/// The Kirchhoff flow balance `Σ(flow) = 0` over every port of a node — each
/// flow signed **into** its component, so what leaves one component enters the
/// others.
///
/// Port of `kirchhoffBalance`. This is `ΣQ̇=0` for heat, `ΣI=0` for electrical,
/// `Στ=0` for mechanical rotational and `ΣF=0` for translational; the sum is
/// built left-associatively in endpoint order, as the Java does.
pub fn kirchhoff_balance<S: AsRef<str>>(
    streams: &[S],
    flow_member: &str,
    prefix: &str,
) -> Equation {
    let mut sum: Option<Expr> = None;
    for st in streams {
        let f = stream_member(st.as_ref(), flow_member);
        sum = Some(match sum {
            None => f,
            Some(acc) => Expr::bin(BinOp::Add, acc, f),
        });
    }
    Equation::new(
        // An endpoint-less node cannot reach here (a connect needs two); the
        // Java would dereference null, this yields the vacuous 0 = 0.
        sum.unwrap_or_else(|| Expr::num(0.0)),
        Expr::num(0.0),
        format!("{prefix}sum({flow_member}) = 0"),
    )
}

/// Whether a connect endpoint is an inlet or an outlet, read from its port
/// name.
///
/// Port of `portDirection`. The test is `contains`, not equality, and `out` is
/// tested first — so `hot_in` is an inlet, `mot_in` is an inlet, `port` is
/// unknowable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirection {
    In,
    Out,
    Unknown,
}

/// Port of `portDirection`.
pub fn port_direction(reference: &str) -> PortDirection {
    let port = match reference.rfind('.') {
        Some(dot) => &reference[dot + 1..],
        None => reference,
    };
    if port.contains("out") {
        return PortDirection::Out;
    }
    if port.contains("in") {
        return PortDirection::In;
    }
    PortDirection::Unknown
}

/// `Σ(outlet ṁ) = Σ(inlet ṁ)` for a 3+-port fluid node.
///
/// Port of `massConservation`. The fluid flow rule is directional, not a signed
/// Kirchhoff sum: inlet/outlet comes from the port name, and an endpoint whose
/// direction cannot be read is a hard error rather than a guess.
///
/// `streams[i]` must be the stream `refs[i]` resolves to. `source_text` is the
/// `connect(...)` declaration's own text, quoted back in the rejection.
pub fn mass_conservation<S: AsRef<str>, R: AsRef<str>>(
    streams: &[S],
    refs: &[R],
    prefix: &str,
    source_text: &str,
) -> Result<Equation> {
    let mut outlets: Option<Expr> = None;
    let mut inlets: Option<Expr> = None;
    for (stream, reference) in streams.iter().zip(refs.iter()) {
        let reference = reference.as_ref();
        let mdot = stream_member(stream.as_ref(), "mdot");
        let side = match port_direction(reference) {
            PortDirection::Out => &mut outlets,
            PortDirection::In => &mut inlets,
            PortDirection::Unknown => {
                return Err(FreesError::parse(format!(
                    "connect(...): cannot tell whether '{reference}' is an inlet or an \
                     outlet for the mass balance — name the port with 'in'/'out', or \
                     split the flow with a Splitter/Mixer component. {source_text}"
                )));
            }
        };
        *side = Some(match side.take() {
            None => mdot,
            Some(acc) => Expr::bin(BinOp::Add, acc, mdot),
        });
    }
    match (outlets, inlets) {
        (Some(outlets), Some(inlets)) => Ok(Equation::new(
            outlets,
            inlets,
            format!("{prefix}sum(mdot_out) = sum(mdot_in)"),
        )),
        _ => Err(FreesError::parse(format!(
            "connect(...): a branching node needs at least one inlet and one outlet \
             port. {source_text}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Builds a `StreamMembers` from `(stream, &[members])` pairs.
    fn members(spec: &[(&str, &[&str])]) -> StreamMembers {
        let mut m = StreamMembers::new();
        for (stream, ms) in spec {
            for member in *ms {
                m.add(stream, member);
            }
        }
        m
    }

    fn params(spec: &[(&str, &str)]) -> Vec<(String, String)> {
        spec.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn types(spec: &[(&str, &str)], members: &StreamMembers) -> ConnectorTypes {
        let mut t = ConnectorTypes::new();
        for (stream, dom) in spec {
            let ports = params(&[("p", stream)]);
            t.tag_instance(dom, &ports, members).unwrap();
        }
        t
    }

    fn err(e: FreesError) -> String {
        match e {
            FreesError::Parse { message, .. } => message,
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    // ── the (across, flow) pairs ────────────────────────────────────────────

    #[test]
    fn each_domain_declares_its_across_and_flow_pair() {
        assert_eq!(Domain::Fluid.across_members(), ["p", "h"]);
        assert_eq!(Domain::Fluid.flow_member(), Some("mdot"));

        assert_eq!(Domain::Heat.across_members(), ["t"]);
        assert_eq!(Domain::Heat.flow_member(), Some("qdot"));

        assert_eq!(Domain::Electrical.across_members(), ["v"]);
        assert_eq!(Domain::Electrical.flow_member(), Some("i"));

        assert_eq!(Domain::Mechanical.across_members(), ["w"]);
        assert_eq!(Domain::Mechanical.flow_member(), Some("tau"));

        assert_eq!(Domain::Translational.across_members(), ["vel"]);
        assert_eq!(Domain::Translational.flow_member(), Some("f"));

        // The causal signal domain is across-only: one writer, many readers.
        assert_eq!(Domain::Signal.across_members(), ["sig"]);
        assert_eq!(Domain::Signal.flow_member(), None);
    }

    #[test]
    fn junction_rules_match_the_domains() {
        assert_eq!(Domain::Fluid.junction_rule(), JunctionRule::FluidMass);
        assert_eq!(
            Domain::Heat.junction_rule(),
            JunctionRule::Kirchhoff("qdot")
        );
        assert_eq!(
            Domain::Electrical.junction_rule(),
            JunctionRule::Kirchhoff("i")
        );
        assert_eq!(
            Domain::Mechanical.junction_rule(),
            JunctionRule::Kirchhoff("tau")
        );
        assert_eq!(
            Domain::Translational.junction_rule(),
            JunctionRule::Kirchhoff("f")
        );
        assert_eq!(Domain::Signal.junction_rule(), JunctionRule::Broadcast);
    }

    #[test]
    fn domain_names_are_the_java_lowercase_spellings() {
        assert_eq!(Domain::Fluid.to_string(), "fluid");
        assert_eq!(Domain::Heat.to_string(), "heat");
        assert_eq!(Domain::Electrical.to_string(), "electrical");
        assert_eq!(Domain::Mechanical.to_string(), "mechanical");
        assert_eq!(Domain::Translational.to_string(), "translational");
        assert_eq!(Domain::Signal.to_string(), "signal");
    }

    // ── classification: the through-variable wins ───────────────────────────

    #[test]
    fn through_variable_classifies_each_domain() {
        let m = members(&[
            ("f1", &["p", "h", "mdot"]),
            ("q1", &["t", "qdot"]),
            ("e1", &["v", "i"]),
            ("r1", &["w", "tau"]),
            ("x1", &["vel", "f"]),
            ("c1", &["sig"]),
        ]);
        assert_eq!(endpoint_domain("f1", &m), Domain::Fluid);
        assert_eq!(endpoint_domain("q1", &m), Domain::Heat);
        assert_eq!(endpoint_domain("e1", &m), Domain::Electrical);
        assert_eq!(endpoint_domain("r1", &m), Domain::Mechanical);
        assert_eq!(endpoint_domain("x1", &m), Domain::Translational);
        assert_eq!(endpoint_domain("c1", &m), Domain::Signal);
    }

    #[test]
    fn moist_air_stream_carrying_t_and_mdot_is_fluid_not_heat() {
        // The headline case: the through-variable wins.
        let m = members(&[("air", &["p", "t", "h", "mdot", "w"])]);
        assert_eq!(endpoint_domain("air", &m), Domain::Fluid);
    }

    #[test]
    fn across_only_sources_are_still_placed_in_their_domain() {
        // A thermal source's T, a ground's V, a mechanical ground's w, a
        // translational anchor's vel: no through variable at all, yet each
        // lands in its own domain rather than the fluid default.
        let m = members(&[
            ("tsrc", &["t"]),
            ("gnd", &["v"]),
            ("shaft", &["w"]),
            ("anchor", &["vel"]),
        ]);
        assert_eq!(endpoint_domain("tsrc", &m), Domain::Heat);
        assert_eq!(endpoint_domain("gnd", &m), Domain::Electrical);
        assert_eq!(endpoint_domain("shaft", &m), Domain::Mechanical);
        assert_eq!(endpoint_domain("anchor", &m), Domain::Translational);
    }

    #[test]
    fn a_signal_member_beats_every_other_member() {
        // `sig` is tested before `mdot`: a probe that also names a flow member
        // is still a signal wire.
        let m = members(&[("s", &["sig", "mdot", "qdot", "i", "tau"])]);
        assert_eq!(endpoint_domain("s", &m), Domain::Signal);
    }

    #[test]
    fn flow_members_outrank_across_members_of_other_domains() {
        // `t` alone means heat, but `t` + `mdot` is fluid; `w` alone means
        // mechanical, but `w` + `mdot` is a moist-air fluid line.
        assert_eq!(
            endpoint_domain("s", &members(&[("s", &["t"])])),
            Domain::Heat
        );
        assert_eq!(
            endpoint_domain("s", &members(&[("s", &["t", "mdot"])])),
            Domain::Fluid
        );
        assert_eq!(
            endpoint_domain("s", &members(&[("s", &["w"])])),
            Domain::Mechanical
        );
        assert_eq!(
            endpoint_domain("s", &members(&[("s", &["w", "mdot"])])),
            Domain::Fluid
        );
        // qdot outranks the electrical across `v`.
        assert_eq!(
            endpoint_domain("s", &members(&[("s", &["v", "qdot"])])),
            Domain::Heat
        );
    }

    #[test]
    fn an_unknown_stream_defaults_to_fluid() {
        let m = StreamMembers::new();
        assert_eq!(endpoint_domain("nowhere", &m), Domain::Fluid);
        assert_eq!(node_domain::<&str>(&[], &m), Domain::Fluid);
    }

    #[test]
    fn node_domain_unions_every_endpoints_members() {
        // A source carrying only `t` joined to a wall carrying `qdot`: heat.
        let m = members(&[("a", &["t"]), ("b", &["qdot"])]);
        assert_eq!(node_domain(&["a", "b"], &m), Domain::Heat);
    }

    // ── checkSingleDomain ───────────────────────────────────────────────────

    #[test]
    fn single_domain_accepts_a_homogeneous_node() {
        let m = members(&[("a", &["t", "qdot"]), ("b", &["t"]), ("c", &["t", "qdot"])]);
        check_single_domain(&["a", "b", "c"], &["hx.wall", "ts.port", "mass.port"], &m).unwrap();
    }

    #[test]
    fn heat_cannot_connect_to_electrical() {
        let m = members(&[("ts", &["t"]), ("ra", &["v", "i"])]);
        let e = check_single_domain(&["ts", "ra"], &["ts.port", "r.a"], &m).unwrap_err();
        assert_eq!(
            err(e),
            "connect(ts.port, r.a): cannot connect a heat port (ts.port) to a \
             electrical port (r.a) — different physical domains. Couple domains \
             through a transducer component (a motor, pump, heating resistor, …), \
             not a direct connect."
        );
    }

    #[test]
    fn fluid_cannot_connect_to_a_mechanical_shaft() {
        let m = members(&[("src", &["p", "h", "mdot"]), ("g", &["w"])]);
        let e = check_single_domain(&["src", "g"], &["src.out", "g.port"], &m).unwrap_err();
        assert_eq!(
            err(e),
            "connect(src.out, g.port): cannot connect a fluid port (src.out) to a \
             mechanical port (g.port) — different physical domains. Couple domains \
             through a transducer component (a motor, pump, heating resistor, …), \
             not a direct connect."
        );
    }

    #[test]
    fn a_signal_wire_cannot_be_tied_to_a_physical_port() {
        let m = members(&[("cmd", &["sig"]), ("v", &["p", "h", "mdot"])]);
        let msg = err(check_single_domain(&["cmd", "v"], &["pid.out", "vlv.in"], &m).unwrap_err());
        assert!(
            msg.starts_with("connect(pid.out, vlv.in): cannot connect a signal port (pid.out) to a fluid port (vlv.in)"),
            "{msg}"
        );
    }

    #[test]
    fn translational_and_rotational_mechanics_are_different_domains() {
        // ω/τ and v/F are separate pairs; a rod is not a shaft.
        let m = members(&[("shaft", &["w", "tau"]), ("rod", &["vel", "f"])]);
        let msg = err(check_single_domain(&["shaft", "rod"], &["i.a", "m.b"], &m).unwrap_err());
        assert!(
            msg.contains("a mechanical port (i.a) to a translational port (m.b)"),
            "{msg}"
        );
    }

    #[test]
    fn the_rejection_names_the_first_classifiable_endpoint_not_the_first_endpoint() {
        // The bare stream `x` has no members, so it is skipped entirely and the
        // heat endpoint becomes the reference the message quotes.
        let m = members(&[("q", &["t", "qdot"]), ("e", &["v", "i"])]);
        let msg = err(
            check_single_domain(&["x", "q", "e"], &["bare", "hx.wall", "r.a"], &m).unwrap_err(),
        );
        assert!(
            msg.contains("a heat port (hx.wall) to a electrical port (r.a)"),
            "{msg}"
        );
        // …but every written endpoint is still listed in the connect(…) echo.
        assert!(msg.starts_with("connect(bare, hx.wall, r.a):"), "{msg}");
    }

    #[test]
    fn unclassifiable_endpoints_alone_never_fail() {
        let m = StreamMembers::new();
        check_single_domain(&["a", "b"], &["x.p", "y.p"], &m).unwrap();
    }

    // ── checkFluidConnectorType ─────────────────────────────────────────────

    #[test]
    fn pneumatic_cannot_connect_to_hydraulic() {
        // Oracle: `PneumaticSupply SUP(fluid$=Air, P=700000, T=300)` +
        // `HydraulicTank TNK(P=0)` + `connect(SUP.out, TNK.port)` — the Java
        // engine rejects it with exactly this text.
        let m = members(&[("sup", &["p", "h", "mdot"]), ("tnk", &["p", "h", "mdot"])]);
        let t = types(&[("sup", "gas"), ("tnk", "oil")], &m);
        let e =
            check_fluid_connector_type(&["sup", "tnk"], &["sup.out", "tnk.port"], &t).unwrap_err();
        assert_eq!(
            err(e),
            "connect(sup.out, tnk.port): cannot connect a 'gas' line (sup.out) to a \
             'oil' line (tnk.port). Thermofluid ('fluid'), liquid ('liquid'), \
             two-phase ('twophase'), pneumatic ('gas'), hydraulic ('oil') and \
             humid-air ('moistair') are incompatible fluid connector types; couple \
             them only through a heat exchanger."
        );
    }

    #[test]
    fn pneumatic_cannot_connect_to_generic_thermofluid() {
        // A user component that declares no `domain$` defaults to 'fluid', and
        // a gas line still cannot reach it. Oracle-verified.
        let m = members(&[("sup", &["p", "h", "mdot"]), ("snk", &["p", "mdot"])]);
        let t = types(&[("sup", "gas"), ("snk", "fluid")], &m);
        let msg = err(
            check_fluid_connector_type(&["sup", "snk"], &["sup.out", "snk.in"], &t).unwrap_err(),
        );
        assert!(
            msg.contains("a 'gas' line (sup.out) to a 'fluid' line (snk.in)"),
            "{msg}"
        );
    }

    #[test]
    fn a_gas_line_cannot_reach_a_moist_air_line() {
        // Oracle: `PneumaticSupply` + `MoistAirSink` in one connect.
        let m = members(&[("g", &["p", "h", "mdot"]), ("a", &["p", "h", "mdot", "w"])]);
        let t = types(&[("g", "gas"), ("a", "moistair")], &m);
        let msg = err(check_fluid_connector_type(&["g", "a"], &["g.out", "a.in"], &t).unwrap_err());
        assert!(
            msg.contains("a 'gas' line (g.out) to a 'moistair' line (a.in)"),
            "{msg}"
        );
    }

    #[test]
    fn same_connector_types_connect_cleanly() {
        let m = members(&[
            ("a", &["p", "h", "mdot"]),
            ("b", &["p", "h", "mdot"]),
            ("c", &["p", "h", "mdot"]),
        ]);
        let t = types(&[("a", "gas"), ("b", "gas"), ("c", "gas")], &m);
        check_fluid_connector_type(&["a", "b", "c"], &["sup.out", "ori.in", "atm.port"], &t)
            .unwrap();
    }

    #[test]
    fn untagged_streams_are_skipped_by_the_connector_check() {
        // Heat / electrical streams carry no connector type at all.
        let m = members(&[("q", &["t", "qdot"]), ("g", &["p", "h", "mdot"])]);
        let t = types(&[("g", "oil")], &m);
        assert_eq!(t.get("q"), None);
        check_fluid_connector_type(&["q", "g"], &["hx.wall", "t.port"], &t).unwrap();
    }

    // ── buildStreamDomainMap ────────────────────────────────────────────────

    #[test]
    fn connector_types_tag_only_fluid_class_streams() {
        let m = members(&[
            ("s1", &["p", "h", "mdot"]),
            ("wall", &["t", "qdot"]),
            ("wire", &["v", "i"]),
        ]);
        let ports = params(&[("in", "s1"), ("hot", "wall"), ("term", "wire")]);
        let sp = params(&[("fluid$", "water"), ("domain$", "liquid")]);
        let t = build_connector_types(
            &[InstanceView {
                string_params: &sp,
                ports: &ports,
            }],
            &m,
        )
        .unwrap();
        assert_eq!(t.get("s1"), Some("liquid"));
        assert_eq!(t.get("wall"), None);
        assert_eq!(t.get("wire"), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn a_component_without_domain_param_defaults_to_fluid() {
        assert_eq!(instance_connector_type(&[]), "fluid");
        assert_eq!(
            instance_connector_type(&params(&[("fluid$", "water")])),
            "fluid"
        );
        assert_eq!(
            instance_connector_type(&params(&[("model$", "eta"), ("domain$", "gas")])),
            "gas"
        );
    }

    #[test]
    fn two_conflicting_connector_types_on_one_shared_stream_are_rejected() {
        let m = members(&[("s1", &["p", "h", "mdot"])]);
        let gas = params(&[("domain$", "gas")]);
        let oil = params(&[("domain$", "oil")]);
        let ports = params(&[("out", "s1")]);
        let e = build_connector_types(
            &[
                InstanceView {
                    string_params: &gas,
                    ports: &ports,
                },
                InstanceView {
                    string_params: &oil,
                    ports: &ports,
                },
            ],
            &m,
        )
        .unwrap_err();
        assert_eq!(
            err(e),
            "Incompatible fluid connector types on stream 's1': 'gas' and 'oil' \
             bound to the same stream. Pneumatic ('gas'), hydraulic ('oil') and \
             thermofluid ('fluid') lines are different connector types and cannot \
             share a port."
        );
    }

    #[test]
    fn the_same_connector_type_twice_on_one_stream_is_fine() {
        let m = members(&[("s1", &["p", "h", "mdot"])]);
        let sp = params(&[("domain$", "twophase")]);
        let ports = params(&[("out", "s1")]);
        let view = InstanceView {
            string_params: &sp,
            ports: &ports,
        };
        let t = build_connector_types(&[view, view], &m).unwrap();
        assert_eq!(t.get("s1"), Some("twophase"));
    }

    #[test]
    fn node_fluid_type_takes_the_first_tagged_stream_and_defaults_to_fluid() {
        let m = members(&[("a", &["mdot"]), ("b", &["mdot"])]);
        let t = types(&[("b", "oil")], &m);
        assert_eq!(node_fluid_type(&["a", "b"], &t), "oil");
        assert_eq!(node_fluid_type(&["a"], &t), "fluid");
        assert_eq!(node_fluid_type::<&str>(&[], &t), "fluid");
    }

    // ── the domain$ / portFluid trap ────────────────────────────────────────

    #[test]
    fn domain_param_is_never_mistaken_for_a_fluid_name() {
        // Oracle-verified: a `PARAM domain$ = moistair` sink on stream `s1`
        // with `s1.T = 350` solves to the plain variable `s1.t = 350`. Had
        // `domain$` been read as a fluid, `s1.T` would have become the property
        // call `prop$temperature$moistair$p$h` instead — which is exactly what
        // the control document does: the same sink with `fluid$ = Water` and
        // `s2.T = 350` inverts through CoolProp to `s2.h = 321839.136…`.
        //
        // MoistAirSink(in): PARAM domain$ = moistair — no fluid at all.
        let sp = params(&[("domain$", "moistair")]);
        assert_eq!(port_fluid("in", &sp), None);
        assert_eq!(port_fluid("out", &sp), None);
        // Same for every other reserved / non-fluid string parameter.
        let sp = params(&[("domain$", "oil"), ("model$", "turbulent"), ("map$", "eta")]);
        assert_eq!(port_fluid("a", &sp), None);
        assert_eq!(port_fluid("b", &sp), None);
        assert_eq!(port_fluid("port", &sp), None);
    }

    #[test]
    fn an_explicit_fluid_param_applies_to_every_port() {
        let sp = params(&[("fluid$", "r1234yf"), ("domain$", "twophase")]);
        assert_eq!(port_fluid("in", &sp), Some("r1234yf"));
        assert_eq!(port_fluid("out", &sp), Some("r1234yf"));
        assert_eq!(port_fluid("wall", &sp), Some("r1234yf"));
    }

    #[test]
    fn a_port_name_prefixed_param_covers_only_its_own_side() {
        // A two-fluid heat exchanger: hot$ covers hot_in / hot_out, cold$ the
        // cold side, and neither leaks onto the other.
        let sp = params(&[("hot$", "water"), ("cold$", "air"), ("arr$", "counter")]);
        assert_eq!(port_fluid("hot_in", &sp), Some("water"));
        assert_eq!(port_fluid("hot_out", &sp), Some("water"));
        assert_eq!(port_fluid("cold_in", &sp), Some("air"));
        assert_eq!(port_fluid("cold_out", &sp), Some("air"));
        // A non-fluid string parameter matches nothing…
        assert_eq!(port_fluid("wall", &sp), None);
        // …unless the port name genuinely starts with its base, which is the
        // Java's documented behaviour and why `arr$` is listed last here.
        assert_eq!(port_fluid("arrival", &sp), Some("counter"));
    }

    #[test]
    fn fluid_param_wins_over_declaration_order() {
        // `fluid$` is looked up directly first, ahead of any prefix scan.
        let sp = params(&[("hot$", "water"), ("fluid$", "ammonia")]);
        assert_eq!(port_fluid("hot_in", &sp), Some("ammonia"));
    }

    // ── acrossMembersForNode ────────────────────────────────────────────────

    #[test]
    fn across_members_for_a_plain_fluid_node_are_pressure_and_enthalpy() {
        let m = members(&[("s1", &["p", "h", "mdot"])]);
        let t = types(&[("s1", "fluid")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["s1"], &m, &t),
            ["p", "h"]
        );
    }

    #[test]
    fn non_fluid_domains_never_gain_riders() {
        // Even if a heat / mechanical stream happens to carry a `w` or a `z`.
        let m = members(&[("q", &["t", "qdot", "z"]), ("r", &["w", "tau", "y"])]);
        let t = ConnectorTypes::new();
        assert_eq!(across_members_for_node(Domain::Heat, &["q"], &m, &t), ["t"]);
        assert_eq!(
            across_members_for_node(Domain::Mechanical, &["r"], &m, &t),
            ["w"]
        );
        assert_eq!(
            across_members_for_node(Domain::Signal, &["r"], &m, &t),
            ["sig"]
        );
    }

    #[test]
    fn a_moist_air_node_carries_the_humidity_ratio_across_a_pass_through() {
        // (P, ṁ_da, h, W): W is equal across a plain connect — that is what
        // lets a humid-air series chain be wired without a mixing box.
        //
        // Oracle: MoistAirSource → HeatingCoil → MoistAirSink wired with two
        // plain `connect(...)` nodes carries W = 0.008 unchanged end to end
        // (`mas.out.w` = `hc.in.w` = `hc.out.w` = `mak.in.w`) while h rises by
        // Q/ṁ across the coil.
        let m = members(&[
            ("a1", &["p", "h", "mdot", "w"]),
            ("a2", &["p", "h", "mdot"]),
        ]);
        let t = types(&[("a1", "moistair"), ("a2", "moistair")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["a1", "a2"], &m, &t),
            ["p", "h", "w"]
        );
    }

    #[test]
    fn the_humidity_rider_comes_from_the_connector_type_not_the_member() {
        // Oracle-verified with two documents identical but for `domain$`:
        // with `domain$ = moistair` the Java solves and reports
        // `s.out.w = e.in.w = 0.01`; with the default `fluid` it refuses —
        // "Free quantity (no defining relation): e.wx … Coupled to: e.in.w" —
        // because `w` was never equated across the node.
        //
        // `w` rides because the node is `moistair`, even where only one
        // endpoint has been seen carrying it…
        let m = members(&[("a", &["p", "h", "mdot"])]);
        let t = types(&[("a", "moistair")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["a"], &m, &t),
            ["p", "h", "w"]
        );
        // …and a plain `fluid` line never gains it, whatever it carries.
        let m = members(&[("b", &["p", "h", "mdot", "w"])]);
        let t = types(&[("b", "fluid")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["b"], &m, &t),
            ["p", "h"]
        );
    }

    #[test]
    fn the_zeotropic_blend_composition_rides_when_present() {
        let m = members(&[("s1", &["p", "h", "mdot", "z"])]);
        let t = types(&[("s1", "twophase")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["s1"], &m, &t),
            ["p", "h", "z"]
        );
        // A pure refrigerant carries no `z`, so its bond stays (P, ṁ, h).
        let m = members(&[("s2", &["p", "h", "mdot"])]);
        let t = types(&[("s2", "twophase")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["s2"], &m, &t),
            ["p", "h"]
        );
    }

    #[test]
    fn the_species_riders_propagate_in_their_declared_order() {
        // Oracle: a `domain$ = gas` source → pipe → sink chain wired with two
        // `connect(...)` nodes carries yo2 = 0.21, yco2 = 0.01, yh2o = 0.02 and
        // yn2 = 0.76 unchanged across every node. The member set is deliberately
        // listed out of order here — the rider order is SPECIES_RIDERS', not
        // the stream's.
        let m = members(&[(
            "g",
            &["p", "h", "mdot", "oc", "yn2", "y", "yh2o", "yco2", "yo2"],
        )]);
        let t = types(&[("g", "gas")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["g"], &m, &t),
            ["p", "h", "y", "yo2", "yco2", "yh2o", "yn2", "oc"]
        );
    }

    #[test]
    fn a_rider_present_on_any_endpoint_rides_the_whole_node() {
        let m = members(&[
            ("a", &["p", "h", "mdot"]),
            ("b", &["p", "h", "mdot", "yo2"]),
        ]);
        let t = types(&[("a", "gas"), ("b", "gas")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["a", "b"], &m, &t),
            ["p", "h", "yo2"]
        );
    }

    #[test]
    fn riders_stack_in_the_order_w_then_z_then_species() {
        let m = members(&[("s", &["p", "h", "mdot", "z", "y", "oc"])]);
        let t = types(&[("s", "moistair")], &m);
        assert_eq!(
            across_members_for_node(Domain::Fluid, &["s"], &m, &t),
            ["p", "h", "w", "z", "y", "oc"]
        );
    }

    // ── canonical units ─────────────────────────────────────────────────────

    #[test]
    fn w_means_a_humidity_ratio_on_a_fluid_stream_and_a_speed_on_a_shaft() {
        assert_eq!(canonical_unit(Domain::Fluid, "w"), Some("-"));
        assert_eq!(canonical_unit(Domain::Mechanical, "w"), Some("rad/s"));
    }

    #[test]
    fn member_units_are_keyed_by_the_streams_own_domain() {
        let m = members(&[
            ("s2", &["p", "h", "mdot", "w"]),
            ("wall", &["t", "qdot"]),
            ("wire", &["v", "i"]),
            ("shaft", &["w", "tau"]),
            ("rod", &["vel", "f"]),
            ("cmd", &["sig"]),
        ]);
        let u = member_units(&m);
        assert_eq!(u.get("s2$p"), Some(&"Pa"));
        assert_eq!(u.get("s2$h"), Some(&"J/kg"));
        assert_eq!(u.get("s2$mdot"), Some(&"kg/s"));
        assert_eq!(u.get("s2$w"), Some(&"-"));
        assert_eq!(u.get("wall$t"), Some(&"K"));
        assert_eq!(u.get("wall$qdot"), Some(&"W"));
        assert_eq!(u.get("wire$v"), Some(&"V"));
        assert_eq!(u.get("wire$i"), Some(&"A"));
        assert_eq!(u.get("shaft$w"), Some(&"rad/s"));
        assert_eq!(u.get("shaft$tau"), Some(&"N-m"));
        assert_eq!(u.get("rod$vel"), Some(&"m/s"));
        assert_eq!(u.get("rod$f"), Some(&"N"));
        // The signal domain has no unit table at all.
        assert_eq!(u.get("cmd$sig"), None);
    }

    #[test]
    fn derived_properties_of_a_fluid_stream_get_no_canonical_unit() {
        // `s2.T` / `s2.x` / `s2.rho` are rewritten to property calls that carry
        // their own units; only the stored unknowns are grounded here.
        let m = members(&[("s2", &["p", "h", "mdot", "t", "x", "rho"])]);
        let u = member_units(&m);
        assert_eq!(u.get("s2$t"), None);
        assert_eq!(u.get("s2$x"), None);
        assert_eq!(u.get("s2$rho"), None);
        assert_eq!(u.len(), 3);
    }

    // ── the junction equations ──────────────────────────────────────────────

    #[test]
    fn an_across_equality_names_both_streams_in_its_source_text() {
        let eq = equality("s1", "p", "s2", "CONNECT A.out, B.in: ");
        assert_eq!(eq.lhs, Expr::var("s1$p"));
        assert_eq!(eq.rhs, Expr::var("s2$p"));
        assert_eq!(eq.source_text, "CONNECT A.out, B.in: s1.p = s2.p");
    }

    #[test]
    fn heat_conserves_qdot_as_a_kirchhoff_sum() {
        // ΣQ̇ = 0 over three endpoints, summed left-associatively.
        let eq = kirchhoff_balance(&["a", "b", "c"], "qdot", "CONNECT: ");
        let expected = Expr::bin(
            BinOp::Add,
            Expr::bin(BinOp::Add, Expr::var("a$qdot"), Expr::var("b$qdot")),
            Expr::var("c$qdot"),
        );
        assert_eq!(eq.lhs, expected);
        assert_eq!(eq.rhs, Expr::num(0.0));
        assert_eq!(eq.source_text, "CONNECT: sum(qdot) = 0");
    }

    #[test]
    fn electrical_mechanical_and_translational_use_the_same_kirchhoff_shape() {
        for (domain, member) in [
            (Domain::Electrical, "i"),
            (Domain::Mechanical, "tau"),
            (Domain::Translational, "f"),
        ] {
            let JunctionRule::Kirchhoff(flow) = domain.junction_rule() else {
                panic!("{domain} should use a Kirchhoff balance");
            };
            assert_eq!(flow, member);
            let eq = kirchhoff_balance(&["p", "n"], flow, "");
            assert_eq!(
                eq.lhs,
                Expr::bin(
                    BinOp::Add,
                    Expr::var(format!("p${member}")),
                    Expr::var(format!("n${member}"))
                )
            );
            assert_eq!(eq.source_text, format!("sum({member}) = 0"));
        }
    }

    #[test]
    fn a_branching_fluid_node_balances_outlets_against_inlets() {
        // Σ(ṁ_out) = Σ(ṁ_in) — the fluid rule is directional, not a signed sum.
        let eq = mass_conservation(
            &["s1", "s2", "s3"],
            &["SPL.in", "SPL.out1", "SPL.out2"],
            "CONNECT: ",
            "connect(SPL.in, SPL.out1, SPL.out2)",
        )
        .unwrap();
        assert_eq!(
            eq.lhs,
            Expr::bin(BinOp::Add, Expr::var("s2$mdot"), Expr::var("s3$mdot"))
        );
        assert_eq!(eq.rhs, Expr::var("s1$mdot"));
        assert_eq!(eq.source_text, "CONNECT: sum(mdot_out) = sum(mdot_in)");
    }

    #[test]
    fn port_direction_reads_in_and_out_out_of_the_port_name() {
        assert_eq!(port_direction("SPL.out1"), PortDirection::Out);
        assert_eq!(port_direction("HX.hot_in"), PortDirection::In);
        assert_eq!(port_direction("EJ.mot_in"), PortDirection::In);
        assert_eq!(port_direction("TNK.port"), PortDirection::Unknown);
        assert_eq!(port_direction("out"), PortDirection::Out);
        // `out` is tested before `in`, so a name carrying both is an outlet.
        assert_eq!(port_direction("X.outin"), PortDirection::Out);
    }

    #[test]
    fn an_undirected_endpoint_is_rejected_not_guessed() {
        let e = mass_conservation(
            &["s1", "s2", "s3"],
            &["A.out", "B.in", "MIX.c"],
            "CONNECT: ",
            "connect(A.out, B.in, MIX.c)",
        )
        .unwrap_err();
        assert_eq!(
            err(e),
            "connect(...): cannot tell whether 'MIX.c' is an inlet or an outlet for \
             the mass balance — name the port with 'in'/'out', or split the flow with \
             a Splitter/Mixer component. connect(A.out, B.in, MIX.c)"
        );
    }

    #[test]
    fn a_branch_with_no_inlet_is_rejected() {
        let e = mass_conservation(
            &["s1", "s2"],
            &["A.out", "B.out"],
            "CONNECT: ",
            "connect(A.out, B.out)",
        )
        .unwrap_err();
        assert_eq!(
            err(e),
            "connect(...): a branching node needs at least one inlet and one outlet \
             port. connect(A.out, B.out)"
        );
    }

    #[test]
    fn an_empty_node_yields_the_vacuous_balance_rather_than_panicking() {
        let eq = kirchhoff_balance::<&str>(&[], "i", "");
        assert_eq!(eq.lhs, Expr::num(0.0));
        assert_eq!(eq.rhs, Expr::num(0.0));
    }

    // ── member collection off a component body ──────────────────────────────

    #[test]
    fn member_collection_maps_port_references_onto_bound_streams() {
        // COMPONENT HeatingCoil(in, out): out.h = in.h + Q / in.mdot
        let ports = params(&[("in", "a1"), ("out", "a2")]);
        let eq = Equation::new(
            Expr::var("out.h"),
            Expr::bin(
                BinOp::Add,
                Expr::var("in.h"),
                Expr::bin(BinOp::Div, Expr::var("q"), Expr::var("in.mdot")),
            ),
            "out.h = in.h + Q / in.mdot",
        );
        let mut m = StreamMembers::new();
        m.collect_from_equation(&eq, &ports);
        assert!(m.has("a1", "h"));
        assert!(m.has("a1", "mdot"));
        assert!(m.has("a2", "h"));
        // A bare local (`q`) is not a port member.
        assert_eq!(m.len(), 2);
        assert_eq!(endpoint_domain("a1", &m), Domain::Fluid);
    }

    #[test]
    fn member_collection_ignores_unknown_ports_and_nested_dots() {
        let ports = params(&[("in", "s1")]);
        let mut m = StreamMembers::new();
        // `nope` is not a port of this instance.
        m.collect_from_expr(&Expr::var("nope.h"), &ports);
        // `in.a.b` has a dotted member — the Java records nothing.
        m.collect_from_expr(&Expr::var("in.a.b"), &ports);
        assert!(m.is_empty());
        m.collect_from_expr(&Expr::var("in.mdot"), &ports);
        assert!(m.has("s1", "mdot"));
    }

    #[test]
    fn member_collection_walks_calls_arrays_and_comparisons() {
        let ports = params(&[("in", "s1"), ("out", "s2")]);
        let mut m = StreamMembers::new();
        m.collect_from_expr(
            &Expr::call(
                "enthalpy",
                vec![Expr::var("in.p"), Expr::Neg(Box::new(Expr::var("in.t")))],
            ),
            &ports,
        );
        m.collect_from_expr(
            &Expr::ArrayLiteral(vec![Expr::ArrayAccess {
                name: "tab".into(),
                indices: vec![Expr::var("out.mdot")],
            }]),
            &ports,
        );
        m.collect_from_expr(
            &Expr::Compare {
                op: crate::ast::CmpOp::Gt,
                left: Box::new(Expr::var("out.p")),
                right: Box::new(Expr::num(0.0)),
            },
            &ports,
        );
        assert!(m.has("s1", "p") && m.has("s1", "t"));
        assert!(m.has("s2", "mdot") && m.has("s2", "p"));
    }

    #[test]
    fn is_fluid_stream_needs_mdot_not_merely_the_fluid_default() {
        let m = members(&[("s1", &["p", "h", "mdot"]), ("s2", &["p"])]);
        assert!(is_fluid_stream("s1", &m));
        // `s2` classifies FLUID by the default fallback but is not a fluid
        // stream — the distinction that keeps a pass-through seed honest.
        assert_eq!(endpoint_domain("s2", &m), Domain::Fluid);
        assert!(!is_fluid_stream("s2", &m));
        assert!(!is_fluid_stream("unknown", &m));
    }
}
