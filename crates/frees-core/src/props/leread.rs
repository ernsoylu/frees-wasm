//! Little-endian primitive reads, shared by the generated-artifact decoders.
//!
//! Both offline generators write the same way — `tools/table-gen` for
//! `FRPHTAB1` and `tools/aux-gen` for `FRAUX1` — so both readers
//! ([`satsplit`](super::satsplit) and [`auxtable`](super::auxtable)) decode the
//! same way, and that agreement belongs in one place rather than in two
//! independently-maintained copies of four functions.
//!
//! Every read here is **bounds-checked by the slice index**, which is the
//! contract the callers rely on: they validate the declared payload size
//! against `bytes.len()` before decoding, and a slip in that arithmetic must
//! come back as a panic-free `Result` from the caller rather than a silent
//! read of adjacent memory. Nothing in this module uses `unsafe`.

/// A `u16` at `at`.
pub(crate) fn u16_at(bytes: &[u8], at: usize) -> u16 {
    let mut buf = [0u8; 2];
    buf.copy_from_slice(&bytes[at..at + 2]);
    u16::from_le_bytes(buf)
}

/// A `u32` at `at`.
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(buf)
}

/// An `f64` at `at`.
pub(crate) fn f64_at(bytes: &[u8], at: usize) -> f64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[at..at + 8]);
    f64::from_le_bytes(buf)
}

/// `n` consecutive `f64`s from `*at`, advancing it.
pub(crate) fn f64_block(bytes: &[u8], at: &mut usize, n: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(f64_at(bytes, *at));
        *at += 8;
    }
    out
}

/// `n` elements from `*at` widened to `f64`, reading `f32` or `f64` per
/// `f32_elems`, and advancing `at`.
///
/// The payload is deliberately **not** zero-copy castable: a fetched `Vec<u8>`
/// is 1-byte aligned and casting it would need `unsafe`, which this port does
/// not use.
pub(crate) fn widened_block(bytes: &[u8], at: &mut usize, n: usize, f32_elems: bool) -> Vec<f64> {
    if !f32_elems {
        return f64_block(bytes, at, n);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&bytes[*at..*at + 4]);
        out.push(f64::from(f32::from_le_bytes(buf)));
        *at += 4;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_primitives_read_what_the_generators_write() {
        // Byte patterns as the Java `Buf` emits them: little-endian, unpadded.
        let bytes = [0x34u8, 0x12, 0x78, 0x56, 0x34, 0x12];
        assert_eq!(u16_at(&bytes, 0), 0x1234);
        assert_eq!(u32_at(&bytes, 2), 0x1234_5678);

        let pi = std::f64::consts::PI.to_le_bytes();
        assert_eq!(f64_at(&pi, 0), std::f64::consts::PI);
    }

    /// `f32` and `f64` payloads land on the same values through one entry
    /// point — this is the property that let two decoders share it.
    #[test]
    fn a_block_widens_f32_and_passes_f64_through() {
        let mut raw = Vec::new();
        for v in [1.5f32, -2.25, 1e-6] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        let mut at = 0;
        assert_eq!(
            widened_block(&raw, &mut at, 3, true),
            vec![1.5, -2.25, f64::from(1e-6f32)]
        );
        assert_eq!(at, 12);

        let mut raw = Vec::new();
        for v in [1.5f64, -2.25, 1e-6] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        let mut at = 0;
        assert_eq!(
            widened_block(&raw, &mut at, 3, false),
            vec![1.5, -2.25, 1e-6]
        );
        assert_eq!(at, 24);
    }
}
