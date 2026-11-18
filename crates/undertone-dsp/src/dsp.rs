// The in-house Web Audio DSP: render quanta, channel mixing and the node kernels Undertone uses
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What `crate::render_graph::RenderGraph` computes a booked graph with, one render quantum of
//! 128 frames at a time, as the Web Audio specification renders: the AudioParam timeline, the
//! band-limited oscillator, the biquad, the compressor, the delay line, the convolver and the FFT
//! under it, and the stereo panner. Every kernel is plain `f32`/`f64` arithmetic in a fixed order,
//! so a render is the same bits on every run. A block holds one or two channels; Undertone renders
//! mono or stereo only.

pub mod biquad;
pub mod compressor;
pub mod convolver;
pub mod delay;
pub mod fft;
pub mod oscillator;
pub mod panner;
pub mod timeline;

/// Frames in a render quantum.
pub const QUANTUM: usize = 128;

/// The most channels a block carries.
pub const MAX_CHANNELS: usize = 2;

/// One render quantum of one node's output: `channels` channels of `data`, or silence when
/// `channels` is 0. Only the first `channels` rows are meaningful.
#[derive(Clone)]
pub struct Block {
	/// How many channels carry sound; zero is silence.
	pub channels: usize,
	/// The samples, one row a channel.
	pub data: [[f32; QUANTUM]; MAX_CHANNELS],
}

impl Default for Block {
	fn default() -> Self {
		Block::silent()
	}
}

impl std::fmt::Debug for Block {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Block({} channels)", self.channels)
	}
}

impl Block {
	/// A silent block.
	pub const fn silent() -> Self {
		Block {
			channels: 0,
			data: [[0.0; QUANTUM]; MAX_CHANNELS],
		}
	}

	/// Whether the block carries no channels.
	pub fn is_silent(&self) -> bool {
		self.channels == 0
	}

	/// Makes the block `channels` channels of zeros.
	pub fn zero(&mut self, channels: usize) {
		self.channels = channels;
		for row in self.data.iter_mut().take(channels) {
			row.fill(0.0);
		}
	}

	/// Adds `from` into this block, up- or down-mixed to this block's channels by the
	/// specification's speaker rules: mono to stereo copies, stereo to mono is the average.
	pub fn add(&mut self, from: &Block) {
		match (from.channels, self.channels) {
			(0, _) | (_, 0) => {}
			(1, 2) => {
				for c in 0..2 {
					for (o, i) in self.data[c].iter_mut().zip(&from.data[0]) {
						*o += *i;
					}
				}
			}
			(2, 1) => {
				let (left, right) = (&from.data[0], &from.data[1]);
				for (n, o) in self.data[0].iter_mut().enumerate() {
					*o += 0.5 * (left[n] + right[n]);
				}
			}
			(a, _) => {
				for c in 0..a {
					for (o, i) in self.data[c].iter_mut().zip(&from.data[c]) {
						*o += *i;
					}
				}
			}
		}
	}

	/// The block as stereo: a mono block's channel copied to both sides.
	pub fn upmix_stereo(&mut self) {
		if self.channels == 1 {
			self.data[1] = self.data[0];
			self.channels = 2;
		}
	}
}

/// How a node's input counts its channels (`channelCountMode`, with `channelCount`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
	/// `max`: as many as the widest connection.
	Max,
	/// `clamped-max`: as many as the widest connection, at most this.
	ClampedMax(usize),
	/// `explicit`: always this many.
	Explicit(usize),
}

impl ChannelMode {
	/// `computedNumberOfChannels` for connections whose widest is `widest` (0 when all are silent).
	pub fn computed(self, widest: usize) -> usize {
		match self {
			ChannelMode::Max => widest,
			ChannelMode::ClampedMax(count) => widest.min(count),
			ChannelMode::Explicit(count) => {
				if widest == 0 {
					0
				} else {
					count
				}
			}
		}
	}
}

/// A value too small to matter, flushed to zero so recursive filters and feedback never fall into
/// slow subnormal arithmetic.
pub const FLUSH: f64 = 1e-30;

/// `x`, or 0 when it is smaller than `FLUSH`.
#[inline]
pub fn flush(x: f64) -> f64 {
	if x.abs() < FLUSH { 0.0 } else { x }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn mono_up_mixes_by_copying_and_stereo_down_mixes_by_averaging() {
		let mut mono = Block::silent();
		mono.zero(1);
		mono.data[0][0] = 1.0;
		let mut stereo = Block::silent();
		stereo.zero(2);
		stereo.add(&mono);
		assert_eq!((stereo.data[0][0], stereo.data[1][0]), (1.0, 1.0));
		let mut down = Block::silent();
		down.zero(1);
		stereo.data[1][0] = 0.0;
		down.add(&stereo);
		assert_eq!(down.data[0][0], 0.5);
		assert_eq!(ChannelMode::ClampedMax(2).computed(1), 1);
		assert_eq!(ChannelMode::Explicit(2).computed(1), 2);
		assert_eq!(ChannelMode::Explicit(2).computed(0), 0);
	}
}
