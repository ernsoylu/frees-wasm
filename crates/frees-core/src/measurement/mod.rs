//! Measured-data analysis: resampling, decimation and calculated signals.
//!
//! Port of the numeric half of
//! `../frEES/backend/core/src/main/java/com/frees/backend/measurement/`.
//! MDF4 reading was ported in Phase 10 and then **removed outright** in
//! decision D6 (`docs/decisions/0006-remove-mdf4.md`) — measured data now
//! enters the app as CSV, parsed on the browser's main thread, and reaches
//! this module as inline series through the wasm boundary's
//! `measurement_calc`. Nothing here holds a recording; these are pure
//! functions over sampled columns.
//!
//! # Contract
//!
//! | Rust | Java |
//! |---|---|
//! | [`series::SampledSeries`] | `SampledSeries` |
//! | [`decimate::Envelope`] | `EnvelopeDecimator.Envelope` |
//! | [`MeasurementError`] | `MeasurementParseException` |
//!
//! Time is **always seconds**, values are **always `f64`**, and a gap is
//! `NaN` — never an absent sample. Interpolation must not bridge a `NaN`
//! (`SampledSeries::at` in the Java is explicit about this: "gaps stay gaps").

pub mod calc;
pub mod decimate;
pub mod raster;
pub mod series;

use core::fmt;

/// Everything that can go wrong reading or evaluating measured data.
///
/// The Java threw `MeasurementParseException` for all of these and let the
/// controller map it to a typed JSON body. Here the variants *are* the type —
/// the wasm boundary renders `code()` into the `error` payload the frontend
/// already switches on (`RASTER_CAP_EXCEEDED` is the one it handles specially,
/// by offering the suggested `dt`).
#[derive(Debug, Clone, PartialEq)]
pub enum MeasurementError {
    /// The bytes are not a readable MDF4 file, or use a feature this reader
    /// does not implement. The message must say **which**, because the user's
    /// only remedy is to re-export the recording.
    Parse(String),
    /// The document referenced a group or channel that is not in the file.
    NotFound(String),
    /// A calculated-signal formula failed to parse, or referenced an input
    /// that was not bound.
    Formula(String),
    /// The merged raster would exceed the point cap. Carries the numbers the
    /// frontend needs to offer a fix rather than just refuse.
    RasterCapExceeded {
        actual_points: u64,
        suggested_dt: f64,
        cap: u32,
    },
}

impl MeasurementError {
    /// Stable machine-readable tag, for the `error.code` field on the wire.
    pub fn code(&self) -> &'static str {
        match self {
            MeasurementError::Parse(_) => "MEASUREMENT_PARSE_FAILED",
            MeasurementError::NotFound(_) => "CHANNEL_NOT_FOUND",
            MeasurementError::Formula(_) => "FORMULA_ERROR",
            MeasurementError::RasterCapExceeded { .. } => "RASTER_CAP_EXCEEDED",
        }
    }
}

impl fmt::Display for MeasurementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeasurementError::Parse(m) => write!(f, "{m}"),
            MeasurementError::NotFound(m) => write!(f, "{m}"),
            MeasurementError::Formula(m) => write!(f, "{m}"),
            // Worded as the Java worded it: state the cap, then the way out.
            MeasurementError::RasterCapExceeded {
                actual_points,
                suggested_dt,
                cap,
            } => write!(
                f,
                "The merged raster has {actual_points} points, above the {cap}-point cap. \
                 Use a fixed sample interval of dt = {suggested_dt} s (or coarser) instead."
            ),
        }
    }
}

impl core::error::Error for MeasurementError {}

pub type Result<T> = core::result::Result<T, MeasurementError>;
