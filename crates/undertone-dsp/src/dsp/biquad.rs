// The BiquadFilterNode's lowpass, highpass and allpass, with the specification's coefficients
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The specification's formulas (after the Audio EQ Cookbook): `w0 = 2π f0 / Fs` with
//! `f0 = frequency · 2^(detune/1200)`; the lowpass and highpass take `Q` in decibels
//! (`αQdB = sin w0 / (2 · 10^(Q/20))`, so `Q` 1 is a 1 dB peak at the cutoff) and the allpass
//! takes it as a plain number (`αQ = sin w0 / (2Q)`). A cutoff at or past Nyquist, or at or below
//! 0, takes the specification's limits. The filter runs in direct form I in `f64`, one state a
//! channel.

use std::f64::consts::PI;

use crate::dsp::flush;
use crate::graph::FilterType;

/// Normalised coefficients: `y = b0 x + b1 x1 + b2 x2 − a1 y1 − a2 y2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coefficients {
	/// Feed-forward coefficient for the current sample.
	pub b0: f64,
	/// Feed-forward coefficient for the previous sample.
	pub b1: f64,
	/// Feed-forward coefficient for the sample before that.
	pub b2: f64,
	/// Feedback coefficient for the previous output.
	pub a1: f64,
	/// Feedback coefficient for the output before that.
	pub a2: f64,
}

const PASS: Coefficients = Coefficients {
	b0: 1.0,
	b1: 0.0,
	b2: 0.0,
	a1: 0.0,
	a2: 0.0,
};

const ZERO: Coefficients = Coefficients {
	b0: 0.0,
	b1: 0.0,
	b2: 0.0,
	a1: 0.0,
	a2: 0.0,
};

/// The coefficients for a filter type at `frequency` Hz (detune applied) and `q`.
pub fn coefficients(kind: FilterType, frequency: f64, q: f64, sample_rate: f64) -> Coefficients {
	let nyquist = sample_rate / 2.0;
	let f = (frequency / nyquist).clamp(0.0, 1.0);
	match kind {
		FilterType::Lowpass if f >= 1.0 => return PASS,
		FilterType::Lowpass if f <= 0.0 => return ZERO,
		FilterType::Highpass if f >= 1.0 => return ZERO,
		FilterType::Highpass if f <= 0.0 => return PASS,
		FilterType::Allpass if f <= 0.0 || f >= 1.0 => return PASS,
		_ => {}
	}
	let w0 = PI * f;
	let (sin, cos) = w0.sin_cos();
	let (b0, b1, b2, a0, a1, a2) = match kind {
		FilterType::Lowpass => {
			let alpha = sin / (2.0 * 10f64.powf(q / 20.0));
			let b1 = 1.0 - cos;
			(b1 / 2.0, b1, b1 / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
		}
		FilterType::Highpass => {
			let alpha = sin / (2.0 * 10f64.powf(q / 20.0));
			let b0 = (1.0 + cos) / 2.0;
			(b0, -(1.0 + cos), b0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
		}
		FilterType::Allpass => {
			if q <= 0.0 {
				return Coefficients { b0: -1.0, ..PASS };
			}
			let alpha = sin / (2.0 * q);
			(
				1.0 - alpha,
				-2.0 * cos,
				1.0 + alpha,
				1.0 + alpha,
				-2.0 * cos,
				1.0 - alpha,
			)
		}
	};
	Coefficients {
		b0: b0 / a0,
		b1: b1 / a0,
		b2: b2 / a0,
		a1: a1 / a0,
		a2: a2 / a0,
	}
}

impl Coefficients {
	/// `|H(e^{iω})|` at `frequency` Hz.
	pub fn magnitude(&self, frequency: f64, sample_rate: f64) -> f64 {
		let w = 2.0 * PI * frequency / sample_rate;
		let z1 = (w.cos(), -w.sin());
		let z2 = ((2.0 * w).cos(), -(2.0 * w).sin());
		let num = (
			self.b0 + self.b1 * z1.0 + self.b2 * z2.0,
			self.b1 * z1.1 + self.b2 * z2.1,
		);
		let den = (
			1.0 + self.a1 * z1.0 + self.a2 * z2.0,
			self.a1 * z1.1 + self.a2 * z2.1,
		);
		(num.0.hypot(num.1)) / (den.0.hypot(den.1))
	}
}

/// One channel's filter memory.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct State {
	x1: f64,
	x2: f64,
	y1: f64,
	y2: f64,
}

impl State {
	/// Filters one sample.
	#[inline]
	pub fn tick(&mut self, c: &Coefficients, x: f64) -> f64 {
		let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
		self.x2 = self.x1;
		self.x1 = x;
		self.y2 = self.y1;
		self.y1 = flush(y);
		self.y1
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_lowpass_peaks_by_q_decibels_at_its_cutoff() {
		let rate = 48_000.0;
		let c = coefficients(FilterType::Lowpass, 1_000.0, 1.0, rate);
		let db = 20.0 * c.magnitude(1_000.0, rate).log10();
		assert!((db - 1.0).abs() < 1e-9, "{db}");
		assert!((c.magnitude(1.0, rate) - 1.0).abs() < 1e-6);
		let flat = coefficients(FilterType::Lowpass, 1_000.0, 0.0, rate);
		assert!((flat.magnitude(1_000.0, rate) - 1.0).abs() < 1e-9);
	}

	#[test]
	fn a_highpass_mirrors_it_and_an_allpass_is_flat() {
		let rate = 44_100.0;
		let c = coefficients(FilterType::Highpass, 2_000.0, 1.0, rate);
		assert!((20.0 * c.magnitude(2_000.0, rate).log10() - 1.0).abs() < 1e-9);
		assert!(c.magnitude(20_000.0, rate) > 0.99);
		let a = coefficients(FilterType::Allpass, 1_000.0, 1.0, rate);
		for f in [50.0, 1_000.0, 10_000.0] {
			assert!((a.magnitude(f, rate) - 1.0).abs() < 1e-9);
		}
	}

	#[test]
	fn a_filter_settles_to_the_magnitude_on_a_sine() {
		let rate = 48_000.0;
		let c = coefficients(FilterType::Lowpass, 500.0, 1.0, rate);
		let mut state = State::default();
		let mut peak = 0f64;
		for n in 0..48_000 {
			let x = (2.0 * PI * 500.0 * n as f64 / rate).sin();
			let y = state.tick(&c, x);
			if n > 24_000 {
				peak = peak.max(y.abs());
			}
		}
		assert!((peak - c.magnitude(500.0, rate)).abs() < 1e-3);
	}
}
