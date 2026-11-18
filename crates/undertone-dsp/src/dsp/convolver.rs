// The ConvolverNode: a zero-latency, two-stage partitioned FFT convolution with the spec's scaling
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The impulse response is cut into two stages. The head (its first 2048 frames) is convolved in
//! partitions of one render quantum, overlap-save with 256-point FFTs, so each quantum's output is
//! ready as soon as its input is: no latency. The rest is convolved in partitions of 2048 frames
//! with 4096-point FFTs; because that stage starts 2048 frames into the response, a block of its
//! output is due only after its whole block of input has arrived, and the products of its older
//! partitions are spread over the quanta in between, so no single quantum carries the stage. Both
//! channels go through one complex FFT (left as the real part, right as the imaginary), and the
//! two spectra are split from it by symmetry. Every buffer is allocated when the convolver is
//! made; `process` allocates nothing.
//!
//! Channels: the input is mono or stereo and the response one or two channels; each output
//! channel convolves its own input channel (a mono input feeds both) with its own response channel
//! (a mono response serves both), so a stereo response makes a stereo output. With
//! `normalize` (Undertone never turns it off) the response is scaled by the specification's
//! `calculateNormalizationScale`: `0.00125 / rms`, times `44100 / sampleRate`, the RMS floored at
//! 0.000125.

// Frame and bin loops index several buffers at once; the indices read as the formulas do.
#![allow(clippy::needless_range_loop)]

use crate::dsp::fft::{Complex, Fft};
use crate::dsp::{Block, QUANTUM};
use crate::graph::Buffer;

/// Frames of the head stage.
const HEAD: usize = 2048;
/// Partition size of the tail stage.
const TAIL_BLOCK: usize = 2048;

/// `calculateNormalizationScale(buffer)`.
pub fn normalization_scale(buffer: &Buffer) -> f32 {
	const GAIN_CALIBRATION: f64 = 0.00125;
	const GAIN_CALIBRATION_SAMPLE_RATE: f64 = 44100.0;
	const MIN_POWER: f64 = 0.000125;
	let channels = buffer.channels.len();
	let length = buffer.len();
	let mut power = 0f64;
	for channel in &buffer.channels {
		let mut channel_power = 0f64;
		for sample in channel {
			let s = f64::from(*sample);
			channel_power += s * s;
		}
		power += channel_power;
	}
	power = (power / (channels * length).max(1) as f64).sqrt();
	if !power.is_finite() || power < MIN_POWER {
		power = MIN_POWER;
	}
	let mut scale = GAIN_CALIBRATION / power;
	if buffer.sample_rate > 0.0 {
		scale *= GAIN_CALIBRATION_SAMPLE_RATE / buffer.sample_rate;
	}
	if channels == 4 {
		scale *= 0.5;
	}
	scale as f32
}

/// One uniformly partitioned stage.
#[derive(Debug, Clone)]
struct Stage {
	block: usize,
	fft: Fft,
	/// Each partition's spectrum, bins 0 to `block`, for each response channel.
	partitions: Vec<[Vec<Complex>; 2]>,
	/// The latest input spectra, newest at `head`, bins 0 to `block`, left and right.
	history: Vec<[Vec<Complex>; 2]>,
	head: usize,
	/// The previous block of input and the one filling, left and right.
	input: [Vec<f32>; 2],
	filled: usize,
	scratch: Vec<Complex>,
	/// The products' sum for the block being filled, bins 0 to `block`, left and right.
	sum: [Vec<Complex>; 2],
	/// The next older partition to add into `sum`: the products of partitions 1 on are known as
	/// soon as the previous block is, so they are spread over the quanta that fill this one.
	next_partition: usize,
}

