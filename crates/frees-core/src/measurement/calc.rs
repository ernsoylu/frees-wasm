//! Calculated signals: one frees formula evaluated at every raster point.
//!
//! Port of `TimeSeriesEvaluator.java` (374 lines).
//!
//! The formula language is **the frees expression language**, not a bespoke
//! calc dialect. That is the whole point: `enthalpy(R134a, T=t_evap, P=p_rail)`
//! over a measured channel is a real property call through the same
//! [`crate::props`] backend a document uses, which is what conventional
//! measurement tools — whose calc engines are C-like arithmetic — cannot do.
//! Keeping the language shared is also why this file compiles to
//! [`crate::eval`] rather than reimplementing a function library.
//!
//! # Why the formula is compiled
//!
//! A raster is routinely 10⁵–10⁶ points, and the formula is re-evaluated at
//! every one of them. Walking the [`Expr`] per point would do a hash lookup per
//! variable read; the Java's design contract instead compiles the AST **once**
//! into a tree of primitives whose variable read is an array index into a
//! reused slot buffer. This port keeps that contract, with two Rust-specific
//! choices:
//!
//! * The compiled tree is an **enum**, not `Box<dyn Fn(&[f64]) -> f64>`. Two
//!   things the Java lambdas got for free are awkward behind `Fn`: the reused
//!   scratch scope (the Java closes over one mutable `HashMap`, which under
//!   `Fn` would need `Rc<RefCell<…>>`) and error propagation (the Java throws;
//!   a closure returning `f64` cannot carry a [`crate::diag::FreesError`]
//!   without boxing one into every leaf). An enum takes both as ordinary
//!   parameters — `eval(&self, slots, scratch) -> Result<f64>` — and still has
//!   no per-point allocation and no per-point map lookup, which is what the
//!   contract is actually about.
//! * The arithmetic operator rides *inside* the node rather than selecting a
//!   specialised closure. A match on a `Copy` two-bit discriminant is a jump
//!   table; the Java's per-operator lambda buys nothing here.
//!
//! Only [`Expr::Call`] falls back to the full evaluator, through one scratch
//! scope whose entries are overwritten in place per point. That boxing is
//! dwarfed by the property-table lookup it exists to reach.
//!
//! # Arithmetic here is IEEE, unlike the document evaluator
//!
//! `1/0` in a *document* is [`crate::eval`]'s "division by zero" error, because
//! a residual that silently became `inf` would poison a Newton block. In a
//! *calculated signal* it is `inf` — as in the Java, whose compiled `/` is a
//! bare `l / r`. Measured data has zeros in it (a stopped engine, a closed
//! valve), and a 500 000-point channel must not fail wholesale because one
//! sample divided by zero; the sample goes non-finite and the chart draws a
//! gap. The distinction is deliberate on both sides and is reproduced here.
//! Inside a [`Expr::Call`] argument the document semantics apply again, because
//! the whole subtree is handed to [`crate::eval::eval`] — the Java splits the
//! same way, for the same reason.
//!
//! # Time operators
//!
//! `delta(x)`, `integral(x)`, `movavg(x, w)` and `delay(x, tau)` are not
//! functions of a value; they are functions of a *series*. They take an input
//! variable and are replaced, in a pre-pass, by a synthetic input computed once
//! over the whole raster, so per-point evaluation stays pure and order-free.

use std::collections::{BTreeMap, HashMap};

use crate::ast::{BinOp, CmpOp, Expr, LogicOp};
use crate::diag::FreesError;
use crate::eval::Scope;
use crate::measurement::series::SampledSeries;
use crate::measurement::{MeasurementError, Result};
use crate::parser::{parse_bool_expr, Cursor};

/// Deepest formula tree the rewrite and the compiler will walk.
///
/// Both recurse over the AST, as does the compiled tree's own evaluation and
/// its `Drop`. A stack overflow is an abort, not a catchable error, so the
/// depth has to be refused before the walk starts. The parser's own budget is
/// the natural ceiling: nothing [`parse_formula`] can build exceeds it, so this
/// guard only ever fires on an [`Expr`] assembled by some other route, and it
/// fires as a typed error instead of killing the tab.
const MAX_FORMULA_DEPTH: u32 = crate::parser::expr::MAX_EXPR_DEPTH;

/// Nodes a formula may contain.
///
/// [`MAX_FORMULA_DEPTH`] bounds how *tall* a formula is. Nothing bounded how
/// **wide** it is — and every cost in this file is `nodes × something`, so a
/// formula that is shallow and enormous was unbounded in three directions at
/// once. `(A + A)` doubled *k* times is a tree of depth *k* with 2^k leaves, so
/// the depth budget is no constraint at all on the node count. Measured on this
/// port, in release:
///
/// * **51 s** to evaluate a 24 KB call-free formula (4096 terms) over the wasm
///   boundary's million-point raster, and about fourteen minutes for a megabyte
///   of formula. The worker is wedged and nothing can cancel it.
/// * **6.2 s** to *compile* a 90 KB formula over a **four-point** raster —
///   below every byte-counting cap there could be, because it is not a memory
///   problem. Every [`Expr::Call`] node built and sorted its own copy of the
///   whole slot table, so the cost was `calls × slots` and both factors grow
///   with the formula. That one is fixed outright rather than merely bounded
///   (see [`evaluate`]), but the node count is what made it reachable.
/// * **781 MB** of synthetic columns from a 14 KB formula of 1024 `movavg`
///   calls — 52 GB from a megabyte, which on wasm32 is an allocator trap, and
///   a trap is an abort rather than a diagnostic. That one needs
///   [`MAX_SYNTHETIC_SAMPLES`] as well, because it is a *product* with a raster
///   length this module does not choose.
///
/// 1024 is comfortably above anything the depth budget admits in the tall
/// direction: measured, the longest chains the parser will hand out are 249
/// terms bare and 245 inside a call, which is ~500 nodes. A calculated signal a
/// person writes is under a hundred. At this ceiling the worst case left is
/// ~5 s for a crafted call-free formula over a million-point raster — the same
/// order as the 128-input case the boundary already documents.
const MAX_FORMULA_NODES: usize = 1024;

/// Samples the time-operator rewrite may materialise for one formula.
///
/// Each `delta`/`integral`/`movavg`/`delay` allocates a whole extra column, one
/// `f64` per raster point, and they are all held at once because the formula
/// may read any of them at any point. [`MAX_FORMULA_NODES`] bounds *how many*;
/// this bounds the **product** with the raster the caller chose, which is the
/// number that is actually in bytes — the same "bound stated in the wrong unit"
/// shape as the `MAX_RECORDS`/`cn_cycle_count` and `MAX_BLOCKS` defects in
/// `mdf4.rs`.
///
/// 16 777 216 samples is 128 MiB, and it is deliberately `mdf4::MAX_RECORDS`
/// again: that is the number the reader already promises to survive holding, so
/// there is one ceiling to reason about rather than three. At the boundary's own
/// 100 000-point cap for a call-bearing formula it admits 167 time operators; at
/// a million points, 16. Both are far past any real formula, and a formula is
/// the only thing this bounds — the input columns have their own ceiling in
/// [`MAX_INPUT_COLUMN_SAMPLES`], because the boundary's cap on them is a
/// *count* and does not bound their bytes either.
const MAX_SYNTHETIC_SAMPLES: u64 = 16_777_216;

