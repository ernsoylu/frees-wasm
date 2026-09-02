//! Psychrometric chart *geometry* — the oblique coordinate transform.
//!
//! [`super::psychro`] answers "what are the curves?" in physical coordinates:
//! dry-bulb on one axis, humidity ratio on the other. That is the right shape
//! for a plotting library that will scale the axes itself, and it is what
//! `POST /api/plot/psychart` has always returned.
//!
//! This module answers a different question: **where do those curves belong on a
//! real psychrometric chart?** A psychrometric chart is not a plot of `W`
//! against `t_db`. It is an oblique-angle chart whose true coordinates are
//! enthalpy and humidity ratio, arranged so that lines of constant enthalpy come
//! out straight and parallel. Drawing one as a rectangular scatter of `(t, W)`
//! gives a picture that is numerically right and visually not a psychrometric
//! chart.
//!
//! # The construction
//!
//! Define the **reduced sensible coordinate**
//!
//! ```text
//! σ = h − h_g,ref · W = t_db · (c_p,da + c_p,wv · W)
//! ```
//!
//! Two properties follow directly from that definition, and between them they
//! are the whole chart:
//!
//! * **Constant enthalpy.** `σ = h − h_g,ref·W` is linear in `W` with slope
//!   `−h_g,ref`, for every `h`. Straight, and parallel to one another.
//! * **Constant dry-bulb.** `σ = t·(c_p,da + c_p,wv·W)` is linear in `W` with
//!   slope `c_p,wv·t`. Straight, but *not* parallel — the isotherms fan out as
//!   `t` rises. That divergence is the visible skew of an ASHRAE chart, and it
//!   falls out of the thermodynamics rather than being applied as a shear.
//!
//! The ASHRAE and Mollier i-x layouts turn out to be the same reduced space with
//! the axes exchanged, so both are exact and neither is a special case of the
//! other. Mollier's defining horizontal 0 °C isotherm is simply `σ = 0`.
//!
//! Because both families are straight here, a renderer needs two endpoints per
//! enthalpy or dry-bulb line instead of a sampled polyline.
//!
//! # Why this is pure algebra
//!
//! Nothing here calls a property backend. The transform is defined by the moist
//! air enthalpy relation alone, so it holds at any pressure and needs no fluid
//! data — which is what lets it be inverted exactly rather than solved.

/// Specific heat of dry air, kJ/(kg_da·K) — ASHRAE RP-1485.
const CP_DA: f64 = 1.006;

/// Specific heat of water vapour, kJ/(kg_wv·K) — RP-1485. Note `1.84`, not the
/// `1.86` of older tables.
const CP_WV: f64 = 1.84;

/// Enthalpy of saturated water vapour at 0 °C, kJ/kg_wv — RP-1485. Note
/// `2499.86`, not `2501`.
///
/// Public because it is the offset the whole transform is defined against: a
/// caller moving between the reduced coordinate and enthalpy needs it, and
/// substituting `2501` would put every isenthalp on a slightly wrong slope.
pub const H_G_REF: f64 = 2_499.86;

/// Which chart layout coordinates are expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartLayout {
    /// ASHRAE format: reduced sensible coordinate horizontal, humidity ratio vertical.
    Ashrae,
    /// Mollier i-x diagram: humidity ratio horizontal, reduced coordinate vertical.
    ///
    /// The 0 °C isotherm is horizontal at `y = 0`, which is the layout's
    /// defining feature.
    MollierIx,
}

/// A point in chart space.
///
/// One axis carries the reduced coordinate (kJ/kg_da) and the other humidity
/// ratio (kg/kg_da); which is which depends on the layout. Scaling to pixels is
/// the caller's job — this module owns no view state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPoint {
    /// Horizontal chart coordinate.
    pub x: f64,
    /// Vertical chart coordinate.
    pub y: f64,
}

/// An axis-aligned region of chart space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartBounds {
    /// Minimum horizontal coordinate.
    pub x_min: f64,
    /// Maximum horizontal coordinate.
    pub x_max: f64,
    /// Minimum vertical coordinate.
    pub y_min: f64,
    /// Maximum vertical coordinate.
    pub y_max: f64,
}

/// The reduced sensible coordinate `σ = h − h_g,ref·W = t·(c_p,da + c_p,wv·W)`.
#[must_use]
pub fn reduced_coordinate(t_db_c: f64, w: f64) -> f64 {
    t_db_c * (CP_DA + CP_WV * w)
}

/// Recovers dry-bulb temperature from the reduced coordinate and humidity ratio.
///
/// Exact inverse of [`reduced_coordinate`]. The denominator is bounded below by
/// `c_p,da` for any physical `W ≥ 0`, so it cannot divide by zero.
#[must_use]
pub fn temperature_from_reduced(sigma: f64, w: f64) -> f64 {
    sigma / (CP_DA + CP_WV * w)
}

