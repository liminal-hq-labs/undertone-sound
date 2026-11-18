// The DynamicsCompressorNode as the specification describes it and Chromium's kernel computes it
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The algorithm of the specification's DynamicsCompressorNode section, which Chromium's
//! `DynamicsCompressorKernel` (and Firefox, which shares it) implements, in `f32` as they do:
//!
//! - a 6 ms pre-delay (`floor(0.006 · sampleRate)` frames): the gain is worked out from the
//!   undelayed input and applied to the delayed one;
//! - the static curve: linear to the threshold, an exponential knee whose `k` is found by
//!   bisection so its slope reaches `1/ratio` at `threshold + knee`, then a constant ratio;
//! - a detector that follows the curve's attenuation instantly down and releases up over 2.5 ms;
//! - the compressor gain, recomputed every 32 frames: an attack toward the detector's level at a
//!   rate set by the attack time and the largest difference seen in the attack, or an adaptive
//!   release (a fourth-order curve through the release zones 0.09, 0.16, 0.42 and 0.98 of the
//!   release time) that releases faster the more it is compressing;
//! - a sine warp of the gain, and the automatic make-up gain `(1 / curve(1))^0.6`.
//!
//! The detector starts at 0 and the gain at 1, as the specification's internal slots do, so a
//! sound at the very start of a render is turned down for a few milliseconds while the detector
//! rises. The params are k-rate. Undertone's master bus cancels the make-up with a gain after it.

/// The pre-delay, in seconds.
const PRE_DELAY: f32 = 0.006;
/// Frames between recomputations of the envelope rate.
const DIVISION: usize = 32;
/// The detector's release.
const SAT_RELEASE: f32 = 0.0025;
/// Fractions of the release time the adaptive release passes through.
const RELEASE_ZONES: [f32; 4] = [0.09, 0.16, 0.42, 0.98];
/// The decibels between the adaptive release's points.
const DB_SPACING: f32 = 5.0;
/// The longest pre-delay the buffer holds, a power of two.
const MAX_PRE_DELAY: usize = 1024;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

fn db_to_linear(db: f32) -> f32 {
	10f32.powf(0.05 * db)
}

fn linear_to_db(linear: f32) -> f32 {
	20.0 * linear.log10()
}

/// The compressor's settings for one render quantum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
	/// Level in decibels above which the compressor acts.
	pub threshold: f32,
	/// Width in decibels of the soft transition around the threshold.
	pub knee: f32,
	/// Input change in decibels for a 1 dB change at the output above the knee.
	pub ratio: f32,
	/// Seconds to reduce the gain by 10 dB.
	pub attack: f32,
	/// Seconds to raise the gain by 10 dB.
	pub release: f32,
}

impl Default for Settings {
	/// Web Audio's defaults.
	fn default() -> Self {
		Settings {
			threshold: -24.0,
			knee: 30.0,
			ratio: 12.0,
			attack: 0.003,
			release: 0.25,
		}
	}
}

/// The static curve for one threshold, knee and ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
	threshold_db: f32,
	knee_db: f32,
	ratio: f32,
	linear_threshold: f32,
	slope: f32,
	knee_threshold: f32,
	knee_threshold_db: f32,
	y_knee_threshold_db: f32,
	k: f32,
}

impl Curve {
	/// The static curve for a threshold, knee and ratio.
	pub fn new(threshold_db: f32, knee_db: f32, ratio: f32) -> Self {
		let mut curve = Curve {
			threshold_db,
			knee_db,
			ratio,
			linear_threshold: db_to_linear(threshold_db),
			slope: 1.0 / ratio,
			knee_threshold: 0.0,
			knee_threshold_db: 0.0,
			y_knee_threshold_db: 0.0,
			k: 0.0,
		};
		let k = curve.k_at_slope(1.0 / ratio);
		curve.knee_threshold_db = threshold_db + knee_db;
		curve.knee_threshold = db_to_linear(curve.knee_threshold_db);
		curve.y_knee_threshold_db = linear_to_db(curve.knee_curve(curve.knee_threshold, k));
		curve.k = k;
		curve
	}

	fn knee_curve(&self, x: f32, k: f32) -> f32 {
		if x < self.linear_threshold {
			return x;
		}
		self.linear_threshold + (1.0 - (-k * (x - self.linear_threshold)).exp()) / k
	}

	/// The output level for an input level, both linear.
	pub fn saturate(&self, x: f32) -> f32 {
		if x < self.knee_threshold {
			self.knee_curve(x, self.k)
		} else {
			let x_db = linear_to_db(x);
			let y_db = self.y_knee_threshold_db + self.slope * (x_db - self.knee_threshold_db);
			db_to_linear(y_db)
		}
	}

	fn slope_at(&self, x: f32, k: f32) -> f32 {
		if x < self.linear_threshold {
			return 1.0;
		}
		let x2 = x * 1.001;
		let x_db = linear_to_db(x);
		let x2_db = linear_to_db(x2);
		let y_db = linear_to_db(self.knee_curve(x, k));
		let y2_db = linear_to_db(self.knee_curve(x2, k));
		(y2_db - y_db) / (x2_db - x_db)
	}

