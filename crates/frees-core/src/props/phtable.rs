//! Bicubic `(P, h)` property tables with **analytic** first derivatives.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/PhPropertyTable.java`
//! (289 LOC), plus the on-disk format the browser build needs (the Java samples
//! CoolProp in-process; wasm cannot, so the samples arrive as bytes).
//!
//! # Why this is the browser's hot path (decision D1)
//!
//! A thermofluid Jacobian needs `rho(P,h)`, `T(P,h)` *and their partials*
//! thousands of times per Newton step. Calling a Helmholtz EOS in that inner
//! loop is far too slow and, across the saturation lines, only C⁰. This module
//! samples a property on a structured `(P, h)` grid **once, offline**, then
//! serves every later query from a globally C¹ piecewise-bicubic Hermite
//! surface whose value *and* both partials are evaluated in closed form. The
//! smoothing therefore lives in the same analytic derivative path as the
//! values — never a finite-difference afterthought.
//!
//! All quantities are SI, matching the rest of frees.
//!
//! ---
//!
//! # On-disk format: the `FREESPH1` section
//!
//! **This section is the contract between the offline table generator and the
//! wasm build.** The generator writes bytes; [`PhPropertyTable::decode`] reads
//! them; nothing else is shared. The wasm build is expected to
//! `include_bytes!` a generated file and hand the slice straight to `decode`.
//!
//! Everything is **little-endian**. `f64` is IEEE-754 binary64
//! (`f64::to_le_bytes`), `u32` is 4 bytes, `u8` is 1 byte. There is no padding
//! anywhere and no alignment requirement on the slice.
//!
//! ```text
//! offset  size          field
//! ------  ------------  -------------------------------------------------
//!      0             8  magic, the ASCII bytes "FREESPH1" (no NUL)
//!      8             1  kind             = 0x01 (property-table section)
//!      9             1  section_flags    bit0 = node-flag plane present
//!                                        bits 1..7 reserved, must be 0
//!     10             1  axis_p_kind      AxisKind of the FIRST axis
//!     11             1  axis_h_kind      AxisKind of the SECOND axis
//!     12             1  value_kind       ValueKind of the node plane
//!     13             3  reserved, must be 0
//!     16             4  n_p : u32        first-axis node count,  >= 2
//!     20             4  n_h : u32        second-axis node count, >= 2
//!     24        8*n_p  first-axis nodes  : f64, STRICTLY INCREASING
//!      .        8*n_h  second-axis nodes : f64, STRICTLY INCREASING
//!      .  8*n_p*n_h    node values       : f64, ROW-MAJOR
//!      .    n_p*n_h    node flags        : u8,  ROW-MAJOR (iff bit0 set)
//! ```
//!
//! The header is 24 bytes, so every `f64` block starts 8-byte aligned within
//! the file even though the reader does not depend on it.
//!
//! ## Ordering
//!
//! Row-major with the **first axis outermost**:
//!
//! ```text
//! value[i * n_h + j] = prop(axis_p[i], axis_h[j])
//! ```
//!
//! `i` indexes the first (pressure) axis, `j` the second (enthalpy) axis. The
//! node-flag plane uses the identical index. Both axes must be strictly
//! increasing; `decode` rejects a file where they are not, rather than
//! producing a silently wrong interpolant.
//!
//! ## Units
//!
//! Axes and the value plane are self-describing through [`AxisKind`] and
//! [`ValueKind`]. Every unit is SI:
//!
//! * [`AxisKind::Pressure`] — absolute pressure `P` [Pa]
//! * [`AxisKind::Enthalpy`] — specific enthalpy `h` [J/kg]
//! * [`AxisKind::Superheat`] — dome-following `Δh = h − h_g(P)` [J/kg], `≥ 0`
//! * [`AxisKind::Subcooling`] — dome-following `Δh = h_f(P) − h` [J/kg], `≥ 0`
//! * [`ValueKind`] — `K`, `kg/m³`, `J/kg`, `J/(kg·K)`, `Pa`, `m³/kg`, `Pa·s`,
//!   `W/(m·K)` or dimensionless
//!
//! The two dome-following axis kinds exist because the split tables
//! ([`super::satsplit`]) fit the single-phase regions in coordinates that
//! follow the saturation line; a reader that does not know which coordinate a
//! file uses cannot interpret it, so the file says.
//!
//! ## Derivatives are not stored
//!
//! Only the value plane is in the file. `∂f/∂P`, `∂f/∂h` and `∂²f/∂P∂h` at the
//! nodes are recomputed at load time by the stencil in [`nodal_partial`]
//! (grid-spacing-aware central differences, one-sided at the edges) — the same
//! stencil the Java's `build` uses. This keeps files ~4× smaller, which the
//! wasm bundle budget cares about, and makes it *impossible* for a generator to
//! ship tangents that disagree with the interpolant.
//!
//! ## Two-phase and other node marks
//!
//! The optional node-flag plane carries one `u8` per node, a bitfield:
//!
//! | bit | mask | name | meaning |
//! |---|---|---|---|
//! | 0 | `0x01` | [`NODE_TWO_PHASE`] | the node lies **inside the vapour dome**: `h_f(P) ≤ h ≤ h_g(P)` |
//! | 1 | `0x02` | [`NODE_SUPERCRITICAL`] | `P ≥ p_crit` — no dome exists at this node |
//! | 2 | `0x04` | [`NODE_BACKFILLED`] | the source returned a non-finite sample here; the value was filled from its nearest finite neighbour |
//! | 3–7 | — | reserved | must be 0 |
//!
//! A **cell** is the interpolation patch bounded by nodes
//! `(i, j)`, `(i+1, j)`, `(i, j+1)`, `(i+1, j+1)`. [`PhPropertyTable::cell_flags`]
//! reports it as:
//!
//! * **entirely two-phase** — all four corners carry `NODE_TWO_PHASE`
//!   ([`CellPhase::TwoPhase`]);
//! * **dome-crossing** — some but not all corners do
//!   ([`CellPhase::DomeCrossing`]). A bicubic across such a cell smears the
//!   `h_f`/`h_g` kink, which is exactly what `satsplit` exists to avoid; a
//!   generator that emits a whole-fluid `(P, h)` table should treat these cells
//!   as the honest error bound, and a consumer may refuse to serve from them;
//! * **single-phase** otherwise ([`CellPhase::SinglePhase`]).
//!
//! Files whose region is single-phase by construction — the vapour and liquid
//! pieces of a split table, where the second axis is superheat or subcooling —
//! may omit the plane entirely (`section_flags` bit 0 clear). Omission means
//! "unmarked", never "single-phase asserted".
//!
//! ## Compatibility
//!
//! The magic ends in the format version. A reader that does not recognise the
//! magic must refuse the file; there is no in-band "minor version" escape
//! hatch, because a table misread as a different layout produces plausible
//! numbers rather than an error.

// The bicubic kernel below indexes several parallel node planes by the same
// pair of loop variables, mirroring the Java `double[][]` source it is
// transcribed from. Iterator rewrites obscure that correspondence, so the
// indexed form stays.
#![allow(clippy::needless_range_loop)]
// Float guards written `!(x > y)` are negated on purpose: the negation makes
// NaN take the reject branch, which `x <= y` would not. A NaN grid node must be
// refused, not accepted as "not increasing enough". This matches the Java guard
// being ported (`if (g[i] <= g[i-1]) throw` reads the same way only because a
// NaN comparison is false there too — the negated Rust form is what preserves
// it).
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::diag::{FreesError, Result};

