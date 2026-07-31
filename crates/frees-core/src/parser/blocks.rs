//! Declarative block AST — `PARAMETRIC`, `PLOT` and `STATE TABLE`.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/ast/ParametricTable.java`
//! (14 LOC), `PlotDef.java` (17) and `StateTableDef.java` (20).
//!
//! These types are a **fixed contract**, exactly as [`crate::parser::defs`] is
//! for the Phase-4 procedural layer and [`crate::components::def`] is for the
//! Phase-6 component layer: [`crate::parser::toplevel`] fills them from the
//! grammar rules `parametricDef` / `paramColumn`, `plotDef` / `plotAttr` /
//! `plotValue` and `stateTableDef` / `stateTableAttr` / `stateAttrValue`, and
//! [`crate::analysis::parametric`] consumes the first of them.
//!
//! # What the three have in common: none of them is an equation
//!
//! Every Java doc comment here says the same thing — "it never enters the
//! equation system". A `PARAMETRIC` block describes a sweep the *Tables* tab
//! runs (one solve per row, see [`crate::analysis::parametric`]); a `PLOT`
//! block describes a graph the frontend renders; a `STATE TABLE` block groups
//! state points under one fluid for the Fluid States window. So a document that
//! declares one of them and nothing else has **no equations to solve**, and a
//! document whose base system is underspecified *on purpose* because a swept
//! column will supply the missing value still fails a plain Solve — which is
//! exactly what the Java oracle does (`fixtures/corpus-pending/golden/
//! projectile-trajectory.json`: `SolverException`, "underspecified"). Parsing
//! these blocks must not change that, and does not: they land here, not in
//! [`Document::statements`](crate::parser::Document::statements).
//!
//! # Case handling is not uniform, and the difference is transcribed
//!
//! `AstBuilder` is inconsistent about lowercasing across the three builders, and
//! the port keeps each one as written because these names are echoed back to the
//! frontend verbatim:
//!
//! | field | Java | case |
//! |---|---|---|
//! | [`ParametricTable::name`] | `ctx.IDENT().getText()` | as written |
//! | [`ParametricTable::vars`] | `ctx.paramList().IDENT()` read **directly** | as written |
//! | [`PlotDef::name`] | `unquote(STRING_LITERAL)` | as written |
//! | plot attribute keys | `attr.IDENT().getText().toLowerCase()` | lowercase |
//! | plot attribute values | string content / number text / `IDENT` text | as written |
//! | [`StateTableDef::name`] | `ctx.IDENT().getText()` | as written |
//! | [`StateTableDef::variables`] | `buildParamList(...)` | **lowercase** |
//! | [`StateTableDef::fluid`] | `unquote(...)` / `IDENT` text | as written |
//!
//! The one place a lookup crosses that boundary is
//! [`ParametricTable::column`], which lowercases its argument exactly as
//! `buildParametricDef` does (`columns.get(var.toLowerCase())`).

/// A parametric run-table declared with `PARAMETRIC name(v1, v2, …) … END`.
///
/// Port of `ast/ParametricTable.java`. It is not a callable definition and
/// never enters the equation system: it only describes a sweep — the declared
/// variables and the value of each column per row — that the Tables tab turns
/// into a run table and executes one solve per row (see
/// [`crate::analysis::parametric`]).
///
/// `rows` is **row-major and aligned to `vars`**; a `None` cell means that
/// column has no value for that row, which is the ordinary state of a declared
/// *output* column — `PARAMETRIC trajectory (t, time, x, y)` with a range for
/// `t` only leaves `time`/`x`/`y` `None` in every row, and the solve fills them.
#[derive(Debug, Clone, PartialEq)]
pub struct ParametricTable {
    /// Table name, in the case the user wrote it.
    pub name: String,
    /// Declared column names, in header order, in the case the user wrote them.
    pub vars: Vec<String>,
    /// Row-major cells, each row aligned to `vars`.
    pub rows: Vec<Vec<Option<f64>>>,
}

impl ParametricTable {
    /// The number of runs this table declares — `rows.len()`, which
    /// `buildParametricDef` computes as the longest column.
    pub fn run_count(&self) -> usize {
        self.rows.len()
    }

    /// Every value of one declared column, `None` where the row has no cell.
    ///
    /// The name is matched case-insensitively, the way `buildParametricDef`
    /// aligns its `LinkedHashMap<String, List<Double>>` (keyed lowercase) to
    /// the as-written [`ParametricTable::vars`].
    pub fn column(&self, name: &str) -> Option<Vec<Option<f64>>> {
        let wanted = name.to_ascii_lowercase();
        let index = self
            .vars
            .iter()
            .position(|v| v.to_ascii_lowercase() == wanted)?;
        Some(
            self.rows
                .iter()
                .map(|row| row.get(index).copied().flatten())
                .collect(),
        )
    }
}

