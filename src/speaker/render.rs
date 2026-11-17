// Renders the PC speaker's frame-by-frame pitches as a band-limited square wave through a small speaker
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/engines/speaker/render.ts`. The filter state runs in doubles and each
//! output sample is rounded to `f32`, as the TypeScript does through its `Float32Array`.

use super::sequence::SPEAKER_FRAME_RATE;
use crate::chip::RenderedSound;
use crate::numbers::round;

const LEVEL: f64 = 0.18;
/// The cone rolls off highs and can't move much air at the bottom.
const LOWPASS_HZ: f64 = 7000.0;
const HIGHPASS_HZ: f64 = 90.0;

/// PolyBLEP correction near a step, which keeps the square wave from aliasing badly.
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Renders one pass; the phase runs on across frames so pitch changes don't click.
pub fn render_speaker(frames: &[f32], sample_rate: u32) -> RenderedSound {
    let rate = f64::from(sample_rate);
    let samples_per_frame = rate / SPEAKER_FRAME_RATE;
    let length = round(frames.len() as f64 * samples_per_frame).max(1.0) as usize;
    let mut out = vec![0f32; length];
    let low_a = 1.0 - (-2.0 * std::f64::consts::PI * LOWPASS_HZ / rate).exp();
    let high_a = (-2.0 * std::f64::consts::PI * HIGHPASS_HZ / rate).exp();
    let mut phase = 0.0f64;
    let mut low = 0.0f64;
    let mut high_in = 0.0f64;
    let mut high_out = 0.0f64;
    for (i, slot) in out.iter_mut().enumerate() {
        let at = ((i as f64 / samples_per_frame).floor() as usize).min(frames.len() - 1);
        let hz = f64::from(frames[at]);
        let mut sample = 0.0;
        if hz > 0.0 {
            let dt = hz / rate;
            sample = if phase < 0.5 { 1.0 } else { -1.0 };
            sample += poly_blep(phase, dt);
            sample -= poly_blep((phase + 0.5) % 1.0, dt);
            phase = (phase + dt) % 1.0;
        }
        low += low_a * (sample * LEVEL - low);
        high_out = high_a * (high_out + low - high_in);
        high_in = low;
        *slot = high_out as f32;
    }
    RenderedSound {
        sample_rate,
        left: out.clone(),
        right: out,
    }
}