/// Specific enthalpy (kJ/kg_da) from the reduced coordinate and humidity ratio.
///
/// The reduced coordinate is enthalpy with the latent reference removed, so this
/// simply puts it back: `h = σ + h_g,ref·W`. Handy when a hit-test has produced
/// chart coordinates and the caller wants the isenthalp through that point.
#[must_use]
pub fn enthalpy_from_reduced(sigma: f64, w: f64) -> f64 {
    sigma + H_G_REF * w
}

/// The reduced coordinate from specific enthalpy and humidity ratio.
///
/// Inverse of [`enthalpy_from_reduced`]. Because this is linear in `W` at fixed
/// `h`, plotting a constant-enthalpy line needs only its two endpoints.
#[must_use]
pub fn reduced_from_enthalpy(h: f64, w: f64) -> f64 {
    h - H_G_REF * w
}

/// Maps dry-bulb temperature (°C) and humidity ratio into chart space.
#[must_use]
pub fn to_chart(t_db_c: f64, w: f64, layout: ChartLayout) -> ChartPoint {
    let sigma = reduced_coordinate(t_db_c, w);
    match layout {
        ChartLayout::Ashrae => ChartPoint { x: sigma, y: w },
        ChartLayout::MollierIx => ChartPoint { x: w, y: sigma },
    }
}

/// Recovers `(t_db_c, W)` from a chart-space point.
///
/// Exact inverse of [`to_chart`]. This is the leg a pointer drag runs: the view
/// turns pixels into chart space, and this turns chart space into properties.
#[must_use]
pub fn from_chart(point: ChartPoint, layout: ChartLayout) -> (f64, f64) {
    let (sigma, w) = match layout {
        ChartLayout::Ashrae => (point.x, point.y),
        ChartLayout::MollierIx => (point.y, point.x),
    };
    (temperature_from_reduced(sigma, w), w)
}

/// The chart-space bounds of a physical `(t, W)` window.
///
/// Taken over the window's corners rather than its temperature limits: the
/// reduced coordinate depends on `W` as well as `t`, so the extremes need not
/// lie where the temperatures do.
#[must_use]
pub fn bounds(
    t_min_c: f64,
    t_max_c: f64,
    w_min: f64,
    w_max: f64,
    layout: ChartLayout,
) -> ChartBounds {
    let mut b = ChartBounds {
        x_min: f64::INFINITY,
        x_max: f64::NEG_INFINITY,
        y_min: f64::INFINITY,
        y_max: f64::NEG_INFINITY,
    };
    for (t, w) in [
        (t_min_c, w_min),
        (t_min_c, w_max),
        (t_max_c, w_min),
        (t_max_c, w_max),
    ] {
        let p = to_chart(t, w, layout);
        b.x_min = b.x_min.min(p.x);
        b.x_max = b.x_max.max(p.x);
        b.y_min = b.y_min.min(p.y);
        b.y_max = b.y_max.max(p.y);
    }
    b
}

/// Whether members of a constant-property family are straight lines in chart
/// space, and therefore need only two endpoints.
///
/// Enthalpy and dry-bulb are; relative humidity, wet-bulb and specific volume
/// are not.
#[must_use]
pub const fn is_straight(family: StraightFamily) -> bool {
    matches!(
        family,
        StraightFamily::Enthalpy | StraightFamily::DryBulb | StraightFamily::HumidityRatio
    )
}

