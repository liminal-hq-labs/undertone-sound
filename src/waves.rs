// The shaped waveforms (half-sine, abs-sine, pulse-sine) as band-limited periodic waves
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `shapedWaveTable` from Undertone's `waves.ts`: each shape's first 64 Fourier coefficients from a
//! 4096-point analysis of one period, the DC term left at 0. The sums run in doubles in the same
//! order as the TypeScript and the coefficients are stored as `f32`.

use std::f64::consts::PI;

use crate::math;
use crate::params::SoundType;

/// Harmonics kept: the band limit.
pub const HARMONICS: usize = 64;

/// Points a period for the analysis.
const ANALYSIS_SAMPLES: usize = 4096;

/// One period of a shape at `theta` in `[0, 2π)`.
fn shape(kind: SoundType, theta: f64) -> f64 {
    let s = math::sin(theta);
    match kind {
        SoundType::HalfSine => math::max(s, 0.0),
        SoundType::AbsSine => s.abs(),
        // The rising quarter of each half-cycle of |sine|, then silence.
        _ => {
            if theta % PI < PI / 2.0 {
                s.abs()
            } else {
                0.0
            }
        }
    }
}

/// The real and imaginary coefficients of a shaped wave, `HARMONICS + 1` of each.
pub fn shaped_wave_table(kind: SoundType) -> (Vec<f32>, Vec<f32>) {
    let samples: Vec<f64> = (0..ANALYSIS_SAMPLES)
        .map(|n| shape(kind, (2.0 * PI * n as f64) / ANALYSIS_SAMPLES as f64))
        .collect();
    let mut real = vec![0.0f32; HARMONICS + 1];
    let mut imag = vec![0.0f32; HARMONICS + 1];
    for k in 1..=HARMONICS {
        let mut cos_sum = 0.0;
        let mut sin_sum = 0.0;
        for (n, sample) in samples.iter().enumerate() {
            let angle = (2.0 * PI * k as f64 * n as f64) / ANALYSIS_SAMPLES as f64;
            cos_sum += sample * math::cos(angle);
            sin_sum += sample * math::sin(angle);
        }
        real[k] = ((2.0 * cos_sum) / ANALYSIS_SAMPLES as f64) as f32;
        imag[k] = ((2.0 * sin_sum) / ANALYSIS_SAMPLES as f64) as f32;
    }
    (real, imag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_sine_is_half_its_fundamental() {
        let (real, imag) = shaped_wave_table(SoundType::HalfSine);
        assert_eq!(real.len(), HARMONICS + 1);
        assert_eq!(real[0], 0.0);
        assert!((imag[1] - 0.5).abs() < 1e-6);
        assert!((real[2] + 0.212_206_7).abs() < 1e-6);
    }

    #[test]
    fn an_abs_sine_has_only_even_harmonics() {
        let (real, _) = shaped_wave_table(SoundType::AbsSine);
        assert!(real[1].abs() < 1e-6);
        assert!(real[2].abs() > 0.1);
    }
}
