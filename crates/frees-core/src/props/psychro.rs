//! Psychrometric chart data.
//!
//! Port of
//! `../frEES/backend/core/src/main/java/com/frees/backend/props/Psychrometrics.java`
//! (148 LOC), which backs `POST /api/plot/psychart`.
//!
//! Dry-bulb temperature on the x axis, humidity ratio (kg water / kg dry air)
//! on the y axis, at a fixed total pressure, all SI. Families follow the
//! standard chart: the saturation line, constant relative humidity, constant
//! wet-bulb, constant mixture enthalpy and constant specific volume.
//!
//! Every point is one `HAPropsSI("W", "T", …, "P", …, <third>, …)` call through
//! [`crate::props::propfun`]. A point the backend declines is emitted as a
//! `None` gap ([`Curve`] is shared with [`crate::props::diagrams`]), so a
//! partial humid-air backend draws a visibly broken line rather than an
//! invented one — and an *absent* backend draws a chart whose every point is a
//! gap, which is the honest rendering of "this build has no HAPropsSI".

use crate::diag::{FreesError, Result};
use crate::props::diagrams::Curve;
use crate::props::propfun::ha_props_si_or_nan;

/// A full psychrometric chart: the pressure and dry-bulb window it was built
/// for, plus every curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub pressure: f64,
    pub t_min: f64,
    pub t_max: f64,
    pub curves: Vec<Curve>,
}

const POINTS: usize = 50;
/// Wet-bulb / enthalpy / volume lines are nearly straight; few samples needed.
const LINE_POINTS: usize = 12;

struct Generator {
    pressure: f64,
    t_min: f64,
    t_max: f64,
}

/// Standard chart range when not specified: 0–50 °C dry bulb at 1 atm.
/// Port of `generate(pressureOrNull, tMinOrNull, tMaxOrNull)`.
pub fn generate(pressure: Option<f64>, t_min: Option<f64>, t_max: Option<f64>) -> Result<Chart> {
    let p = pressure.unwrap_or(101_325.0);
    let lo = t_min.unwrap_or(273.15);
    let hi = t_max.unwrap_or(323.15);
    // Java: `p <= 1000 || hi <= lo`. Written the same way round so a NaN
    // pressure fails neither comparison and reaches the sweeps as NaN, exactly
    // as it does in the Java — the curves then come back as gaps.
    if p <= 1000.0 || hi <= lo {
        return Err(FreesError::property(
            "Psychrometric chart needs pressure > 1 kPa and tMax > tMin (SI units)".to_string(),
        ));
    }
    let generator = Generator {
        pressure: p,
        t_min: lo,
        t_max: hi,
    };
    let mut curves = Vec::new();
    curves.extend(generator.relative_humidity_lines());
    curves.extend(generator.wet_bulb_lines());
    curves.extend(generator.enthalpy_lines());
    curves.extend(generator.volume_lines());
    Ok(Chart {
        pressure: p,
        t_min: lo,
        t_max: hi,
        curves,
    })
}

impl Generator {
    /// RH from 10 % to 100 %; the 100 % line is the saturation boundary.
    fn relative_humidity_lines(&self) -> Vec<Curve> {
        let mut out = Vec::new();
        let mut pct = 10;
        while pct <= 100 {
            let rh = f64::from(pct) / 100.0;
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for i in 0..POINTS {
                let t = self.t_min + (self.t_max - self.t_min) * i as f64 / (POINTS as f64 - 1.0);
                self.add_point(&mut xs, &mut ys, t, "R", rh);
            }
            let (family, label) = if pct == 100 {
                ("saturation", "Saturation".to_string())
            } else {
                ("rh", format!("\u{3c6} = {pct}%"))
            };
            out.push(Curve {
                family: family.to_string(),
                label,
                x: xs,
                y: ys,
            });
            pct += 10;
        }
        out
    }