/// The `key -> values` attribute list of one [`PlotDef`].
///
/// Port of `PlotDef`'s `Map<String, List<String>>`, which `buildPlotDef`
/// creates as a **`LinkedHashMap`**: iteration follows first-insertion order
/// and re-assigning a key replaces the value *in its original slot*. A `Vec` of
/// pairs reproduces both without an ordered-map dependency, exactly as
/// [`ParamOverrides`](crate::components::def::ParamOverrides) does for the
/// component layer; a plot has a handful of attributes, so the linear lookup is
/// never on a hot path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlotAttributes(Vec<(String, Vec<String>)>);

impl PlotAttributes {
    pub fn new() -> PlotAttributes {
        PlotAttributes(Vec::new())
    }

    /// `LinkedHashMap.put` — returns the value it displaced, if any.
    pub fn put(&mut self, key: String, values: Vec<String>) -> Option<Vec<String>> {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => Some(std::mem::replace(&mut slot.1, values)),
            None => {
                self.0.push((key, values));
                None
            }
        }
    }

    /// `Map.get` — the key is already lowercase, as `buildPlotDef` stores it.
    pub fn get(&self, lowercase_key: &str) -> Option<&[String]> {
        self.0
            .iter()
            .find(|(k, _)| k == lowercase_key)
            .map(|(_, values)| values.as_slice())
    }

    /// The single value of a key that carries exactly one, for the scalar
    /// attributes (`kind`, `type`, `xlabel`, …).
    pub fn single(&self, lowercase_key: &str) -> Option<&str> {
        match self.get(lowercase_key) {
            Some([only]) => Some(only.as_str()),
            _ => None,
        }
    }

    /// Entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.0
            .iter()
            .map(|(key, values)| (key.as_str(), values.as_slice()))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A plot declared with `PLOT 'name' … END`.
///
/// Port of `ast/PlotDef.java`. Like a [`ParametricTable`] it never enters the
/// equation system: it only describes a graph the frontend renders (xy /
/// property / psychro / bode / …) in the dedicated Plots tab.
///
/// Attributes are kept as a raw `key -> values` list, keys lowercased and
/// values normalised to their string form — unquoted string content, the
/// number's literal source text, or a variable's base name. The frontend maps
/// them onto its `PlotSpec`, so the backend stays decoupled from the plot
/// presentation model; that is why an `x = speed[1:N]` records `"speed"` and
/// drops the slice.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotDef {
    /// The plot name, as written between the quotes — this is the string a
    /// `[Graph='…']` tag in the report refers to.
    pub name: String,
    pub attributes: PlotAttributes,
}

/// A fluid state table declared with `STATE TABLE name(v1, v2, …) … END`.
///
/// Port of `ast/StateTableDef.java`. Like a [`ParametricTable`] or [`PlotDef`]
/// it never enters the equation system: it groups the listed state-point
/// variables into one circuit and declares the fluid those states belong to, so
/// property look-ups and the frontend's state table are fluid-aware — a `Water`
/// state 1 and an `R134a` state 1 stay separate.
#[derive(Debug, Clone, PartialEq)]
pub struct StateTableDef {
    /// The circuit/table name (`WaterCircuit1`), in the case the user wrote it.
    pub name: String,
    /// The declared state-point variables, **lowercased** (`[pw1, pw_2, tw1]`);
    /// their numeric values are captured from the solve, not declared here.
    pub variables: Vec<String>,
    /// The CoolProp fluid for every state in this block (`Water`, `R134a`), as
    /// written; `None` when the block declared no `FLUID = …` attribute.
    pub fluid: Option<String>,
}

/// A `LINEARIZE name(block = blk, a = A, …) … END` block.
///
/// Port of `ast/LinearizeSystem.java`. It names a transient network (a
/// `DYNAMIC` block, by [`dynamic_name`](LinearizeSystem::dynamic_name)) plus the
/// exogenous inputs and observed outputs; the solver linearizes that block about
/// its operating point into `A`/`B`/`C`/`D` and **injects the matrix entries as
/// equations**, which is why this record does not live in
/// [`DeclarativeBlocks`] — unlike a plot or a parametric table it does reach the
/// equation system.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearizeSystem {
    /// Block name, lowercased (`AstBuilder.buildLinearizeDef` lowercases it,
    /// unlike the `DYNAMIC` name it points at).
    pub name: String,
    /// The `DYNAMIC` block being linearized, lowercased.
    pub dynamic_name: String,
    /// Result matrix variable names, in the case the header wrote them;
    /// defaulting to the upper-case `A`/`B`/`C`/`D`.
    pub a_name: String,
    pub b_name: String,
    pub c_name: String,
    pub d_name: String,
    /// Exogenous inputs, dotted display names kept verbatim but lowercased.
    pub inputs: Vec<String>,
    /// Observed outputs, same spelling rule.
    pub outputs: Vec<String>,
    /// Original block text, for diagnostics.
    pub source_text: String,
}