/// Samples the **input** columns may materialise for one evaluation, plus the
/// output: a second 128 MiB ceiling, alongside [`MAX_SYNTHETIC_SAMPLES`].
///
/// Step 1 below samples every bound input onto the raster and holds every
/// column at once, because the formula may read any of them at any point. So
/// the input side costs `raster × inputs × 8` bytes — a *product*, and the two
/// factors are set independently and cheaply by one request. The caller's own
/// caps do not bound it: the wasm boundary's `MAX_INPUTS` is a count (128) and
/// its `MAX_INPUT_SAMPLES` counts the *source* samples, so 128 one-point inline
/// series satisfies both while asking for 128 full-length columns.
///
/// Measured with a counting global allocator, before this ceiling existed: 8
/// inputs on a million-point raster peaked at 72 MB, 32 at 264 MB, 128 at
/// **1 032 MB**. Through `measurement_calc` the same shape — 128 one-point
/// inline inputs and `{"mode":"fixed","dt":1}` over their span — is a
/// **5 604-byte** request body and a **1 044 MB** peak, a 186 000×
/// amplification. On wasm32 that is past what a tab will grow linear memory to,
/// and the allocation site is `sample_on`'s `collect()`: under `panic = "abort"`
/// the failure is the tab, not a diagnostic. Afterwards the same call peaks at
/// 132 MB and answers with a named refusal.
///
/// Same number and same reasoning as [`MAX_SYNTHETIC_SAMPLES`]: it is
/// `mdf4::MAX_RECORDS`, what the reader already promises to survive holding.
/// Kept as a separate total so each refusal can name which half of the request
/// to shrink — and because the two halves genuinely have different owners, the
/// input list and the formula. It leaves the case merging exists for alone: ten
/// channels on one 100 kHz master for ten seconds merge to a million-point
/// raster and eleven columns, which is 11 M against 16.7 M.
const MAX_INPUT_COLUMN_SAMPLES: u64 = 16_777_216;

/// The four series-valued operators, which the pre-pass rewrites away.
const TIME_OPS: [&str; 4] = ["delta", "integral", "movavg", "delay"];

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse one frees expression — a calculated-signal formula.
///
/// The entry point is `boolExpr`, not `expr`, and that is load-bearing: plain
/// arithmetic falls through it unchanged, while a top-level condition
/// (`speed > 25 AND gear = 3`) also parses. Those boolean formulas are the
/// channels the Event List consumes, so refusing them here would delete a
/// feature.
///
/// Trailing input is an error rather than a silent truncation: `x + 1 )` means
/// the user mistyped something, and evaluating the prefix would hand them a
/// column of numbers for a formula they did not write.
pub fn parse_formula(source: &str) -> Result<Expr> {
    let tokens = crate::lexer::tokenize(source).map_err(|e| formula_error(source, &e))?;
    let mut cursor = Cursor::new(&tokens, source);
    let expr = parse_bool_expr(&mut cursor).map_err(|e| formula_error(source, &e))?;
    if !cursor.is_eof() {
        let (_, column) = cursor.span().line_col(source);
        return Err(MeasurementError::Formula(format!(
            "Formula error: column {column}: unexpected {} after the formula",
            cursor.peek().describe()
        )));
    }
    // Refused here as well as in `evaluate`, so a formula that cannot be
    // evaluated never reaches the caller's own raster construction either.
    if !within_node_budget(&expr) {
        return Err(too_wide());
    }
    Ok(expr)
}

/// Is `e` inside [`MAX_FORMULA_NODES`]?
///
/// Iterative, for the same reason [`contains_call`] is: this walk exists to
/// defend against an oversized tree, so it must not be one more unbounded
/// recursion itself. Unlike `contains_call` it descends *every* node kind,
/// including the three the compiler goes on to refuse — a subtree hidden in an
/// array literal is still nodes the rewrite clones and `Drop` walks.
///
/// The stack is bounded as well as the count: every entry on it is a distinct
/// node, so a stack past the budget already proves the tree is.
fn within_node_budget(e: &Expr) -> bool {
    let mut budget = MAX_FORMULA_NODES;
    let mut stack: Vec<&Expr> = vec![e];
    while let Some(node) = stack.pop() {
        if budget == 0 || stack.len() > MAX_FORMULA_NODES {
            return false;
        }
        budget -= 1;
        match node {
            Expr::BinOp { left, right, .. }
            | Expr::Compare { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
            } => {
                stack.push(left.as_ref());
                stack.push(right.as_ref());
            }
            Expr::Neg(inner) | Expr::Not(inner) => stack.push(inner.as_ref()),
            Expr::Call { args: list, .. }
            | Expr::ArrayAccess { indices: list, .. }
            | Expr::ArrayLiteral(list) => stack.extend(list.iter()),
            Expr::Num { .. } | Expr::Str(_) | Expr::Var(_) => {}
        }
    }
    true
}

/// Render a parse failure the way the Java did — `Formula error:` first, so the
/// frontend can show it verbatim, then the column, so the editor can point at
/// it. A formula is a single line, which is why the column alone is enough.
fn formula_error(source: &str, error: &FreesError) -> MeasurementError {
    let message = match error {
        FreesError::Parse { message, .. } => message.clone(),
        other => other.to_string(),
    };
    match error.span() {
        Some(span) => {
            let (_, column) = span.line_col(source);
            MeasurementError::Formula(format!("Formula error: column {column}: {message}"))
        }
        None => MeasurementError::Formula(format!("Formula error: {message}")),
    }
}

