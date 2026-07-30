//! Discrete-signal kernels: `FFT` / `IFFT` / `Convolve`.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/core/SignalProcessing.java`
//! (62 LOC). Like the Java, the transform is a **direct DFT** (O(n²)) — exact
//! for any length n with no power-of-two restriction, trading asymptotic speed
//! for correctness and simplicity at the modest array sizes of a declarative
//! engineering document.
//!
//! All trigonometry goes through [`libm`] so native and wasm runs agree bit
//! for bit (the crate-wide determinism rule).

use crate::diag::{FreesError, Result};

/// Discrete Fourier transform of the complex sequence `re + i·im`.
/// Returns `(out_re, out_im)`. When `inverse` is true, computes the inverse
/// transform (including the 1/n normalization).
pub fn dft(re: &[f64], im: &[f64], inverse: bool) -> Result<(Vec<f64>, Vec<f64>)> {
    let n = re.len();
    if im.len() != n {
        return Err(FreesError::evaluation(
            "FFT real and imaginary parts must have equal length.",
        ));
    }
    let mut out_re = vec![0.0; n];
    let mut out_im = vec![0.0; n];
    let sign = if inverse { 1.0 } else { -1.0 };
    for k in 0..n {
        let mut sum_re = 0.0;
        let mut sum_im = 0.0;
        for j in 0..n {
            let angle = sign * 2.0 * std::f64::consts::PI * (j as f64) * (k as f64) / (n as f64);
            let cos = libm::cos(angle);
            let sin = libm::sin(angle);
            sum_re += re[j] * cos - im[j] * sin;
            sum_im += re[j] * sin + im[j] * cos;
        }
        if inverse {
            sum_re /= n as f64;
            sum_im /= n as f64;
        }
        out_re[k] = sum_re;
        out_im[k] = sum_im;
    }
    Ok((out_re, out_im))
}

/// Linear convolution of `a` and `b` (length `a.len() + b.len() − 1`).
///
/// The Java computes `new double[m + n - 1]`, which for two empty inputs is a
/// negative array size (an unchecked crash); this port refuses empty input
/// with an explicit error instead.
pub fn convolve(a: &[f64], b: &[f64]) -> Result<Vec<f64>> {
    let m = a.len();
    let n = b.len();
    if m == 0 || n == 0 {
        return Err(FreesError::evaluation(
            "Convolve requires two non-empty sequences.",
        ));
    }
    let mut c = vec![0.0; m + n - 1];
    for (i, &ai) in a.iter().enumerate() {
        for (j, &bj) in b.iter().enumerate() {
            c[i + j] += ai * bj;
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!(
            (a - b).abs() <= 1e-12 * b.abs().max(1.0),
            "expected {b}, got {a}"
        );
    }

    #[test]
    fn dft_of_a_unit_impulse_is_flat() {
        let (re, im) = dft(&[1.0, 0.0, 0.0, 0.0], &[0.0; 4], false).unwrap();
        for k in 0..4 {
            close(re[k], 1.0);
            close(im[k], 0.0);
        }
    }

    #[test]
    fn dft_known_values_for_a_small_real_sequence() {
        // X = DFT([0, 1, 0, 1]) = [2, 0, −2, 0] (all real).
        let (re, im) = dft(&[0.0, 1.0, 0.0, 1.0], &[0.0; 4], false).unwrap();
        let expected = [2.0, 0.0, -2.0, 0.0];
        for k in 0..4 {
            close(re[k], expected[k]);
            close(im[k], 0.0);
        }
    }

    #[test]
    fn inverse_dft_round_trips_and_normalizes() {
        let x_re = [3.0, -1.0, 0.5, 2.25, -7.0];
        let x_im = [0.0, 1.0, -2.0, 0.0, 4.5];
        let (f_re, f_im) = dft(&x_re, &x_im, false).unwrap();
        let (b_re, b_im) = dft(&f_re, &f_im, true).unwrap();
        for k in 0..5 {
            close(b_re[k], x_re[k]);
            close(b_im[k], x_im[k]);
        }
    }

    #[test]
    fn dft_rejects_mismatched_component_lengths() {
        let err = dft(&[1.0, 2.0], &[0.0], false).unwrap_err().to_string();
        assert!(err.contains("equal length"), "{err}");
    }

    #[test]
    fn convolve_known_values() {
        // [1,2,3] * [4,5] = [4, 13, 22, 15]
        let c = convolve(&[1.0, 2.0, 3.0], &[4.0, 5.0]).unwrap();
        assert_eq!(c.len(), 4);
        close(c[0], 4.0);
        close(c[1], 13.0);
        close(c[2], 22.0);
        close(c[3], 15.0);
    }

    #[test]
    fn convolve_with_a_unit_impulse_is_identity() {
        let c = convolve(&[1.0], &[7.0, -2.0, 0.5]).unwrap();
        assert_eq!(c, vec![7.0, -2.0, 0.5]);
    }

    #[test]
    fn convolve_refuses_empty_input() {
        let err = convolve(&[], &[1.0]).unwrap_err().to_string();
        assert!(err.contains("non-empty"), "{err}");
    }
}