impl Stage {
	fn new(block: usize, response: &[Vec<f32>; 2], from: usize, to: usize) -> Self {
		let size = 2 * block;
		let fft = Fft::new(size);
		let count = (to - from).div_ceil(block);
		let mut partitions = Vec::with_capacity(count);
		let mut scratch = vec![Complex::ZERO; size];
		for p in 0..count {
			let start = from + p * block;
			let end = (start + block).min(to);
			scratch.fill(Complex::ZERO);
			for (i, n) in (start..end).enumerate() {
				scratch[i] = Complex::new(response[0][n], response[1][n]);
			}
			fft.forward(&mut scratch);
			partitions.push(split(&scratch, block));
		}
		let history = vec![
			[
				vec![Complex::ZERO; block + 1],
				vec![Complex::ZERO; block + 1]
			];
			count
		];
		Stage {
			block,
			fft,
			partitions,
			history,
			head: 0,
			input: [vec![0.0; size], vec![0.0; size]],
			filled: 0,
			scratch,
			sum: [
				vec![Complex::ZERO; block + 1],
				vec![Complex::ZERO; block + 1],
			],
			next_partition: 1,
		}
	}

	/// Takes one quantum of input; when that completes a block, returns the block of output the
	/// stage's partitions give for it (left and right, `block` frames each).
	fn push(&mut self, left: &[f32], right: &[f32], out: &mut [Vec<f32>; 2]) -> bool {
		let block = self.block;
		for c in 0..2 {
			let source = if c == 0 { left } else { right };
			self.input[c][block + self.filled..block + self.filled + source.len()]
				.copy_from_slice(source);
		}
		self.filled += left.len();
		let count = self.partitions.len();
		if self.filled < block {
			// A share of the older partitions now, so no quantum does them all.
			let quanta = (block / left.len().max(1)).max(2);
			let share = (count.saturating_sub(1)).div_ceil(quanta - 1);
			let until = (self.next_partition + share).min(count);
			self.accumulate(until);
			return false;
		}
		self.filled = 0;
		if self.partitions.is_empty() {
			for c in 0..2 {
				self.input[c].copy_within(block.., 0);
			}
			return false;
		}
		let size = 2 * block;
		for i in 0..size {
			self.scratch[i] = Complex::new(self.input[0][i], self.input[1][i]);
		}
		for c in 0..2 {
			self.input[c].copy_within(block.., 0);
		}
		self.fft.forward(&mut self.scratch);
		// Y_c = Σ_p X_c(newest − p) · H_c,p, bins 0 to block: the older partitions first (what
		// is left of them), then the newest block's.
		self.accumulate(count);
		self.head = (self.head + count - 1) % count;
		split_into(&self.scratch, block, &mut self.history[self.head]);
		{
			let x = &self.history[self.head];
			let h = &self.partitions[0];
			for c in 0..2 {
				for k in 0..=block {
					let product = x[c][k].times(h[c][k]);
					self.sum[c][k].re += product.re;
					self.sum[c][k].im += product.im;
				}
			}
		}
		let sum = &self.sum;
		// Left as the real part and right as the imaginary of one inverse transform.
		for k in 0..=block {
			let (l, r) = (sum[0][k], sum[1][k]);
			self.scratch[k] = Complex::new(l.re - r.im, l.im + r.re);
		}
		for k in block + 1..size {
			let (l, r) = (sum[0][size - k].conj(), sum[1][size - k].conj());
			self.scratch[k] = Complex::new(l.re - r.im, l.im + r.re);
		}
		self.fft.inverse(&mut self.scratch);
		let scale = 1.0 / size as f32;
		for i in 0..block {
			let v = self.scratch[block + i];
			out[0][i] = v.re * scale;
			out[1][i] = v.im * scale;
		}
		for channel in self.sum.iter_mut() {
			channel.fill(Complex::ZERO);
		}
		self.next_partition = 1;
		true
	}

	/// Adds partitions `next_partition` to `until` (exclusive) of the block being filled: partition
	/// `p` meets the input `p − 1` blocks before the newest complete one.
	fn accumulate(&mut self, until: usize) {
		let count = self.partitions.len();
		let block = self.block;
		while self.next_partition < until {
			let p = self.next_partition;
			let x = &self.history[(self.head + p - 1) % count];
			let h = &self.partitions[p];
			for c in 0..2 {
				for k in 0..=block {
					let product = x[c][k].times(h[c][k]);
					self.sum[c][k].re += product.re;
					self.sum[c][k].im += product.im;
				}
			}
			self.next_partition += 1;
		}
	}
}