/// Every declarative block one document contains.
///
/// The three lists mirror the three fields `AstBuilder.ParseResult` carries
/// (`parametricTables`, `plots`, `stateTables`); like the Java they are plain
/// append-ordered lists with no name-collision handling — two `PARAMETRIC`
/// blocks of one name both survive, and the frontend shows two tabs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeclarativeBlocks {
    /// `PARAMETRIC … END` run-tables, in declaration order.
    pub parametric_tables: Vec<ParametricTable>,
    /// `PLOT '…' … END` graphs, in declaration order.
    pub plots: Vec<PlotDef>,
    /// `STATE TABLE … END` circuits, in declaration order.
    pub state_tables: Vec<StateTableDef>,
}

impl DeclarativeBlocks {
    pub fn is_empty(&self) -> bool {
        self.parametric_tables.is_empty() && self.plots.is_empty() && self.state_tables.is_empty()
    }

    /// The **first** parametric table with this name, matched
    /// case-insensitively (the name is stored as written).
    pub fn parametric_table(&self, name: &str) -> Option<&ParametricTable> {
        let wanted = name.to_ascii_lowercase();
        self.parametric_tables
            .iter()
            .find(|t| t.name.to_ascii_lowercase() == wanted)
    }

    /// The **first** state table with this name, matched case-insensitively.
    pub fn state_table(&self, name: &str) -> Option<&StateTableDef> {
        let wanted = name.to_ascii_lowercase();
        self.state_tables
            .iter()
            .find(|t| t.name.to_ascii_lowercase() == wanted)
    }

    /// The declared fluid of the state table that lists this (lowercase)
    /// variable, if any. `ComponentLibrary`'s rule — "a model of an R134a
    /// system that forgets `fluid$` should error, not quietly run as water" —
    /// is what makes this lookup worth having: a state point belongs to exactly
    /// one circuit, and the first block that claims it wins.
    pub fn fluid_of(&self, lowercase_variable: &str) -> Option<&str> {
        self.state_tables
            .iter()
            .find(|t| t.variables.iter().any(|v| v == lowercase_variable))
            .and_then(|t| t.fluid.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> ParametricTable {
        ParametricTable {
            name: "Sweep".into(),
            vars: vec!["T_in".into(), "mdot".into(), "Q".into()],
            rows: vec![
                vec![Some(-50.0), Some(0.1), None],
                vec![Some(-49.0), Some(0.2), None],
            ],
        }
    }

    #[test]
    fn a_column_is_found_case_insensitively_and_keeps_its_gaps() {
        let t = table();
        assert_eq!(t.run_count(), 2);
        assert_eq!(t.column("t_in"), Some(vec![Some(-50.0), Some(-49.0)]));
        assert_eq!(t.column("MDOT"), Some(vec![Some(0.1), Some(0.2)]));
        // A declared output column has no cells at all.
        assert_eq!(t.column("q"), Some(vec![None, None]));
        assert_eq!(t.column("nope"), None);
    }

    #[test]
    fn plot_attributes_keep_insertion_order_and_replace_in_place() {
        let mut attrs = PlotAttributes::new();
        attrs.put("kind".into(), vec!["xy".into()]);
        attrs.put("y".into(), vec!["a".into(), "b".into()]);
        assert_eq!(
            attrs.put("kind".into(), vec!["bode".into()]),
            Some(vec!["xy".into()])
        );
        assert_eq!(
            attrs.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            vec!["kind", "y"]
        );
        assert_eq!(attrs.single("kind"), Some("bode"));
        // A multi-valued key has no single value.
        assert_eq!(attrs.single("y"), None);
        assert_eq!(attrs.get("y").map(<[String]>::len), Some(2));
        assert_eq!(attrs.len(), 2);
        assert!(!attrs.is_empty());
    }

    #[test]
    fn state_tables_answer_which_fluid_owns_a_state_point() {
        let blocks = DeclarativeBlocks {
            parametric_tables: vec![table()],
            plots: Vec::new(),
            state_tables: vec![
                StateTableDef {
                    name: "WaterLoop".into(),
                    variables: vec!["pw_1".into(), "tw_1".into()],
                    fluid: Some("Water".into()),
                },
                StateTableDef {
                    name: "RefrigerantLoop".into(),
                    variables: vec!["pref_1".into()],
                    fluid: None,
                },
            ],
        };
        assert!(!blocks.is_empty());
        assert_eq!(blocks.fluid_of("tw_1"), Some("Water"));
        // Declared, but the block named no fluid.
        assert_eq!(blocks.fluid_of("pref_1"), None);
        assert_eq!(blocks.fluid_of("unlisted"), None);
        assert_eq!(
            blocks.parametric_table("sweep").map(|t| t.vars.len()),
            Some(3)
        );
        assert_eq!(
            blocks.state_table("waterloop").map(|t| t.variables.len()),
            Some(2)
        );
        assert!(blocks.state_table("nope").is_none());
        assert!(DeclarativeBlocks::default().is_empty());
    }
}
