//! The one window shape every instrument reads.
//!
//! Port of `ChannelWindowDto.java` (62 lines).
//!
//! A window is either **raw** — the requested range fit the point budget, so
//! `v` carries one value per `t` — or **decimated**, in which case `v` is absent
//! and `min`/`max` carry the M4-style envelope from [`super::decimate`]. The two
//! forms share a type on purpose: the frontend `ChannelStore` consumes exactly
//! this shape, so a chart, a cursor readout and the Event List do not each need
//! to know which form they were handed.
//!
//! In the Java tier the shared shape existed so that `RemoteSource` (a window
//! fetched from `/api/measurements/…`) and `InMemorySource` (a window computed
//! in the browser) were interchangeable. This port deletes the remote half —
//! the file never leaves the tab — so the DTO's remaining job is narrower and
//! worth stating plainly: it is the boundary shape between the wasm module and
//! the store, and it stays as it is because the frontend already parses it.
//!
//! `min`/`max` are the whole reason a boolean or enum channel survives zooming
//! out. A bucket containing a single-sample pulse spans both levels, so the
//! envelope keeps the transition that a naive "every Nth sample" reduction
//! would drop — see [`super::decimate`] for the reduction itself.

use crate::measurement::decimate::Envelope;
use crate::measurement::ChannelKind;

/// One channel's samples over a requested time range.
///
/// The Java record hand-wrote `equals`/`hashCode`/`toString` because Java array
/// fields compare by identity; `Vec<f64>` compares structurally, so the derives
/// are the same contract for free.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelWindow {
    /// Sample times in seconds. One entry per raw sample, or one per envelope
    /// bucket when [`ChannelWindow::decimated`] is set.
    pub t: Vec<f64>,
    /// Raw values, present only on an undecimated window.
    pub v: Option<Vec<f64>>,
    /// Per-bucket minimum, present only on a decimated window.
    pub min: Option<Vec<f64>>,
    /// Per-bucket maximum, present only on a decimated window.
    pub max: Option<Vec<f64>>,
    /// Which of the two forms above this is.
    pub decimated: bool,
    /// Samples in the underlying channel, *before* any reduction — what the UI
    /// reports as the recording's true length, not the length of `t`.
    pub total_samples: u64,
    /// The channel's unit as recorded, or `None` when the file carried none.
    pub unit: Option<String>,
    pub kind: ChannelKind,
}

impl ChannelWindow {
    /// The range fit the point budget: hand back the samples themselves.
    pub fn raw(
        t: Vec<f64>,
        v: Vec<f64>,
        total_samples: u64,
        unit: Option<String>,
        kind: ChannelKind,
    ) -> ChannelWindow {
        ChannelWindow {
            t,
            v: Some(v),
            min: None,
            max: None,
            decimated: false,
            total_samples,
            unit,
            kind,
        }
    }

    /// The range did not fit: hand back the min/max envelope.
    pub fn decimated(
        envelope: Envelope,
        total_samples: u64,
        unit: Option<String>,
        kind: ChannelKind,
    ) -> ChannelWindow {
        ChannelWindow {
            t: envelope.t,
            v: None,
            min: Some(envelope.min),
            max: Some(envelope.max),
            decimated: true,
            total_samples,
            unit,
            kind,
        }
    }

    /// Points in this window — buckets when decimated, samples when not.
    pub fn points(&self) -> usize {
        self.t.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_window_carries_values_and_no_envelope() {
        let w = ChannelWindow::raw(
            vec![0.0, 0.5, 1.0],
            vec![10.0, 20.0, 30.0],
            3,
            Some("rpm".into()),
            ChannelKind::Analog,
        );
        assert!(!w.decimated);
        assert_eq!(w.v.as_deref(), Some(&[10.0, 20.0, 30.0][..]));
        assert_eq!(w.min, None);
        assert_eq!(w.max, None);
        assert_eq!(w.total_samples, 3);
        assert_eq!(w.points(), 3);
    }

    /// The envelope moves into the window; `total_samples` keeps reporting the
    /// unreduced length, which is what stops the UI claiming a 2 M-sample
    /// recording is 800 points long.
    #[test]
    fn a_decimated_window_carries_the_envelope_and_no_values() {
        let envelope = Envelope {
            t: vec![0.0, 1.0],
            min: vec![-1.0, 0.0],
            max: vec![1.0, 4.0],
        };
        let w = ChannelWindow::decimated(envelope, 2_000_000, None, ChannelKind::Boolean);
        assert!(w.decimated);
        assert_eq!(w.v, None);
        assert_eq!(w.min.as_deref(), Some(&[-1.0, 0.0][..]));
        assert_eq!(w.max.as_deref(), Some(&[1.0, 4.0][..]));
        assert_eq!(w.total_samples, 2_000_000);
        assert_eq!(w.unit, None);
        assert_eq!(w.kind, ChannelKind::Boolean);
        assert_eq!(w.points(), 2);
    }
}