	fn k_at_slope(&self, desired: f32) -> f32 {
		let x = db_to_linear(self.threshold_db + self.knee_db);
		let (mut min_k, mut max_k, mut k) = (0.1f32, 10_000f32, 5f32);
		for _ in 0..15 {
			if self.slope_at(x, k) < desired {
				max_k = k;
			} else {
				min_k = k;
			}
			k = (min_k * max_k).sqrt();
		}
		k
	}

	fn same(&self, settings: &Settings) -> bool {
		self.threshold_db == settings.threshold
			&& self.knee_db == settings.knee
			&& self.ratio == settings.ratio
	}
}

/// A compressor's state across quanta.
#[derive(Debug, Clone)]
pub struct Compressor {
	sample_rate: f32,
	curve: Curve,
	detector_average: f32,
	compressor_gain: f32,
	max_attack_diff_db: f32,
	delay: [Vec<f32>; 2],
	read: usize,
	write: usize,
}

impl Compressor {
	/// A compressor at `sample_rate` with Web Audio's default settings.
	pub fn new(sample_rate: f64) -> Self {
		let sample_rate = sample_rate as f32;
		let frames = ((PRE_DELAY * sample_rate) as usize).min(MAX_PRE_DELAY - 1);
		let defaults = Settings::default();
		Compressor {
			sample_rate,
			curve: Curve::new(defaults.threshold, defaults.knee, defaults.ratio),
			detector_average: 0.0,
			compressor_gain: 1.0,
			max_attack_diff_db: -1.0,
			delay: [vec![0.0; MAX_PRE_DELAY], vec![0.0; MAX_PRE_DELAY]],
			read: 0,
			write: frames,
		}
	}

	/// Compresses two channels in place (a mono input arrives copied to both). `frames` is a
	/// multiple of 32. `gains`, when given, receives the gain each output frame was multiplied by
	/// (the make-up included); output frame `n` is input frame `n − latency()` times it.
	// The release curve's coefficients are written to the digits Chromium writes them.
	#[allow(clippy::excessive_precision)]
	pub fn process(
		&mut self,
		settings: Settings,
		channels: [&mut [f32]; 2],
		mut gains: Option<&mut [f32]>,
	) {
		if !self.curve.same(&settings) {
			self.curve = Curve::new(settings.threshold, settings.knee, settings.ratio);
		}
		let curve = self.curve;
		let makeup = (1.0 / curve.saturate(1.0)).powf(0.6);
		let sample_rate = self.sample_rate;
		let attack_frames = settings.attack.max(0.001) * sample_rate;
		let release_frames = sample_rate * settings.release;
		let sat_release_frames = SAT_RELEASE * sample_rate;
		let [y1, y2, y3, y4] = RELEASE_ZONES.map(|zone| release_frames * zone);
		// A fourth-order polynomial through (0, y1), (1, y2), (2, y3), (3, y4).
		let a = 0.999_999_999_999_999_8_f32 * y1 + 1.843_221_968_432_392_3e-16_f32 * y2
			- 1.937_339_435_167_642_3e-16_f32 * y3
			+ 8.824_516_011_816_245e-18_f32 * y4;
		let b = -1.578_832_035_284_588_8_f32 * y1 + 2.330_583_703_207_428_6_f32 * y2
			- 0.914_119_420_484_042_9_f32 * y3
			+ 0.162_367_752_561_203_2_f32 * y4;
		let c = 0.533_414_286_910_642_4_f32 * y1 - 1.272_736_789_213_631_f32 * y2
			+ 0.925_885_604_220_751_2_f32 * y3
			- 0.186_563_101_917_762_26_f32 * y4;
		let d = 0.087_834_631_382_072_34_f32 * y1 - 0.169_416_296_792_562_2_f32 * y2
			+ 0.085_880_579_515_952_72_f32 * y3
			- 0.004_298_914_105_462_83_f32 * y4;
		let e = -0.042_416_883_008_123_074_f32 * y1 + 0.111_569_382_798_760_2_f32 * y2
			- 0.097_646_763_252_658_72_f32 * y3
			+ 0.028_494_263_462_021_576_f32 * y4;

		let [left, right] = channels;
		let frames = left.len();
		let mask = MAX_PRE_DELAY - 1;
		let mut frame = 0;
		while frame < frames {
			if !self.detector_average.is_finite() {
				self.detector_average = 1.0;
			}
			let desired_gain = self.detector_average;
			let scaled_desired_gain = desired_gain.asin() / HALF_PI;
			let releasing = scaled_desired_gain > self.compressor_gain;
			let mut diff_db = linear_to_db(self.compressor_gain / scaled_desired_gain);
			let envelope_rate = if releasing {
				self.max_attack_diff_db = -1.0;
				if !diff_db.is_finite() {
					diff_db = -1.0;
				}
				let x = 0.25 * (diff_db.clamp(-12.0, 0.0) + 12.0);
				let (x2, x3, x4) = (x * x, x * x * x, x * x * x * x);
				let frames = a + b * x + c * x2 + d * x3 + e * x4;
				db_to_linear(DB_SPACING / frames)
			} else {
				if !diff_db.is_finite() {
					diff_db = 1.0;
				}
				if self.max_attack_diff_db == -1.0 || self.max_attack_diff_db < diff_db {
					self.max_attack_diff_db = diff_db;
				}
				let effective = self.max_attack_diff_db.max(0.5);
				1.0 - (0.25 / effective).powf(1.0 / attack_frames)
			};
			let mut detector_average = self.detector_average;
			let mut compressor_gain = self.compressor_gain;
			for _ in 0..DIVISION {
				let (l, r) = (left[frame], right[frame]);
				self.delay[0][self.write] = l;
				self.delay[1][self.write] = r;
				let input = l.abs().max(r.abs());
				let shaped = curve.saturate(input);
				let attenuation = if input <= 0.0001 { 1.0 } else { shaped / input };
				let attenuation_db = (-linear_to_db(attenuation)).max(2.0);
				let db_per_frame = attenuation_db / sat_release_frames;
				let sat_release_rate = db_to_linear(db_per_frame) - 1.0;
				let rate = if attenuation > detector_average {
					sat_release_rate
				} else {
					1.0
				};
				detector_average += (attenuation - detector_average) * rate;
				detector_average = detector_average.min(1.0);
				if !detector_average.is_finite() {
					detector_average = 1.0;
				}
				if envelope_rate < 1.0 {
					compressor_gain += (scaled_desired_gain - compressor_gain) * envelope_rate;
				} else {
					compressor_gain = (compressor_gain * envelope_rate).min(1.0);
				}
				let warped = (f64::from(HALF_PI * compressor_gain)).sin() as f32;
				let total = makeup * warped;
				if let Some(gains) = gains.as_deref_mut() {
					gains[frame] = total;
				}
				left[frame] = self.delay[0][self.read] * total;
				right[frame] = self.delay[1][self.read] * total;
				self.read = (self.read + 1) & mask;
				self.write = (self.write + 1) & mask;
				frame += 1;
			}
			self.detector_average = detector_average;
			self.compressor_gain = compressor_gain;
		}
	}