/// Section magic: ASCII `FREESPH1`. The trailing digit is the format version.
pub const PH_TABLE_MAGIC: &[u8; 8] = b"FREESPH1";
/// `kind` byte identifying a property-table section.
pub const PH_TABLE_KIND: u8 = 0x01;
/// Fixed header length in bytes.
pub const PH_TABLE_HEADER_LEN: usize = 24;

/// `section_flags` bit 0 — a node-flag plane follows the value plane.
pub const SECTION_HAS_NODE_FLAGS: u8 = 0x01;

/// Node flag: the node lies inside the vapour dome.
pub const NODE_TWO_PHASE: u8 = 0x01;
/// Node flag: the node is at or above the critical pressure.
pub const NODE_SUPERCRITICAL: u8 = 0x02;
/// Node flag: the sample was non-finite and was back-filled from a neighbour.
pub const NODE_BACKFILLED: u8 = 0x04;
/// Every bit this format version defines; anything else must be zero.
pub const NODE_FLAG_MASK: u8 = NODE_TWO_PHASE | NODE_SUPERCRITICAL | NODE_BACKFILLED;

/// What an axis of a table measures. All SI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    /// Absolute pressure `P` [Pa].
    Pressure,
    /// Specific enthalpy `h` [J/kg].
    Enthalpy,
    /// Dome-following superheat `Δh = h − h_g(P)` [J/kg].
    Superheat,
    /// Dome-following subcooling `Δh = h_f(P) − h` [J/kg].
    Subcooling,
}

impl AxisKind {
    fn code(self) -> u8 {
        match self {
            AxisKind::Pressure => 0,
            AxisKind::Enthalpy => 1,
            AxisKind::Superheat => 2,
            AxisKind::Subcooling => 3,
        }
    }

    fn from_code(code: u8) -> Result<AxisKind> {
        match code {
            0 => Ok(AxisKind::Pressure),
            1 => Ok(AxisKind::Enthalpy),
            2 => Ok(AxisKind::Superheat),
            3 => Ok(AxisKind::Subcooling),
            other => Err(bad_table(format!("unknown axis kind {other}"))),
        }
    }

    /// The SI unit symbol this axis is measured in.
    pub fn unit(self) -> &'static str {
        match self {
            AxisKind::Pressure => "Pa",
            AxisKind::Enthalpy | AxisKind::Superheat | AxisKind::Subcooling => "J/kg",
        }
    }
}

/// What the node plane of a table measures. All SI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Dimensionless (quality, compressibility, …).
    Dimensionless,
    /// Temperature [K].
    Temperature,
    /// Mass density [kg/m³].
    Density,
    /// Specific enthalpy [J/kg].
    Enthalpy,
    /// Specific entropy [J/(kg·K)].
    Entropy,
    /// Pressure [Pa].
    Pressure,
    /// Specific volume [m³/kg].
    SpecificVolume,
    /// Dynamic viscosity [Pa·s].
    Viscosity,
    /// Thermal conductivity [W/(m·K)].
    Conductivity,
}

impl ValueKind {
    fn code(self) -> u8 {
        match self {
            ValueKind::Dimensionless => 0,
            ValueKind::Temperature => 1,
            ValueKind::Density => 2,
            ValueKind::Enthalpy => 3,
            ValueKind::Entropy => 4,
            ValueKind::Pressure => 5,
            ValueKind::SpecificVolume => 6,
            ValueKind::Viscosity => 7,
            ValueKind::Conductivity => 8,
        }
    }

    fn from_code(code: u8) -> Result<ValueKind> {
        match code {
            0 => Ok(ValueKind::Dimensionless),
            1 => Ok(ValueKind::Temperature),
            2 => Ok(ValueKind::Density),
            3 => Ok(ValueKind::Enthalpy),
            4 => Ok(ValueKind::Entropy),
            5 => Ok(ValueKind::Pressure),
            6 => Ok(ValueKind::SpecificVolume),
            7 => Ok(ValueKind::Viscosity),
            8 => Ok(ValueKind::Conductivity),
            other => Err(bad_table(format!("unknown value kind {other}"))),
        }
    }

    /// The SI unit symbol this plane is measured in.
    pub fn unit(self) -> &'static str {
        match self {
            ValueKind::Dimensionless => "-",
            ValueKind::Temperature => "K",
            ValueKind::Density => "kg/m^3",
            ValueKind::Enthalpy => "J/kg",
            ValueKind::Entropy => "J/(kg-K)",
            ValueKind::Pressure => "Pa",
            ValueKind::SpecificVolume => "m^3/kg",
            ValueKind::Viscosity => "Pa-s",
            ValueKind::Conductivity => "W/(m-K)",
        }
    }

    /// The CoolProp output key a generator samples for this quantity.
    ///
    /// Mass-basis keys throughout, matching what `SaturationSplitTable` asks
    /// CoolProp for (`"T"`, `"Dmass"`, `"Smass"`).
    pub fn coolprop_key(self) -> &'static str {
        match self {
            ValueKind::Dimensionless => "Q",
            ValueKind::Temperature => "T",
            ValueKind::Density => "Dmass",
            ValueKind::Enthalpy => "Hmass",
            ValueKind::Entropy => "Smass",
            ValueKind::Pressure => "P",
            ValueKind::SpecificVolume => "Dmass",
            ValueKind::Viscosity => "V",
            ValueKind::Conductivity => "L",
        }
    }
}

/// How a table cell relates to the vapour dome, from its corner node flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellPhase {
    /// No corner is marked two-phase (or the table carries no flags).
    SinglePhase,
    /// All four corners are inside the dome.
    TwoPhase,
    /// Some corners are inside the dome and some are not — a bicubic here
    /// smears the `h_f`/`h_g` kink.
    DomeCrossing,
}

/// A property value with its analytic partials at a query point.
///
/// Port of the Java record `PhPropertyTable.Value`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Value {
    /// The interpolated property.
    pub value: f64,
    /// `∂value/∂P` in the first axis' unit⁻¹.
    pub d_value_d_p: f64,
    /// `∂value/∂h` in the second axis' unit⁻¹.
    pub d_value_d_h: f64,
}

fn bad_table(msg: impl Into<String>) -> FreesError {
    FreesError::property(format!("(P,h) property table: {}", msg.into()))
}

/// A piecewise-bicubic Hermite surface over a structured `(P, h)` grid.
#[derive(Debug, Clone, PartialEq)]
pub struct PhPropertyTable {
    /// Strictly increasing first-axis nodes.
    p_grid: Vec<f64>,
    /// Strictly increasing second-axis nodes.
    h_grid: Vec<f64>,
    /// Nodal values, row-major: `f[i * nh + j]`.
    f: Vec<f64>,
    /// `∂f/∂P` at the nodes.
    fp: Vec<f64>,
    /// `∂f/∂h` at the nodes.
    fh: Vec<f64>,
    /// `∂²f/∂P∂h` at the nodes.
    fph: Vec<f64>,
    /// Node marks, row-major, or empty when the table carries none.
    node_flags: Vec<u8>,
    axis_p_kind: AxisKind,
    axis_h_kind: AxisKind,
    value_kind: ValueKind,
}

