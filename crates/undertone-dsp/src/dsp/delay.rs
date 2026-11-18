// The DelayNode's line: a ring of past input read at a fractional delay with linear interpolation
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Frame `n` of the input is kept at `n mod size`, and the output at frame `n` reads the input at
//! `n − delayTime · sampleRate`, interpolating linearly between the two frames around it, as the
//! browsers do. Inside a cycle the renderer reads a quantum before it writes one, so the delay is
//! held to at least one render quantum there, as the specification requires. The line is stereo
//! once any stereo input has reached it (a mono line is copied to both sides then); before that it
//! stays mono.

use crate::dsp::{Block, QUANTUM, flush};

#[derive(Debug, Clone)]
/// A delay line over a ring buffer, one ring a channel.
pub struct DelayLine {
	ring: [Vec<f32>; 2],
	stereo: bool,
	/// The last frame written with sound, if any.
	last_sound: Option<usize>,
	/// The longest delay, in frames.
	max_frames: usize,
}

impl DelayLine {
	/// A line for delays up to `max_seconds`.
	pub fn new(max_seconds: f64, sample_rate: f64) -> Self {
		let max_frames = (max_seconds * sample_rate).ceil() as usize + 1;
		let size = max_frames + 2 * QUANTUM + 2;
		DelayLine {
			ring: [vec![0.0; size], vec![0.0; size]],
			stereo: false,
			last_sound: None,
			max_frames,
		}
	}

	fn size(&self) -> usize {
		self.ring[0].len()
	}

	/// Writes one quantum of input starting at frame `first`.
	pub fn write(&mut self, first: usize, input: &Block) {
		let size = self.size();
		if input.channels == 2 && !self.stereo {
			self.stereo = true;
			let (left, right) = self.ring.split_at_mut(1);
			right[0].copy_from_slice(&left[0]);
		}
		let channels = if self.stereo { 2 } else { 1 };
		for c in 0..channels {
			for i in 0..QUANTUM {
				let value = match input.channels {
					0 => 0.0,
					1 => input.data[0][i],
					_ if self.stereo => input.data[c][i],
					_ => 0.5 * (input.data[0][i] + input.data[1][i]),
				};
				self.ring[c][(first + i) % size] = flush(f64::from(value)) as f32;
			}
		}
		if !input.is_silent() {
			self.last_sound = Some(first + QUANTUM - 1);
		}
	}

	/// Reads one quantum starting at frame `first`, delayed by `delays` frames (one value a frame).
	/// Silent once everything the longest delay can reach is silence.
	pub fn read(&self, first: usize, delays: &[f64; QUANTUM], out: &mut Block) {
		let quiet = match self.last_sound {
			None => true,
			Some(last) => first > last + self.max_frames + 1,
		};
		if quiet {
			out.channels = 0;
			return;
		}
		let size = self.size();
		let channels = if self.stereo { 2 } else { 1 };
		out.zero(channels);
		for (i, delay) in delays.iter().enumerate() {
			let position = (first + i) as f64 - delay;
			// Frames before the first are silence.
			if position <= -1.0 {
				continue;
			}
			let floor = position.floor();
			let frac = (position - floor) as f32;
			let index = (floor as i64).rem_euclid(size as i64) as usize;
			let before_start = floor < 0.0;
			for c in 0..channels {
				let a = if before_start {
					0.0
				} else {
					self.ring[c][index]
				};
				let b = if frac > 0.0 {
					self.ring[c][(index + 1) % size]
				} else {
					0.0
				};
				out.data[c][i] = a + (b - a) * frac;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn an_impulse_comes_out_after_the_delay() {
		let rate = 48_000.0;
		let mut line = DelayLine::new(1.0, rate);
		let mut impulse = Block::silent();
		impulse.zero(1);
		impulse.data[0][0] = 1.0;
		let delays = [0.25 * rate; QUANTUM];
		let mut found = None;
		let mut out = Block::silent();
		let silence = Block::silent();
		for q in 0..200 {
			let first = q * QUANTUM;
			line.write(first, if q == 0 { &impulse } else { &silence });
			line.read(first, &delays, &mut out);
			for i in 0..QUANTUM {
				if out.channels > 0 && out.data[0][i] != 0.0 {
					found.get_or_insert((first + i, out.data[0][i]));
				}
			}
		}
		assert_eq!(found, Some((12_000, 1.0)));
	}

	#[test]
	fn a_fractional_delay_interpolates() {
		let mut line = DelayLine::new(0.1, 1_000.0);
		let mut ramp = Block::silent();
		ramp.zero(1);
		for i in 0..QUANTUM {
			ramp.data[0][i] = i as f32;
		}
		line.write(0, &ramp);
		let mut out = Block::silent();
		line.read(0, &[2.5; QUANTUM], &mut out);
		assert_eq!(out.data[0][10], 7.5);
	}
}