	/// Frames of pre-delay.
	pub fn latency(&self) -> usize {
		(self.write + MAX_PRE_DELAY - self.read) & (MAX_PRE_DELAY - 1)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_hard_knee_curve_is_threshold_plus_the_rest_over_the_ratio() {
		let curve = Curve::new(-2.0, 0.0, 20.0);
		for x_db in [-20.0f32, -2.0, 0.0, 6.0] {
			let y_db = linear_to_db(curve.saturate(db_to_linear(x_db)));
			let expected = if x_db <= -2.0 {
				x_db
			} else {
				-2.0 + (x_db + 2.0) / 20.0
			};
			assert!((y_db - expected).abs() < 1e-3, "{x_db}: {y_db}");
		}
	}

	#[test]
	fn a_soft_knee_is_smooth_and_reaches_the_ratio() {
		let curve = Curve::new(-24.0, 30.0, 12.0);
		let y = |x_db: f32| linear_to_db(curve.saturate(db_to_linear(x_db)));
		assert!((y(-30.0) + 30.0).abs() < 1e-3);
		// Above the knee the slope is 1/12.
		let slope = (y(10.0) - y(8.0)) / 2.0;
		assert!((slope - 1.0 / 12.0).abs() < 1e-3, "{slope}");
		// Inside it, between the two.
		let inside = (y(-9.0) - y(-10.0)) / 1.0;
		assert!(inside < 1.0 && inside > 1.0 / 12.0, "{inside}");
	}

	#[test]
	fn a_steady_level_settles_on_the_static_curve_after_the_pre_delay() {
		let rate = 48_000.0;
		let mut compressor = Compressor::new(rate);
		assert_eq!(compressor.latency(), 288);
		let settings = Settings {
			threshold: -12.0,
			knee: 0.0,
			ratio: 4.0,
			attack: 0.003,
			release: 0.1,
		};
		let level = db_to_linear(-4.0);
		let mut out = 0.0;
		for _ in 0..(48_000 / 128) {
			let mut l = [level; 128];
			let mut r = [level; 128];
			compressor.process(settings, [&mut l, &mut r], None);
			out = l[127];
		}
		let curve = Curve::new(-12.0, 0.0, 4.0);
		let makeup = (1.0 / curve.saturate(1.0)).powf(0.6);
		let expected_db = -12.0 + 8.0 / 4.0 + linear_to_db(makeup);
		assert!(
			(linear_to_db(out) - expected_db).abs() < 0.05,
			"{}",
			linear_to_db(out)
		);
		let mut silent = Compressor::new(44_100.0);
		assert_eq!(silent.latency(), 264);
		let mut l = [0.0; 128];
		let mut r = [0.0; 128];
		silent.process(settings, [&mut l, &mut r], None);
		assert!(l.iter().all(|v| *v == 0.0));
	}
}