/// The families whose chart-space straightness is a property of the transform
/// rather than of the fluid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StraightFamily {
    /// Constant dry-bulb temperature — straight, not parallel.
    DryBulb,
    /// Constant humidity ratio — straight and parallel.
    HumidityRatio,
    /// Constant enthalpy — straight and parallel.
    Enthalpy,
    /// Constant relative humidity — curved.
    RelativeHumidity,
    /// Constant thermodynamic wet-bulb — curved.
    WetBulb,
    /// Constant specific volume — curved.
    SpecificVolume,
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUTS: [ChartLayout; 2] = [ChartLayout::Ashrae, ChartLayout::MollierIx];

    /// The forward and inverse maps must agree to the last bit: a drag runs
    /// screen → chart → properties → chart → screen on every pointer move, and
    /// any asymmetry shows up as a point sliding out from under the cursor.
    #[test]
    fn round_trips_in_both_layouts() {
        for layout in LAYOUTS {
            let mut t = -20.0_f64;
            while t <= 60.0 {
                let mut w = 0.0_f64;
                while w <= 0.040 {
                    let (t_back, w_back) = from_chart(to_chart(t, w, layout), layout);
                    assert!((t_back - t).abs() < 1e-12, "{t} vs {t_back}");
                    assert!((w_back - w).abs() < 1e-15);
                    w += 0.002;
                }
                t += 2.5;
            }
        }
    }

    /// Constant-enthalpy lines are straight and parallel, at slope `−h_g,ref`.
    #[test]
    fn enthalpy_lines_are_straight_and_parallel() {
        let w_of = |h: f64, t: f64| (h - CP_DA * t) / (H_G_REF + CP_WV * t);
        let mut slopes = Vec::new();
        for h in [20.0_f64, 50.0, 90.0] {
            let p: Vec<ChartPoint> = [5.0_f64, 20.0, 35.0]
                .iter()
                .map(|&t| to_chart(t, w_of(h, t), ChartLayout::Ashrae))
                .collect();
            let s1 = (p[1].x - p[0].x) / (p[1].y - p[0].y);
            let s2 = (p[2].x - p[1].x) / (p[2].y - p[1].y);
            assert!((s1 - s2).abs() < 1e-6, "not straight: {s1} vs {s2}");
            assert!((s1 + H_G_REF).abs() < 1e-6, "slope {s1}, want {}", -H_G_REF);
            slopes.push(s1);
        }
        assert!((slopes[0] - slopes[2]).abs() < 1e-6, "not parallel");
    }

    /// Constant dry-bulb lines are straight but fan out with temperature. That
    /// divergence is the chart's skew and must not be flattened away.
    #[test]
    fn dry_bulb_lines_are_straight_but_fan_out() {
        let slope_at = |t: f64| {
            let a = to_chart(t, 0.000, ChartLayout::Ashrae);
            let b = to_chart(t, 0.010, ChartLayout::Ashrae);
            let c = to_chart(t, 0.020, ChartLayout::Ashrae);
            let s1 = (b.x - a.x) / (b.y - a.y);
            let s2 = (c.x - b.x) / (c.y - b.y);
            assert!((s1 - s2).abs() < 1e-9);
            s1
        };
        assert!(slope_at(0.0).abs() < 1e-9, "the 0 C isotherm is vertical");
        assert!((slope_at(20.0) - CP_WV * 20.0).abs() < 1e-9);
        assert!(slope_at(40.0) > slope_at(20.0), "isotherms must fan out");
    }

    /// Mollier is the axis swap, and its 0 °C isotherm is horizontal.
    #[test]
    fn mollier_is_the_axis_swap() {
        for &(t, w) in &[(24.0, 0.009), (0.0, 0.004), (40.0, 0.02)] {
            let a = to_chart(t, w, ChartLayout::Ashrae);
            let m = to_chart(t, w, ChartLayout::MollierIx);
            assert_eq!(m.x, a.y);
            assert_eq!(m.y, a.x);
        }
        for w in [0.0, 0.005, 0.02] {
            assert!(to_chart(0.0, w, ChartLayout::MollierIx).y.abs() < 1e-12);
        }
    }

    /// The reduced coordinate is exactly `h − h_g,ref·W`, and converts back.
    #[test]
    fn reduced_coordinate_matches_the_enthalpy_relation() {
        for &(t, w) in &[(24.0, 0.0093), (-8.0, 0.001), (45.0, 0.028)] {
            let h = CP_DA * t + w * (H_G_REF + CP_WV * t);
            let sigma = reduced_coordinate(t, w);
            assert!((sigma - reduced_from_enthalpy(h, w)).abs() < 1e-9);
            assert!((enthalpy_from_reduced(sigma, w) - h).abs() < 1e-9);
        }
    }

    #[test]
    fn bounds_enclose_the_window() {
        for layout in LAYOUTS {
            let b = bounds(-10.0, 50.0, 0.0, 0.030, layout);
            let mut t = -10.0_f64;
            while t <= 50.0 {
                let mut w = 0.0_f64;
                while w <= 0.030 {
                    let p = to_chart(t, w, layout);
                    assert!(p.x >= b.x_min - 1e-9 && p.x <= b.x_max + 1e-9);
                    assert!(p.y >= b.y_min - 1e-9 && p.y <= b.y_max + 1e-9);
                    w += 0.005;
                }
                t += 5.0;
            }
        }
    }

    #[test]
    fn straightness_is_declared_per_family() {
        assert!(is_straight(StraightFamily::Enthalpy));
        assert!(is_straight(StraightFamily::DryBulb));
        assert!(is_straight(StraightFamily::HumidityRatio));
        assert!(!is_straight(StraightFamily::RelativeHumidity));
        assert!(!is_straight(StraightFamily::WetBulb));
        assert!(!is_straight(StraightFamily::SpecificVolume));
    }
}