/// True when the formula contains any function call.
///
/// Upstream this picks the raster cap: a property call per sample is orders of
/// magnitude slower than an add, so call-bearing formulas get a smaller point
/// budget.
///
/// Iterative on purpose. The recursive form would be one more unbounded AST
/// walk to defend, and this one is `pub` with no way to report a refusal —
/// there is no `Result` to put it in. A worklist has no depth to overflow.
///
/// Array literals, array elements and index ranges are *not* descended into,
/// matching the Java's `default -> false`. Nothing is lost: [`compile`] refuses
/// those constructs outright, so a call hidden inside one is never evaluated.
pub fn contains_call(e: &Expr) -> bool {
    let mut stack = vec![e];
    while let Some(node) = stack.pop() {
        match node {
            Expr::Call { .. } => return true,
            Expr::BinOp { left, right, .. }
            | Expr::Compare { left, right, .. }
            | Expr::Logical { left, right, .. } => {
                stack.push(left.as_ref());
                stack.push(right.as_ref());
            }
            Expr::Neg(inner) | Expr::Not(inner) => stack.push(inner.as_ref()),
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate `formula` at every point of `raster`.
///
/// Inputs are addressed by their formula variable name, case-insensitively (a
/// frees invariant — `TQ` and `tq` are the same variable). The reserved name
/// `time` resolves to the raster point itself unless an input claims it.
///
/// The order of the stages below is the Java's and is load-bearing: every input
/// is sampled onto the raster *before* the time-operator rewrite, because the
/// rewrite computes its synthetic series from those samples; slots are assigned
/// *after* the rewrite, because the rewrite adds series.
pub fn evaluate(
    formula: &Expr,
    raster: &[f64],
    inputs: &BTreeMap<String, SampledSeries>,
) -> Result<Vec<f64>> {
    // 0. Refuse an oversized tree before anything is allocated. `evaluate` is
    //    `pub` and every stage below costs `nodes × something`, so the check
    //    cannot live only in `parse_formula`.
    if !within_node_budget(formula) {
        return Err(too_wide());
    }

    // 1. Sample every input onto the raster once, keyed by lowercased name.
    let mut columns = Columns::default();
    // The output is a raster-length column too, and it is the one allocation
    // that exists even with no inputs at all — so it is claimed first, before
    // the `Vec::with_capacity(raster.len())` in step 5 can abort on a raster no
    // caller should have built.
    columns.claim_input(raster.len(), "the output column")?;
    let mut spellings: HashMap<String, &str> = HashMap::new();
    for (name, series) in inputs {
        let key = name.to_ascii_lowercase();
        // The Java's `LinkedHashMap.put` would keep whichever of `Speed` and
        // `speed` it happened to visit last and drop the other's samples with
        // no signal. Names are case-insensitive in frees, so those two *are*
        // one variable and the binding is genuinely ambiguous — a caller that
        // built such a map has a bug upstream, and a silent wrong column is
        // the worst possible way to tell them.
        if let Some(first) = spellings.insert(key.clone(), name.as_str()) {
            return Err(MeasurementError::Formula(format!(
                "Inputs \"{first}\" and \"{name}\" name the same variable — frees variable \
                 names are case-insensitive. Bind only one of them."
            )));
        }
        // Claimed *before* `sample_on`, which is the allocation being bounded.
        columns.claim_input(raster.len(), &format!("input \"{name}\""))?;
        columns.push(key, series.sample_on(raster));
    }

    // 2. Rewrite the series-valued operators into synthetic inputs.
    let rewritten = rewrite_time_ops(formula, raster, inputs, &mut columns, 0)?;

    // 3. Slot assignment: `time` is slot 0, then one slot per sampled series.
    //    An input actually *named* `time` overwrites the reserved slot, exactly
    //    as the Java's second `put` does; slot 0 then goes unread.
    let mut slots: HashMap<String, usize> = HashMap::with_capacity(columns.len() + 1);
    slots.insert("time".to_string(), 0);
    for (name, index) in &columns.index {
        slots.insert(name.clone(), index + 1);
    }

    // 4. Compile once.
    let compiled = compile(&rewritten, &slots, 0)?;

    // 4a. The scratch scope every `Call` node hands to the general evaluator,
    //     flattened to `(name, slot)` **once for the whole formula**.
    //
    //     It used to be built, cloned and sorted per `Call` node, and every one
    //     of them then rewrote the whole table on every raster point — so the
    //     cost was `calls × slots`, in allocation at compile time and in hash
    //     writes at run time, with both factors growing together as the formula
    //     grows. Measured on a *four-point* raster, which is to say with the
    //     per-point term all but switched off: 1024 leaves 0.33 s, 4096 leaves
    //     **6.2 s**. Quadratic, and invisible to any cap counting samples or
    //     bytes, because it is neither.
    //
    //     Hoisting it is exactly equivalent, and the signature is what proves
    //     it: [`crate::eval::eval`] takes `&Scope`, so a call node cannot change
    //     what a later one would read, and every node wrote the same values out
    //     of the same slot buffer anyway. `Compiled::eval` therefore takes
    //     `&Scope` now too — the borrow checker states the invariant that makes
    //     the hoist sound.
    //
    //     Skipped when the formula has no calls, which is the common case and
    //     the one where the refresh would be pure waste. `contains_call` is the
    //     right test because `compile` only ever builds a `Compiled::Call` from
    //     a node `contains_call` descends to.
    let bindings: Vec<(String, usize)> = if contains_call(&rewritten) {
        let mut flat: Vec<(String, usize)> = slots
            .iter()
            .map(|(name, slot)| (name.clone(), *slot))
            .collect();
        // The map's iteration order is not stable across runs; binding in a
        // fixed order keeps a failing formula's diagnostics reproducible.
        flat.sort_unstable();
        flat
    } else {
        Vec::new()
    };

    // 5. Evaluate at every raster point, over one reused slot buffer and one
    //    reused scratch scope.
    let mut slot_values = vec![0.0; columns.len() + 1];
    let mut scratch: Scope = Scope::with_capacity_and_hasher(bindings.len(), Default::default());
    let mut out = Vec::with_capacity(raster.len());
    for (i, &t) in raster.iter().enumerate() {
        slot_values[0] = t;
        for (c, column) in columns.data.iter().enumerate() {
            slot_values[c + 1] = column[i];
        }
        // Every slot is bound, not just the ones the call subtrees read: a
        // property call's arguments are *named* (`T=`, `P=`), so a variable
        // analysis would have to understand the `prop$…` encoding to be right.
        // Overwriting in place never allocates after the first point.
        for (name, slot) in &bindings {
            let value = slot_values[*slot];
            match scratch.get_mut(name.as_str()) {
                Some(existing) => *existing = value,
                None => {
                    scratch.insert(name.clone(), value);
                }
            }
        }
        match compiled.eval(&slot_values, &scratch) {
            Ok(value) => out.push(value),
            // 6. Report *where* it failed. On a 500 000-point raster "it
            //    failed" is not actionable; the timestamp is the only thing
            //    that lets the user look at the sample that broke it.
            Err(e) => {
                return Err(MeasurementError::Formula(format!(
                    "Formula failed at t = {t}: {e}"
                )))
            }
        }
    }
    Ok(out)
}

/// The sampled series backing the slot buffer, in slot order.
///
/// Column `i` is slot `i + 1`; slot 0 is the raster time. Insertion order is
/// preserved because the time-operator rewrite appends to it *and* names its
/// synthetics after the current length.
#[derive(Default)]
struct Columns {
    /// Lowercased series name → index into [`Columns::data`].
    index: HashMap<String, usize>,
    data: Vec<Vec<f64>>,
    /// Samples the time-operator rewrite has claimed so far, against
    /// [`MAX_SYNTHETIC_SAMPLES`]. Separate from [`Columns::input`] so that a
    /// refusal can name which half of the request is the one to shrink: the
    /// formula and the input list have different owners.
    synthetic: u64,
    /// Samples the input columns and the output have claimed, against
    /// [`MAX_INPUT_COLUMN_SAMPLES`]. Counted rather than measured from `data`,
    /// because the output column is in the budget without being in the map.
    input: u64,
}

impl Columns {
    fn len(&self) -> usize {
        self.data.len()
    }

    /// Reserve one more raster-length synthetic column, or refuse **before** it
    /// is allocated. See [`MAX_SYNTHETIC_SAMPLES`].
    ///
    /// `what` names the operator that tipped the total over, because on a
    /// formula with a hundred of them "too much memory" alone leaves the user
    /// guessing which half of the product to shrink.
    fn claim_synthetic(&mut self, points: usize, what: &str) -> Result<()> {
        self.synthetic = self.synthetic.saturating_add(points as u64);
        if self.synthetic > MAX_SYNTHETIC_SAMPLES {
            return Err(MeasurementError::Formula(format!(
                "This formula's time operators need {} samples of working space once {what}() is \
                 included ({} bytes), above the {MAX_SYNTHETIC_SAMPLES}-sample limit a browser tab \
                 can hold at once. Each delta/integral/movavg/delay costs one full-length column, \
                 so use fewer of them or a coarser sample interval.",
                self.synthetic,
                self.synthetic.saturating_mul(8)
            )));
        }
        Ok(())
    }

    /// Reserve one more raster-length input column, or refuse **before** it is
    /// allocated. See [`MAX_INPUT_COLUMN_SAMPLES`].
    fn claim_input(&mut self, points: usize, what: &str) -> Result<()> {
        self.input = self.input.saturating_add(points as u64);
        if self.input > MAX_INPUT_COLUMN_SAMPLES {
            return Err(MeasurementError::Formula(format!(
                "Evaluating this signal needs {} samples of working space once {what} is \
                 included ({} bytes), above the {MAX_INPUT_COLUMN_SAMPLES}-sample limit a browser \
                 tab can hold at once. Every bound input costs one full-length column whether the \
                 formula reads it or not, so bind fewer signals or use a coarser sample interval.",
                self.input,
                self.input.saturating_mul(8)
            )));
        }
        Ok(())
    }

    fn push(&mut self, name: String, values: Vec<f64>) {
        self.index.insert(name, self.data.len());
        self.data.push(values);
    }

    fn get(&self, name: &str) -> Option<&[f64]> {
        self.index.get(name).map(|i| self.data[*i].as_slice())
    }
}

// ---------------------------------------------------------------------------
// Time-operator rewrite
// ---------------------------------------------------------------------------

fn rewrite_time_ops(
    e: &Expr,
    raster: &[f64],
    inputs: &BTreeMap<String, SampledSeries>,
    columns: &mut Columns,
    depth: u32,
) -> Result<Expr> {
    if depth > MAX_FORMULA_DEPTH {
        return Err(too_deep());
    }
    let next = depth + 1;
    match e {
        Expr::Call { function, args } if is_time_op(function) => {
            // Deliberately *not* rewritten depth-first: the first argument must
            // be an input variable, so `delta(delta(x))` is an error rather
            // than a second-difference. The Java is the same, and the message
            // is the one the user needs — the operator is series-valued, so
            // there is nothing sensible to hand it but a series.
            let Some(Expr::Var(input_name)) = args.first() else {
                return Err(MeasurementError::Formula(format!(
                    "{function}() takes an input signal as its first argument."
                )));
            };
            if columns.get(input_name).is_none() {
                return Err(MeasurementError::Formula(format!(
                    "{function}(): unknown input \"{input_name}\"."
                )));
            }
            // Claimed before the series is built — the whole point is not to
            // allocate it. Every synthetic column is exactly raster-length.
            columns.claim_synthetic(raster.len(), function)?;
            let base = columns.get(input_name).expect("checked just above");
            let mut param = 0.0;
            if let Some(second) = args.get(1) {
                let Expr::Num { value, .. } = second else {
                    return Err(MeasurementError::Formula(format!(
                        "{function}(): the second argument must be a numeric constant."
                    )));
                };
                param = *value;
            }
            let synthetic = format!("__{function}_{input_name}_{}", columns.len());
            let series = match function.as_str() {
                "delta" => delta(base),
                "integral" => integral(raster, base),
                "movavg" => movavg(raster, base, require_positive(function, param)?),
                // `delay` alone reads the *original* series rather than the
                // raster sampling: shifting an already-rastered column would
                // quantise the delay to the raster step, and the source's own
                // interpolation mode is exactly the question `at()` answers.
                _ => delay(raster, find_input(inputs, input_name), param),
            };
            columns.push(synthetic.clone(), series);
            Ok(Expr::Var(synthetic))
        }
        Expr::Call { function, args } => {
            let mut rewritten = Vec::with_capacity(args.len());
            for a in args {
                rewritten.push(rewrite_time_ops(a, raster, inputs, columns, next)?);
            }
            Ok(Expr::Call {
                function: function.clone(),
                args: rewritten,
            })
        }
        Expr::BinOp { op, left, right } => Ok(Expr::BinOp {
            op: *op,
            left: Box::new(rewrite_time_ops(left, raster, inputs, columns, next)?),
            right: Box::new(rewrite_time_ops(right, raster, inputs, columns, next)?),
        }),
        Expr::Compare { op, left, right } => Ok(Expr::Compare {
            op: *op,
            left: Box::new(rewrite_time_ops(left, raster, inputs, columns, next)?),
            right: Box::new(rewrite_time_ops(right, raster, inputs, columns, next)?),
        }),
        Expr::Logical { op, left, right } => Ok(Expr::Logical {
            op: *op,
            left: Box::new(rewrite_time_ops(left, raster, inputs, columns, next)?),
            right: Box::new(rewrite_time_ops(right, raster, inputs, columns, next)?),
        }),
        Expr::Neg(inner) => Ok(Expr::Neg(Box::new(rewrite_time_ops(
            inner, raster, inputs, columns, next,
        )?))),
        Expr::Not(inner) => Ok(Expr::Not(Box::new(rewrite_time_ops(
            inner, raster, inputs, columns, next,
        )?))),
        other => Ok(other.clone()),
    }
}

fn is_time_op(function: &str) -> bool {
    TIME_OPS.contains(&function)
}

/// The window/delay guard.
///
/// Written `!(x > 0.0)` rather than `x <= 0.0` so that a `NaN` window takes the
/// reject branch — `movavg(x, 0/0)` must refuse, not run with a comparison that
/// is false forever. `neg_cmp_op_on_partial_ord` exists to catch the accidental
/// form; here the `NaN` behaviour is the point, as it is in [`crate::eval`],
/// which allows the lint for the same reason.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn require_positive(function: &str, x: f64) -> Result<f64> {
    if !(x > 0.0) {
        return Err(MeasurementError::Formula(format!(
            "{function}(): the window/delay must be > 0 seconds."
        )));
    }
    Ok(x)
}

/// The input series whose name matches `lower` case-insensitively.
///
/// `inputs` is keyed by the caller's spelling; the formula's variable names are
/// lowercased by the parser. `None` when the name belongs to a synthetic series
/// rather than a real input, which is what makes `delay` of one yield `NaN`
/// instead of failing — the Java's `inputs.get(...)` returns null there.
fn find_input<'a>(
    inputs: &'a BTreeMap<String, SampledSeries>,
    lower: &str,
) -> Option<&'a SampledSeries> {
    inputs
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(lower))
        .map(|(_, series)| series)
}

