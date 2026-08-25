//! Slot-indexed evaluation of a prepared block's residuals and Jacobian.
//!
//! # Why
//!
//! A callgrind profile of the per-step transient path (Wave A2, round 4) put
//! **27.8 % of all instructions** in string-keyed map machinery — `Env::get`,
//! `HashMap::<String, f64>::get_mut`, `FxHasher::hash_one` and the `bcmp` each
//! successful probe ends with. The reason is structural rather than wasteful:
//! `Scope` is keyed by variable *name*, so every residual read, every write of
//! a Newton iterate and every Jacobian entry pays a hash, a probe and a key
//! comparison — measured at **~105 instructions per scalar access**, to move
//! one `f64`.
//!
//! The blocks that pay it hardest are the smallest ones. A pinned per-step
//! solve is ~16 one-variable blocks of the shape `var = <literal>` or
//! `var = <small arithmetic expression>`, re-solved at every integrator stage;
//! on the `hot-transient` stand-in that is 211 322 `solve_block` calls, each
//! spending ~700 instructions on hashing to solve what is arithmetically a
//! single assignment.
//!
//! # What this module does
//!
//! It compiles a block's residual and derivative expressions **once**, when
//! [`crate::engine`] builds the block cache, into postfix programs over a
//! dense `Vec<f64>` *slot vector*: every `Expr::Var` becomes a `Load(i)`, and
//! the block's unknowns become known slot indices. `solve_block` then fills
//! the slot vector from the `Scope` once per call, runs the whole Newton solve
//! against slots, and writes the answer back once. The String-keyed `Scope`
//! stays the boundary everything outside the block loop sees; only the inner
//! loop is indexed.
//!
//! One residual is **one** program, not two: the compiled form of equation `k`
//! is `lhs`, `rhs`, `Diff`, which is `residuals_into`'s own `lhs − rhs` with
//! the operands evaluated in the same order — halving the per-equation call
//! and stack-setup overhead the first cut paid.
//!
//! # Why the arithmetic is byte-identical
//!
//! The corpus is a parity oracle, so this is the load-bearing property:
//!
//! * **The same operations, in the same order.** [`eval_program`] is a
//!   transcription of [`crate::eval::eval_in`]'s arithmetic arms: `BinOp`
//!   evaluates left, then right, then defers to the *same*
//!   [`crate::eval::apply_binop`]; `Compare` and `Logical` evaluate both
//!   operands with no short-circuit, as the Java does; `Neg` and `Not` negate
//!   after their operand. Postfix preserves that order exactly — a binary
//!   tree's left-to-right leaf order is the same walked either way — so the
//!   *first* error to surface is also the same one.
//! * **Nothing else compiles.** [`compile_block`] answers `None` for any node
//!   outside that set — every `Call`, `ArrayAccess`, `ArrayLiteral`, `Range`
//!   and `Str` — and for any expression deeper than [`MAX_STACK`]. A block
//!   that contains one keeps the ordinary `Expr` + `Scope` path verbatim, so
//!   there is no mixed mode in which a fallback subtree could read a stale
//!   `Scope`.
//! * **Volatile literals are re-read per call.** A prep's *pin* equations are
//!   rewritten in place between calls (that is what makes the prep reusable),
//!   so their literals compile to `Op::Const(k)` — an index into a per-call
//!   `consts` vector that [`refresh_consts`] refills from the live expressions
//!   before every solve. Every other literal is baked, on two independent
//!   guarantees: the prep's cache key is a **full structural equality** check
//!   of the template equations, literals included, so a template literal
//!   cannot change under the cache; and a pin value can never reach a
//!   *derivative*, because a pin residual is `var − c` and `c` enters only as
//!   `d(c)/d(var) = 0` — the same invariant Wave G3 already relies on to cache
//!   the derivative trees themselves.
//! * **The refresh is a structural check, not just a copy.** It walks the same
//!   variants the compiler accepts and refuses anything else, and the caller
//!   compares the count it collected against the compile-time count. A block
//!   whose expressions changed shape under the cache therefore falls back
//!   instead of evaluating against stale ops.

