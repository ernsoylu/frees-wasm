//! Physical quantities over the seven SI base dimensions.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/units/Quantity.java`.

/// Number of SI base dimensions.
pub const DIMENSIONS: usize = 7;

/// Base dimension order: `[kg, m, s, K, mol, A, cd]`.
pub const BASE_SYMBOLS: [&str; DIMENSIONS] = ["kg", "m", "s", "K", "mol", "A", "cd"];

/// Dimension exponents, indexed as [`BASE_SYMBOLS`].
pub type Dims = [f64; DIMENSIONS];

/// Tolerance for comparing dimension exponents, matching the Java `1e-9`.
const DIM_EPS: f64 = 1e-9;

/// A physical quantity: a multiplicative factor to SI plus exponents over the
/// seven SI base dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub factor: f64,
    pub dims: Dims,
}

impl Quantity {
    pub fn new(factor: f64, dims: Dims) -> Quantity {
        Quantity { factor, dims }
    }

    /// A pure scale factor with no dimensions.
    pub fn dimensionless(factor: f64) -> Quantity {
        Quantity {
            factor,
            dims: [0.0; DIMENSIONS],
        }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.dims.iter().all(|d| d.abs() <= DIM_EPS)
    }

    pub fn same_dimensions_as(&self, other: &Quantity) -> bool {
        self.dims
            .iter()
            .zip(other.dims.iter())
            .all(|(a, b)| (a - b).abs() <= DIM_EPS)
    }

    pub fn multiply(&self, other: &Quantity) -> Quantity {
        let mut dims = self.dims;
        for (d, o) in dims.iter_mut().zip(other.dims.iter()) {
            *d += o;
        }
        Quantity {
            factor: self.factor * other.factor,
            dims,
        }
    }

    pub fn divide(&self, other: &Quantity) -> Quantity {
        let mut dims = self.dims;
        for (d, o) in dims.iter_mut().zip(other.dims.iter()) {
            *d -= o;
        }
        Quantity {
            factor: self.factor / other.factor,
            dims,
        }
    }

    pub fn powf(&self, exponent: f64) -> Quantity {
        let mut dims = self.dims;
        for d in dims.iter_mut() {
            *d *= exponent;
        }
        Quantity {
            factor: self.factor.powf(exponent),
            dims,
        }
    }

    /// Human-readable SI dimension string, e.g. `"kg^1 m^-3"`, or `"-"` when
    /// dimensionless. Exponents of exactly 1 are printed bare, and integral
    /// exponents print without a decimal point.
    pub fn dimension_string(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (symbol, &e) in BASE_SYMBOLS.iter().zip(self.dims.iter()) {
            if e.abs() <= DIM_EPS {
                continue;
            }
            if (e - 1.0).abs() <= DIM_EPS {
                parts.push(symbol.to_string());
            } else if e == e.round() {
                parts.push(format!("{symbol}^{}", e as i64));
            } else {
                parts.push(format!("{symbol}^{e}"));
            }
        }
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join(" ")
        }
    }
}

/// A unit that carries an additive offset as well as a scale — the temperature
/// scales (`C`, `F`) and gauge pressures.
///
/// Conversion to SI is `si = value * factor + offset`. Port of
/// `UnitRegistry.OffsetQuantity`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetQuantity {
    pub factor: f64,
    pub offset: f64,
    pub dims: Dims,
}

impl OffsetQuantity {
    pub fn new(factor: f64, offset: f64, dims: Dims) -> OffsetQuantity {
        OffsetQuantity {
            factor,
            offset,
            dims,
        }
    }

    /// Convert a value expressed in this unit into SI.
    pub fn to_si(&self, value: f64) -> f64 {
        value * self.factor + self.offset
    }

    /// Convert an SI value back into this unit.
    pub fn from_si(&self, si: f64) -> f64 {
        (si - self.offset) / self.factor
    }

    /// Drop the offset, yielding the purely multiplicative part.
    pub fn quantity(&self) -> Quantity {
        Quantity {
            factor: self.factor,
            dims: self.dims,
        }
    }
}

impl From<Quantity> for OffsetQuantity {
    fn from(q: Quantity) -> OffsetQuantity {
        OffsetQuantity {
            factor: q.factor,
            offset: 0.0,
            dims: q.dims,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims(kg: f64, m: f64, s: f64) -> Dims {
        [kg, m, s, 0.0, 0.0, 0.0, 0.0]
    }

    #[test]
    fn dimensionless_is_recognised() {
        assert!(Quantity::dimensionless(2.5).is_dimensionless());
        assert!(!Quantity::new(1.0, dims(1.0, 0.0, 0.0)).is_dimensionless());
    }

    #[test]
    fn multiply_adds_exponents_and_divide_subtracts() {
        // Pa = kg m^-1 s^-2 ; m^3 → Pa*m^3 = kg m^2 s^-2 (an energy)
        let pa = Quantity::new(1.0, dims(1.0, -1.0, -2.0));
        let m3 = Quantity::new(1.0, [0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let energy = pa.multiply(&m3);
        assert_eq!(energy.dims[0], 1.0);
        assert_eq!(energy.dims[1], 2.0);
        assert_eq!(energy.dims[2], -2.0);

        let back = energy.divide(&m3);
        assert!(back.same_dimensions_as(&pa));
    }

    #[test]
    fn pow_scales_exponents_and_factor() {
        let km = Quantity::new(1000.0, [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let km2 = km.powf(2.0);
        assert_eq!(km2.factor, 1_000_000.0);
        assert_eq!(km2.dims[1], 2.0);
    }

    #[test]
    fn dimension_string_matches_java_formatting() {
        assert_eq!(Quantity::dimensionless(1.0).dimension_string(), "-");
        let density = Quantity::new(1.0, dims(1.0, -3.0, 0.0));
        assert_eq!(density.dimension_string(), "kg m^-3");
        let velocity = Quantity::new(1.0, dims(0.0, 1.0, -1.0));
        assert_eq!(velocity.dimension_string(), "m s^-1");
    }

    #[test]
    fn celsius_round_trips_through_si() {
        // C → K is value + 273.15
        let celsius = OffsetQuantity::new(1.0, 273.15, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert!((celsius.to_si(25.0) - 298.15).abs() < 1e-12);
        assert!((celsius.from_si(298.15) - 25.0).abs() < 1e-12);
    }

    #[test]
    fn fahrenheit_round_trips_through_si() {
        // F → K is (value + 459.67) * 5/9, i.e. factor 5/9, offset 459.67*5/9.
        let f = OffsetQuantity::new(
            5.0 / 9.0,
            459.67 * 5.0 / 9.0,
            [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
        );
        assert!((f.to_si(32.0) - 273.15).abs() < 1e-9);
        assert!((f.to_si(212.0) - 373.15).abs() < 1e-9);
    }
}