impl PhPropertyTable {
    /// Builds a table by sampling `sampler(P, h)` over the given
    /// strictly-increasing grids.
    ///
    /// Nodal derivatives are estimated once, at build time, by
    /// grid-spacing-aware finite differences — that is what makes the resulting
    /// Hermite surface C¹. Any non-finite sample is back-filled from its
    /// nearest finite neighbour (the near-critical safe-fallback).
    ///
    /// Port of `PhPropertyTable.build`.
    pub fn build(
        p_grid: &[f64],
        h_grid: &[f64],
        mut sampler: impl FnMut(f64, f64) -> f64,
    ) -> Result<PhPropertyTable> {
        validate_grid(p_grid, "P")?;
        validate_grid(h_grid, "h")?;
        let np = p_grid.len();
        let nh = h_grid.len();
        let mut f = vec![0.0f64; np * nh];
        for i in 0..np {
            for j in 0..nh {
                f[i * nh + j] = sampler(p_grid[i], h_grid[j]);
            }
        }
        Ok(Self::from_nodes_unchecked(
            p_grid.to_vec(),
            h_grid.to_vec(),
            f,
            Vec::new(),
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Dimensionless,
        ))
    }

    /// Builds a table from an already-sampled node plane.
    ///
    /// This is the entry point a generator uses when it has the samples in hand
    /// (and the one [`decode`](Self::decode) funnels through). `values` is
    /// row-major, `values[i * h_grid.len() + j] = prop(p_grid[i], h_grid[j])`.
    /// `node_flags`, when non-empty, must have the same length and ordering.
    ///
    /// Non-finite values are back-filled exactly as [`build`](Self::build)
    /// does, and every node so filled gets [`NODE_BACKFILLED`] set when a flag
    /// plane is present.
    pub fn from_nodes(
        p_grid: Vec<f64>,
        h_grid: Vec<f64>,
        values: Vec<f64>,
        node_flags: Vec<u8>,
        axis_p_kind: AxisKind,
        axis_h_kind: AxisKind,
        value_kind: ValueKind,
    ) -> Result<PhPropertyTable> {
        validate_grid(&p_grid, "P")?;
        validate_grid(&h_grid, "h")?;
        let want = p_grid.len() * h_grid.len();
        if values.len() != want {
            return Err(bad_table(format!(
                "value plane must hold {want} nodes ({} x {}), found {}",
                p_grid.len(),
                h_grid.len(),
                values.len()
            )));
        }
        if !node_flags.is_empty() && node_flags.len() != want {
            return Err(bad_table(format!(
                "node-flag plane must hold {want} nodes, found {}",
                node_flags.len()
            )));
        }
        if let Some(bad) = node_flags.iter().find(|b| **b & !NODE_FLAG_MASK != 0) {
            return Err(bad_table(format!(
                "node flag byte {bad:#04x} sets bits this format version reserves"
            )));
        }
        Ok(Self::from_nodes_unchecked(
            p_grid,
            h_grid,
            values,
            node_flags,
            axis_p_kind,
            axis_h_kind,
            value_kind,
        ))
    }

    fn from_nodes_unchecked(
        p_grid: Vec<f64>,
        h_grid: Vec<f64>,
        mut f: Vec<f64>,
        mut node_flags: Vec<u8>,
        axis_p_kind: AxisKind,
        axis_h_kind: AxisKind,
        value_kind: ValueKind,
    ) -> PhPropertyTable {
        let np = p_grid.len();
        let nh = h_grid.len();
        fill_non_finite(&mut f, np, nh, &mut node_flags);
        let mut fp = vec![0.0f64; np * nh];
        let mut fh = vec![0.0f64; np * nh];
        let mut fph = vec![0.0f64; np * nh];
        for i in 0..np {
            for j in 0..nh {
                fp[i * nh + j] = nodal_partial(&f, nh, &p_grid, i, j, true);
                fh[i * nh + j] = nodal_partial(&f, nh, &h_grid, i, j, false);
            }
        }
        // Cross derivative as the P-difference of the h-derivative field.
        for i in 0..np {
            for j in 0..nh {
                fph[i * nh + j] = nodal_partial(&fh, nh, &p_grid, i, j, true);
            }
        }
        PhPropertyTable {
            p_grid,
            h_grid,
            f,
            fp,
            fh,
            fph,
            node_flags,
            axis_p_kind,
            axis_h_kind,
            value_kind,
        }
    }

    /// Re-labels the axis and value kinds without touching the surface.
    ///
    /// [`build`](Self::build) has no way to know what it sampled; a generator
    /// (or `satsplit`, which knows which piece it just built) says so here.
    pub fn with_kinds(
        mut self,
        axis_p_kind: AxisKind,
        axis_h_kind: AxisKind,
        value_kind: ValueKind,
    ) -> PhPropertyTable {
        self.axis_p_kind = axis_p_kind;
        self.axis_h_kind = axis_h_kind;
        self.value_kind = value_kind;
        self
    }

    /// Evaluates the property and its analytic partials at `(P, h)`.
    ///
    /// Queries outside the grid **clamp** to the boundary cell; the surface is
    /// never extrapolated. Port of `PhPropertyTable.eval`.
    pub fn eval(&self, p: f64, h: f64) -> Value {
        let nh = self.h_grid.len();
        let i = locate(&self.p_grid, p);
        let j = locate(&self.h_grid, h);
        let p0 = self.p_grid[i];
        let p1 = self.p_grid[i + 1];
        let h0 = self.h_grid[j];
        let h1 = self.h_grid[j + 1];
        let dp = p1 - p0;
        let dh = h1 - h0;
        let u = clamp01((p - p0) / dp);
        let v = clamp01((h - h0) / dh);

        // Hermite basis (value/tangent) and their derivatives in u and v.
        let bu = hermite(u);
        let bv = hermite(v);
        let du = hermite_deriv(u);
        let dv = hermite_deriv(v);

        let mut val = 0.0;
        let mut d_val_du = 0.0;
        let mut d_val_dv = 0.0;
        // corner a in {0,1} along P, b in {0,1} along h
        for a in 0..2 {
            for b in 0..2 {
                let c = (i + a) * nh + (j + b);
                let fab = self.f[c];
                let fuab = self.fp[c] * dp; // tangent scaled to the unit cell
                let fvab = self.fh[c] * dh;
                let fuvab = self.fph[c] * dp * dh;
                // The value-basis index for this corner is a (h00/h01); the
                // tangent is a+2 (h10/h11).
                let b_u = bu[a];
                let t_u = bu[a + 2];
                let b_v = bv[b];
                let t_v = bv[b + 2];
                let db_u = du[a];
                let dt_u = du[a + 2];
                let db_v = dv[b];
                let dt_v = dv[b + 2];

                val += fab * b_u * b_v + fuab * t_u * b_v + fvab * b_u * t_v + fuvab * t_u * t_v;
                d_val_du +=
                    fab * db_u * b_v + fuab * dt_u * b_v + fvab * db_u * t_v + fuvab * dt_u * t_v;
                d_val_dv +=
                    fab * b_u * db_v + fuab * t_u * db_v + fvab * b_u * dt_v + fuvab * t_u * dt_v;
            }
        }
        Value {
            value: val,
            d_value_d_p: d_val_du / dp,
            d_value_d_h: d_val_dv / dh,
        }
    }