use crate::ast::{BinOp, CmpOp, Equation, Expr, LogicOp};
use crate::diag::{FreesError, Result};
use crate::eval::apply_binop;
use std::collections::HashMap;

/// Deepest postfix stack a compiled program may need. Block residuals are
/// small arithmetic; refusing the rare deep one costs a fallback, and having a
/// bound is what lets `solve_block` size the stack once per call instead of
/// growing it per push.
pub(crate) const MAX_STACK: usize = 32;

/// One postfix operation over a block's slot vector.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Op {
    /// Push `consts[k]` — a volatile literal, refilled per call by
    /// [`refresh_consts`].
    Const(u32),
    /// Push a literal baked in at compile time.
    Num(f64),
    /// Push `slots[i]`.
    Load(u32),
    Neg,
    Bin(BinOp),
    Cmp(CmpOp),
    Logic(LogicOp),
    Not,
    /// `lhs − rhs`: the residual subtraction `residuals_into` writes in Rust,
    /// not an `Expr` operator, so it does not go through `apply_binop`.
    Diff,
}

/// One compiled expression.
#[derive(Debug, Clone, Default)]
pub(crate) struct SlotProgram {
    ops: Vec<Op>,
}

/// A block whose residuals and derivatives are wholly slot-evaluable.
#[derive(Debug)]
pub(crate) struct CompiledBlock {
    /// Every distinct variable the block mentions, in slot order. `solve_block`
    /// reads these out of the `Scope` once per call to fill the slot vector.
    pub(crate) names: Vec<String>,
    /// The slot of each block unknown, in `block.variables` order.
    pub(crate) unknown_slots: Vec<u32>,
    /// `lhs − rhs` per block equation, in block-equation order.
    pub(crate) residuals: Vec<SlotProgram>,
    /// Which block equations carry volatile literals — the pins, whose values
    /// are rewritten between calls. Positions into `residuals`, ascending;
    /// empty for a block of template equations only, which is then free of
    /// per-call refresh work entirely.
    pub(crate) volatile: Vec<usize>,
    /// How many volatile literals [`refresh_consts`] must find. A mismatch
    /// means the expressions changed shape and the block must fall back.
    pub(crate) const_count: usize,
    /// The compiled analytic Jacobian, mirroring
    /// `PinnedBlockStruct::derivs`: an outer `None` is the cached answer
    /// "finite differences", not a cache miss.
    pub(crate) derivs: Option<Vec<Vec<Option<SlotProgram>>>>,
    /// Deepest evaluation stack any of the programs needs, `<= MAX_STACK`.
    pub(crate) depth: usize,
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

/// Where a literal goes: refreshed per call (a pin's) or baked (everything
/// else — see the module docs for the two guarantees behind that).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Literals {
    Refreshed,
    Baked,
}

struct Compiler {
    slots: HashMap<String, u32>,
    names: Vec<String>,
    const_count: usize,
    depth: usize,
    max_depth: usize,
}

impl Compiler {
    fn new() -> Compiler {
        Compiler {
            slots: HashMap::new(),
            names: Vec::new(),
            const_count: 0,
            depth: 0,
            max_depth: 0,
        }
    }

    fn slot_of(&mut self, name: &str) -> u32 {
        if let Some(index) = self.slots.get(name) {
            return *index;
        }
        let index = self.names.len() as u32;
        self.names.push(name.to_string());
        self.slots.insert(name.to_string(), index);
        index
    }

    fn push(&mut self, ops: &mut Vec<Op>, op: Op) -> bool {
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        ops.push(op);
        self.max_depth <= MAX_STACK
    }