/// Sample-to-sample difference on the raster; the first point is 0.
///
/// Note this is a *difference*, not a derivative — it is raster-step dependent,
/// which is why the Java's own test divides by `dt` to recover a rate.
fn delta(v: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    for i in 1..v.len() {
        out[i] = v[i] - v[i - 1];
    }
    out
}

/// Cumulative trapezoid. A segment touching a `NaN` contributes nothing, so a
/// gap holds the accumulator flat instead of poisoning every later point.
fn integral(t: &[f64], v: &[f64]) -> Vec<f64> {
    debug_assert_eq!(t.len(), v.len(), "columns are sampled onto the raster");
    let mut out = vec![0.0; v.len()];
    let mut acc = 0.0;
    for i in 1..v.len() {
        let (a, b) = (v[i - 1], v[i]);
        if !a.is_nan() && !b.is_nan() {
            acc += 0.5 * (a + b) * (t[i] - t[i - 1]);
        }
        out[i] = acc;
    }
    out
}

/// Trailing time-window mean over `[t - window, t]`.
///
/// `NaN` samples are skipped rather than propagated, and an all-`NaN` window is
/// `NaN` — a mean of nothing is not zero.
///
/// # The running sum is repaired, unlike the Java's
///
/// The window slides by adding the entering sample and **subtracting** the
/// leaving one, which is O(n) and is the Java's algorithm. It is also the one
/// place in this module where a single bad sample rewrites a whole channel:
/// once the accumulator has been ±∞ it never recovers, because `∞ - x` is `∞`
/// and `∞ - ∞` is `NaN`. Two shapes reach it from values a file can hold, and
/// both were measured against this port before the repair below:
///
/// * one `+∞` sample, a 2 s window, then ordinary numbers →
///   `[∞, ∞, NaN, NaN, NaN, …]`. Every point after the infinity has *left* the
///   window reports a gap, over data that is perfectly good. This module's
///   headline rule is that a gap is never bridged; manufacturing gaps out of one
///   is the same rule broken in the other direction, and on a 500 000-point
///   channel it is the whole rest of the recording.
/// * two adjacent `1e308` samples — **finite**, an ordinary `f64` a float
///   channel can carry — overflow the sum to `+∞`, and subtracting them back out
///   leaves it there: `[1e308, ∞, ∞, ∞, …]` forever.
///
/// So the window's ±∞ **population** is tracked alongside its `count`, by the
/// same add-on-entry/subtract-on-exit bookkeeping, and it decides the answer
/// outright while it is non-zero: a window holding a `+∞` has mean `+∞` whatever
/// the finite samples do, one holding both signs has mean `NaN`, and neither
/// answer needs to read the accumulator at all. When the population is back to
/// zero and the accumulator is *still* not finite, it is the poison — and the
/// sum is recomputed from the window itself, correct by construction.
///
/// Splitting it that way is what makes the repair affordable, and getting it
/// wrong is a live trap: a first cut recomputed on *every* non-finite
/// accumulator, including the points where the offending sample is still inside
/// the window and the recompute is arithmetically guaranteed to come back ±∞
/// again. Those hopeless passes cost `Σ span ≈ W²/2` for a window of `W` raster
/// points, so any budget was spent long before the *useful* repair — the one
/// after the sample leaves — ever came up. Measured on this port: one `+∞`
/// sample at the head of a 200 000-point channel with a 2 s window at 1 kHz left
/// **195 998 of the remaining points `NaN`**, which is the defect this section
/// exists to close, unclosed. Gating on the population removes the hopeless
/// passes entirely, and the ones that are left are provably `O(n)` in total
/// without any budget: two successful repairs are separated by a whole window
/// (an ∞ has to enter and then leave between them), so their spans telescope.
///
/// The budget below therefore now guards only the second shape — an overflow out
/// of *finite* samples, where the recompute really can fail and really would be
/// retried at every point. A channel engineered so that every window overflows
/// exhausts it and then reports the running sum, which is ±∞: the true answer
/// there anyway, and never a fabricated gap. Ordinary data reaches neither
/// branch, so a real recording's numbers are bit-for-bit the Java's.
///
/// **What this does not fix**, because nothing bounded can: the sliding sum
/// still loses small samples to a huge one by plain cancellation.
/// `1e300 + 1 - 1e300` is `0`, so a trailing mean that has just passed a `1e300`
/// sample reports zero where the truth is one — with the accumulator staying
/// perfectly finite, so there is nothing to detect. The Java has it too, and the
/// only cure is to stop subtracting (a two-stack window sum), which changes the
/// summation order and therefore the answer on *every* ordinary channel. Pinned
/// in `tests/measurement_robustness.rs` rather than papered over.
///
/// [`integral`] deliberately keeps the Java's propagation: it is a *cumulative*
/// quantity, so an infinite sample really does make every later value infinite.
/// A trailing mean is the opposite — the offending sample has left.
fn movavg(t: &[f64], v: &[f64], window: f64) -> Vec<f64> {
    debug_assert_eq!(t.len(), v.len(), "columns are sampled onto the raster");
    let mut out = vec![0.0; v.len()];
    let mut start = 0usize;
    let mut sum = 0.0;
    let mut count = 0usize;
    // The window's ±∞ population, maintained by exactly the bookkeeping `count`
    // is: entering samples add, leaving samples subtract, `NaN`s are skipped by
    // both. While either is non-zero the mean is decided by it alone.
    let mut positive = 0usize;
    let mut negative = 0usize;
    // Four passes over the channel, plus a floor so a short series gets a usable
    // budget too. It is only reachable now from a window of *finite* samples
    // whose sum overflows, where a recompute can fail and would otherwise be
    // retried at every point; the ±∞ repairs above it are already O(n) in total.
    let mut repair = v.len().saturating_mul(4).saturating_add(64);
    for i in 0..v.len() {
        if !v[i].is_nan() {
            sum += v[i];
            count += 1;
            if v[i] == f64::INFINITY {
                positive += 1;
            } else if v[i] == f64::NEG_INFINITY {
                negative += 1;
            }
        }
        // `start < i` is a guard the Java does not have. On an ascending raster
        // it never binds (window > 0 makes `t[i] < t[i] - window` false), but a
        // caller that passed a non-monotonic raster would walk `start` past the
        // end — an out-of-bounds abort in wasm, where Java merely threw.
        while start < i && t[start] < t[i] - window {
            if !v[start].is_nan() {
                sum -= v[start];
                count -= 1;
                if v[start] == f64::INFINITY {
                    positive -= 1;
                } else if v[start] == f64::NEG_INFINITY {
                    negative -= 1;
                }
            }
            start += 1;
        }
        out[i] = if count == 0 {
            // A mean of nothing is not zero.
            f64::NAN
        } else if positive > 0 && negative > 0 {
            // `∞ + (−∞)` has no value, and the finite samples cannot rescue it.
            f64::NAN
        } else if positive > 0 {
            f64::INFINITY
        } else if negative > 0 {
            f64::NEG_INFINITY
        } else {
            // Every sample in the window is finite, so a non-finite accumulator
            // is either poison left by an ∞ that has since gone, or a genuine
            // overflow. Both are answered by recomputing the window.
            if !sum.is_finite() {
                let span = i - start + 1;
                if span <= repair {
                    repair -= span;
                    sum = v[start..=i].iter().copied().filter(|x| !x.is_nan()).sum();
                }
            }
            sum / count as f64
        };
    }
    out
}