    fn wet_bulb_lines(&self) -> Vec<Curve> {
        let mut out = Vec::new();
        // Java: `for (double twb = ceil((tMin - 273.15) / 5) * 5; twb <= tMax - 273.15; twb += 5)`
        // — accumulated in Celsius, so the accumulation error is the Java's.
        let mut twb = libm::ceil((self.t_min - 273.15) / 5.0) * 5.0;
        while twb <= self.t_max - 273.15 {
            let twb_k = twb + 273.15;
            // A wet-bulb line starts on the saturation curve at T = Twb.
            out.push(self.sweep_line(
                "wetbulb",
                &format!("T_wb = {} \u{b0}C", java_round(twb)),
                twb_k,
                "B",
                twb_k,
            ));
            twb += 5.0;
        }
        out
    }

    /// Sweeps `W(Tdb)` for a fixed third constraint from `t_start` to `t_max`.
    fn sweep_line(&self, family: &str, label: &str, t_start: f64, key: &str, value: f64) -> Curve {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let from = t_start.max(self.t_min);
        for i in 0..LINE_POINTS {
            let t = from + (self.t_max - from) * i as f64 / (LINE_POINTS as f64 - 1.0);
            self.add_point(&mut xs, &mut ys, t, key, value);
        }
        Curve {
            family: family.to_string(),
            label: label.to_string(),
            x: xs,
            y: ys,
        }
    }

    fn enthalpy_lines(&self) -> Vec<Curve> {
        let h_min = ha_props_si_or_nan("H", "T", self.t_min, "P", self.pressure, "R", 0.0);
        let h_max = ha_props_si_or_nan("H", "T", self.t_max, "P", self.pressure, "R", 1.0);
        let mut out = Vec::new();
        if h_min.is_nan() || h_max.is_nan() {
            return out;
        }
        let step = 10_000.0; // 10 kJ/kg dry air
        let mut h = libm::ceil(h_min / step) * step;
        while h <= h_max {
            // The line enters the chart where it crosses saturation.
            let mut t_start = ha_props_si_or_nan("T", "P", self.pressure, "H", h, "R", 1.0);
            if t_start.is_nan() {
                t_start = self.t_min;
            }
            out.push(self.sweep_line(
                "enthalpy",
                &format!("h = {} kJ/kg", java_round(h / 1000.0)),
                t_start,
                "H",
                h,
            ));
            h += step;
        }
        out
    }

    fn volume_lines(&self) -> Vec<Curve> {
        let v_min = ha_props_si_or_nan("V", "T", self.t_min, "P", self.pressure, "R", 0.0);
        let v_max = ha_props_si_or_nan("V", "T", self.t_max, "P", self.pressure, "R", 1.0);
        let mut out = Vec::new();
        if v_min.is_nan() || v_max.is_nan() {
            return out;
        }
        let step = 0.01;
        let mut v = libm::ceil(v_min / step) * step;
        while v <= v_max {
            let mut t_start = ha_props_si_or_nan("T", "P", self.pressure, "V", v, "R", 1.0);
            if t_start.is_nan() {
                t_start = self.t_min;
            }
            out.push(self.sweep_line(
                "volume",
                // Java `String.format("%.2f", v)` under the default locale;
                // this port hard-codes '.' as the decimal separator, which is
                // what the oracle machine produces.
                &format!("v = {v:.2} m\u{b3}/kg"),
                t_start,
                "V",
                v,
            ));
            v += step;
        }
        out
    }

    /// Humidity ratio at `(Tdb, P, third constraint)`. Lines never exceed
    /// saturation because RH inputs are bounded at 1 and the other families
    /// start their sweep at the saturation intersection.
    fn add_point(
        &self,
        xs: &mut Vec<Option<f64>>,
        ys: &mut Vec<Option<f64>>,
        t: f64,
        key: &str,
        value: f64,
    ) {
        let w = ha_props_si_or_nan("W", "T", t, "P", self.pressure, key, value);
        // Java: `if (Double.isNaN(w) || w < 0)`.
        if w.is_nan() || w < 0.0 {
            xs.push(None);
            ys.push(None);
            return;
        }
        xs.push(Some(t));
        ys.push(Some(w));
    }
}