    /// Convenience: value only. Port of `PhPropertyTable.value`.
    pub fn value(&self, p: f64, h: f64) -> f64 {
        self.eval(p, h).value
    }

    /// The first-axis nodes.
    pub fn p_grid(&self) -> &[f64] {
        &self.p_grid
    }

    /// The second-axis nodes.
    pub fn h_grid(&self) -> &[f64] {
        &self.h_grid
    }

    /// The nodal value at `(i, j)`, after back-filling.
    pub fn node_value(&self, i: usize, j: usize) -> f64 {
        self.f[i * self.h_grid.len() + j]
    }

    /// The node marks at `(i, j)`, or `0` when the table carries none.
    pub fn node_flags(&self, i: usize, j: usize) -> u8 {
        if self.node_flags.is_empty() {
            0
        } else {
            self.node_flags[i * self.h_grid.len() + j]
        }
    }

    /// Whether this table carries a node-flag plane at all.
    ///
    /// A table without one asserts nothing about phase; see the module docs.
    pub fn has_node_flags(&self) -> bool {
        !self.node_flags.is_empty()
    }

    /// How the cell whose lower corner is `(i, j)` relates to the vapour dome.
    ///
    /// `i` must be `< n_p - 1` and `j < n_h - 1`.
    pub fn cell_flags(&self, i: usize, j: usize) -> CellPhase {
        if self.node_flags.is_empty() {
            return CellPhase::SinglePhase;
        }
        let inside = |a: usize, b: usize| self.node_flags(a, b) & NODE_TWO_PHASE != 0;
        let corners = [
            inside(i, j),
            inside(i + 1, j),
            inside(i, j + 1),
            inside(i + 1, j + 1),
        ];
        if corners.iter().all(|c| *c) {
            CellPhase::TwoPhase
        } else if corners.iter().any(|c| *c) {
            CellPhase::DomeCrossing
        } else {
            CellPhase::SinglePhase
        }
    }

    /// What the first axis measures.
    pub fn axis_p_kind(&self) -> AxisKind {
        self.axis_p_kind
    }

    /// What the second axis measures.
    pub fn axis_h_kind(&self) -> AxisKind {
        self.axis_h_kind
    }

    /// What the node plane measures.
    pub fn value_kind(&self) -> ValueKind {
        self.value_kind
    }

    /// The exact encoded length of this table in bytes.
    pub fn encoded_len(&self) -> usize {
        let n = self.p_grid.len() * self.h_grid.len();
        PH_TABLE_HEADER_LEN
            + 8 * self.p_grid.len()
            + 8 * self.h_grid.len()
            + 8 * n
            + if self.node_flags.is_empty() { 0 } else { n }
    }

    /// Serialises the table as a `FREESPH1` section (see the module docs).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_len());
        self.encode_into(&mut out);
        out
    }

    /// Appends a `FREESPH1` section to `out`, so a bundle can hold several.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(PH_TABLE_MAGIC);
        out.push(PH_TABLE_KIND);
        out.push(if self.node_flags.is_empty() {
            0
        } else {
            SECTION_HAS_NODE_FLAGS
        });
        out.push(self.axis_p_kind.code());
        out.push(self.axis_h_kind.code());
        out.push(self.value_kind.code());
        out.extend_from_slice(&[0u8; 3]); // reserved
        out.extend_from_slice(&(self.p_grid.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.h_grid.len() as u32).to_le_bytes());
        for x in &self.p_grid {
            out.extend_from_slice(&x.to_le_bytes());
        }
        for x in &self.h_grid {
            out.extend_from_slice(&x.to_le_bytes());
        }
        for x in &self.f {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&self.node_flags);
    }

    /// Reads a `FREESPH1` section that spans exactly `bytes`.
    ///
    /// This is what the wasm build calls on an `include_bytes!` slice.
    pub fn decode(bytes: &[u8]) -> Result<PhPropertyTable> {
        let (table, used) = PhPropertyTable::decode_prefix(bytes)?;
        if used != bytes.len() {
            return Err(bad_table(format!(
                "{} trailing bytes after the section",
                bytes.len() - used
            )));
        }
        Ok(table)
    }

    /// Reads a `FREESPH1` section from the start of `bytes`, returning the
    /// table and how many bytes it consumed.
    pub fn decode_prefix(bytes: &[u8]) -> Result<(PhPropertyTable, usize)> {
        if bytes.len() < PH_TABLE_HEADER_LEN {
            return Err(bad_table(format!(
                "need at least {PH_TABLE_HEADER_LEN} header bytes, found {}",
                bytes.len()
            )));
        }
        if &bytes[0..8] != PH_TABLE_MAGIC {
            return Err(bad_table(
                "bad magic — this is not a FREESPH1 property-table section",
            ));
        }
        if bytes[8] != PH_TABLE_KIND {
            return Err(bad_table(format!(
                "section kind {:#04x} is not a property table",
                bytes[8]
            )));
        }
        let section_flags = bytes[9];
        if section_flags & !SECTION_HAS_NODE_FLAGS != 0 {
            return Err(bad_table(format!(
                "section flags {section_flags:#04x} set bits this format version reserves"
            )));
        }
        let axis_p_kind = AxisKind::from_code(bytes[10])?;
        let axis_h_kind = AxisKind::from_code(bytes[11])?;
        let value_kind = ValueKind::from_code(bytes[12])?;
        if bytes[13..16] != [0, 0, 0] {
            return Err(bad_table("reserved header bytes must be zero"));
        }
        let np = read_u32(bytes, 16) as usize;
        let nh = read_u32(bytes, 20) as usize;
        if np < 2 || nh < 2 {
            return Err(bad_table(format!(
                "grid needs at least 2 nodes per axis, header says {np} x {nh}"
            )));
        }
        let nodes = np
            .checked_mul(nh)
            .ok_or_else(|| bad_table("grid size overflows"))?;
        let flags_len = if section_flags & SECTION_HAS_NODE_FLAGS != 0 {
            nodes
        } else {
            0
        };
        let total = PH_TABLE_HEADER_LEN
            .checked_add(8 * np)
            .and_then(|n| n.checked_add(8 * nh))
            .and_then(|n| nodes.checked_mul(8).and_then(|m| n.checked_add(m)))
            .and_then(|n| n.checked_add(flags_len))
            .ok_or_else(|| bad_table("declared size overflows"))?;
        if bytes.len() < total {
            return Err(bad_table(format!(
                "section declares {total} bytes but only {} are present",
                bytes.len()
            )));
        }

        let mut at = PH_TABLE_HEADER_LEN;
        let p_grid = read_f64_block(bytes, &mut at, np);
        let h_grid = read_f64_block(bytes, &mut at, nh);
        let values = read_f64_block(bytes, &mut at, nodes);
        let node_flags = bytes[at..at + flags_len].to_vec();
        at += flags_len;
        debug_assert_eq!(at, total);

        let table = PhPropertyTable::from_nodes(
            p_grid,
            h_grid,
            values,
            node_flags,
            axis_p_kind,
            axis_h_kind,
            value_kind,
        )?;
        Ok((table, total))
    }
}

// ---------------------------------------------------------------------------
// Hermite basis on [0,1], ordered [h00, h01, h10, h11]
// ---------------------------------------------------------------------------
//
// h00 = value at the left corner, h01 = value at the right corner,
// h10 = tangent at the left corner, h11 = tangent at the right corner.

