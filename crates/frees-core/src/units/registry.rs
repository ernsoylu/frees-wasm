//! Engineering unit table and unit-expression parser.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/units/UnitRegistry.java`
//! (588 LOC).
//!
//! Rules honoured here (from the Java class doc):
//!
//! * dash, star, dot or space **multiply** units on the same side of a divisor —
//!   `-` is never a minus sign inside a unit expression;
//! * every `/` opens a new denominator factor, so `W/m^2/K` reads as
//!   `W/(m^2·K)` — engineering shorthand, and how the correlation rules and the
//!   property-package units are written;
//! * exponents may be written with or without `^` (`m^2` == `m2`), and may be
//!   fractional (`m^1.5`);
//! * unit names are case-insensitive, apart from the handful in
//!   [`CASE_SENSITIVE_UNITS`] where case carries meaning (`H` henry vs `h` hour);
//! * `-` on its own (and the empty string) is the explicit dimensionless marker.
//!
//! Everything the engine computes is in SI; a unit is therefore just a
//! multiplicative factor plus a dimension vector ([`Quantity`]), except for the
//! two absolute temperature scales, which also carry an additive offset
//! ([`OffsetQuantity`]).

use crate::diag::{FreesError, Result};
use crate::units::quantity::{Dims, OffsetQuantity, Quantity, BASE_SYMBOLS, DIMENSIONS};

/// Tolerance for comparing dimension exponents, matching the Java `1e-9`.
const DIM_EPS: f64 = 1e-9;

/// `K = (F + 459.67) * 5/9`, so the additive part of the Fahrenheit scale is
/// `459.67 * 5/9`. Port of `UnitRegistry.FAHRENHEIT_OFFSET_K`.
pub const FAHRENHEIT_OFFSET_K: f64 = 459.67 * 5.0 / 9.0;

/// Kelvin offset of the Celsius scale.
pub const CELSIUS_OFFSET_K: f64 = 273.15;

// ---------------------------------------------------------------------------
// Dimension vectors — index order is [kg, m, s, K, mol, A, cd]
// ---------------------------------------------------------------------------

/// Widen a leading slice of exponents into a full dimension vector, exactly as
/// the Java `define(String, double, double...)` varargs helper does.
const fn dims_from(exponents: &[f64]) -> Dims {
    let mut out = [0.0; DIMENSIONS];
    let mut i = 0;
    while i < exponents.len() && i < DIMENSIONS {
        out[i] = exponents[i];
        i += 1;
    }
    out
}