/// Java `Math.round(double)` → `long`, i.e. `floor(x + 0.5)`.
fn java_round(v: f64) -> i64 {
    libm::floor(v + 0.5) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::propfun::{self, RealFluid};
    use std::sync::Arc;

    #[test]
    fn the_range_guard_matches_the_java() {
        let err = generate(Some(999.0), None, None).unwrap_err().to_string();
        assert!(err.contains("pressure > 1 kPa"), "{err}");
        let err = generate(None, Some(320.0), Some(300.0))
            .unwrap_err()
            .to_string();
        assert!(err.contains("tMax > tMin"), "{err}");
        // Exactly 1 kPa is refused (`p <= 1000`), 1 kPa + 1 Pa is not.
        assert!(generate(Some(1000.0), None, None).is_err());
    }

    /// A closed-form stand-in for `HAPropsSI` — enough of the moist-air algebra
    /// to exercise every family and prove the curve *structure* is the Java's.
    /// The numbers are not physical and are not asserted on.
    struct ToyHumidAir;

    impl ToyHumidAir {
        /// A monotone saturation pressure so `W` stays bounded and positive.
        fn p_sat(t: f64) -> f64 {
            610.94 * libm::exp(17.625 * (t - 273.15) / (t - 273.15 + 243.04))
        }

        fn hum_ratio(t: f64, p: f64, rh: f64) -> f64 {
            let pv = rh * Self::p_sat(t);
            if pv >= p {
                return f64::NAN;
            }
            0.621_945 * pv / (p - pv)
        }
    }

    impl RealFluid for ToyHumidAir {
        fn props_si(
            &self,
            _output: &str,
            _n1: &str,
            _v1: f64,
            _n2: &str,
            _v2: f64,
            _fluid: &str,
        ) -> Result<f64> {
            Err(FreesError::property(
                "toy: pure fluids not served".to_string(),
            ))
        }

        fn props1_si(&self, _fluid: &str, param: &str) -> Result<f64> {
            Err(FreesError::property(format!("toy: no {param}")))
        }

        fn ha_props_si(
            &self,
            output: &str,
            name1: &str,
            value1: f64,
            name2: &str,
            value2: f64,
            name3: &str,
            value3: f64,
        ) -> Result<f64> {
            let mut t = f64::NAN;
            let mut p = f64::NAN;
            let mut rh = f64::NAN;
            let mut h = f64::NAN;
            let mut v = f64::NAN;
            let mut b = f64::NAN;
            for (k, val) in [(name1, value1), (name2, value2), (name3, value3)] {
                match k {
                    "T" => t = val,
                    "P" => p = val,
                    "R" => rh = val,
                    "H" => h = val,
                    "V" => v = val,
                    "B" => b = val,
                    _ => {}
                }
            }
            if !p.is_finite() {
                return Err(FreesError::property("toy: no pressure".to_string()));
            }
            // Recover T from whichever third constraint arrived.
            if !t.is_finite() {
                if h.is_finite() {
                    t = 273.15 + h / 1005.0;
                } else if v.is_finite() {
                    t = v * p / 287.0;
                } else if b.is_finite() {
                    t = b;
                } else {
                    return Err(FreesError::property("toy: underdetermined".to_string()));
                }
            }
            // A wet-bulb constraint is treated as saturated at Twb.
            if !rh.is_finite() {
                rh = if b.is_finite() { 1.0 } else { 0.5 };
            }
            let value = match output {
                "W" => Self::hum_ratio(t, p, rh),
                "T" => t,
                "H" => 1005.0 * (t - 273.15),
                "V" => 287.0 * t / p,
                other => return Err(FreesError::property(format!("toy: no {other}"))),
            };
            if value.is_finite() {
                Ok(value)
            } else {
                Err(FreesError::property("toy: out of range".to_string()))
            }
        }
    }

    fn with_toy<T>(body: impl FnOnce() -> T) -> T {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::install(Arc::new(ToyHumidAir));
        let out = body();
        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }
        out
    }

    #[test]
    fn the_chart_carries_the_java_family_inventory() {
        with_toy(|| {
            let chart = generate(None, None, None).unwrap();
            assert_eq!(chart.pressure, 101_325.0);
            assert_eq!(chart.t_min, 273.15);
            assert_eq!(chart.t_max, 323.15);
            // Nine RH lines plus the saturation line.
            let rh: Vec<&Curve> = chart.curves.iter().filter(|c| c.family == "rh").collect();
            assert_eq!(rh.len(), 9);
            assert_eq!(rh[0].label, "\u{3c6} = 10%");
            assert_eq!(rh[8].label, "\u{3c6} = 90%");
            assert!(rh.iter().all(|c| c.x.len() == POINTS));
            let saturation: Vec<&Curve> = chart
                .curves
                .iter()
                .filter(|c| c.family == "saturation")
                .collect();
            assert_eq!(saturation.len(), 1);
            assert_eq!(saturation[0].label, "Saturation");
            // Wet bulb: 0, 5, … 50 °C -> 11 lines of LINE_POINTS samples.
            let wb: Vec<&Curve> = chart
                .curves
                .iter()
                .filter(|c| c.family == "wetbulb")
                .collect();
            assert_eq!(wb.len(), 11);
            assert_eq!(wb[0].label, "T_wb = 0 \u{b0}C");
            assert_eq!(wb[10].label, "T_wb = 50 \u{b0}C");
            assert!(wb.iter().all(|c| c.x.len() == LINE_POINTS));
            // Enthalpy and volume families exist and are labelled the Java way.
            let h: Vec<&Curve> = chart
                .curves
                .iter()
                .filter(|c| c.family == "enthalpy")
                .collect();
            assert!(!h.is_empty());
            assert!(h[0].label.starts_with("h = "), "{}", h[0].label);
            assert!(h[0].label.ends_with(" kJ/kg"), "{}", h[0].label);
            let v: Vec<&Curve> = chart
                .curves
                .iter()
                .filter(|c| c.family == "volume")
                .collect();
            assert!(!v.is_empty());
            assert!(v[0].label.starts_with("v = 0."), "{}", v[0].label);
            assert!(v[0].label.ends_with(" m\u{b3}/kg"), "{}", v[0].label);
        });
    }

    #[test]
    fn every_point_is_a_gap_when_no_humid_air_backend_exists() {
        let _guard = propfun::test_swap_guard();
        let previous = propfun::uninstall();
        let chart = generate(None, None, None).unwrap();
        match previous {
            Some(p) => {
                propfun::install(p);
            }
            None => {
                propfun::uninstall();
            }
        }
        // The RH families still exist (they do not depend on a probe), and
        // every one of their points is a declared gap — never a number.
        let rh: Vec<&Curve> = chart.curves.iter().filter(|c| c.family == "rh").collect();
        assert_eq!(rh.len(), 9);
        assert!(rh
            .iter()
            .all(|c| c.x.iter().all(Option::is_none) && c.y.iter().all(Option::is_none)));
        // The enthalpy and volume families probe first and come back empty
        // rather than emitting all-gap curves — exactly the Java's early return.
        assert!(!chart.curves.iter().any(|c| c.family == "enthalpy"));
        assert!(!chart.curves.iter().any(|c| c.family == "volume"));
    }

    #[test]
    fn a_custom_window_moves_the_wet_bulb_ladder() {
        with_toy(|| {
            // 10 °C … 30 °C -> wet-bulb lines at 10, 15, 20, 25, 30.
            let chart = generate(Some(90_000.0), Some(283.15), Some(303.15)).unwrap();
            let wb: Vec<&Curve> = chart
                .curves
                .iter()
                .filter(|c| c.family == "wetbulb")
                .collect();
            assert_eq!(wb.len(), 5);
            assert_eq!(wb[0].label, "T_wb = 10 \u{b0}C");
            assert_eq!(wb[4].label, "T_wb = 30 \u{b0}C");
            assert_eq!(chart.pressure, 90_000.0);
        });
    }

    #[test]
    fn java_round_is_floor_of_x_plus_half() {
        assert_eq!(java_round(4.5), 5);
        assert_eq!(java_round(4.4999), 4);
    }
}