/// The two real signals' spectra, bins 0 to `block`, from the transform of `left + i·right`.
fn split(z: &[Complex], block: usize) -> [Vec<Complex>; 2] {
	let mut out = [
		vec![Complex::ZERO; block + 1],
		vec![Complex::ZERO; block + 1],
	];
	split_into(z, block, &mut out);
	out
}

/// `split` into spectra already allocated, as a render does.
fn split_into(z: &[Complex], block: usize, out: &mut [Vec<Complex>; 2]) {
	let size = z.len();
	let [left, right] = out;
	for k in 0..=block {
		let a = z[k];
		let b = z[(size - k) % size].conj();
		left[k] = Complex::new(0.5 * (a.re + b.re), 0.5 * (a.im + b.im));
		// (a − b) / 2i
		let d = Complex::new(a.re - b.re, a.im - b.im);
		right[k] = Complex::new(0.5 * d.im, -0.5 * d.re);
	}
}

/// A convolver's state.
#[derive(Debug, Clone)]
pub struct Convolver {
	head: Stage,
	tail: Option<Stage>,
	/// The tail's output for the next `TAIL_BLOCK` frames, and how far it has been read.
	pending: [Vec<f32>; 2],
	read: usize,
	head_out: [Vec<f32>; 2],
	tail_out: [Vec<f32>; 2],
	stereo_response: bool,
	/// Frames of response.
	length: usize,
	/// Quanta since the input last had sound, once it has; `None` while all is silence.
	since_sound: Option<usize>,
}

impl Convolver {
	/// A convolver over `buffer` (one or two channels), scaled when `normalize`.
	pub fn new(buffer: &Buffer, normalize: bool) -> Self {
		let scale = if normalize {
			normalization_scale(buffer)
		} else {
			1.0
		};
		let length = buffer.len();
		let channel = |c: usize| -> Vec<f32> {
			let source = buffer.channels.get(c).unwrap_or(&buffer.channels[0]);
			source.iter().map(|s| s * scale).collect()
		};
		let response = [channel(0), channel(1)];
		let head = Stage::new(QUANTUM, &response, 0, length.min(HEAD));
		let tail = (length > HEAD).then(|| Stage::new(TAIL_BLOCK, &response, HEAD, length));
		Convolver {
			head,
			tail,
			pending: [vec![0.0; TAIL_BLOCK], vec![0.0; TAIL_BLOCK]],
			read: 0,
			head_out: [vec![0.0; QUANTUM], vec![0.0; QUANTUM]],
			tail_out: [vec![0.0; TAIL_BLOCK], vec![0.0; TAIL_BLOCK]],
			stereo_response: buffer.channels.len() > 1,
			length,
			since_sound: None,
		}
	}

	/// Convolves one quantum. The output is silent once the response has rung out after the
	/// input's last sound, and the state then starts again from zero, which it is.
	pub fn process(&mut self, input: &Block, out: &mut Block) {
		if input.is_silent() {
			match self.since_sound {
				None => {
					out.channels = 0;
					return;
				}
				Some(quanta) => {
					let rung = (self.length + 2 * TAIL_BLOCK + 2 * QUANTUM).div_ceil(QUANTUM);
					if quanta > rung {
						self.reset();
						out.channels = 0;
						return;
					}
					self.since_sound = Some(quanta + 1);
				}
			}
		} else {
			self.since_sound = Some(0);
		}
		let zeros = [0f32; QUANTUM];
		let (left, right): (&[f32], &[f32]) = match input.channels {
			0 => (&zeros, &zeros),
			1 => (&input.data[0], &input.data[0]),
			_ => (&input.data[0], &input.data[1]),
		};
		let channels = if input.channels == 2 || self.stereo_response {
			2
		} else {
			1
		};
		out.zero(channels);
		if self.head.push(left, right, &mut self.head_out) {
			for c in 0..channels {
				out.data[c].copy_from_slice(&self.head_out[c][..QUANTUM]);
			}
		}
		if let Some(tail) = &mut self.tail {
			for c in 0..channels {
				for i in 0..QUANTUM {
					out.data[c][i] += self.pending[c][self.read + i];
				}
			}
			self.read += QUANTUM;
			if tail.push(left, right, &mut self.tail_out) {
				std::mem::swap(&mut self.pending, &mut self.tail_out);
				self.read = 0;
			}
		}
	}