const DIMENSIONLESS: Dims = dims_from(&[]);
const MASS: Dims = dims_from(&[1.0]);
const LENGTH: Dims = dims_from(&[0.0, 1.0]);
const AREA: Dims = dims_from(&[0.0, 2.0]);
const VOLUME: Dims = dims_from(&[0.0, 3.0]);
const TIME: Dims = dims_from(&[0.0, 0.0, 1.0]);
const TEMPERATURE: Dims = dims_from(&[0.0, 0.0, 0.0, 1.0]);
const AMOUNT: Dims = dims_from(&[0.0, 0.0, 0.0, 0.0, 1.0]);
const CURRENT: Dims = dims_from(&[0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
const LUMINOUS: Dims = dims_from(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
const FORCE: Dims = dims_from(&[1.0, 1.0, -2.0]);
const PRESSURE: Dims = dims_from(&[1.0, -1.0, -2.0]);
const ENERGY: Dims = dims_from(&[1.0, 2.0, -2.0]);
const POWER: Dims = dims_from(&[1.0, 2.0, -3.0]);
const FREQUENCY: Dims = dims_from(&[0.0, 0.0, -1.0]);
const VOLTAGE: Dims = dims_from(&[1.0, 2.0, -3.0, 0.0, 0.0, -1.0]);
const RESISTANCE: Dims = dims_from(&[1.0, 2.0, -3.0, 0.0, 0.0, -2.0]);
const CAPACITANCE: Dims = dims_from(&[-1.0, -2.0, 4.0, 0.0, 0.0, 2.0]);
const INDUCTANCE: Dims = dims_from(&[1.0, 2.0, -2.0, 0.0, 0.0, -2.0]);
const CHARGE: Dims = dims_from(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
const CONDUCTANCE: Dims = dims_from(&[-1.0, -2.0, 3.0, 0.0, 0.0, 2.0]);
const MAGNETIC_FLUX: Dims = dims_from(&[1.0, 2.0, -2.0, 0.0, 0.0, -1.0]);
const FLUX_DENSITY: Dims = dims_from(&[1.0, 0.0, -2.0, 0.0, 0.0, -1.0]);
/// Dynamic viscosity, `Pa·s = kg/(m·s)`.
const DYNAMIC_VISCOSITY: Dims = dims_from(&[1.0, -1.0, -1.0]);
/// Kinematic viscosity, `m^2/s`.
const KINEMATIC_VISCOSITY: Dims = dims_from(&[0.0, 2.0, -1.0]);

/// The dimension of an absolute temperature (and of a temperature difference —
/// they are indistinguishable, which is why offsets are opt-in).
const TEMPERATURE_DIMS: Dims = TEMPERATURE;

/// The energy/torque dimension `kg·m²·s⁻²` — shared by joules and moments.
const MOMENT_DIMS: Dims = ENERGY;

// ---------------------------------------------------------------------------
// The unit table
// ---------------------------------------------------------------------------

/// `(symbol, factor to SI, dimensions)`.
///
/// Symbols are stored **lowercased**, matching the Java `UNITS` map, whose keys
/// go through `String.toLowerCase()`; lookups lowercase the user's spelling
/// before probing this table. (That is why the ohm appears as `ω`: Java stores
/// `"Ω".toLowerCase()`.)
pub static UNITS: &[(&str, f64, Dims)] = &[
    // ---- Dimensionless ----------------------------------------------------
    ("1", 1.0, DIMENSIONLESS),
    ("rad", 1.0, DIMENSIONLESS),
    ("deg", core::f64::consts::PI / 180.0, DIMENSIONLESS),
    ("db", 1.0, DIMENSIONLESS),
    // ---- Mass -------------------------------------------------------------
    ("kg", 1.0, MASS),
    ("g", 1e-3, MASS),
    ("mg", 1e-6, MASS),
    ("lbm", 0.45359237, MASS),
    ("lb", 0.45359237, MASS),
    ("lbs", 0.45359237, MASS),
    ("slug", 14.59390294, MASS),
    ("tonne", 1000.0, MASS),
    // ---- Length -----------------------------------------------------------
    ("m", 1.0, LENGTH),
    ("cm", 0.01, LENGTH),
    ("mm", 0.001, LENGTH),
    ("km", 1000.0, LENGTH),
    ("in", 0.0254, LENGTH),
    ("inch", 0.0254, LENGTH),
    ("ft", 0.3048, LENGTH),
    ("yd", 0.9144, LENGTH),
    ("mile", 1609.344, LENGTH),
    // ---- Time -------------------------------------------------------------
    ("s", 1.0, TIME),
    ("sec", 1.0, TIME),
    ("secs", 1.0, TIME),
    ("second", 1.0, TIME),
    ("seconds", 1.0, TIME),
    ("ms", 1e-3, TIME),
    ("millisecond", 1e-3, TIME),
    ("milliseconds", 1e-3, TIME),
    ("us", 1e-6, TIME),
    ("microsecond", 1e-6, TIME),
    ("microseconds", 1e-6, TIME),
    ("ns", 1e-9, TIME),
    ("nanosecond", 1e-9, TIME),
    ("nanoseconds", 1e-9, TIME),
    ("min", 60.0, TIME),
    ("mins", 60.0, TIME),
    ("minute", 60.0, TIME),
    ("minutes", 60.0, TIME),
    ("hr", 3600.0, TIME),
    ("hrs", 3600.0, TIME),
    ("hour", 3600.0, TIME),
    ("hours", 3600.0, TIME),
    ("day", 86400.0, TIME),
    ("days", 86400.0, TIME),
    ("week", 604800.0, TIME),
    ("weeks", 604800.0, TIME),
    ("year", 3.1536e7, TIME),
    ("years", 3.1536e7, TIME),
    ("yr", 3.1536e7, TIME),
    // ---- Temperature (multiplicative scale only; see `parse_with_offset`) --
    ("k", 1.0, TEMPERATURE),
    ("c", 1.0, TEMPERATURE),
    ("r", 5.0 / 9.0, TEMPERATURE),
    ("f", 5.0 / 9.0, TEMPERATURE),
    // ---- Amount / current / luminosity ------------------------------------
    ("mol", 1.0, AMOUNT),
    ("kmol", 1000.0, AMOUNT),
    ("a", 1.0, CURRENT),
    ("ma", 1e-3, CURRENT),
    ("cd", 1.0, LUMINOUS),
    // ---- Force: kg·m/s² ---------------------------------------------------
    ("n", 1.0, FORCE),
    ("kn", 1e3, FORCE),
    ("mn", 1e6, FORCE),
    ("lbf", 4.4482216152605, FORCE),
    ("dyne", 1e-5, FORCE),
    // ---- Pressure: kg/(m·s²) ----------------------------------------------
    ("pa", 1.0, PRESSURE),
    ("kpa", 1e3, PRESSURE),
    ("mpa", 1e6, PRESSURE),
    ("gpa", 1e9, PRESSURE),
    ("bar", 1e5, PRESSURE),
    ("atm", 101325.0, PRESSURE),
    ("psi", 6894.757293168, PRESSURE),
    ("psia", 6894.757293168, PRESSURE),
    ("torr", 133.3223684, PRESSURE),
    ("mmhg", 133.3223684, PRESSURE),
    // ---- Energy: kg·m²/s² -------------------------------------------------
    ("j", 1.0, ENERGY),
    ("kj", 1e3, ENERGY),
    ("mj", 1e6, ENERGY),
    ("btu", 1055.05585262, ENERGY),
    ("cal", 4.1868, ENERGY),
    ("kcal", 4186.8, ENERGY),
    ("kwh", 3.6e6, ENERGY),
    // ---- Power: kg·m²/s³ --------------------------------------------------
    ("w", 1.0, POWER),
    ("kw", 1e3, POWER),
    ("mw", 1e6, POWER),
    ("hp", 745.69987158, POWER),
    // ---- Volume -----------------------------------------------------------
    ("l", 1e-3, VOLUME),
    ("liter", 1e-3, VOLUME),
    ("ml", 1e-6, VOLUME),
    ("gal", 0.003785411784, VOLUME),
    // ---- Frequency: Hz = s⁻¹ ----------------------------------------------
    ("hz", 1.0, FREQUENCY),
    ("hertz", 1.0, FREQUENCY),
    ("khz", 1e3, FREQUENCY),
    ("mhz", 1e6, FREQUENCY),
    ("ghz", 1e9, FREQUENCY),
    // Angular velocity: rad/s (rad is dimensionless, so this is s⁻¹).
    // rpm = revolutions per minute = 2π rad / 60 s.
    ("rpm", 2.0 * core::f64::consts::PI / 60.0, FREQUENCY),
    // ---- Electrical -------------------------------------------------------
    // Voltage: V = kg·m²·s⁻³·A⁻¹
    ("v", 1.0, VOLTAGE),
    ("kv", 1e3, VOLTAGE),
    ("mv", 1e-3, VOLTAGE),
    // Resistance: Ω = kg·m²·s⁻³·A⁻²
    ("ω", 1.0, RESISTANCE),
    ("ohm", 1.0, RESISTANCE),
    ("ohms", 1.0, RESISTANCE),
    ("kohm", 1e3, RESISTANCE),
    ("mohm", 1e6, RESISTANCE),
    // Capacitance: F = s⁴·A²·kg⁻¹·m⁻²
    ("farad", 1.0, CAPACITANCE),
    ("uf", 1e-6, CAPACITANCE),
    ("nf", 1e-9, CAPACITANCE),
    ("pf", 1e-12, CAPACITANCE),
    // Inductance: H = kg·m²·s⁻²·A⁻² (bare `H` lives in CASE_SENSITIVE_UNITS)
    ("henry", 1.0, INDUCTANCE),
    ("mh", 1e-3, INDUCTANCE),
    ("uh", 1e-6, INDUCTANCE),
    // Charge: C = A·s
    ("coulomb", 1.0, CHARGE),
    ("couloumb", 1.0, CHARGE),
    ("uc", 1e-6, CHARGE),
    ("nc", 1e-9, CHARGE),
    ("pc", 1e-12, CHARGE),
    // Conductance: S = A²·s³·kg⁻¹·m⁻²
    ("siemens", 1.0, CONDUCTANCE),
    ("siemes", 1.0, CONDUCTANCE),
    ("msiemens", 1e-3, CONDUCTANCE),
    ("usiemens", 1e-6, CONDUCTANCE),
    // Magnetic flux: Wb = kg·m²·s⁻²·A⁻¹
    ("wb", 1.0, MAGNETIC_FLUX),
    ("weber", 1.0, MAGNETIC_FLUX),
    ("mwb", 1e-3, MAGNETIC_FLUX),
    // Magnetic flux density: T = kg·s⁻²·A⁻¹
    ("tesla", 1.0, FLUX_DENSITY),
    ("t", 1.0, FLUX_DENSITY),
    ("mt", 1e-3, FLUX_DENSITY),
    ("ut", 1e-6, FLUX_DENSITY),
    // ---- Viscosity (extension over the Java table; see module deviations) --
    // Dynamic: 1 P = 0.1 Pa·s, 1 cP = 1 mPa·s.
    ("poise", 0.1, DYNAMIC_VISCOSITY),
    ("centipoise", 1e-3, DYNAMIC_VISCOSITY),
    ("cpoise", 1e-3, DYNAMIC_VISCOSITY),
    ("cp", 1e-3, DYNAMIC_VISCOSITY),
    ("micropoise", 1e-7, DYNAMIC_VISCOSITY),
    // Kinematic: 1 St = 1e-4 m²/s, 1 cSt = 1e-6 m²/s.
    ("stokes", 1e-4, KINEMATIC_VISCOSITY),
    ("centistokes", 1e-6, KINEMATIC_VISCOSITY),
    ("cstokes", 1e-6, KINEMATIC_VISCOSITY),
    ("cst", 1e-6, KINEMATIC_VISCOSITY),
];

/// SI symbols whose meaning depends on letter case (`H` henry vs `h` hour).
/// Probed before [`UNITS`], with the user's spelling untouched.
pub static CASE_SENSITIVE_UNITS: &[(&str, f64, Dims)] =
    &[("h", 3600.0, TIME), ("H", 1.0, INDUCTANCE)];

/// Named SI units, tried in order, that [`UnitRegistry::si_name`] prefers over a
/// composed `kg m^2/s^3` style string.
static NAMED_SI_UNITS: &[(&str, Dims)] = &[
    ("N", FORCE),
    ("Pa", PRESSURE),
    ("J", ENERGY),
    ("W", POWER),
    // Heat-capacity rate / thermal conductance, common in heat transfer.
    ("W/K", dims_from(&[1.0, 2.0, -3.0, -1.0])),
    // Convective heat-transfer coefficient and heat flux — the htc/q'' that
    // pervade HX sizing; without these they print as kg/s^3-K and kg/s^3.
    ("W/m^2-K", dims_from(&[1.0, 0.0, -3.0, -1.0])),
    ("W/m^2", dims_from(&[1.0, 0.0, -3.0])),
    // Engineering composites common in thermodynamics output.
    ("J/kg", dims_from(&[0.0, 2.0, -2.0])),
    ("J/kg-K", dims_from(&[0.0, 2.0, -2.0, -1.0])),
    ("W/m-K", dims_from(&[1.0, 1.0, -3.0, -1.0])),
    ("Pa-s", DYNAMIC_VISCOSITY),
    ("m/s", dims_from(&[0.0, 1.0, -1.0])),
    ("kg/m^3", dims_from(&[1.0, -3.0])),
    ("m^3/kg", dims_from(&[-1.0, 3.0])),
    ("V", VOLTAGE),
    ("Ω", RESISTANCE),
    ("Hz", FREQUENCY),
    ("Coulomb", CHARGE),
    ("Siemens", CONDUCTANCE),
    ("Farad", CAPACITANCE),
    ("Henry", INDUCTANCE),
    ("Wb", MAGNETIC_FLUX),
    ("T", FLUX_DENSITY),
];

/// Angular-rate units (s⁻¹) that engineers expect displayed as rad/s, not Hz.
static ANGULAR_RATE_UNITS: &[&str] = &[
    "rpm",
    "rad/s",
    "rad/sec",
    "radian/s",
    "radians/s",
    "rad/min",
    "rad/h",
    "rad/hr",
    "rad/hour",
];

/// Newton-metre *moment/torque* spellings the engineer expects kept as `N-m`
/// rather than canonicalised to the dimensionally-identical joule. Compared
/// after stripping spaces and lowercasing, so `N-m`, `N·m`, `N m` all match —
/// but *not* the explicit-multiply form `N*m` (kept as `n*m`), which is read as
/// a product and still reduces to J.
static MOMENT_UNITS: &[&str] = &[
    "n-m",
    "n·m",
    "n.m",
    "nm",
    "newton-meter",
    "newton-metre",
    "newtonmeter",
    "newtonmetre",
];

/// True when two dimension vectors agree to within [`DIM_EPS`].
fn dims_match(a: &Dims, b: &Dims) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (x - y).abs() <= DIM_EPS)
}

/// True when a dimension vector is all (near) zero.
fn is_dimensionless(dims: &Dims) -> bool {
    dims.iter().all(|d| d.abs() <= DIM_EPS)
}

// ---------------------------------------------------------------------------
// Display unit systems
// ---------------------------------------------------------------------------

/// Which display-unit family a solution is rendered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSystem {
    Si,
    EngSi,
    English,
}

/// A preferred display unit for a dimension: `display = (si - offset) / factor`.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayUnit {
    pub name: String,
    pub factor: f64,
    pub offset: f64,
    pub dims: Dims,
}