fn hermite(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        2.0 * t3 - 3.0 * t2 + 1.0, // h00
        -2.0 * t3 + 3.0 * t2,      // h01
        t3 - 2.0 * t2 + t,         // h10
        t3 - t2,                   // h11
    ]
}

fn hermite_deriv(t: f64) -> [f64; 4] {
    let t2 = t * t;
    [
        6.0 * t2 - 6.0 * t,       // h00'
        -6.0 * t2 + 6.0 * t,      // h01'
        3.0 * t2 - 4.0 * t + 1.0, // h10'
        3.0 * t2 - 2.0 * t,       // h11'
    ]
}

/// Grid-spacing-aware central/one-sided finite difference along one axis.
///
/// `g` is a row-major `np x nh` plane with row stride `nh`; `along_p` selects
/// the first axis. Port of the private `PhPropertyTable.partial`.
fn nodal_partial(g: &[f64], nh: usize, axis: &[f64], i: usize, j: usize, along_p: bool) -> f64 {
    let n = axis.len();
    let k = if along_p { i } else { j };
    let here = g[i * nh + j];
    if k == 0 {
        let fwd = if along_p {
            g[(i + 1) * nh + j]
        } else {
            g[i * nh + j + 1]
        };
        return (fwd - here) / (axis[1] - axis[0]);
    }
    if k == n - 1 {
        let back = if along_p {
            g[(i - 1) * nh + j]
        } else {
            g[i * nh + j - 1]
        };
        return (here - back) / (axis[n - 1] - axis[n - 2]);
    }
    // Non-uniform central difference.
    let back = if along_p {
        g[(i - 1) * nh + j]
    } else {
        g[i * nh + j - 1]
    };
    let fwd = if along_p {
        g[(i + 1) * nh + j]
    } else {
        g[i * nh + j + 1]
    };
    let h_prev = axis[k] - axis[k - 1];
    let h_next = axis[k + 1] - axis[k];
    central_non_uniform(back, here, fwd, h_prev, h_next)
}

/// Standard second-order non-uniform central difference.
fn central_non_uniform(back: f64, here: f64, fwd: f64, h_prev: f64, h_next: f64) -> f64 {
    let a = -h_next / (h_prev * (h_prev + h_next));
    let b = (h_next - h_prev) / (h_prev * h_next);
    let c = h_prev / (h_next * (h_prev + h_next));
    a * back + b * here + c * fwd
}

/// Replaces every non-finite node with its nearest finite neighbour, marking
/// each one it touched when a flag plane is present.
///
/// Port of the private `PhPropertyTable.fillNonFinite`. The scan order is
/// load-bearing: the plane is mutated in place, so a node filled early can be
/// the "nearest finite neighbour" of a later one — the Java does the same, and
/// the resulting surface depends on it.
fn fill_non_finite(f: &mut [f64], np: usize, nh: usize, node_flags: &mut [u8]) {
    for i in 0..np {
        for j in 0..nh {
            if !f[i * nh + j].is_finite() {
                f[i * nh + j] = nearest_finite(f, i, j, np, nh);
                if !node_flags.is_empty() {
                    node_flags[i * nh + j] |= NODE_BACKFILLED;
                }
            }
        }
    }
}

/// The first finite value found scanning outward from `(i, j)` in square blocks
/// of growing radius, or `0.0` if the whole plane is non-finite.
///
/// Port of the private `PhPropertyTable.nearestFinite`, including its scan
/// order (`di` then `dj`, each from `-r` to `r`, over the full square rather
/// than the ring).
fn nearest_finite(f: &[f64], i: usize, j: usize, np: usize, nh: usize) -> f64 {
    let max = np.max(nh) as isize;
    for r in 1..max {
        for di in -r..=r {
            for dj in -r..=r {
                let ni = i as isize + di;
                let nj = j as isize + dj;
                if ni >= 0 && ni < np as isize && nj >= 0 && nj < nh as isize {
                    let v = f[ni as usize * nh + nj as usize];
                    if v.is_finite() {
                        return v;
                    }
                }
            }
        }
    }
    0.0
}