	fn reset(&mut self) {
		let fresh = |stage: &mut Stage| {
			for history in &mut stage.history {
				for channel in history.iter_mut() {
					channel.fill(Complex::ZERO);
				}
			}
			for channel in &mut stage.input {
				channel.fill(0.0);
			}
			for channel in stage.sum.iter_mut() {
				channel.fill(Complex::ZERO);
			}
			stage.filled = 0;
			stage.head = 0;
			stage.next_partition = 1;
		};
		fresh(&mut self.head);
		if let Some(tail) = &mut self.tail {
			fresh(tail);
		}
		for channel in &mut self.pending {
			channel.fill(0.0);
		}
		self.read = 0;
		self.since_sound = None;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn direct(x: &[f32], h: &[f32], n: usize) -> f32 {
		(0..=n)
			.filter(|k| *k < h.len() && n - k < x.len())
			.map(|k| x[n - k] * h[k])
			.sum()
	}

	#[test]
	fn an_impulse_returns_the_scaled_response_with_no_latency() {
		let rate = 48_000.0;
		let length = 5_000;
		let left: Vec<f32> = (0..length)
			.map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0)
			.collect();
		let right: Vec<f32> = (0..length)
			.map(|i| ((i * 104_729) % 97) as f32 / 48.0 - 1.0)
			.collect();
		let buffer = Buffer {
			sample_rate: rate,
			channels: vec![left.clone(), right.clone()],
		};
		let mut convolver = Convolver::new(&buffer, false);
		let mut out = Vec::new();
		for q in 0..60 {
			let mut input = Block::silent();
			input.zero(1);
			if q == 0 {
				input.data[0][0] = 1.0;
			}
			let mut block = Block::silent();
			convolver.process(&input, &mut block);
			assert_eq!(block.channels, 2);
			for i in 0..QUANTUM {
				out.push((block.data[0][i], block.data[1][i]));
			}
		}
		for n in 0..length {
			assert!((out[n].0 - left[n]).abs() < 1e-4, "left {n}");
			assert!((out[n].1 - right[n]).abs() < 1e-4, "right {n}");
		}
		assert!(out[length + 10].0.abs() < 1e-4);
	}

	#[test]
	fn a_signal_is_convolved_as_the_direct_sum_gives() {
		let length = 3_000;
		let h: Vec<f32> = (0..length)
			.map(|i| (-(i as f32) / 600.0).exp() * ((i % 13) as f32 - 6.0) / 6.0)
			.collect();
		let x: Vec<f32> = (0..QUANTUM * 40)
			.map(|i| ((i as f32) * 0.05).sin())
			.collect();
		let buffer = Buffer {
			sample_rate: 48_000.0,
			channels: vec![h.clone()],
		};
		let mut convolver = Convolver::new(&buffer, false);
		let mut got = Vec::new();
		for q in 0..40 {
			let mut input = Block::silent();
			input.zero(1);
			input.data[0].copy_from_slice(&x[q * QUANTUM..(q + 1) * QUANTUM]);
			let mut block = Block::silent();
			convolver.process(&input, &mut block);
			assert_eq!(block.channels, 1);
			got.extend_from_slice(&block.data[0]);
		}
		for n in (0..got.len()).step_by(37) {
			assert!((got[n] - direct(&x, &h, n)).abs() < 2e-3, "frame {n}");
		}
	}

	#[test]
	fn normalisation_follows_the_specification() {
		let buffer = Buffer {
			sample_rate: 44_100.0,
			channels: vec![vec![0.5; 100], vec![-0.5; 100]],
		};
		assert!((normalization_scale(&buffer) - 0.0025).abs() < 1e-9);
		let quiet = Buffer {
			sample_rate: 88_200.0,
			channels: vec![vec![0.0; 10]],
		};
		assert!((normalization_scale(&quiet) - 5.0).abs() < 1e-6);
	}
}