impl DisplayUnit {
    /// Convert an SI value into this display unit.
    pub fn from_si(&self, si: f64) -> f64 {
        (si - self.offset) / self.factor
    }

    /// Convert a value written in this display unit back to SI.
    pub fn to_si(&self, value: f64) -> f64 {
        value * self.factor + self.offset
    }
}

/// Temperature is deliberately absent from both display tables: a temperature
/// difference is dimensionally identical to an absolute temperature, so a
/// blanket affine C/F conversion would corrupt deltas (a 75 K difference is not
/// -198.15 C). Absolute display in C/F is opt-in per variable.
static ENG_SI_DISPLAY: &[(&str, f64, f64, Dims)] = &[
    ("kPa", 1e3, 0.0, PRESSURE),
    ("kJ", 1e3, 0.0, ENERGY),
    ("kW", 1e3, 0.0, POWER),
];

static ENGLISH_DISPLAY: &[(&str, f64, f64, Dims)] = &[
    ("psi", 6894.757293168, 0.0, PRESSURE),
    ("Btu", 1055.05585262, 0.0, ENERGY),
    ("hp", 745.69987158, 0.0, POWER),
    ("lbf", 4.4482216152605, 0.0, FORCE),
    ("lbm", 0.45359237, 0.0, MASS),
    ("ft", 0.3048, 0.0, LENGTH),
    ("ft^2", 0.09290304, 0.0, AREA),
    ("ft^3", 0.028316846592, 0.0, VOLUME),
    ("lbm/ft^3", 16.01846337396014, 0.0, dims_from(&[1.0, -3.0])),
    ("ft/s", 0.3048, 0.0, dims_from(&[0.0, 1.0, -1.0])),
    ("ft/s^2", 0.3048, 0.0, dims_from(&[0.0, 1.0, -2.0])),
];

/// One entry in the reference table exposed to the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitInfo {
    pub symbol: String,
    pub dimension: String,
    pub si_factor: f64,
}

/// The unit table.
#[derive(Debug, Default)]
pub struct UnitRegistry;

impl UnitRegistry {
    /// Parse a unit expression (`kJ/kg-K`, `m^3/s`, `lbm/ft^3`) into a scale
    /// factor and dimension exponents. Purely multiplicative — see
    /// [`UnitRegistry::parse_with_offset`] for temperature scales.
    pub fn parse(_unit: &str) -> Result<Quantity> {
        let text = _unit.trim();
        // `-` and the empty string are the explicit dimensionless markers.
        if text.is_empty() || text == "-" {
            return Ok(Quantity::dimensionless(1.0));
        }
        let mut parser = UnitExprParser::new(text);
        let quantity = parser.parse_expr()?;
        if !parser.at_end() {
            // A stray ')' (or anything else `parse_expr` refuses to consume).
            return Err(parser.malformed());
        }
        Ok(quantity)
    }

    /// Like [`UnitRegistry::parse`] but retains the additive offset that `C`
    /// and `F` need. Conversion to SI is `value * factor + offset`.
    ///
    /// Only the *bare* scales carry an offset: a compound expression such as
    /// `kJ/kg-C` is a per-degree (delta) unit and stays multiplicative.
    pub fn parse_with_offset(_unit: &str) -> Result<OffsetQuantity> {
        let text = _unit.trim().to_lowercase();
        if text == "c" {
            return Ok(OffsetQuantity::new(1.0, CELSIUS_OFFSET_K, TEMPERATURE_DIMS));
        }
        if text == "f" {
            return Ok(OffsetQuantity::new(
                5.0 / 9.0,
                FAHRENHEIT_OFFSET_K,
                TEMPERATURE_DIMS,
            ));
        }
        Ok(UnitRegistry::parse(_unit)?.into())
    }