/// The index of the cell containing `q`, clamped into `[0, len-2]`.
///
/// Port of the private `PhPropertyTable.locate`.
fn locate(g: &[f64], q: f64) -> usize {
    if q <= g[0] {
        return 0;
    }
    if q >= g[g.len() - 1] {
        return g.len() - 2;
    }
    let mut lo = 0usize;
    let mut hi = g.len() - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if g[mid] <= q {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn clamp01(t: f64) -> f64 {
    // `Math.max(0.0, Math.min(1.0, t))` in the Java, which **propagates NaN**
    // (`Math.min`/`Math.max` both return NaN if either operand is NaN). Rust's
    // `f64::clamp` propagates it too, so a NaN query still yields NaN rather
    // than silently snapping to a grid corner. Do not "fix" this to 0.0.
    t.clamp(0.0, 1.0)
}

/// `n` evenly spaced points from `a` to `b` inclusive.
///
/// Port of the private `PhPropertyTable.linspace`.
pub fn linspace(a: f64, b: f64, n: usize) -> Result<Vec<f64>> {
    if n < 2 {
        return Err(bad_table("grid needs at least 2 points"));
    }
    let mut g = vec![0.0f64; n];
    for i in 0..n {
        g[i] = a + (b - a) * i as f64 / (n - 1) as f64;
    }
    Ok(g)
}

/// Port of the private `PhPropertyTable.validateGrid`.
fn validate_grid(g: &[f64], name: &str) -> Result<()> {
    if g.len() < 2 {
        return Err(bad_table(format!("{name} grid needs at least 2 points")));
    }
    for i in 1..g.len() {
        if !(g[i] > g[i - 1]) {
            // Negated on purpose: a NaN node must be rejected, and `<=` would
            // accept it.
            return Err(bad_table(format!(
                "{name} grid must be strictly increasing"
            )));
        }
    }
    Ok(())
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(buf)
}

fn read_f64_block(bytes: &[u8], at: &mut usize, n: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[*at..*at + 8]);
        out.push(f64::from_le_bytes(buf));
        *at += 8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lin(a: f64, b: f64, n: usize) -> Vec<f64> {
        linspace(a, b, n).unwrap()
    }

    // -- the Java PhPropertyTableTest, ported ------------------------------

    #[test]
    fn reproduces_nodal_values_exactly() {
        let p = lin(1e5, 1e7, 21);
        let h = lin(1e5, 5e5, 21);
        let t = PhPropertyTable::build(&p, &h, |pp, hh| 3.0 + 2e-6 * pp + 4e-6 * hh).unwrap();
        for pp in &p {
            for hh in &h {
                let want = 3.0 + 2e-6 * pp + 4e-6 * hh;
                assert!((t.value(*pp, *hh) - want).abs() < 1e-6, "{pp} {hh}");
            }
        }
    }

    #[test]
    fn reproduces_bilinear_function_and_its_derivatives() {
        // f = a + b*p + c*h + d*p*h — a bicubic Hermite with FD nodal tangents
        // is exact for this.
        let (a, b, c, d) = (1.0, 2e-6, 5e-6, 1e-12);
        let p = lin(1e5, 1e7, 17);
        let h = lin(1e5, 5e5, 17);
        let t = PhPropertyTable::build(&p, &h, |pp, hh| a + b * pp + c * hh + d * pp * hh).unwrap();

        let pq = 3.21e6;
        let hq = 2.34e5;
        let v = t.eval(pq, hq);
        assert!((v.value - (a + b * pq + c * hq + d * pq * hq)).abs() < 1e-6);
        assert!((v.d_value_d_p - (b + d * hq)).abs() < 1e-12);
        assert!((v.d_value_d_h - (c + d * pq)).abs() < 1e-12);
    }

    #[test]
    fn analytic_partials_match_finite_difference_on_a_smooth_surface() {
        let f = |pp: f64, hh: f64| (pp / 2.0e6).sin() * (hh / 2.0e5).cos();
        let p = lin(1e5, 1e7, 81);
        let h = lin(1e5, 5e5, 81);
        let t = PhPropertyTable::build(&p, &h, f).unwrap();

        let pq = 4.0e6;
        let hq = 3.0e5;
        let v = t.eval(pq, hq);
        let dp = 1e3;
        let dh = 1e2;
        let fd_p = (t.value(pq + dp, hq) - t.value(pq - dp, hq)) / (2.0 * dp);
        let fd_h = (t.value(pq, hq + dh) - t.value(pq, hq - dh)) / (2.0 * dh);
        assert!((v.d_value_d_p - fd_p).abs() < 1e-9, "{v:?} vs {fd_p}");
        assert!((v.d_value_d_h - fd_h).abs() < 1e-9, "{v:?} vs {fd_h}");
        assert!((v.value - f(pq, hq)).abs() < 1e-3);
    }

    #[test]
    fn clamps_outside_the_grid() {
        let p = lin(1e5, 1e7, 11);
        let h = lin(1e5, 5e5, 11);
        let t = PhPropertyTable::build(&p, &h, |pp, hh| pp + hh).unwrap();
        assert!((t.value(-1.0, -1.0) - (1e5 + 1e5)).abs() < 1e-3);
        assert!((t.value(1e9, 1e9) - (1e7 + 5e5)).abs() < 1e-3);
    }

    #[test]
    fn backfills_non_finite_samples_into_a_smooth_surface() {
        let p = lin(1e5, 1e7, 21);
        let h = lin(1e5, 5e5, 21);
        let t = PhPropertyTable::build(&p, &h, |pp, hh| {
            if pp > 4e6 && pp < 6e6 && hh > 2e5 && hh < 3e5 {
                f64::NAN
            } else {
                10.0 + 1e-6 * pp
            }
        })
        .unwrap();
        for pp in lin(1e5, 1e7, 50) {
            for hh in lin(1e5, 5e5, 50) {
                assert!(t.value(pp, hh).is_finite(), "({pp}, {hh})");
            }
        }
    }

    // -- kernel details ---------------------------------------------------

    #[test]
    fn hermite_basis_is_a_partition_at_the_corners() {
        assert_eq!(hermite(0.0), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(hermite(1.0), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(hermite_deriv(0.0), [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(hermite_deriv(1.0), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn locate_clamps_and_brackets() {
        let g = [0.0, 1.0, 2.0, 3.0];
        assert_eq!(locate(&g, -5.0), 0);
        assert_eq!(locate(&g, 0.0), 0);
        assert_eq!(locate(&g, 0.5), 0);
        assert_eq!(locate(&g, 1.0), 1);
        assert_eq!(locate(&g, 2.5), 2);
        assert_eq!(locate(&g, 3.0), 2);
        assert_eq!(locate(&g, 99.0), 2);
    }

    #[test]
    fn non_uniform_grids_are_supported() {
        // A geometric P axis and a quadratic h axis — the shapes satsplit uses.
        let p: Vec<f64> = (0..24).map(|i| 1e5 * 1.25f64.powi(i)).collect();
        let h: Vec<f64> = (0..24)
            .map(|i| {
                let t = i as f64 / 23.0;
                5e5 * t * t
            })
            .collect();
        // h[0] == h[1] == 0 would break strict monotonicity; shift off zero.
        let h: Vec<f64> = h.iter().map(|x| x + 1.0e3).collect();
        let f = |pp: f64, hh: f64| 3.0 + 1e-7 * pp + 2e-6 * hh + 1e-13 * pp * hh;
        let t = PhPropertyTable::build(&p, &h, f).unwrap();
        // Bilinear is reproduced exactly on any grid spacing.
        let pq = 3.7e6;
        let hq = 2.2e5;
        let v = t.eval(pq, hq);
        assert!((v.value - f(pq, hq)).abs() < 1e-6, "{v:?}");
        assert!((v.d_value_d_p - (1e-7 + 1e-13 * hq)).abs() < 1e-14, "{v:?}");
        assert!((v.d_value_d_h - (2e-6 + 1e-13 * pq)).abs() < 1e-14, "{v:?}");
    }

    #[test]
    fn degenerate_grids_are_rejected() {
        let ok = lin(0.0, 1.0, 4);
        assert!(PhPropertyTable::build(&[1.0], &ok, |_, _| 0.0).is_err());
        assert!(PhPropertyTable::build(&ok, &[1.0], |_, _| 0.0).is_err());
        assert!(PhPropertyTable::build(&[0.0, 0.0], &ok, |_, _| 0.0).is_err());
        assert!(PhPropertyTable::build(&[1.0, 0.0], &ok, |_, _| 0.0).is_err());
        assert!(PhPropertyTable::build(&[0.0, f64::NAN], &ok, |_, _| 0.0).is_err());
        assert!(linspace(0.0, 1.0, 1).is_err());
    }

    #[test]
    fn a_fully_non_finite_plane_falls_back_to_zero() {
        let g = lin(0.0, 1.0, 3);
        let t = PhPropertyTable::build(&g, &g, |_, _| f64::NAN).unwrap();
        assert_eq!(t.value(0.5, 0.5), 0.0);
    }

    // -- node flags -------------------------------------------------------

    #[test]
    fn cell_flags_classify_the_dome() {
        let p = lin(1.0, 3.0, 3);
        let h = lin(1.0, 3.0, 3);
        // Mark the whole j = 0 column two-phase.
        let mut flags = vec![0u8; 9];
        for i in 0..3 {
            flags[i * 3] = NODE_TWO_PHASE;
        }
        let t = PhPropertyTable::from_nodes(
            p,
            h,
            vec![1.0; 9],
            flags,
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .unwrap();
        assert!(t.has_node_flags());
        assert_eq!(t.cell_flags(0, 0), CellPhase::DomeCrossing);
        assert_eq!(t.cell_flags(1, 0), CellPhase::DomeCrossing);
        assert_eq!(t.cell_flags(0, 1), CellPhase::SinglePhase);
    }

    #[test]
    fn an_all_two_phase_cell_is_reported_as_such() {
        let g = lin(1.0, 2.0, 2);
        let t = PhPropertyTable::from_nodes(
            g.clone(),
            g,
            vec![1.0; 4],
            vec![NODE_TWO_PHASE; 4],
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Temperature,
        )
        .unwrap();
        assert_eq!(t.cell_flags(0, 0), CellPhase::TwoPhase);
    }

    #[test]
    fn backfilled_nodes_are_marked_when_a_flag_plane_exists() {
        let g = lin(0.0, 2.0, 3);
        let mut values = vec![7.0; 9];
        values[4] = f64::NAN; // the centre node
        let t = PhPropertyTable::from_nodes(
            g.clone(),
            g,
            values,
            vec![0u8; 9],
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .unwrap();
        assert_eq!(t.node_flags(1, 1) & NODE_BACKFILLED, NODE_BACKFILLED);
        assert_eq!(t.node_flags(0, 0), 0);
        assert_eq!(t.node_value(1, 1), 7.0);
    }

    #[test]
    fn a_table_without_flags_reports_single_phase_and_zero_marks() {
        let g = lin(0.0, 1.0, 3);
        let t = PhPropertyTable::build(&g, &g, |a, b| a + b).unwrap();
        assert!(!t.has_node_flags());
        assert_eq!(t.node_flags(2, 2), 0);
        assert_eq!(t.cell_flags(0, 0), CellPhase::SinglePhase);
    }

    #[test]
    fn reserved_flag_bits_are_rejected() {
        let g = lin(0.0, 1.0, 2);
        let err = PhPropertyTable::from_nodes(
            g.clone(),
            g,
            vec![0.0; 4],
            vec![0x80, 0, 0, 0],
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .unwrap_err();
        assert!(matches!(err, FreesError::Property { .. }), "{err}");
    }

    #[test]
    fn mismatched_plane_lengths_are_rejected() {
        let g = lin(0.0, 1.0, 3);
        assert!(PhPropertyTable::from_nodes(
            g.clone(),
            g.clone(),
            vec![0.0; 8],
            Vec::new(),
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .is_err());
        assert!(PhPropertyTable::from_nodes(
            g.clone(),
            g,
            vec![0.0; 9],
            vec![0u8; 4],
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .is_err());
    }

    // -- grounded in real CoolProp data ------------------------------------
    //
    // The Java's `matchesCoolPropDensityInSuperheatedRegion` builds this table
    // from the native library at test time. Wasm cannot, so the nodes below are
    // the *same* CoolProp values, pulled through `tools/golden-dumper` against
    // the Java engine (`Density(R134a, P=…, h=…)`) and pasted in. That makes
    // this the one test here that checks the interpolant against real physics
    // rather than an analytic surface.

    /// Superheated R134a density [kg/m³] on a 9x9 grid,
    /// P = 5 bar … 20 bar, h = 430 … 520 kJ/kg, row-major (P outer).
    const R134A_D: [[f64; 9]; 9] = [
        [
            21.590045886602393,
            20.51697662881405,
            19.581210754971607,
            18.755632682058945,
            18.020120157709812,
            17.35928762590622,
            16.761314623624358,
            16.216751618029164,
            15.718092780466023,
        ],
        [
            30.41412719292106,
            28.80556887253222,
            27.417174708259175,
            26.202737199045487,
            25.128465672528538,
            24.169164065031158,
            23.305601711489565,
            22.52271193768259,
            21.80859308560074,
        ],
        [
            39.676500527357845,
            37.44628214870719,
            35.5410835631309,
            33.88870067683449,
            32.43744362547471,
            31.149285971689576,
            29.995663822931967,
            28.954396736338175,
            28.00831053906976,
        ],
        [
            49.40613599301391,
            46.45804531592226,
            43.96547623268103,
            41.82204169639046,
            39.952848856381884,
            38.30362866285491,
            36.83413047588201,
            35.51366423716841,
            34.31847430569117,
        ],
        [
            59.633955623771875,
            55.86051198059161,
            52.703235965222255,
            50.01137846532784,
            47.68043298048334,
            45.636141336091676,
            43.823745124480176,
            42.202302397796316,
            40.74009684706917,
        ],
        [
            70.39080229636309,
            65.67281736736321,
            61.76655660000405,
            58.464406287544364,
            55.62531632195909,
            53.149965466846325,
            50.96653029691536,
            49.02145262401091,
            47.274067787038554,
        ],
        [
            81.71022608100142,
            75.9156158867477,
            71.16864743492191,
            67.1900151289967,
            63.793215805202415,
            60.84912532315589,
            58.26509737183527,
            55.972850075431566,
            53.92122530560676,
        ],
        [
            93.62084294058774,
            86.60564217596081,
            80.9192707549752,
            76.19379620675883,
            72.18768662208036,
            68.73546484157474,
            65.72021603578222,
            63.05695592210188,
            60.681574345723085,
        ],
        [
            106.15230172156777,
            97.760570667928,
            91.02943619509709,
            85.48277655351514,
            80.81270356045438,
            76.81146796690464,
            73.33361339052762,
            70.27442515522732,
            67.55560288717487,
        ],
    ];

    /// Off-grid `(P, h, rho)` probes from the same CoolProp source.
    const R134A_PROBES: &[(f64, f64, f64)] = &[
        (1100000.0, 480000.0, 40.68851440931156),
        (730000.0, 455000.0, 28.927038341548865),
        (1620000.0, 502000.0, 57.12373571569075),
        (990000.0, 441000.0, 42.98251386483073),
        (1850000.0, 490000.0, 69.2545949596724),
    ];

    #[test]
    fn matches_coolprop_density_in_the_superheated_region() {
        let p = lin(5e5, 2e6, 9);
        let h = lin(4.3e5, 5.2e5, 9);
        let values: Vec<f64> = R134A_D.iter().flat_map(|row| row.iter().copied()).collect();
        let t = PhPropertyTable::from_nodes(
            p.clone(),
            h.clone(),
            values,
            Vec::new(),
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .unwrap();

        // Nodes are reproduced exactly — the Hermite basis is interpolating.
        for i in 0..9 {
            for j in 0..9 {
                assert_eq!(t.value(p[i], h[j]), R134A_D[i][j], "node ({i}, {j})");
            }
        }

        // Off-grid, a 9x9 grid over this box holds ~1%. (The Java samples
        // 41x41 and asserts 2%; a coarser grid is the honest trade a browser
        // bundle makes, and this pins the error it actually costs.)
        let mut worst: f64 = 0.0;
        for (pq, hq, exact) in R134A_PROBES {
            let got = t.value(*pq, *hq);
            let err = (got - exact).abs() / exact;
            worst = worst.max(err);
            assert!(err < 1.0e-2, "rho({pq}, {hq}) = {got}, CoolProp {exact}");
        }
        assert!(worst > 1e-6, "the probes must actually be off-grid");

        // The analytic partials are the derivatives of that same surface, and
        // density falls with enthalpy / rises with pressure in superheat.
        let v = t.eval(1.1e6, 4.8e5);
        assert!(v.d_value_d_p > 0.0, "{v:?}");
        assert!(v.d_value_d_h < 0.0, "{v:?}");
        // A central difference of the interpolant carries O(dp^2/cell^2)
        // truncation, so this is a scaling/chain-rule check, not an exactness
        // one — `analytic_partials_match_finite_difference_on_a_smooth_surface`
        // is where the tight version lives.
        let fd_p = (t.value(1.1e6 + 1e3, 4.8e5) - t.value(1.1e6 - 1e3, 4.8e5)) / 2e3;
        let fd_h = (t.value(1.1e6, 4.8e5 + 1e2) - t.value(1.1e6, 4.8e5 - 1e2)) / 2e2;
        assert!(
            (v.d_value_d_p - fd_p).abs() < 1e-3 * fd_p.abs(),
            "{v:?} vs {fd_p}"
        );
        assert!(
            (v.d_value_d_h - fd_h).abs() < 1e-3 * fd_h.abs(),
            "{v:?} vs {fd_h}"
        );
    }

    #[test]
    fn a_coolprop_grounded_table_survives_the_wire_format() {
        let p = lin(5e5, 2e6, 9);
        let h = lin(4.3e5, 5.2e5, 9);
        let values: Vec<f64> = R134A_D.iter().flat_map(|row| row.iter().copied()).collect();
        let t = PhPropertyTable::from_nodes(
            p,
            h,
            values,
            Vec::new(),
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        )
        .unwrap();
        let back = PhPropertyTable::decode(&t.encode()).unwrap();
        for (pq, hq, exact) in R134A_PROBES {
            assert_eq!(back.value(*pq, *hq), t.value(*pq, *hq));
            assert!((back.value(*pq, *hq) - exact).abs() / exact < 1.0e-2);
        }
    }

    // -- the on-disk format ------------------------------------------------

    fn sample_table(with_flags: bool) -> PhPropertyTable {
        let p: Vec<f64> = (0..9).map(|i| 1e5 * 1.7f64.powi(i)).collect();
        let h = lin(2.0e5, 5.0e5, 7);
        let values: Vec<f64> = p
            .iter()
            .flat_map(|pp| {
                h.iter()
                    .map(move |hh| 300.0 + 1e-5 * pp + 2e-4 * hh + 1e-11 * pp * hh)
            })
            .collect();
        let flags = if with_flags {
            let mut f = vec![0u8; p.len() * h.len()];
            f[0] = NODE_TWO_PHASE;
            f[1] = NODE_TWO_PHASE | NODE_SUPERCRITICAL;
            f
        } else {
            Vec::new()
        };
        PhPropertyTable::from_nodes(
            p,
            h,
            values,
            flags,
            AxisKind::Pressure,
            AxisKind::Superheat,
            ValueKind::Temperature,
        )
        .unwrap()
    }

    #[test]
    fn header_layout_is_what_the_documentation_says() {
        let t = sample_table(true);
        let bytes = t.encode();
        assert_eq!(&bytes[0..8], PH_TABLE_MAGIC);
        assert_eq!(bytes[8], PH_TABLE_KIND);
        assert_eq!(bytes[9], SECTION_HAS_NODE_FLAGS);
        assert_eq!(bytes[10], AxisKind::Pressure.code());
        assert_eq!(bytes[11], AxisKind::Superheat.code());
        assert_eq!(bytes[12], ValueKind::Temperature.code());
        assert_eq!(&bytes[13..16], &[0, 0, 0]);
        assert_eq!(read_u32(&bytes, 16), 9);
        assert_eq!(read_u32(&bytes, 20), 7);
        // The first f64 after the header is p_grid[0].
        let mut at = PH_TABLE_HEADER_LEN;
        let first = read_f64_block(&bytes, &mut at, 1);
        assert_eq!(first[0], 1e5);
        assert_eq!(bytes.len(), t.encoded_len());
        assert_eq!(bytes.len(), 24 + 8 * 9 + 8 * 7 + 8 * 63 + 63);
    }

    #[test]
    fn encode_decode_round_trips_bit_for_bit() {
        for with_flags in [false, true] {
            let t = sample_table(with_flags);
            let bytes = t.encode();
            let back = PhPropertyTable::decode(&bytes).unwrap();
            assert_eq!(t, back, "with_flags = {with_flags}");
            assert_eq!(back.encode(), bytes);
            // and the surface it serves is identical
            assert_eq!(t.eval(3.3e5, 3.1e5), back.eval(3.3e5, 3.1e5));
        }
    }

    #[test]
    fn row_major_ordering_is_p_outer_h_inner() {
        let t = sample_table(false);
        let bytes = t.encode();
        let np = t.p_grid().len();
        let nh = t.h_grid().len();
        let mut at = PH_TABLE_HEADER_LEN + 8 * np + 8 * nh;
        let values = read_f64_block(&bytes, &mut at, np * nh);
        for i in 0..np {
            for j in 0..nh {
                assert_eq!(values[i * nh + j], t.node_value(i, j), "({i}, {j})");
            }
        }
    }

    #[test]
    fn decode_rejects_corrupt_sections() {
        let good = sample_table(true).encode();

        assert!(PhPropertyTable::decode(&good[..10]).is_err());

        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[8] = 0x02; // wrong section kind
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[9] = 0x40; // reserved section flag
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[10] = 9; // unknown axis kind
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[12] = 40; // unknown value kind
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[14] = 1; // reserved header byte
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[16..20].copy_from_slice(&1u32.to_le_bytes()); // n_p < 2
        assert!(PhPropertyTable::decode(&bad).is_err());

        let mut bad = good.clone();
        bad[20..24].copy_from_slice(&9999u32.to_le_bytes()); // truncated body
        assert!(PhPropertyTable::decode(&bad).is_err());

        // A non-increasing axis must be refused, not silently interpolated.
        let mut bad = good.clone();
        let at = PH_TABLE_HEADER_LEN + 8;
        bad[at..at + 8].copy_from_slice(&0.0f64.to_le_bytes());
        assert!(PhPropertyTable::decode(&bad).is_err());

        // Trailing bytes are an error for `decode`...
        let mut extra = good.clone();
        extra.push(0);
        assert!(PhPropertyTable::decode(&extra).is_err());
        // ...but `decode_prefix` reports how far it read.
        let (_, used) = PhPropertyTable::decode_prefix(&extra).unwrap();
        assert_eq!(used, good.len());
    }

    #[test]
    fn kinds_survive_a_round_trip_and_name_their_units() {
        let t = sample_table(false);
        let back = PhPropertyTable::decode(&t.encode()).unwrap();
        assert_eq!(back.axis_p_kind(), AxisKind::Pressure);
        assert_eq!(back.axis_h_kind(), AxisKind::Superheat);
        assert_eq!(back.value_kind(), ValueKind::Temperature);
        assert_eq!(back.axis_p_kind().unit(), "Pa");
        assert_eq!(back.axis_h_kind().unit(), "J/kg");
        assert_eq!(back.value_kind().unit(), "K");
        assert_eq!(back.value_kind().coolprop_key(), "T");
        let relabelled =
            back.with_kinds(AxisKind::Pressure, AxisKind::Enthalpy, ValueKind::Density);
        assert_eq!(relabelled.value_kind().coolprop_key(), "Dmass");
    }

    #[test]
    fn a_decoded_table_serves_the_same_surface_as_the_builder() {
        let p = lin(1e5, 1e7, 25);
        let h = lin(1e5, 5e5, 25);
        let f = |pp: f64, hh: f64| (pp / 3.0e6).sin() * (1.0 + hh / 1.0e6);
        let built = PhPropertyTable::build(&p, &h, f).unwrap().with_kinds(
            AxisKind::Pressure,
            AxisKind::Enthalpy,
            ValueKind::Density,
        );
        let decoded = PhPropertyTable::decode(&built.encode()).unwrap();
        for pq in [1.5e5, 2.2e6, 6.66e6, 9.9e6] {
            for hq in [1.1e5, 2.5e5, 4.9e5] {
                assert_eq!(built.eval(pq, hq), decoded.eval(pq, hq), "({pq}, {hq})");
            }
        }
    }
}
