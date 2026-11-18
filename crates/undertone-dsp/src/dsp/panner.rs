// The StereoPannerNode's equal-power law, for a mono and a stereo input, as the specification has it
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! A mono input at pan `p` goes left by `cos(x·π/2)` and right by `sin(x·π/2)` with
//! `x = (p + 1) / 2`. A stereo input keeps its own side and moves the other into it: for `p ≤ 0`,
//! `x = p + 1`, left is `L + R·cos(x·π/2)` and right is `R·sin(x·π/2)`; for `p > 0`, `x = p`, left
//! is `L·cos(x·π/2)` and right is `R + L·sin(x·π/2)`.

use std::f64::consts::FRAC_PI_2;

/// One frame panned: `(left, right)`.
#[inline]
pub fn pan_frame(pan: f32, channels: usize, left: f32, right: f32) -> (f32, f32) {
    let pan = f64::from(pan.clamp(-1.0, 1.0));
    if channels == 1 {
        let x = (pan + 1.0) / 2.0;
        let (gain_r, gain_l) = (x * FRAC_PI_2).sin_cos();
        return (
            (f64::from(left) * gain_l) as f32,
            (f64::from(left) * gain_r) as f32,
        );
    }
    let (l, r) = (f64::from(left), f64::from(right));
    if pan <= 0.0 {
        let (gain_r, gain_l) = ((pan + 1.0) * FRAC_PI_2).sin_cos();
        ((l + r * gain_l) as f32, (r * gain_r) as f32)
    } else {
        let (gain_r, gain_l) = (pan * FRAC_PI_2).sin_cos();
        ((l * gain_l) as f32, (r + l * gain_r) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mono_input_keeps_its_power_across_the_field() {
        for pan in [-1.0f32, -0.5, 0.0, 0.3, 1.0] {
            let (l, r) = pan_frame(pan, 1, 1.0, 0.0);
            assert!((l * l + r * r - 1.0).abs() < 1e-6, "{pan}");
        }
        let (l, r) = pan_frame(0.0, 1, 1.0, 0.0);
        assert!((l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-7 && (l - r).abs() < 1e-7);
        assert_eq!(pan_frame(-1.0, 1, 1.0, 0.0), (1.0, 0.0));
    }

    #[test]
    fn a_stereo_input_passes_at_the_centre_and_folds_to_one_side() {
        assert_eq!(pan_frame(0.0, 2, 0.25, 0.75), (0.25, 0.75));
        let (l, r) = pan_frame(-1.0, 2, 0.25, 0.75);
        assert!((l - 1.0).abs() < 1e-7 && r.abs() < 1e-7);
        let (l, r) = pan_frame(1.0, 2, 0.25, 0.75);
        assert!(l.abs() < 1e-7 && (r - 1.0).abs() < 1e-7);
    }
}