    /// The canonical SI display name for a parsed unit, e.g. `kPa` → `Pa`.
    ///
    /// Angular-rate units (rpm, rad/s) are dimensionally identical to frequency
    /// (s⁻¹) because radians are dimensionless, so [`UnitRegistry::si_name`]
    /// alone would canonicalise them to `Hz`; this preserves the engineer's
    /// intent. Likewise a moment written `N-m` stays `N-m` rather than becoming
    /// the dimensionally-equal `J`.
    pub fn si_display_name(_unit: &str, _dims: &crate::units::Dims) -> String {
        let normalized = _unit.trim().to_lowercase().replace(' ', "");
        if ANGULAR_RATE_UNITS.contains(&normalized.as_str()) {
            return "rad/s".to_string();
        }
        // The dims guard keeps a true nanometre, were "nm" ever a length, safe.
        if dims_match(_dims, &MOMENT_DIMS) && MOMENT_UNITS.contains(&normalized.as_str()) {
            return "N-m".to_string();
        }
        UnitRegistry::si_name(_dims)
    }

    /// Canonical SI unit string for a dimension vector: a named unit (`Pa`, `N`,
    /// `J`, `W`, …) where one matches, otherwise a composed, re-parseable
    /// expression such as `kg/m-s^2` or `m/s^2`. Dimensionless yields `-`.
    pub fn si_name(dims: &Dims) -> String {
        if is_dimensionless(dims) {
            return "-".to_string();
        }
        for (symbol, named) in NAMED_SI_UNITS {
            if dims_match(dims, named) {
                return (*symbol).to_string();
            }
        }

        let mut numerator = String::new();
        let mut denominator = String::new();
        for i in 0..DIMENSIONS {
            let e = dims[i];
            if e.abs() <= DIM_EPS {
                continue;
            }
            let positive = e > 0.0;
            let target = if positive {
                &mut numerator
            } else {
                &mut denominator
            };
            if !target.is_empty() {
                target.push(if positive { ' ' } else { '-' });
            }
            target.push_str(BASE_SYMBOLS[i]);
            let abs = e.abs();
            if (abs - 1.0).abs() > DIM_EPS {
                target.push('^');
                if abs == abs.round() {
                    target.push_str(&(abs as i64).to_string());
                } else {
                    target.push_str(&abs.to_string());
                }
            }
        }
        if denominator.is_empty() {
            return numerator;
        }
        let head = if numerator.is_empty() {
            "1"
        } else {
            numerator.as_str()
        };
        format!("{head}/{denominator}")
    }

    /// Every known unit, for `/api/reference`. Sorted by dimension, then symbol,
    /// exactly as the Java `listUnits()`.
    pub fn all_units() -> Vec<UnitInfo> {
        let mut out: Vec<UnitInfo> = UNITS
            .iter()
            .chain(CASE_SENSITIVE_UNITS.iter())
            .map(|(symbol, factor, dims)| UnitInfo {
                symbol: (*symbol).to_string(),
                dimension: UnitRegistry::si_name(dims),
                si_factor: *factor,
            })
            .collect();
        out.sort_by(|a, b| {
            a.dimension
                .cmp(&b.dimension)
                .then_with(|| a.symbol.cmp(&b.symbol))
        });
        out
    }

    /// `Convert('From', 'To')`: the multiplicative factor between two unit
    /// expressions.
    pub fn convert(from: &str, to: &str) -> Result<f64> {
        let source = UnitRegistry::parse(from)?;
        let target = UnitRegistry::parse(to)?;
        if !source.same_dimensions_as(&target) {
            return Err(FreesError::evaluation(format!(
                "Convert({from}, {to}): units have different dimensions [{}] vs [{}].",
                source.dimension_string(),
                target.dimension_string()
            )));
        }
        Ok(source.factor / target.factor)
    }

    /// Preferred display unit for a dimension in the given system; `None` means
    /// "keep the SI form as-is".
    pub fn preferred_display_unit(dims: &Dims, system: UnitSystem) -> Option<DisplayUnit> {
        let table: &[(&str, f64, f64, Dims)] = match system {
            UnitSystem::Si => &[],
            UnitSystem::EngSi => ENG_SI_DISPLAY,
            UnitSystem::English => ENGLISH_DISPLAY,
        };
        table
            .iter()
            .find(|(_, _, _, candidate)| dims_match(dims, candidate))
            .map(|(name, factor, offset, candidate)| DisplayUnit {
                name: (*name).to_string(),
                factor: *factor,
                offset: *offset,
                dims: *candidate,
            })
    }

    /// Whether a single unit *name* (not expression) is in the table.
    pub fn is_known_unit(name: &str) -> bool {
        lookup(name).is_ok()
    }
}

// ---------------------------------------------------------------------------
// Unit-expression parser
// ---------------------------------------------------------------------------

/// Look one unit name up: case-sensitive table first, then the lowercased one.
fn lookup(name: &str) -> Result<Quantity> {
    if let Some((_, factor, dims)) = CASE_SENSITIVE_UNITS.iter().find(|(n, _, _)| *n == name) {
        return Ok(Quantity::new(*factor, *dims));
    }
    let lower = name.to_lowercase();
    if let Some((_, factor, dims)) = UNITS.iter().find(|(n, _, _)| *n == lower) {
        return Ok(Quantity::new(*factor, *dims));
    }
    Err(FreesError::UnknownUnit {
        unit: name.to_string(),
    })
}

/// Characters that multiply two factors on the same side of a divisor. The
/// dash is the important one: inside a unit string it is *never* a minus.
fn is_separator(c: char) -> bool {
    c == '-'
        || c == '*'
        || c == '\u{b7}'
        || c == '\u{2219}'
        || c == '\u{22c5}'
        || c == '.'
        || c.is_whitespace()
}

/// Recursive-descent parser over the characters of one unit expression.
///
/// ```text
/// expr   := term ('/' term)*          // every '/' opens a denominator factor
/// term   := factor (sep+ factor)*     // sep = '-' | '*' | '·' | '.' | space
/// factor := (name | '(' expr ')' | '1') exponent?
/// exp    := '^' ws* ('-' | '+')? number | digits
/// ```
struct UnitExprParser {
    chars: Vec<char>,
    pos: usize,
    /// Open parentheses currently being parsed, bounded by [`MAX_UNIT_DEPTH`].
    depth: u32,
}

/// Maximum parenthesis nesting inside one unit expression.
///
/// `parse_factor` recurses into `parse_expr` for `( … )`, so an annotation like
/// `[((((…m…))))]` recurses once per paren and overflows the stack — an abort,
/// not a diagnostic. Real unit expressions nest one or two levels
/// (`kJ/(kg-K)`); 32 is generous and keeps the recursion bounded.
const MAX_UNIT_DEPTH: u32 = 32;