/// The input's own series evaluated at `t - tau`, through its own interpolation
/// mode. Before the source's first sample there is nothing to hold, so the
/// leading `tau` seconds of a delayed channel are `NaN`.
fn delay(raster: &[f64], source: Option<&SampledSeries>, tau: f64) -> Vec<f64> {
    raster
        .iter()
        .map(|&x| source.map_or(f64::NAN, |s| s.at(x - tau)))
        .collect()
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/// The five arithmetic operators a calculated signal admits.
///
/// A narrower type than [`BinOp`] on purpose: it makes the per-point match
/// total, so the hot path carries no unreachable arm and no panic.
#[derive(Debug, Clone, Copy)]
enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// The formula, resolved against the slot buffer.
enum Compiled {
    Const(f64),
    Slot(usize),
    Neg(Box<Compiled>),
    Arith {
        op: ArithOp,
        left: Box<Compiled>,
        right: Box<Compiled>,
    },
    Compare {
        op: CmpOp,
        left: Box<Compiled>,
        right: Box<Compiled>,
    },
    Logical {
        op: LogicOp,
        left: Box<Compiled>,
        right: Box<Compiled>,
    },
    Not(Box<Compiled>),
    /// A function call, left as an [`Expr`] and handed to the general evaluator
    /// against the scratch scope [`evaluate`] loaded for this raster point.
    Call {
        subtree: Expr,
    },
}

impl Compiled {
    fn eval(&self, slots: &[f64], scratch: &Scope) -> crate::diag::Result<f64> {
        Ok(match self {
            Compiled::Const(v) => *v,
            Compiled::Slot(i) => slots[*i],
            Compiled::Neg(a) => -a.eval(slots, scratch)?,
            Compiled::Arith { op, left, right } => {
                let l = left.eval(slots, scratch)?;
                let r = right.eval(slots, scratch)?;
                match op {
                    ArithOp::Add => l + r,
                    ArithOp::Sub => l - r,
                    ArithOp::Mul => l * r,
                    // Bare IEEE division — see the module docs on why a
                    // calculated signal does not adopt the solver's guard.
                    ArithOp::Div => l / r,
                    ArithOp::Pow => java_pow(l, r),
                }
            }
            Compiled::Compare { op, left, right } => {
                let l = left.eval(slots, scratch)?;
                let r = right.eval(slots, scratch)?;
                let truth = match op {
                    CmpOp::Lt => l < r,
                    CmpOp::Gt => l > r,
                    CmpOp::Le => l <= r,
                    CmpOp::Ge => l >= r,
                    CmpOp::Ne => l != r,
                    CmpOp::Eq => l == r,
                };
                // Boolean channels are `f64` like every other channel: the
                // Event List reads 1/0, and a chart can draw them.
                if truth {
                    1.0
                } else {
                    0.0
                }
            }
            // Short-circuit, because the Java's compiled lambda does:
            // `l.eval(s) != 0.0 && r.eval(s) != 0.0` never touches the right
            // operand once the left has decided. That is not a micro-
            // optimisation, it is what makes a call *guardable* —
            // `p > 0 and enthalpy(R134a, P=p, H=h) > 0` runs the property
            // lookup only on the samples where the state exists, and on a
            // 500 000-point channel a single undefined sample would otherwise
            // fail the whole signal with nothing to plot.
            //
            // This is the one place where the calc path deliberately differs
            // from [`crate::eval`], and the *Java* differs there in the same
            // direction: `ast/Evaluator` really does evaluate both operands
            // before combining them. Two Java sites, two behaviours, each
            // reproduced where it lives.
            Compiled::Logical { op, left, right } => {
                let l = left.eval(slots, scratch)? != 0.0;
                let truth = match op {
                    LogicOp::And => l && right.eval(slots, scratch)? != 0.0,
                    LogicOp::Or => l || right.eval(slots, scratch)? != 0.0,
                };
                if truth {
                    1.0
                } else {
                    0.0
                }
            }
            Compiled::Not(a) => {
                if a.eval(slots, scratch)? == 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            // The scope already holds this point's slot values — see stage 4a
            // of `evaluate`. `&Scope` rather than `&mut` is what makes loading
            // it once per point rather than once per call node sound.
            Compiled::Call { subtree } => crate::eval::eval(subtree, scratch)?,
        })
    }
}

/// `Math.pow`, which is **not** C's `pow` — and the difference is reachable
/// over measured data.
///
/// Java specifies two results IEEE 754 and `libm` do not share:
///
/// * `pow(x, NaN)` is `NaN` for *every* `x`, where C returns 1 when `x == 1`.
///   A `NaN` here is a gap in the exponent channel, so C's answer invents a
///   `1.0` nobody recorded. "A gap is never bridged" is this module's headline
///   rule (`series.rs`), not a parity footnote, and `base ^ gap` was quietly
///   breaking it wherever the base happened to sit at exactly 1.
/// * `pow(±1, ±∞)` is `NaN`, where C returns 1. `±∞` is an ordinary value in a
///   calculated signal — division here is bare IEEE, so a stopped sensor makes
///   one — which puts this within reach too.
///
/// Everything else stays `libm::pow`: `libm` rather than the host intrinsic so
/// a native run and a wasm run agree bit for bit (crate-wide rule), at the
/// cost of the odd last-ULP disagreement with the JVM's `Math.pow` intrinsic.
/// That trade is about *accuracy*; the two rules above are about a different
/// answer, which is why only they are reproduced.
fn java_pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (exponent.is_infinite() && base.abs() == 1.0) {
        return f64::NAN;
    }
    libm::pow(base, exponent)
}

fn compile(e: &Expr, slots: &HashMap<String, usize>, depth: u32) -> Result<Compiled> {
    if depth > MAX_FORMULA_DEPTH {
        return Err(too_deep());
    }
    let next = depth + 1;
    match e {
        Expr::Num { value, .. } => Ok(Compiled::Const(*value)),
        Expr::Var(name) => match slots.get(name) {
            Some(slot) => Ok(Compiled::Slot(*slot)),
            None => Err(MeasurementError::Formula(format!(
                "Unknown variable \"{name}\" — bind it to a signal input."
            ))),
        },
        Expr::Neg(inner) => Ok(Compiled::Neg(Box::new(compile(inner, slots, next)?))),
        Expr::BinOp { op, left, right } => Ok(Compiled::Arith {
            op: arith_op(*op)?,
            left: Box::new(compile(left, slots, next)?),
            right: Box::new(compile(right, slots, next)?),
        }),
        Expr::Compare { op, left, right } => Ok(Compiled::Compare {
            op: *op,
            left: Box::new(compile(left, slots, next)?),
            right: Box::new(compile(right, slots, next)?),
        }),
        Expr::Logical { op, left, right } => Ok(Compiled::Logical {
            op: *op,
            left: Box::new(compile(left, slots, next)?),
            right: Box::new(compile(right, slots, next)?),
        }),
        Expr::Not(inner) => Ok(Compiled::Not(Box::new(compile(inner, slots, next)?))),
        Expr::Call { .. } => Ok(Compiled::Call { subtree: e.clone() }),
        // Everything below has no meaning over a single raster point. Naming
        // the construct matters: "not supported" alone leaves the user
        // rewriting the formula at random.
        Expr::Str(text) => Err(unsupported(&format!("a string literal ('{text}')"))),
        Expr::ArrayLiteral(_) => Err(unsupported("an array literal ([…])")),
        Expr::ArrayAccess { name, .. } => {
            Err(unsupported(&format!("an array element ({name}[…])")))
        }
        Expr::Range { .. } => Err(unsupported("an index range (a:b)")),
    }
}

/// The operator subset a calculated signal admits.
///
/// Left division and the four element-wise forms are matrix operators; a
/// calculated signal is scalar per point, so they are refused rather than
/// silently degraded to their scalar equivalents. The Java refuses them too —
/// its compile switch has no case for them.
fn arith_op(op: BinOp) -> Result<ArithOp> {
    match op {
        BinOp::Add => Ok(ArithOp::Add),
        BinOp::Sub => Ok(ArithOp::Sub),
        BinOp::Mul => Ok(ArithOp::Mul),
        BinOp::Div => Ok(ArithOp::Div),
        BinOp::Pow => Ok(ArithOp::Pow),
        other => Err(MeasurementError::Formula(format!(
            "Unsupported operator in a calculated signal: {}",
            other.as_str()
        ))),
    }
}

fn unsupported(construct: &str) -> MeasurementError {
    MeasurementError::Formula(format!(
        "This construct is not supported in calculated signals: {construct}"
    ))
}

fn too_deep() -> MeasurementError {
    MeasurementError::Formula(
        "Formula error: the formula is too deeply nested to evaluate; split it into \
         several calculated signals."
            .to_string(),
    )
}

fn too_wide() -> MeasurementError {
    MeasurementError::Formula(format!(
        "Formula error: the formula has more than {MAX_FORMULA_NODES} terms to evaluate; split \
         it into several calculated signals."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::series::{Interp, SampledSeries};

    // Every `SampledSeries` in this file is built here, so a change to the
    // constructor is a one-line fix.
    fn series(t: &[f64], v: &[f64], interp: Interp) -> SampledSeries {
        SampledSeries::new(t.to_vec(), v.to_vec(), interp)
    }

    fn ramp(n: usize, dt: f64) -> Vec<f64> {
        (0..n).map(|i| i as f64 * dt).collect()
    }

    fn bind(pairs: &[(&str, SampledSeries)]) -> BTreeMap<String, SampledSeries> {
        pairs
            .iter()
            .map(|(name, s)| ((*name).to_string(), s.clone()))
            .collect()
    }

    fn run(formula: &str, raster: &[f64], inputs: &BTreeMap<String, SampledSeries>) -> Vec<f64> {
        let parsed = parse_formula(formula).unwrap_or_else(|e| panic!("`{formula}`: {e}"));
        evaluate(&parsed, raster, inputs).unwrap_or_else(|e| panic!("`{formula}`: {e}"))
    }

    fn run_err(formula: &str, raster: &[f64], inputs: &BTreeMap<String, SampledSeries>) -> String {
        let parsed = parse_formula(formula).unwrap_or_else(|e| panic!("`{formula}`: {e}"));
        match evaluate(&parsed, raster, inputs) {
            Ok(v) => panic!("`{formula}` should have failed, got {v:?}"),
            Err(e) => e.to_string(),
        }
    }

    fn close(a: f64, b: f64, tol: f64, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
    }

    // -- arithmetic ------------------------------------------------------

    /// Port of `arithmeticFormulaMatchesAnalyticResult`.
    #[test]
    fn arithmetic_formula_matches_the_analytic_result() {
        let raster = ramp(1000, 0.01);
        let tq: Vec<f64> = (0..1000).map(|i| 100.0 + i as f64 * 0.1).collect();
        let w: Vec<f64> = (0..1000).map(|i| 200.0 - i as f64 * 0.05).collect();
        let inputs = bind(&[
            ("tq", series(&raster, &tq, Interp::Linear)),
            ("w", series(&raster, &w, Interp::Linear)),
        ]);
        let out = run("tq * w / 1000", &raster, &inputs);
        for i in (0..1000).step_by(137) {
            close(out[i], tq[i] * w[i] / 1000.0, 1e-12, "tq*w/1000");
        }
    }

    /// Port of `reservedTimeVariableAndBooleanConditionsWork`.
    #[test]
    fn the_reserved_time_variable_and_boolean_conditions_work() {
        let raster = ramp(101, 0.01);
        let x = vec![7.0; 101];
        let inputs = bind(&[("x", series(&raster, &x, Interp::Step))]);

        let out = run("time > 0.5 AND x >= 7", &raster, &inputs);
        assert_eq!(out[50], 0.0, "t = 0.50 is not > 0.5");
        assert_eq!(out[51], 1.0);

        let t2 = run("x * time", &raster, &inputs);
        close(t2[25], 7.0 * 0.25, 1e-12, "x * time");
    }

    /// An input actually named `time` shadows the raster, as in the Java.
    #[test]
    fn an_input_named_time_takes_the_reserved_slot() {
        let raster = ramp(4, 1.0);
        let inputs = bind(&[(
            "time",
            series(&raster, &[10.0, 20.0, 30.0, 40.0], Interp::Step),
        )]);
        assert_eq!(run("time", &raster, &inputs), vec![10.0, 20.0, 30.0, 40.0]);
    }

    /// Port of `comparisonLogicalAndNegationOperatorsEvaluate`.
    #[test]
    fn comparison_logical_and_negation_operators_evaluate() {
        let raster = ramp(5, 1.0);
        let inputs = bind(&[("x", series(&raster, &raster, Interp::Step))]);
        let cases: &[(&str, [f64; 5])] = &[
            ("x < 2", [1.0, 1.0, 0.0, 0.0, 0.0]),
            ("x <= 2", [1.0, 1.0, 1.0, 0.0, 0.0]),
            ("x >= 3", [0.0, 0.0, 0.0, 1.0, 1.0]),
            ("x = 2", [0.0, 0.0, 1.0, 0.0, 0.0]),
            ("x <> 2", [1.0, 1.0, 0.0, 1.0, 1.0]),
            ("x > 1 and x < 3", [0.0, 0.0, 1.0, 0.0, 0.0]),
            ("x < 1 or x > 3", [1.0, 0.0, 0.0, 0.0, 1.0]),
            ("not (x > 0)", [1.0, 0.0, 0.0, 0.0, 0.0]),
            ("-x", [0.0, -1.0, -2.0, -3.0, -4.0]),
            ("x ^ 2", [0.0, 1.0, 4.0, 9.0, 16.0]),
        ];
        for (formula, expected) in cases {
            let out = run(formula, &raster, &inputs);
            assert_eq!(&out[..], &expected[..], "{formula}");
        }
    }

    /// The calc path does *not* inherit the solver's division guard. A zero in
    /// measured data must not fail the whole channel.
    #[test]
    fn division_by_zero_is_infinity_not_an_error() {
        let raster = ramp(3, 1.0);
        let inputs = bind(&[("x", series(&raster, &[0.0, 2.0, 0.0], Interp::Step))]);
        let out = run("1 / x", &raster, &inputs);
        assert!(out[0].is_infinite() && out[0] > 0.0, "{:?}", out);
        assert_eq!(out[1], 0.5);
        // …but inside a call the document semantics apply again, because the
        // whole subtree goes to the general evaluator.
        let message = run_err("abs(1 / x)", &raster, &inputs);
        assert!(message.contains("Formula failed at t = 0"), "{message}");
        assert!(message.contains("division by zero"), "{message}");
    }

    // -- time operators --------------------------------------------------

    /// Port of `deltaIntegralMovavgDelayMatchAnalyticSignals`.
    #[test]
    fn the_time_operators_match_analytic_signals() {
        let n = 1001;
        let dt = 0.01;
        let raster = ramp(n, dt);
        let lin: Vec<f64> = raster.iter().map(|t| 3.0 * t).collect(); // x = 3t
        let inputs = bind(&[("x", series(&raster, &lin, Interp::Linear))]);

        let d = run("delta(x)", &raster, &inputs);
        assert_eq!(d[0], 0.0);
        close(d[500], 3.0 * dt, 1e-9, "delta");

        // ∫3t dt = 1.5 t², at t = 4.
        let integ = run("integral(x)", &raster, &inputs);
        close(integ[400], 1.5 * 16.0, 1e-6, "integral");

        // Trailing 1 s mean of 3t at t = 5 is the mean over [4, 5] = 13.5.
        let ma = run("movavg(x, 1)", &raster, &inputs);
        close(ma[500], 13.5, 0.05, "movavg");

        let del = run("delay(x, 2)", &raster, &inputs);
        close(del[500], 3.0 * (5.0 - 2.0), 1e-9, "delay");
        assert!(del[100].is_nan(), "t = 1 s: the source has nothing at -1 s");

        // Time ops compose with arithmetic: d/dt of 3t is 3.
        let combo = run("delta(x) / 0.01 - 3", &raster, &inputs);
        close(combo[700], 0.0, 1e-6, "delta/dt - 3");
    }

    /// Port of `timeOpsNestInsideNegationComparisonAndCalls`, including the
    /// uppercase-input lookup `delay` needs.
    #[test]
    fn time_ops_nest_inside_negation_comparison_and_calls() {
        let raster = ramp(11, 0.1);
        let lin: Vec<f64> = raster.iter().map(|t| 2.0 * t).collect();
        let inputs = bind(&[("x", series(&raster, &lin, Interp::Linear))]);

        close(run("-delta(x)", &raster, &inputs)[5], -0.2, 1e-12, "-delta");
        assert_eq!(run("not (delta(x) > 0.1)", &raster, &inputs)[5], 0.0);
        close(
            run("abs(-delta(x))", &raster, &inputs)[5],
            0.2,
            1e-9,
            "abs(-delta)",
        );

        // The input is spelled `X`; the formula says `x`. `delay` reads the
        // original series, so it is the one operator that has to find it.
        let upper = bind(&[("X", series(&raster, &lin, Interp::Linear))]);
        close(
            run("delay(x, 0.2)", &raster, &upper)[5],
            lin[3],
            1e-12,
            "delay via original key",
        );
    }

    /// A gap must stay a gap. Each operator has its own rule for what a `NaN`
    /// does, and each is asserted here because getting one wrong silently
    /// changes every downstream number.
    #[test]
    fn the_time_operators_handle_gaps() {
        let raster = ramp(5, 1.0); // 0,1,2,3,4
        let v = [1.0, f64::NAN, 3.0, 4.0, 5.0];
        let inputs = bind(&[("x", series(&raster, &v, Interp::Step))]);

        // delta: any difference touching the gap is NaN, and only those.
        let d = run("delta(x)", &raster, &inputs);
        assert_eq!(d[0], 0.0);
        assert!(d[1].is_nan() && d[2].is_nan(), "{d:?}");
        assert_eq!(d[3], 1.0);

        // integral: the two segments touching the gap add nothing, so the
        // accumulator holds flat across it and *resumes* afterwards — the area
        // under the gap is lost, not the area after it.
        // segments: (0,1)→gap, (1,2)→gap, (2,3)→3.5, (3,4)→4.5 ⇒ 0,0,0,3.5,8.
        let i = run("integral(x)", &raster, &inputs);
        assert_eq!(&i[..], &[0.0, 0.0, 0.0, 3.5, 8.0][..]);

        // movavg: NaN samples are skipped, not propagated. Window 2 s at t = 2
        // spans t ∈ {0,1,2} → mean of {1, 3} = 2.
        let m = run("movavg(x, 2)", &raster, &inputs);
        assert_eq!(m[0], 1.0);
        assert_eq!(m[1], 1.0, "the only live sample in [-1,1] is x(0)");
        assert_eq!(m[2], 2.0);

        // An all-NaN window is NaN, never 0.
        let all_nan = bind(&[("x", series(&raster, &[f64::NAN; 5], Interp::Step))]);
        assert!(run("movavg(x, 2)", &raster, &all_nan)[3].is_nan());

        // delay: a gap is carried through, and STEP holds the *stored* NaN.
        let del = run("delay(x, 1)", &raster, &inputs);
        assert!(del[0].is_nan(), "nothing before the first sample");
        assert_eq!(del[1], 1.0);
        assert!(del[2].is_nan(), "the gap, shifted by 1 s");
    }

    /// Port of `timeOpArgumentValidationIsTyped`.
    #[test]
    fn time_op_argument_validation_is_typed() {
        let raster = ramp(10, 0.1);
        let inputs = bind(&[("x", series(&raster, &[0.0; 10], Interp::Step))]);

        let m = run_err("delta(1 + 2)", &raster, &inputs);
        assert!(m.contains("takes an input signal"), "{m}");
        let m = run_err("integral(zzz)", &raster, &inputs);
        assert!(m.contains("zzz") && m.contains("unknown input"), "{m}");
        let m = run_err("movavg(x, x)", &raster, &inputs);
        assert!(m.contains("numeric constant"), "{m}");
        let m = run_err("movavg(x, 0)", &raster, &inputs);
        assert!(m.contains("> 0"), "{m}");
        // `-1` is `Neg(Num)`, not a `Num`, so it is refused as non-constant —
        // the same answer the Java gives, one message earlier than "> 0".
        let m = run_err("movavg(x, -1)", &raster, &inputs);
        assert!(m.contains("numeric constant"), "{m}");
        // A missing window is a zero window.
        let m = run_err("movavg(x)", &raster, &inputs);
        assert!(m.contains("> 0"), "{m}");
        // Nesting a time op inside a time op is refused, not silently made a
        // second difference.
        let m = run_err("delta(delta(x))", &raster, &inputs);
        assert!(m.contains("takes an input signal"), "{m}");

        // `delta()` cannot be *written* — this grammar's `argList` needs at
        // least one argument, where ANTLR's admitted an empty one — so the
        // no-args guard is only reachable from a hand-built AST. Exercised
        // there, because an unreachable guard is still a guard that has to be
        // right if anything else ever constructs an `Expr`.
        assert!(parse_formula("delta()").is_err());
        let empty = Expr::call("delta", vec![]);
        let m = evaluate(&empty, &raster, &inputs).unwrap_err().to_string();
        assert!(m.contains("takes an input signal"), "{m}");
    }

    /// `delay` is the one operator the Java does **not** range-check: its
    /// `requirePositive` guard is wired only to `movavg`. Pinned here so the
    /// asymmetry is a decision on the record rather than an oversight.
    ///
    /// It is narrower than it looks. A *negative* tau is unreachable from
    /// source either way, because `-1` parses as `Neg(Num)` and the second
    /// argument must be a bare `Num`; the whole reachable difference is that
    /// `delay(x, 0)` is the identity instead of an error.
    #[test]
    fn delay_accepts_a_zero_tau_as_the_java_does() {
        let raster = ramp(5, 1.0);
        let v = [0.0, 1.0, 2.0, 3.0, 4.0];
        let inputs = bind(&[("x", series(&raster, &v, Interp::Linear))]);
        assert_eq!(run("delay(x, 0)", &raster, &inputs), v.to_vec());
        let m = run_err("delay(x, -1)", &raster, &inputs);
        assert!(m.contains("numeric constant"), "{m}");
    }

    // -- errors ----------------------------------------------------------

    /// Port of `unknownVariableIsATypedError`.
    #[test]
    fn an_unknown_variable_is_a_typed_error() {
        let raster = ramp(10, 0.1);
        let inputs = bind(&[("a", series(&raster, &[0.0; 10], Interp::Step))]);
        let m = run_err("a + b", &raster, &inputs);
        assert!(m.contains("Unknown variable \"b\""), "{m}");
        assert!(m.contains("bind it to a signal input"), "{m}");
    }

    /// Port of `syntaxErrorsAreTypedWithColumn` and `trailingGarbageIsATypedError`.
    #[test]
    fn parse_failures_are_typed_and_carry_a_column() {
        let e = parse_formula("x + * 2").unwrap_err();
        assert_eq!(e.code(), "FORMULA_ERROR");
        let m = e.to_string();
        assert!(m.starts_with("Formula error:"), "{m}");
        assert!(m.contains("column"), "{m}");

        let m = parse_formula("x + 1 )").unwrap_err().to_string();
        assert!(m.starts_with("Formula error:"), "{m}");
        assert!(m.contains("unexpected"), "{m}");

        // The lexer's own failures land in the same shape.
        let m = parse_formula("x + 'unterminated").unwrap_err().to_string();
        assert!(m.starts_with("Formula error:"), "{m}");
    }

    /// Port of `runtimeFailureInsideACallIsWrappedWithTheTimestamp`.
    #[test]
    fn a_runtime_failure_inside_a_call_carries_the_timestamp() {
        let raster = ramp(3, 0.5);
        let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0], Interp::Step))]);
        let m = run_err("nosuchfunction(x)", &raster, &inputs);
        assert!(m.contains("Formula failed at t = 0"), "{m}");
    }

    /// Constructs with no per-point meaning name themselves rather than
    /// evaluating to a quiet zero.
    #[test]
    fn constructs_without_calc_meaning_are_refused_by_name() {
        let raster = ramp(3, 1.0);
        let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0], Interp::Step))]);

        let m = run_err("'text'", &raster, &inputs);
        assert!(m.contains("string literal") && m.contains("text"), "{m}");
        let m = run_err("[1, 2, 3]", &raster, &inputs);
        assert!(m.contains("array literal"), "{m}");
        let m = run_err("x \\ 2", &raster, &inputs);
        assert!(
            m.contains("Unsupported operator") && m.contains('\\'),
            "{m}"
        );
        let m = run_err("x .* 2", &raster, &inputs);
        assert!(m.contains("Unsupported operator"), "{m}");

        // An array element parses as `ArrayAccess`, which has no raster meaning.
        let m = run_err("x[1]", &raster, &inputs);
        assert!(m.contains("array element"), "{m}");
    }

    /// Two bindings that differ only in case are one variable, so the pair is
    /// refused instead of one silently winning.
    #[test]
    fn inputs_differing_only_in_case_are_refused() {
        let raster = ramp(3, 1.0);
        let both = bind(&[
            ("Speed", series(&raster, &[1.0, 2.0, 3.0], Interp::Step)),
            ("speed", series(&raster, &[9.0, 9.0, 9.0], Interp::Step)),
        ]);
        let m = run_err("speed", &raster, &both);
        assert!(m.contains("Speed") && m.contains("case-insensitive"), "{m}");
    }

    // -- structural ------------------------------------------------------

    /// Port of `containsCallRecursesEveryNodeKind`.
    #[test]
    fn contains_call_sees_every_node_kind() {
        let yes = [
            "1 + abs(x)",
            "-abs(x)",
            "abs(x) > 1",
            "abs(x) > 1 and x < 2",
            "not (abs(x) > 1)",
        ];
        for f in yes {
            assert!(contains_call(&parse_formula(f).unwrap()), "{f}");
        }
        let no = "not (x > 1) or x * 2 < -3";
        assert!(!contains_call(&parse_formula(no).unwrap()), "{no}");
    }

    /// A formula built from a long left-associative chain reaches the deepest
    /// tree the parser will hand out. It has to evaluate rather than abort —
    /// this is the shape that produced a stack-overflow abort in the Phase 7–8
    /// sweep, and the compiled tree, its evaluation and its `Drop` are three
    /// more recursive walks over it.
    #[test]
    fn a_deeply_nested_formula_evaluates_instead_of_aborting() {
        let raster = ramp(3, 1.0);
        let inputs = bind(&[("x", series(&raster, &[1.0, 2.0, 3.0], Interp::Step))]);

        // The exact ceiling is *found*, not hardcoded, so this keeps testing
        // "whatever the parser admits" if the budget is ever retuned.
        let chain_of = |n: usize| vec!["x"; n].join(" + ");
        let mut terms = 2;
        while parse_formula(&chain_of(terms * 2)).is_ok() {
            terms *= 2;
        }
        let (mut lo, mut hi) = (terms, terms * 2);
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if parse_formula(&chain_of(mid)).is_ok() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let out = run(&chain_of(lo), &raster, &inputs);
        assert_eq!(out[1], 2.0 * lo as f64, "flat chain of {lo} terms");

        // Nested parentheses, at the parser's nesting ceiling.
        let depth = 60;
        let nested = format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
        assert_eq!(run(&nested, &raster, &inputs)[2], 3.0);

        // Nested calls, the heaviest shape there is — every level is a `Call`,
        // so every level also re-enters the general evaluator.
        let calls = format!("{}x{}", "abs(".repeat(depth), ")".repeat(depth));
        assert_eq!(run(&calls, &raster, &inputs)[2], 3.0);

        // Past the parser's budget it is a diagnostic, never an abort.
        let m = parse_formula(&chain_of(hi)).unwrap_err().to_string();
        assert!(m.contains("too deeply nested"), "{m}");
    }

    /// The differentiator over conventional calc engines: a real-fluid property
    /// call, per sample, on measured data. Oracle (CoolProp 8.0.0, via
    /// `tools/golden-dumper`): `Enthalpy(Water, T=300 [K], P=101325 [Pa])` =
    /// 112654.89965464505.
    #[test]
    fn a_property_function_evaluates_over_measured_data() {
        crate::props::propfun::test_with_builtin_tables(|| {
            let raster = ramp(5, 0.1);
            let t_k = [300.0, 310.0, 320.0, 330.0, 340.0];
            let inputs = bind(&[("tk", series(&raster, &t_k, Interp::Linear))]);

            let formula = parse_formula("enthalpy(Water, T=tk, P=101325)").unwrap();
            assert!(contains_call(&formula), "a property call is a Call node");
            let out = evaluate(&formula, &raster, &inputs).expect("property call must evaluate");

            let rel = (out[0] - 112_654.899_654_645_05).abs() / 112_654.899_654_645_05;
            assert!(rel < 1e-4, "h(300 K) = {}, rel = {rel:e}", out[0]);
            // Enthalpy rises with temperature along an isobar.
            for w in out.windows(2) {
                assert!(w[1] > w[0], "{out:?}");
            }
        });
    }

    /// A property call whose state comes from a *time operator* — the two
    /// features have to compose, because the rewrite happens before compilation
    /// and the synthetic series has to be visible to the general evaluator.
    #[test]
    fn a_property_call_reads_a_synthetic_time_op_series() {
        crate::props::propfun::test_with_builtin_tables(|| {
            let raster = ramp(4, 1.0);
            let t_k = [300.0, 300.0, 310.0, 320.0];
            let inputs = bind(&[("tk", series(&raster, &t_k, Interp::Linear))]);
            // movavg over a 1 s trailing window at t = 3 is the mean of
            // {310, 320} = 315 K.
            let out = run(
                "enthalpy(Water, T=movavg(tk, 1), P=101325)",
                &raster,
                &inputs,
            );
            let direct = run("enthalpy(Water, T=tk, P=101325)", &raster, &inputs);
            assert!(out[3] > direct[2] && out[3] < direct[3], "{out:?}");
        });
    }
}
