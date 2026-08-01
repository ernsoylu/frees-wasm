//! Measured-data analysis: MDF4 reading, resampling, decimation and
//! calculated signals.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/measurement/`
//! (11 files, ~1.1k LOC). The Java tier ran this **server-side**: a `.mf4` was
//! uploaded to `/api/measurements`, indexed on disk, and read back as windowed
//! decimated envelopes so the browser never held the raw file.
//!
//! In this port the reasoning inverts. The file is *already* on the user's
//! machine, so the only reason to upload it was that the parser lived on the
//! server. With the parser in wasm the upload is pure cost — and pure risk,
//! because measurement recordings are frequently confidential. **A `.mf4`
//! opened here never leaves the tab.** That is the single most user-visible
//! consequence of the whole port, and it is why the Java `Mf4Parser` →
//! `FallbackMeasurementParser` → Python `mdf-sidecar` ladder collapses to one
//! rung: there is no second process to fall back to, and nothing to fall back
//! *for* — see `mdf4.rs` for what that costs in format coverage.
//!
//! # Contract
//!
//! These types are the fixed interface between the submodules and the wasm
//! boundary. They mirror the Java records so the existing frontend DTOs
//! (`web/src/analyzer/measurementApi.ts`) keep parsing unchanged:
//!
//! | Rust | Java |
//! |---|---|
//! | [`MeasurementMetadata`] | `MeasurementMetadata` |
//! | [`GroupInfo`] / [`ChannelInfo`] | its two nested records |
//! | [`ChannelData`] | `ChannelData` |
//! | [`series::SampledSeries`] | `SampledSeries` |
//! | [`decimate::Envelope`] | `EnvelopeDecimator.Envelope` |
//! | [`window::ChannelWindow`] | `ChannelWindowDto` |
//! | [`MeasurementError`] | `MeasurementParseException` |
//!
//! Time is **always seconds**, values are **always `f64`**, and a gap is
//! `NaN` — never an absent sample. Interpolation must not bridge a `NaN`
//! (`SampledSeries::at` in the Java is explicit about this: "gaps stay gaps").

pub mod calc;
pub mod decimate;
pub mod mdf4;
pub mod raster;
pub mod series;
pub mod window;

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

/// What a channel's samples are, for the frontend's rendering choice.
///
/// The Java carried this as a bare `String` ("analog" / "boolean" / "string");
/// the wire form keeps those spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Analog,
    Boolean,
    Text,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelKind::Analog => "analog",
            ChannelKind::Boolean => "boolean",
            ChannelKind::Text => "string",
        }
    }
}

/// One channel's identity within a group.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub name: String,
    pub unit: Option<String>,
    /// The group's time master. Exactly one channel per group should have it.
    pub time_master: bool,
    pub kind: ChannelKind,
}

/// One channel group — an independently rastered set of channels.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupInfo {
    pub index: usize,
    pub name: String,
    pub records: u64,
    pub channels: Vec<ChannelInfo>,
}

/// Everything readable from a file's headers, without touching sample data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeasurementMetadata {
    pub groups: Vec<GroupInfo>,
}

/// One channel's samples, widened to `f64`, paired with its time master.
///
/// `time` and `values` are the same length; an unreadable sample is `NaN`
/// (the Java's `AS_DOUBLE` visitor did the same) so index alignment with the
/// time master always holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChannelData {
    pub time: Vec<f64>,
    pub values: Vec<f64>,
}

impl ChannelData {
    pub fn len(&self) -> usize {
        self.time.len()
    }

    pub fn is_empty(&self) -> bool {
        self.time.is_empty()
    }
}

/// The Java `MeasurementParser` interface, minus the `Path` — a browser has
/// bytes, not a filesystem.
pub trait MeasurementSource {
    fn metadata(&self) -> Result<MeasurementMetadata>;
    fn channel(&self, group_index: usize, channel_name: &str) -> Result<ChannelData>;
}