impl UnitExprParser {
    fn new(text: &str) -> UnitExprParser {
        UnitExprParser {
            chars: text.chars().collect(),
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// The whole (trimmed) expression, used as the payload of a syntax error —
    /// `FreesError::UnknownUnit` has nowhere else to put the context.
    fn malformed(&self) -> FreesError {
        FreesError::UnknownUnit {
            unit: self.chars.iter().collect(),
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    /// Consume a run of multiplication separators; reports whether any were
    /// present (two adjacent factors *must* be separated).
    fn skip_separators(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_separator(c)) {
            self.pos += 1;
        }
        self.pos > start
    }

    /// A term ends at the end of input, at a '/', or at a closing paren.
    fn term_ended(&self) -> bool {
        matches!(self.peek(), None | Some('/') | Some(')'))
    }

    fn parse_expr(&mut self) -> Result<Quantity> {
        let mut result = self.parse_term()?;
        while self.peek() == Some('/') {
            self.bump();
            let divisor = self.parse_term()?;
            result = result.divide(&divisor);
        }
        Ok(result)
    }

    fn parse_term(&mut self) -> Result<Quantity> {
        let mut product = Quantity::dimensionless(1.0);
        // Leading and trailing separators are noise, as they are in the Java
        // `split` (which simply drops the empty tokens they produce).
        self.skip_separators();
        while !self.term_ended() {
            let factor = self.parse_factor()?;
            product = product.multiply(&factor);
            let separated = self.skip_separators();
            if self.term_ended() {
                break;
            }
            if !separated {
                // e.g. "m^2K" — the Java regex would not match the token either.
                return Err(self.malformed());
            }
        }
        Ok(product)
    }

    fn parse_factor(&mut self) -> Result<Quantity> {
        let c = match self.peek() {
            Some(c) => c,
            None => return Err(self.malformed()),
        };

        let base = if c == '(' {
            if self.depth >= MAX_UNIT_DEPTH {
                return Err(self.malformed());
            }
            self.bump();
            self.depth += 1;
            let inner = self.parse_expr();
            self.depth -= 1;
            let inner = inner?;
            if self.peek() != Some(')') {
                return Err(self.malformed());
            }
            self.bump();
            inner
        } else if c.is_alphabetic() {
            let start = self.pos;
            while matches!(self.peek(), Some(ch) if ch.is_alphabetic()) {
                self.pos += 1;
            }
            let name: String = self.chars[start..self.pos].iter().collect();
            lookup(&name)?
        } else if c.is_ascii_digit() {
            // Only a bare `1` is meaningful (a no-op factor); any other bare
            // number is a syntax error, exactly as in the Java parser.
            if self.read_number() == Some(1.0) {
                Quantity::dimensionless(1.0)
            } else {
                return Err(self.malformed());
            }
        } else {
            return Err(self.malformed());
        };

        match self.parse_exponent()? {
            Some(exponent) => Ok(base.powf(exponent)),
            None => Ok(base),
        }
    }

    /// `^3`, `^-1`, `^1.5` or the caret-less `3` shorthand.
    fn parse_exponent(&mut self) -> Result<Option<f64>> {
        match self.peek() {
            Some('^') => {
                self.bump();
                self.skip_whitespace();
                let sign = match self.peek() {
                    Some('-') => {
                        self.bump();
                        -1.0
                    }
                    Some('+') => {
                        self.bump();
                        1.0
                    }
                    _ => 1.0,
                };
                match self.read_number() {
                    Some(value) => Ok(Some(sign * value)),
                    None => Err(self.malformed()),
                }
            }
            Some(c) if c.is_ascii_digit() => match self.read_number() {
                Some(value) => Ok(Some(value)),
                None => Err(self.malformed()),
            },
            _ => Ok(None),
        }
    }

    /// An unsigned `\d+(\.\d+)?`.
    fn read_number(&mut self) -> Option<f64> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        // A '.' only belongs to the number when digits follow it; otherwise it
        // is a multiplication separator ("N.m").
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(unit: &str) -> Quantity {
        UnitRegistry::parse(unit).unwrap_or_else(|e| panic!("parse({unit:?}) failed: {e}"))
    }

    fn factor(unit: &str) -> f64 {
        q(unit).factor
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
            "expected {expected}, got {actual}"
        );
    }

    fn assert_dims(unit: &str, expected: Dims) {
        let got = q(unit).dims;
        assert!(
            dims_match(&got, &expected),
            "unit {unit:?}: expected dims {expected:?}, got {got:?}"
        );
    }

    fn unknown(unit: &str) -> String {
        match UnitRegistry::parse(unit) {
            Err(FreesError::UnknownUnit { unit }) => unit,
            other => panic!("expected UnknownUnit for {unit:?}, got {other:?}"),
        }
    }

    // -- base and prefixed units -------------------------------------------

    #[test]
    fn si_base_units_are_unit_factors() {
        for (unit, dims) in [
            ("kg", MASS),
            ("m", LENGTH),
            ("s", TIME),
            ("K", TEMPERATURE),
            ("mol", AMOUNT),
            ("A", CURRENT),
            ("cd", LUMINOUS),
        ] {
            assert_eq!(factor(unit), 1.0, "{unit}");
            assert_dims(unit, dims);
        }
    }

    #[test]
    fn metric_prefixes_scale_the_factor() {
        assert_eq!(factor("km"), 1000.0);
        assert_eq!(factor("cm"), 0.01);
        assert_eq!(factor("mm"), 0.001);
        assert_eq!(factor("g"), 1e-3);
        assert_eq!(factor("mg"), 1e-6);
        assert_eq!(factor("ms"), 1e-3);
        assert_eq!(factor("us"), 1e-6);
        assert_eq!(factor("ns"), 1e-9);
        assert_eq!(factor("kJ"), 1e3);
        assert_eq!(factor("MJ"), 1e6);
        assert_eq!(factor("kPa"), 1e3);
        assert_eq!(factor("MPa"), 1e6);
        assert_eq!(factor("GPa"), 1e9);
        assert_eq!(factor("kW"), 1e3);
        assert_eq!(factor("MW"), 1e6);
        assert_eq!(factor("kmol"), 1000.0);
        assert_eq!(factor("mA"), 1e-3);
    }

    #[test]
    fn unit_names_are_case_insensitive() {
        assert_eq!(q("KG"), q("kg"));
        assert_eq!(q("Kg"), q("kg"));
        assert_eq!(q("PSI"), q("psi"));
        assert_eq!(q("BTU"), q("Btu"));
        assert_eq!(q("kj/KG-k"), q("kJ/kg-K"));
    }

    #[test]
    fn case_sensitive_units_win_over_the_lowercased_table() {
        // h is an hour, H is a henry — the whole reason the second table exists.
        assert_eq!(factor("h"), 3600.0);
        assert_dims("h", TIME);
        assert_eq!(factor("H"), 1.0);
        assert_dims("H", INDUCTANCE);
        // The spelled-out names remain case-insensitive.
        assert_eq!(q("HENRY"), q("henry"));
        assert_eq!(factor("hr"), 3600.0);
        assert_eq!(factor("hour"), 3600.0);
    }

    // -- categories ---------------------------------------------------------

    #[test]
    fn time_units_cover_the_whole_ladder() {
        assert_eq!(factor("s"), 1.0);
        assert_eq!(factor("min"), 60.0);
        assert_eq!(factor("minutes"), 60.0);
        assert_eq!(factor("hrs"), 3600.0);
        assert_eq!(factor("day"), 86400.0);
        assert_eq!(factor("week"), 604800.0);
        assert_eq!(factor("year"), 3.1536e7);
        assert_eq!(factor("yr"), 3.1536e7);
    }

    #[test]
    fn english_and_imperial_units() {
        assert_eq!(factor("lbm"), 0.45359237);
        assert_eq!(factor("lb"), 0.45359237);
        assert_eq!(factor("lbs"), 0.45359237);
        assert_eq!(factor("slug"), 14.59390294);
        assert_eq!(factor("ft"), 0.3048);
        assert_eq!(factor("in"), 0.0254);
        assert_eq!(factor("inch"), 0.0254);
        assert_eq!(factor("yd"), 0.9144);
        assert_eq!(factor("mile"), 1609.344);
        assert_eq!(factor("gal"), 0.003785411784);
        assert_eq!(factor("lbf"), 4.4482216152605);
        assert_dims("lbf", FORCE);
        assert_eq!(factor("psi"), 6894.757293168);
        assert_dims("psi", PRESSURE);
        assert_eq!(factor("Btu"), 1055.05585262);
        assert_dims("Btu", ENERGY);
        assert_eq!(factor("hp"), 745.69987158);
        assert_dims("hp", POWER);
    }

    #[test]
    fn pressure_energy_and_power_units() {
        assert_eq!(factor("bar"), 1e5);
        assert_eq!(factor("atm"), 101325.0);
        assert_eq!(factor("torr"), 133.3223684);
        assert_eq!(factor("mmHg"), 133.3223684);
        assert_dims("mmHg", PRESSURE);
        assert_eq!(factor("cal"), 4.1868);
        assert_eq!(factor("kcal"), 4186.8);
        assert_eq!(factor("kWh"), 3.6e6);
        assert_dims("kWh", ENERGY);
        assert_eq!(factor("N"), 1.0);
        assert_eq!(factor("kN"), 1e3);
        assert_eq!(factor("MN"), 1e6);
        assert_eq!(factor("dyne"), 1e-5);
    }

    #[test]
    fn electrical_units_have_ampere_exponents() {
        assert_dims("V", VOLTAGE);
        assert_eq!(factor("kV"), 1e3);
        assert_eq!(factor("mV"), 1e-3);
        assert_dims("ohm", RESISTANCE);
        assert_eq!(q("Ω"), q("ohm"));
        assert_eq!(factor("kohm"), 1e3);
        assert_dims("farad", CAPACITANCE);
        assert_eq!(factor("uF"), 1e-6);
        assert_eq!(factor("pF"), 1e-12);
        assert_dims("coulomb", CHARGE);
        assert_eq!(q("couloumb"), q("coulomb"));
        assert_dims("siemens", CONDUCTANCE);
        assert_dims("Wb", MAGNETIC_FLUX);
        assert_dims("tesla", FLUX_DENSITY);
        assert_eq!(q("T"), q("tesla"));
    }

    #[test]
    fn viscosity_units() {
        assert_eq!(factor("poise"), 0.1);
        assert_dims("poise", DYNAMIC_VISCOSITY);
        assert_eq!(factor("cP"), 1e-3);
        assert_dims("cP", DYNAMIC_VISCOSITY);
        // cP and mPa-s are the same thing.
        assert_close(factor("cP"), factor("Pa-s") * 1e-3);
        assert_eq!(factor("cSt"), 1e-6);
        assert_dims("cSt", KINEMATIC_VISCOSITY);
        // ...and Pa-s is still expressible from the base table.
        assert_dims("Pa-s", DYNAMIC_VISCOSITY);
        assert_dims("m^2/s", KINEMATIC_VISCOSITY);
    }

    #[test]
    fn dimensionless_units() {
        assert!(q("").is_dimensionless());
        assert_eq!(factor(""), 1.0);
        assert!(q("-").is_dimensionless());
        assert_eq!(factor("-"), 1.0);
        assert!(q("1").is_dimensionless());
        assert!(q("rad").is_dimensionless());
        assert_eq!(factor("rad"), 1.0);
        assert!(q("deg").is_dimensionless());
        assert_close(factor("deg"), core::f64::consts::PI / 180.0);
        assert_eq!(factor("dB"), 1.0);
        // whitespace-only is the empty expression
        assert!(q("   ").is_dimensionless());
    }

    // -- expression grammar -------------------------------------------------

    #[test]
    fn dash_star_and_space_all_multiply() {
        let expected = q("kg").multiply(&q("K"));
        for text in ["kg-K", "kg*K", "kg K", "kg  -  K", "kg.K", "kg\u{b7}K"] {
            assert_eq!(q(text), expected, "{text}");
        }
    }

    #[test]
    fn quotients_divide() {
        let specific_heat = q("kJ/kg-K");
        assert_eq!(specific_heat.factor, 1000.0);
        assert_dims("kJ/kg-K", dims_from(&[0.0, 2.0, -2.0, -1.0]));

        assert_dims("m^3/s", dims_from(&[0.0, 3.0, -1.0]));
        assert_dims("kg/m^3", dims_from(&[1.0, -3.0]));
        assert_dims("m/s", dims_from(&[0.0, 1.0, -1.0]));
        assert_dims("W/m^2-K", dims_from(&[1.0, 0.0, -3.0, -1.0]));
    }

    #[test]
    fn every_slash_opens_another_denominator_factor() {
        // "W/m^2/K" reads as W/(m^2*K), not (W/m^2)*K.
        assert_eq!(q("W/m^2/K"), q("W/m^2-K"));
        assert_eq!(q("kg/m^2/s"), q("kg/m^2-s"));
        assert_eq!(q("kJ/kg/K"), q("kJ/kg-K"));
    }

    #[test]
    fn trailing_and_leading_slashes_are_tolerated() {
        assert_eq!(q("kg/"), q("kg"));
        assert_eq!(q("/s"), q("1/s"));
        assert_dims("/s", FREQUENCY);
        assert_eq!(q("1/s"), q("Hz"));
    }

    #[test]
    fn exponents_with_and_without_caret() {
        assert_eq!(q("m^3"), q("m3"));
        assert_dims("m^3", VOLUME);
        assert_dims("m2", AREA);
        assert_eq!(factor("km^2"), 1e6);
        assert_eq!(factor("ft^3"), 0.3048_f64.powi(3));
        // fractional exponents survive
        assert_dims("m^1.5", dims_from(&[0.0, 1.5]));
    }

    #[test]
    fn negative_exponents_need_a_caret() {
        assert_eq!(q("s^-1"), q("1/s"));
        assert_eq!(q("m^-2"), q("1/m^2"));
        assert_eq!(q("kg-m^-3"), q("kg/m^3"));
        assert_eq!(q("s^+2"), q("s^2"));
        // Bare "-1" is *not* an exponent: the dash multiplies and "1" is a no-op.
        assert_eq!(q("kg-1"), q("kg"));
        assert_eq!(q("m-1-s"), q("m-s"));
    }

    #[test]
    fn parentheses_group_denominators() {
        assert_eq!(q("kJ/(kg-K)"), q("kJ/kg-K"));
        assert_eq!(q("W/(m^2-K)"), q("W/m^2-K"));
        assert_eq!(q("(m/s)^2"), q("m^2/s^2"));
        assert_eq!(q("1/(1/s)"), q("s"));
        assert_eq!(q("kg/((m)-(s^2))"), q("Pa"));
        assert!(q("()").is_dimensionless());
    }

    #[test]
    fn engineering_compounds_from_the_corpus() {
        // Btu/hr-ft^2-R — an English heat-transfer coefficient.
        let htc = q("Btu/hr-ft^2-R");
        assert_dims("Btu/hr-ft^2-R", dims_from(&[1.0, 0.0, -3.0, -1.0]));
        let expected = 1055.05585262 / (3600.0 * 0.3048_f64.powi(2) * (5.0 / 9.0));
        assert_close(htc.factor, expected);

        // lbm/ft^3 — the English density factor from the display table.
        assert_close(factor("lbm/ft^3"), 16.01846337396014);
        assert_dims("lbm/ft^3", dims_from(&[1.0, -3.0]));

        // kg-m/s^2 is a newton.
        assert_eq!(q("kg-m/s^2"), q("N"));
        // N-m is a joule (dimensionally).
        assert_eq!(q("N-m"), q("J"));
        // N/m^2 is a pascal.
        assert_eq!(q("N/m^2"), q("Pa"));
    }

    #[test]
    fn whitespace_around_the_expression_is_trimmed() {
        assert_eq!(q("  kJ/kg-K  "), q("kJ/kg-K"));
        assert_eq!(q("\tm\t"), q("m"));
    }

    // -- error paths --------------------------------------------------------

    #[test]
    fn unknown_unit_names_are_reported_not_panicked() {
        assert_eq!(unknown("zorp"), "zorp");
        assert_eq!(unknown("kJ/kg-zorp"), "zorp");
        assert_eq!(unknown("furlong/fortnight"), "furlong");
        // "nm" is deliberately not a unit (see MOMENT_UNITS' dims guard).
        assert_eq!(unknown("nm"), "nm");
    }

    #[test]
    fn malformed_expressions_are_errors() {
        for text in [
            "m^", "m^-", "(m", "m)", "kg/(m", "2", "3-m", "m^2K", "^2", "%",
        ] {
            assert!(
                matches!(
                    UnitRegistry::parse(text),
                    Err(FreesError::UnknownUnit { .. })
                ),
                "expected {text:?} to be rejected, got {:?}",
                UnitRegistry::parse(text)
            );
        }
    }

    #[test]
    fn error_display_mentions_the_offending_text() {
        let err = UnitRegistry::parse("kJ/kg-zorp").unwrap_err();
        assert_eq!(err.to_string(), "unknown unit: zorp");
    }

    // -- temperature --------------------------------------------------------

    #[test]
    fn temperature_scales_are_multiplicative_when_parsed_plainly() {
        assert_eq!(factor("K"), 1.0);
        assert_eq!(factor("C"), 1.0);
        assert_close(factor("R"), 5.0 / 9.0);
        assert_close(factor("F"), 5.0 / 9.0);
        for unit in ["K", "C", "R", "F"] {
            assert_dims(unit, TEMPERATURE);
        }
    }

    #[test]
    fn celsius_carries_the_kelvin_offset() {
        let c = UnitRegistry::parse_with_offset("C").unwrap();
        assert_eq!(c.factor, 1.0);
        assert_eq!(c.offset, 273.15);
        assert!(dims_match(&c.dims, &TEMPERATURE));
        assert_close(c.to_si(25.0), 298.15);
        assert_close(c.from_si(298.15), 25.0);
        // case and padding do not matter
        assert_eq!(UnitRegistry::parse_with_offset("  c ").unwrap(), c);
    }

    #[test]
    fn fahrenheit_carries_factor_and_offset() {
        let f = UnitRegistry::parse_with_offset("F").unwrap();
        assert_close(f.factor, 5.0 / 9.0);
        assert_close(f.offset, 459.67 * 5.0 / 9.0);
        assert!((f.to_si(32.0) - 273.15).abs() < 1e-9);
        assert!((f.to_si(212.0) - 373.15).abs() < 1e-9);
        assert!((f.from_si(273.15) - 32.0).abs() < 1e-9);
        // -40 is the crossing point
        assert!(
            (f.to_si(-40.0) - UnitRegistry::parse_with_offset("C").unwrap().to_si(-40.0)).abs()
                < 1e-9
        );
    }

    #[test]
    fn rankine_and_kelvin_have_no_offset() {
        let r = UnitRegistry::parse_with_offset("R").unwrap();
        assert_close(r.factor, 5.0 / 9.0);
        assert_eq!(r.offset, 0.0);
        assert!((r.to_si(491.67) - 273.15).abs() < 1e-9);

        let k = UnitRegistry::parse_with_offset("K").unwrap();
        assert_eq!(k.factor, 1.0);
        assert_eq!(k.offset, 0.0);
    }

    #[test]
    fn compound_temperature_units_stay_multiplicative() {
        // kJ/kg-C is a per-degree (delta) unit: no offset.
        let cp = UnitRegistry::parse_with_offset("kJ/kg-C").unwrap();
        assert_eq!(cp.offset, 0.0);
        assert_eq!(cp.factor, 1000.0);
        assert!(dims_match(&cp.dims, &dims_from(&[0.0, 2.0, -2.0, -1.0])));

        let per_f = UnitRegistry::parse_with_offset("Btu/lbm-F").unwrap();
        assert_eq!(per_f.offset, 0.0);
    }

    #[test]
    fn parse_with_offset_propagates_unknown_units() {
        assert!(matches!(
            UnitRegistry::parse_with_offset("zorp"),
            Err(FreesError::UnknownUnit { .. })
        ));
    }

    // -- SI naming ----------------------------------------------------------

    #[test]
    fn si_name_prefers_named_units() {
        assert_eq!(UnitRegistry::si_name(&FORCE), "N");
        assert_eq!(UnitRegistry::si_name(&PRESSURE), "Pa");
        assert_eq!(UnitRegistry::si_name(&ENERGY), "J");
        assert_eq!(UnitRegistry::si_name(&POWER), "W");
        assert_eq!(UnitRegistry::si_name(&FREQUENCY), "Hz");
        assert_eq!(UnitRegistry::si_name(&VOLTAGE), "V");
        assert_eq!(UnitRegistry::si_name(&RESISTANCE), "Ω");
        assert_eq!(UnitRegistry::si_name(&DYNAMIC_VISCOSITY), "Pa-s");
        assert_eq!(
            UnitRegistry::si_name(&dims_from(&[0.0, 2.0, -2.0, -1.0])),
            "J/kg-K"
        );
        assert_eq!(
            UnitRegistry::si_name(&dims_from(&[1.0, 0.0, -3.0, -1.0])),
            "W/m^2-K"
        );
        assert_eq!(UnitRegistry::si_name(&dims_from(&[1.0, -3.0])), "kg/m^3");
        assert_eq!(UnitRegistry::si_name(&dims_from(&[-1.0, 3.0])), "m^3/kg");
        assert_eq!(UnitRegistry::si_name(&DIMENSIONLESS), "-");
    }

    #[test]
    fn si_name_composes_unnamed_dimensions() {
        // acceleration is not a named unit
        assert_eq!(
            UnitRegistry::si_name(&dims_from(&[0.0, 1.0, -2.0])),
            "m/s^2"
        );
        // pure denominators keep the leading "1"
        assert_eq!(
            UnitRegistry::si_name(&dims_from(&[0.0, 0.0, -2.0])),
            "1/s^2"
        );
        // several numerator factors are space-separated, denominators dash-separated
        assert_eq!(
            UnitRegistry::si_name(&dims_from(&[1.0, 2.0, -3.0, 0.0, -1.0])),
            "kg m^2/s^3-mol"
        );
        // a fractional exponent prints with its decimals
        assert_eq!(UnitRegistry::si_name(&dims_from(&[0.0, 1.5])), "m^1.5");
        // and the composed form re-parses to the same dimensions
        let dims = dims_from(&[0.0, 1.0, -2.0]);
        assert!(dims_match(&q(&UnitRegistry::si_name(&dims)).dims, &dims));
    }

    #[test]
    fn si_display_name_canonicalises_prefixes() {
        assert_eq!(UnitRegistry::si_display_name("kPa", &PRESSURE), "Pa");
        assert_eq!(UnitRegistry::si_display_name("psi", &PRESSURE), "Pa");
        assert_eq!(UnitRegistry::si_display_name("kJ", &ENERGY), "J");
        assert_eq!(UnitRegistry::si_display_name("hp", &POWER), "W");
        assert_eq!(UnitRegistry::si_display_name("", &POWER), "W");
        assert_eq!(UnitRegistry::si_display_name("-", &DIMENSIONLESS), "-");
    }

    #[test]
    fn si_display_name_keeps_angular_rates_as_rad_per_second() {
        assert_eq!(UnitRegistry::si_display_name("rpm", &FREQUENCY), "rad/s");
        assert_eq!(UnitRegistry::si_display_name("rad/s", &FREQUENCY), "rad/s");
        assert_eq!(UnitRegistry::si_display_name("RAD/S", &FREQUENCY), "rad/s");
        assert_eq!(
            UnitRegistry::si_display_name("rad / s", &FREQUENCY),
            "rad/s"
        );
        assert_eq!(
            UnitRegistry::si_display_name("radians/s", &FREQUENCY),
            "rad/s"
        );
        assert_eq!(UnitRegistry::si_display_name("rad/hr", &FREQUENCY), "rad/s");
        // A real frequency still prints as Hz.
        assert_eq!(UnitRegistry::si_display_name("kHz", &FREQUENCY), "Hz");
    }

    #[test]
    fn si_display_name_keeps_moments_as_newton_metres() {
        assert_eq!(UnitRegistry::si_display_name("N-m", &MOMENT_DIMS), "N-m");
        assert_eq!(UnitRegistry::si_display_name("N m", &MOMENT_DIMS), "N-m");
        assert_eq!(
            UnitRegistry::si_display_name("n\u{b7}m", &MOMENT_DIMS),
            "N-m"
        );
        assert_eq!(
            UnitRegistry::si_display_name("newton-metre", &MOMENT_DIMS),
            "N-m"
        );
        // The explicit-multiply spelling is a product and reduces to J.
        assert_eq!(UnitRegistry::si_display_name("N*m", &MOMENT_DIMS), "J");
        assert_eq!(UnitRegistry::si_display_name("kJ", &MOMENT_DIMS), "J");
        // "nm" only wins with the energy dimension; a length stays a length.
        assert_eq!(UnitRegistry::si_display_name("nm", &LENGTH), "m");
    }

    // -- the reference table ------------------------------------------------

    #[test]
    fn all_units_is_a_large_table_without_duplicates() {
        let units = UnitRegistry::all_units();
        assert!(units.len() > 130, "only {} units", units.len());
        assert_eq!(units.len(), UNITS.len() + CASE_SENSITIVE_UNITS.len());

        let mut seen = std::collections::HashSet::new();
        for info in &units {
            assert!(
                seen.insert(info.symbol.clone()),
                "duplicate symbol {:?}",
                info.symbol
            );
        }
        // Both case-sensitive spellings survive as distinct rows.
        assert!(seen.contains("h"));
        assert!(seen.contains("H"));
    }

    #[test]
    fn all_units_is_sorted_by_dimension_then_symbol() {
        let units = UnitRegistry::all_units();
        for pair in units.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            assert!(
                (a.dimension.as_str(), a.symbol.as_str())
                    < (b.dimension.as_str(), b.symbol.as_str()),
                "{a:?} should sort before {b:?}"
            );
        }
    }

    #[test]
    fn all_units_entries_round_trip_through_parse() {
        for info in UnitRegistry::all_units() {
            let parsed = UnitRegistry::parse(&info.symbol)
                .unwrap_or_else(|e| panic!("table entry {:?} does not parse: {e}", info.symbol));
            assert_eq!(
                parsed.factor, info.si_factor,
                "factor mismatch for {:?}",
                info.symbol
            );
            assert_eq!(
                UnitRegistry::si_name(&parsed.dims),
                info.dimension,
                "dimension mismatch for {:?}",
                info.symbol
            );
        }
    }

    #[test]
    fn all_units_reports_dimensions_and_factors() {
        let units = UnitRegistry::all_units();
        let find = |symbol: &str| {
            units
                .iter()
                .find(|u| u.symbol == symbol)
                .unwrap_or_else(|| panic!("{symbol} missing from the reference table"))
        };
        assert_eq!(find("psi").dimension, "Pa");
        assert_eq!(find("psi").si_factor, 6894.757293168);
        assert_eq!(find("btu").dimension, "J");
        assert_eq!(find("hp").dimension, "W");
        assert_eq!(find("lbm").dimension, "kg");
        assert_eq!(find("ft").dimension, "m");
        assert_eq!(find("rad").dimension, "-");
        assert_eq!(find("H").dimension, "Henry");
        assert_eq!(find("h").dimension, "s");
    }

    // -- convert / display systems -----------------------------------------

    #[test]
    fn convert_returns_the_ratio_of_factors() {
        assert_close(UnitRegistry::convert("ft", "m").unwrap(), 0.3048);
        assert_close(UnitRegistry::convert("m", "ft").unwrap(), 1.0 / 0.3048);
        assert_close(UnitRegistry::convert("hr", "s").unwrap(), 3600.0);
        assert_close(UnitRegistry::convert("kJ/kg-K", "J/kg-K").unwrap(), 1000.0);
        assert_close(
            UnitRegistry::convert("kg/m^3", "lbm/ft^3").unwrap(),
            1.0 / 16.01846337396014,
        );
    }

    #[test]
    fn convert_rejects_dimension_mismatches_and_unknown_units() {
        assert!(matches!(
            UnitRegistry::convert("m", "s"),
            Err(FreesError::Evaluation { .. })
        ));
        assert!(matches!(
            UnitRegistry::convert("m", "zorp"),
            Err(FreesError::UnknownUnit { .. })
        ));
    }

    #[test]
    fn preferred_display_units_per_system() {
        assert!(UnitRegistry::preferred_display_unit(&PRESSURE, UnitSystem::Si).is_none());

        let eng = UnitRegistry::preferred_display_unit(&PRESSURE, UnitSystem::EngSi).unwrap();
        assert_eq!(eng.name, "kPa");
        assert_close(eng.from_si(101325.0), 101.325);

        let english = UnitRegistry::preferred_display_unit(&PRESSURE, UnitSystem::English).unwrap();
        assert_eq!(english.name, "psi");
        assert_close(english.to_si(1.0), 6894.757293168);

        let density =
            UnitRegistry::preferred_display_unit(&dims_from(&[1.0, -3.0]), UnitSystem::English)
                .unwrap();
        assert_eq!(density.name, "lbm/ft^3");

        // Temperature is deliberately absent from both tables.
        assert!(UnitRegistry::preferred_display_unit(&TEMPERATURE, UnitSystem::English).is_none());
        assert!(UnitRegistry::preferred_display_unit(&TEMPERATURE, UnitSystem::EngSi).is_none());
    }

    #[test]
    fn is_known_unit_checks_single_names_only() {
        assert!(UnitRegistry::is_known_unit("kPa"));
        assert!(UnitRegistry::is_known_unit("H"));
        assert!(!UnitRegistry::is_known_unit("zorp"));
        // it is a *name* check, not an expression check
        assert!(!UnitRegistry::is_known_unit("kJ/kg-K"));
    }
}