    /// Emit `expr` in postfix, or answer `false` for a node this module does
    /// not represent — in which case the whole block falls back.
    fn compile(&mut self, expr: &Expr, literals: Literals, ops: &mut Vec<Op>) -> bool {
        match expr {
            Expr::Num { value, .. } => {
                // `unit` and `is_imaginary` are ignored exactly as `eval_in`'s
                // literal arm ignores them.
                let op = match literals {
                    Literals::Refreshed => {
                        let index = self.const_count as u32;
                        self.const_count += 1;
                        Op::Const(index)
                    }
                    Literals::Baked => Op::Num(*value),
                };
                self.push(ops, op)
            }
            Expr::Var(name) => {
                let slot = self.slot_of(name);
                self.push(ops, Op::Load(slot))
            }
            Expr::Neg(operand) => {
                if !self.compile(operand, literals, ops) {
                    return false;
                }
                ops.push(Op::Neg);
                true
            }
            Expr::Not(operand) => {
                if !self.compile(operand, literals, ops) {
                    return false;
                }
                ops.push(Op::Not);
                true
            }
            Expr::BinOp { op, left, right } => {
                self.binary(left, right, Op::Bin(*op), literals, ops)
            }
            Expr::Compare { op, left, right } => {
                self.binary(left, right, Op::Cmp(*op), literals, ops)
            }
            Expr::Logical { op, left, right } => {
                self.binary(left, right, Op::Logic(*op), literals, ops)
            }
            // Everything else — calls, array access, ranges, array literals,
            // string literals — needs the `Env`/`Scope` evaluator. Refusing
            // here is what keeps the two paths from ever mixing.
            _ => false,
        }
    }

    fn binary(
        &mut self,
        left: &Expr,
        right: &Expr,
        combine: Op,
        literals: Literals,
        ops: &mut Vec<Op>,
    ) -> bool {
        if !self.compile(left, literals, ops) {
            return false;
        }
        if !self.compile(right, literals, ops) {
            return false;
        }
        // Two operands in, one value out.
        self.depth -= 1;
        ops.push(combine);
        true
    }

    /// `lhs − rhs` as one program — `residuals_into`'s expression, compiled.
    fn residual(&mut self, equation: &Equation, literals: Literals) -> Option<SlotProgram> {
        let mut ops = Vec::new();
        self.depth = 0;
        if !self.compile(&equation.lhs, literals, &mut ops) {
            return None;
        }
        if !self.compile(&equation.rhs, literals, &mut ops) {
            return None;
        }
        self.depth -= 1;
        ops.push(Op::Diff);
        Some(SlotProgram { ops })
    }

    fn program(&mut self, expr: &Expr, literals: Literals) -> Option<SlotProgram> {
        let mut ops = Vec::new();
        self.depth = 0;
        if self.compile(expr, literals, &mut ops) {
            Some(SlotProgram { ops })
        } else {
            None
        }
    }
}

/// Compile a whole block, or answer `None` if any of its expressions needs the
/// ordinary evaluator. All-or-nothing by design — see the module docs.
///
/// `volatile` marks, per block equation, whether its literals are rewritten
/// between calls (the prep's pins). Everything else is baked.
pub(crate) fn compile_block(
    block_equations: &[&Equation],
    volatile: &[bool],
    variables: &[String],
    derivs: Option<&Vec<Vec<Option<Expr>>>>,
) -> Option<CompiledBlock> {
    if volatile.len() != block_equations.len() {
        return None;
    }
    let mut compiler = Compiler::new();

    let mut residuals = Vec::with_capacity(block_equations.len());
    let mut volatile_positions = Vec::new();
    for (position, (equation, is_volatile)) in block_equations.iter().zip(volatile).enumerate() {
        let literals = if *is_volatile {
            volatile_positions.push(position);
            Literals::Refreshed
        } else {
            Literals::Baked
        };
        residuals.push(compiler.residual(equation, literals)?);
    }

    let compiled_derivs = match derivs {
        None => None,
        Some(rows) => {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let mut compiled_row = Vec::with_capacity(row.len());
                for entry in row {
                    match entry {
                        // A structural zero stays one.
                        None => compiled_row.push(None),
                        Some(expr) => {
                            compiled_row.push(Some(compiler.program(expr, Literals::Baked)?))
                        }
                    }
                }
                out.push(compiled_row);
            }
            Some(out)
        }
    };

    // The blocker guarantees each unknown is mentioned, so it already has a
    // slot; assigning one here anyway keeps the invariant local rather than
    // borrowed from another module.
    let unknown_slots = variables
        .iter()
        .map(|name| compiler.slot_of(name))
        .collect();

    Some(CompiledBlock {
        names: compiler.names,
        unknown_slots,
        residuals,
        volatile: volatile_positions,
        const_count: compiler.const_count,
        derivs: compiled_derivs,
        depth: compiler.max_depth,
    })
}

