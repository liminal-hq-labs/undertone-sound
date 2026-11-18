// Renders register writes to samples through the oplon emulator, linked natively
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Register writes rendered to stereo samples by the `oplon` emulator, with the output scale the
//! TypeScript reference's WebAssembly build uses, so the samples match its recorded ones.

use oplon::Opl2;

use crate::numbers::round;
use crate::sequence::{OplChip, OplSequence};

/// The emulator's per-operator output peaks near +/-4084; a full mix of channels can sum past
/// that, so samples are scaled to leave headroom rather than clip.
pub const OUTPUT_SCALE: f32 = 1.0 / 16_384.0;

/// Seconds of release folded back onto the start of a looping pass.
pub const LOOP_TAIL: f64 = 1.5;

/// The sample rate for passes rendered ahead of time.
pub const SAMPLE_RATE: u32 = 48_000;

/// Rendered stereo samples.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedSound {
	/// The sample rate of both channels.
	pub sample_rate: u32,
	/// The left channel.
	pub left: Vec<f32>,
	/// The right channel; the same samples as the left on a mono chip.
	pub right: Vec<f32>,
}

/// One emulated chip.
pub struct Chip {
	opl: Opl2,
}

impl Chip {
	/// A chip emulator at a sample rate; `opl3` selects the OPL3's mode.
	pub fn new(sample_rate: u32, opl3: bool) -> Self {
		let mut opl = Opl2::new(sample_rate);
		if opl3 {
			opl.set_opl3_enabled(true);
		}
		Self { opl }
	}

	/// Writes a register; 0x100 to 0x1ff are the OPL3's second bank. The value is taken as a byte.
	pub fn write(&mut self, register: u16, value: i32) {
		let value = (value & 0xff) as u8;
		if register >= 0x100 {
			self.opl.write_reg_high((register & 0xff) as u8, value);
		} else {
			self.opl.write_reg((register & 0xff) as u8, value);
		}
	}

	/// Renders frames into `left` and `right` from `offset`.
	pub fn render_into(
		&mut self,
		left: &mut [f32],
		right: &mut [f32],
		offset: usize,
		frames: usize,
	) {
		for i in 0..frames {
			let (l, r) = self.opl.render_frame();
			left[offset + i] = l as f32 * OUTPUT_SCALE;
			right[offset + i] = r as f32 * OUTPUT_SCALE;
		}
	}
}

/// How to render a sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
	/// The sample rate to render at.
	pub sample_rate: u32,
	/// Seconds of release after the last pass.
	pub tail: f64,
	/// Fold the tail back onto the start, so the result is exactly one pass long and loops without a seam.
	pub wrap: bool,
}

impl RenderOptions {
	/// The options for a pass rendered ahead of time to loop.
	pub const LOOPING: RenderOptions = RenderOptions {
		sample_rate: SAMPLE_RATE,
		tail: LOOP_TAIL,
		wrap: true,
	};
}

/// Renders a sequence on a fresh chip: one pass plus `tail` seconds. With `wrap`, the tail is
/// folded back onto the start and the result is exactly one pass long.
pub fn render_sequence(
	sequence: &OplSequence,
	chip: OplChip,
	options: RenderOptions,
) -> RenderedSound {
	let rate = options.sample_rate;
	let pass_frames = round(sequence.seconds * f64::from(rate)).max(1.0) as usize;
	let total = pass_frames + round(options.tail * f64::from(rate)) as usize;
	let mut left = vec![0f32; total];
	let mut right = vec![0f32; total];
	let mut opl = Chip::new(rate, chip == OplChip::Opl3);
	let mut frame = 0usize;
	let mut advance = |opl: &mut Chip, until: usize| {
		let frames = until.min(total).saturating_sub(frame);
		if frames == 0 {
			return;
		}
		opl.render_into(&mut left, &mut right, frame, frames);
		frame += frames;
	};
	for write in &sequence.writes {
		let until = round(write.time * f64::from(rate)).max(0.0) as usize;
		advance(&mut opl, until);
		opl.write(write.register, write.value);
	}
	advance(&mut opl, total);
	if !options.wrap {
		return RenderedSound {
			sample_rate: rate,
			left,
			right,
		};
	}
	let mut out_left = left[..pass_frames].to_vec();
	let mut out_right = right[..pass_frames].to_vec();
	for i in pass_frames..total {
		let at = (i - pass_frames) % pass_frames;
		out_left[at] += left[i];
		out_right[at] += right[i];
	}
	RenderedSound {
		sample_rate: rate,
		left: out_left,
		right: out_right,
	}
}