// ---------------------------------------------------------------------------
// Per-call literal refresh
// ---------------------------------------------------------------------------

/// Refill `out` with the block's volatile literals, in the order
/// [`compile_block`] assigned them.
///
/// Answers `false` if an expression no longer has the shape it was compiled
/// with — the caller then falls back to the ordinary path for this call. A
/// block with no volatile equations does no work at all here.
///
/// `equations` is the document's whole list and `indices` the block's
/// positions into it, rather than the compacted `Vec<&Equation>` this used to
/// take: `solve_block` no longer materialises that list on the compiled path
/// (Wave Q3), and it calls here only when the two are known to line up — every
/// index resolves, so compacted position `p` is `equations[indices[p]]`.
pub(crate) fn refresh_consts(
    equations: &[Equation],
    indices: &[usize],
    volatile: &[usize],
    out: &mut Vec<f64>,
) -> bool {
    out.clear();
    for position in volatile {
        let Some(equation) = indices.get(*position).and_then(|&i| equations.get(i)) else {
            return false;
        };
        if !collect_literals(&equation.lhs, out) || !collect_literals(&equation.rhs, out) {
            return false;
        }
    }
    true
}

/// The left-to-right literal walk, over exactly the variants
/// [`Compiler::compile`] accepts. Leaf order is the same walked in pre-order
/// or postfix, which is why one recursion serves both.
fn collect_literals(expr: &Expr, out: &mut Vec<f64>) -> bool {
    match expr {
        Expr::Num { value, .. } => {
            out.push(*value);
            true
        }
        Expr::Var(_) => true,
        Expr::Neg(operand) | Expr::Not(operand) => collect_literals(operand, out),
        Expr::BinOp { left, right, .. }
        | Expr::Compare { left, right, .. }
        | Expr::Logical { left, right, .. } => {
            collect_literals(left, out) && collect_literals(right, out)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Evaluate one compiled program. A transcription of [`crate::eval::eval_in`]'s
/// arithmetic arms — same operations, same order, same `apply_binop`.
///
/// `stack` is caller-owned and at least `CompiledBlock::depth` long, so the
/// evaluation itself neither allocates nor bounds-grows.
pub(crate) fn eval_program(
    program: &SlotProgram,
    slots: &[f64],
    consts: &[f64],
    stack: &mut [f64],
) -> Result<f64> {
    let mut top = 0usize;
    for op in &program.ops {
        match *op {
            Op::Const(index) => {
                push(stack, &mut top, consts[index as usize])?;
            }
            Op::Num(value) => {
                push(stack, &mut top, value)?;
            }
            Op::Load(slot) => {
                push(stack, &mut top, slots[slot as usize])?;
            }
            Op::Neg => {
                let value = pop(stack, &mut top)?;
                push(stack, &mut top, -value)?;
            }
            Op::Not => {
                let value = pop(stack, &mut top)?;
                push(stack, &mut top, if value == 0.0 { 1.0 } else { 0.0 })?;
            }
            Op::Bin(binop) => {
                let right = pop(stack, &mut top)?;
                let left = pop(stack, &mut top)?;
                push(stack, &mut top, apply_binop(binop, left, right)?)?;
            }
            Op::Diff => {
                let right = pop(stack, &mut top)?;
                let left = pop(stack, &mut top)?;
                push(stack, &mut top, left - right)?;
            }
            Op::Cmp(cmpop) => {
                let right = pop(stack, &mut top)?;
                let left = pop(stack, &mut top)?;
                let truth = match cmpop {
                    CmpOp::Lt => left < right,
                    CmpOp::Gt => left > right,
                    CmpOp::Le => left <= right,
                    CmpOp::Ge => left >= right,
                    CmpOp::Ne => left != right,
                    CmpOp::Eq => left == right,
                };
                push(stack, &mut top, if truth { 1.0 } else { 0.0 })?;
            }
            // Java evaluates *both* operands before combining — no
            // short-circuit — which postfix gives for free.
            Op::Logic(logicop) => {
                let right = pop(stack, &mut top)?;
                let left = pop(stack, &mut top)?;
                let truth = match logicop {
                    LogicOp::And => left != 0.0 && right != 0.0,
                    LogicOp::Or => left != 0.0 || right != 0.0,
                };
                push(stack, &mut top, if truth { 1.0 } else { 0.0 })?;
            }
        }
    }
    pop(stack, &mut top)
}

/// The compiler emits well-formed postfix inside a stack the caller sized, so
/// neither of these can fire; they are internal-error returns rather than
/// panics because the engine must not abort a user's tab on an engine bug.
#[inline(always)]
fn push(stack: &mut [f64], top: &mut usize, value: f64) -> Result<()> {
    match stack.get_mut(*top) {
        Some(slot) => {
            *slot = value;
            *top += 1;
            Ok(())
        }
        None => Err(FreesError::solver(
            "internal error: compiled block program overflowed its stack",
        )),
    }
}

#[inline(always)]
fn pop(stack: &[f64], top: &mut usize) -> Result<f64> {
    match top.checked_sub(1) {
        Some(next) => {
            *top = next;
            Ok(stack[next])
        }
        None => Err(FreesError::solver(
            "internal error: compiled block program underflowed its stack",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::eval::{eval_with, EvalContext, Scope};

    fn scope(pairs: &[(&str, f64)]) -> Scope {
        let mut values = Scope::default();
        for (name, value) in pairs {
            values.insert((*name).to_string(), *value);
        }
        values
    }

    /// Compile `expr = 0` as a block residual and evaluate it against the same
    /// values `eval_with` sees. `lhs − rhs` with `rhs = 0` is `lhs`, so the
    /// two answers must agree bit for bit.
    fn both(expr: &Expr, values: &[(&str, f64)]) -> (Result<f64>, Result<f64>) {
        let equation = crate::ast::Equation::new(expr.clone(), Expr::num(0.0), "t");
        let equations = [&equation];
        let compiled = compile_block(&equations, &[true], &[], None).expect("compilable");
        let mut consts = Vec::new();
        assert!(refresh_consts(
            std::slice::from_ref(&equation),
            &[0],
            &compiled.volatile,
            &mut consts
        ));
        assert_eq!(consts.len(), compiled.const_count);
        let map = scope(values);
        let slots: Vec<f64> = compiled
            .names
            .iter()
            .map(|name| *map.get(name).expect("bound"))
            .collect();
        let mut stack = vec![0.0; MAX_STACK];
        let slot_answer = eval_program(&compiled.residuals[0], &slots, &consts, &mut stack);
        let tree_answer = eval_with(expr, &map, EvalContext::default());
        (slot_answer, tree_answer)
    }

    fn agree(expr: &Expr, values: &[(&str, f64)]) {
        let (slot_answer, tree_answer) = both(expr, values);
        match (slot_answer, tree_answer) {
            (Ok(a), Ok(b)) => assert_eq!(a.to_bits(), b.to_bits(), "value mismatch on {expr:?}"),
            (Err(a), Err(b)) => assert_eq!(
                a.to_string_message(),
                b.to_string_message(),
                "error mismatch on {expr:?}"
            ),
            (a, b) => panic!("outcome mismatch on {expr:?}: {a:?} vs {b:?}"),
        }
    }

    #[test]
    fn arithmetic_matches_the_tree_evaluator_bit_for_bit() {
        let x = Expr::var("x");
        let y = Expr::var("y");
        agree(&x, &[("x", 3.5)]);
        agree(
            &Expr::bin(BinOp::Sub, x.clone(), Expr::num(20.0)),
            &[("x", 95.0)],
        );
        agree(
            &Expr::bin(
                BinOp::Mul,
                Expr::Neg(Box::new(Expr::var("k"))),
                Expr::bin(BinOp::Sub, x.clone(), y.clone()),
            ),
            &[("x", 95.0), ("y", 20.0), ("k", 100.001)],
        );
        agree(
            &Expr::bin(BinOp::Pow, x.clone(), Expr::num(0.5)),
            &[("x", 2.0)],
        );
        // Association and rounding: the same tree walked two ways must give
        // the same last bit.
        agree(
            &Expr::bin(
                BinOp::Add,
                Expr::bin(BinOp::Div, Expr::num(1.0), Expr::num(3.0)),
                Expr::bin(BinOp::Mul, x.clone(), Expr::num(0.1)),
            ),
            &[("x", 0.7)],
        );
        agree(
            &Expr::Not(Box::new(Expr::Compare {
                op: CmpOp::Lt,
                left: Box::new(x.clone()),
                right: Box::new(Expr::num(1.0)),
            })),
            &[("x", 0.7)],
        );
        agree(
            &Expr::Logical {
                op: LogicOp::Or,
                left: Box::new(x.clone()),
                right: Box::new(Expr::num(0.0)),
            },
            &[("x", 0.0)],
        );
    }

    #[test]
    fn the_same_error_surfaces_first() {
        let x = Expr::var("x");
        // Division by zero on the left operand must win over the right's.
        agree(
            &Expr::bin(
                BinOp::Add,
                Expr::bin(BinOp::Div, x.clone(), Expr::num(0.0)),
                Expr::bin(BinOp::Pow, Expr::num(-2.0), Expr::num(0.5)),
            ),
            &[("x", 1.0)],
        );
        agree(
            &Expr::bin(BinOp::Pow, Expr::num(-2.0), Expr::num(0.5)),
            &[("x", 1.0)],
        );
    }

    #[test]
    fn a_call_refuses_to_compile() {
        let call = Expr::Call {
            function: "sqrt".to_string(),
            args: vec![Expr::var("x")],
        };
        let equation = crate::ast::Equation::new(call, Expr::num(0.0), "t");
        assert!(compile_block(&[&equation], &[true], &[], None).is_none());
    }

    #[test]
    fn a_deep_expression_refuses_to_compile() {
        let mut expr = Expr::var("x");
        for _ in 0..MAX_STACK + 2 {
            expr = Expr::bin(BinOp::Add, Expr::num(1.0), expr);
        }
        let equation = crate::ast::Equation::new(expr, Expr::num(0.0), "t");
        assert!(compile_block(&[&equation], &[true], &[], None).is_none());
    }

    #[test]
    fn pin_literals_refresh_and_everything_else_is_baked() {
        // The pin shape: `var = c`, whose value is rewritten between calls.
        let mut pin = crate::ast::Equation::new(Expr::var("temp"), Expr::num(95.0), "temp = 95");
        let variables = vec!["temp".to_string()];
        let derivs = vec![vec![Some(Expr::num(1.0))]];
        let compiled =
            compile_block(&[&pin], &[true], &variables, Some(&derivs)).expect("compilable");
        assert_eq!(compiled.const_count, 1);
        assert_eq!(compiled.volatile, vec![0]);

        let mut consts = Vec::new();
        let mut stack = vec![0.0; MAX_STACK];
        assert!(refresh_consts(
            std::slice::from_ref(&pin),
            &[0],
            &compiled.volatile,
            &mut consts
        ));
        assert_eq!(consts, vec![95.0]);

        pin.rhs = Expr::num(42.25);
        assert!(refresh_consts(
            std::slice::from_ref(&pin),
            &[0],
            &compiled.volatile,
            &mut consts
        ));
        assert_eq!(consts, vec![42.25]);
        let residual = eval_program(&compiled.residuals[0], &[1.0], &consts, &mut stack).unwrap();
        assert_eq!(residual, 1.0 - 42.25);

        // A template equation refreshes nothing and needs no per-call walk.
        let template =
            crate::ast::Equation::new(Expr::var("k"), Expr::num(100.001), "k = kmin + dk");
        let baked =
            compile_block(&[&template], &[false], &["k".to_string()], None).expect("compilable");
        assert_eq!(baked.const_count, 0);
        assert!(baked.volatile.is_empty());
        assert!(refresh_consts(
            std::slice::from_ref(&template),
            &[0],
            &baked.volatile,
            &mut consts
        ));
        assert!(consts.is_empty());
        let residual = eval_program(&baked.residuals[0], &[7.0], &consts, &mut stack).unwrap();
        assert_eq!(residual, 7.0 - 100.001);
    }

    #[test]
    fn a_reshaped_pin_refuses_to_refresh() {
        let pin = crate::ast::Equation::new(Expr::var("temp"), Expr::num(95.0), "t");
        let compiled =
            compile_block(&[&pin], &[true], &["temp".to_string()], None).expect("compilable");
        let reshaped = crate::ast::Equation::new(
            Expr::var("temp"),
            Expr::Call {
                function: "sqrt".to_string(),
                args: vec![Expr::num(4.0)],
            },
            "t",
        );
        let mut consts = Vec::new();
        assert!(!refresh_consts(
            std::slice::from_ref(&reshaped),
            &[0],
            &compiled.volatile,
            &mut consts
        ));
        // And a shape that still walks but carries a different literal count
        // is caught by the caller's count comparison.
        let widened = crate::ast::Equation::new(
            Expr::var("temp"),
            Expr::bin(BinOp::Add, Expr::num(90.0), Expr::num(5.0)),
            "t",
        );
        assert!(refresh_consts(
            std::slice::from_ref(&widened),
            &[0],
            &compiled.volatile,
            &mut consts
        ));
        assert_ne!(consts.len(), compiled.const_count);
    }

    /// Wave Q3: `refresh_consts` resolves the block's equations through its
    /// index list instead of a compacted `Vec<&Equation>` the caller built. A
    /// block whose equations sit out of order in the document must still walk
    /// them in *block* order, because that is the order `compile_block`
    /// assigned the slots.
    #[test]
    fn refresh_walks_block_order_not_document_order() {
        let document = vec![
            crate::ast::Equation::new(Expr::var("a"), Expr::num(1.5), "a = 1.5"),
            crate::ast::Equation::new(Expr::var("spacer"), Expr::num(0.0), "spacer = 0"),
            crate::ast::Equation::new(Expr::var("b"), Expr::num(2.25), "b = 2.25"),
        ];
        // The block is document equations 2 and 0, in that order.
        let indices = [2usize, 0];
        let compacted: Vec<&crate::ast::Equation> = indices.iter().map(|&i| &document[i]).collect();
        let variables = vec!["b".to_string(), "a".to_string()];
        let compiled =
            compile_block(&compacted, &[true, true], &variables, None).expect("compilable");
        assert_eq!(compiled.volatile, vec![0, 1]);

        let mut consts = Vec::new();
        assert!(refresh_consts(
            &document,
            &indices,
            &compiled.volatile,
            &mut consts
        ));
        // Equation 2's literal first, then equation 0's — block order, and the
        // document's `spacer` never walked.
        assert_eq!(consts, vec![2.25, 1.5]);

        // An index that does not resolve refuses, which is what makes
        // `solve_block` fall back rather than evaluate against stale ops.
        assert!(!refresh_consts(
            &document,
            &[9usize, 0],
            &compiled.volatile,
            &mut consts
        ));
    }
}
