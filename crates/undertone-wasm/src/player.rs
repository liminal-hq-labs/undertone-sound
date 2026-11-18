// An OPX player: a stream in, blocks of stereo samples out, through the `oplon` OPL emulator.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The stream is decoded once into timed register writes. Tick `n` of the grid falls on output
//! frame `n * sample_rate / grid_hz`, rounded down, so the writes land where an interrupt at the
//! grid's rate would put them and the player stays exact for any sample rate.

use oplon::Opl2;
use oplstream::{Chip, Command, Decoder, HEADER_LEN, Header, validate};

/// The emulator's per-operator output peaks near 4,084 and a full mix can sum past that, so
/// samples are scaled down to leave headroom rather than clip.
const OUTPUT_SCALE: f32 = 1.0 / 16_384.0;

#[derive(Clone, Copy, Debug)]
struct Write {
	tick: u32,
	bank: u8,
	reg: u8,
	val: u8,
}

/// Plays an OPL2 or OPL3 stream.
pub struct OplPlayer {
	chip: Opl2,
	writes: Vec<Write>,
	next_write: usize,
	grid_hz: u32,
	sample_rate: u32,
	total_ticks: u32,
	/// The tick the next block starts at.
	tick: u32,
	/// The output frame the next block starts at, counted from the start of this pass.
	frame: u64,
	/// Where the output frame counter was when the current pass began (it moves on a loop).
	pass_start_frame: u64,
	loop_tick: Option<u32>,
	loop_write: usize,
	looping: bool,
	ended: bool,
}

impl OplPlayer {
	/// Prepares to play `file` at `sample_rate`. The file must be a valid OPL2 or OPL3 stream.
	pub fn new(file: &[u8], sample_rate: u32) -> Result<Self, String> {
		if sample_rate == 0 {
			return Err("The sample rate must be above zero".to_string());
		}
		let header = validate(file).map_err(|error| format!("The stream is not valid: {error}"))?;
		if header.chip == Chip::Speaker {
			return Err("PC speaker streams cannot be played here, only OPL2 and OPL3".to_string());
		}
		let (writes, loop_tick, loop_write) = decode(file, &header);
		let mut chip = Opl2::new(sample_rate);
		if header.chip == Chip::Opl3 {
			chip.set_opl3_enabled(true);
		}
		Ok(Self {
			chip,
			writes,
			next_write: 0,
			grid_hz: header.grid_hz,
			sample_rate,
			total_ticks: header.total_ticks,
			tick: 0,
			frame: 0,
			pass_start_frame: 0,
			loop_tick,
			loop_write,
			looping: false,
			ended: false,
		})
	}

	/// Whether the stream restarts at its loop point instead of ending. It has no effect on a
	/// stream without a loop point.
	pub fn set_looping(&mut self, looping: bool) {
		self.looping = looping;
	}

	/// Whether a loop point is present.
	pub fn has_loop(&self) -> bool {
		self.loop_tick.is_some()
	}

	/// Whether the stream has reached its `END`. Blocks rendered afterwards are the chip's tail.
	pub fn ended(&self) -> bool {
		self.ended
	}

	/// The tick the next block starts at.
	pub fn tick(&self) -> u32 {
		self.tick
	}

	/// The stream's length in output frames.
	pub fn total_frames(&self) -> u64 {
		self.frame_of(self.total_ticks)
	}

	fn frame_of(&self, tick: u32) -> u64 {
		u64::from(tick) * u64::from(self.sample_rate) / u64::from(self.grid_hz)
	}

	/// Renders `frames` stereo frames as interleaved `f32` (left, right, left, ...).
	pub fn render(&mut self, frames: usize) -> Vec<f32> {
		let mut out = Vec::with_capacity(frames * 2);
		let mut left = frames;
		while left > 0 {
			if !self.ended && self.frame >= self.pass_start_frame + self.frame_of(self.tick) {
				self.run_tick();
				continue;
			}
			let run = if self.ended {
				left
			} else {
				let boundary = self.pass_start_frame + self.frame_of(self.tick);
				usize::try_from(boundary - self.frame).map_or(left, |gap| gap.min(left))
			};
			for _ in 0..run {
				let (l, r) = self.chip.render_frame();
				out.push(l as f32 * OUTPUT_SCALE);
				out.push(r as f32 * OUTPUT_SCALE);
			}
			self.frame += run as u64;
			left -= run;
		}
		out
	}

	/// Applies the writes due on the current tick, then moves to the next one or ends.
	fn run_tick(&mut self) {
		while let Some(write) = self.writes.get(self.next_write) {
			if write.tick != self.tick {
				break;
			}
			if write.bank == 0 {
				self.chip.write_reg(write.reg, write.val);
			} else {
				self.chip.write_reg_high(write.reg, write.val);
			}
			self.next_write += 1;
		}
		if self.tick >= self.total_ticks {
			match self.loop_tick {
				Some(loop_tick) if self.looping => {
					self.tick = loop_tick;
					self.next_write = self.loop_write;
					// The next pass counts frames from here, so the loop point sits on this frame.
					self.pass_start_frame = self.frame - self.frame_of(loop_tick);
				}
				_ => self.ended = true,
			}
		} else {
			self.tick += 1;
		}
	}
}

/// The stream's writes in order, the tick of its loop point and the index of the first write at or
/// after it.
fn decode(file: &[u8], header: &Header) -> (Vec<Write>, Option<u32>, usize) {
	let mut writes = Vec::new();
	let mut loop_tick = None;
	let mut loop_write = 0;
	for step in Decoder::new(&file[HEADER_LEN..], header.chip).flatten() {
		match step.command {
			Command::Write { bank, reg, val } => {
				writes.push(Write {
					tick: step.tick,
					bank,
					reg,
					val,
				});
			}
			Command::Loop => {
				loop_tick = Some(step.tick);
				loop_write = writes.len();
			}
			_ => {}
		}
	}
	(writes, loop_tick, loop_write)
}

#[cfg(test)]
mod tests {
	use super::*;
	use oplstream::Writer;

	const RATE: u32 = 48_000;

	/// Channel 0 with a plain sine-like patch, keyed on at tick 4 and off at tick 100.
	fn note(chip: Chip, loop_at_start: bool) -> Vec<u8> {
		let mut writer = Writer::new(chip, 560);
		if loop_at_start {
			writer.loop_point();
		}
		writer.wait(4);
		for (reg, val) in [
			(0x20, 0x01),
			(0x40, 0x10),
			(0x60, 0xF0),
			(0x80, 0x00),
			(0x23, 0x01),
			(0x43, 0x00),
			(0x63, 0xF0),
			(0x83, 0x00),
			(0xA0, 0x98),
			(0xB0, 0x31),
		] {
			writer.write(0, reg, val);
		}
		writer.wait(96).write(0, 0xB0, 0x11).wait(100);
		writer.finish().unwrap()
	}

	fn peak(samples: &[f32]) -> f32 {
		samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
	}

	#[test]
	fn silence_before_the_key_on_and_sound_after_it() {
		let mut player = OplPlayer::new(&note(Chip::Opl2, false), RATE).unwrap();
		// Tick 4 at 560 Hz is frame 342 at 48 kHz.
		assert_eq!(peak(&player.render(342)), 0.0);
		assert!(peak(&player.render(4_800)) > 0.05);
		assert!(!player.ended());
	}

	#[test]
	fn the_stream_ends_at_its_length_and_the_length_is_exact() {
		let mut player = OplPlayer::new(&note(Chip::Opl2, false), RATE).unwrap();
		assert_eq!(player.total_frames(), 200 * 48_000 / 560);
		let block = player.render(usize::try_from(player.total_frames()).unwrap() + 1);
		assert!(player.ended());
		assert_eq!(block.len(), 2 * (player.total_frames() as usize + 1));
	}

	#[test]
	fn blocks_of_any_size_give_the_same_samples() {
		let file = note(Chip::Opl2, false);
		let mut whole = OplPlayer::new(&file, RATE).unwrap();
		let all = whole.render(20_000);
		for size in [1, 7, 128, 1_000] {
			let mut player = OplPlayer::new(&file, RATE).unwrap();
			let mut joined = Vec::new();
			while joined.len() < all.len() {
				joined.extend(player.render(size.min((all.len() - joined.len()) / 2)));
			}
			assert!(joined == all, "block size {size}");
		}
	}

	#[test]
	fn an_opl3_stream_plays_through_the_second_bank_writes_too() {
		let mut writer = Writer::new(Chip::Opl3, 560);
		writer.write(1, 0x05, 0x01).wait(10);
		let mut player = OplPlayer::new(&writer.finish().unwrap(), RATE).unwrap();
		player.render(1_000);
		assert!(player.ended());
	}

	#[test]
	fn a_loop_restarts_at_the_loop_point_and_only_when_asked() {
		let file = note(Chip::Opl2, true);
		let mut once = OplPlayer::new(&file, RATE).unwrap();
		assert!(once.has_loop());
		once.render(30_000);
		assert!(once.ended());

		let mut looped = OplPlayer::new(&file, RATE).unwrap();
		looped.set_looping(true);
		let first_pass = usize::try_from(looped.total_frames()).unwrap();
		looped.render(first_pass * 2 + 100);
		assert!(!looped.ended());
		assert!(looped.tick() <= 200);
	}

	const OPL2_TITLE: &[u8] = include_bytes!("../../../fixtures/opx/title-theme.opl2.opx");
	const OPL3_TITLE: &[u8] = include_bytes!("../../../fixtures/opx/title-theme.opl3.opx");

	/// The whole stream and half a second of tail, split into its two channels.
	fn whole(file: &[u8]) -> (Vec<f32>, Vec<f32>) {
		let mut player = OplPlayer::new(file, RATE).unwrap();
		let frames = usize::try_from(player.total_frames()).unwrap() + RATE as usize / 2;
		let samples = player.render(frames);
		assert!(player.ended());
		samples
			.as_chunks::<2>()
			.0
			.iter()
			.map(|[l, r]| (*l, *r))
			.unzip()
	}

	#[test]
	fn the_opl2_title_theme_plays_the_same_in_both_channels() {
		let (left, right) = whole(OPL2_TITLE);
		assert!(peak(&left) > 0.05, "peak {}", peak(&left));
		assert!(left == right, "an OPL2 is mono, so the channels are equal");
	}

	/// Channel 0 keyed on with the chip's output choice `choice` (bit 4 left, bit 5 right).
	fn panned(choice: u8) -> (Vec<f32>, Vec<f32>) {
		let mut writer = Writer::new(Chip::Opl3, 560);
		writer.write(1, 0x05, 0x01).wait(1);
		for (reg, val) in [
			(0x20, 0x01),
			(0x40, 0x10),
			(0x60, 0xF0),
			(0x80, 0x00),
			(0x23, 0x01),
			(0x43, 0x00),
			(0x63, 0xF0),
			(0x83, 0x00),
			(0xA0, 0x98),
			(0xC0, choice),
			(0xB0, 0x31),
		] {
			writer.write(0, reg, val);
		}
		writer.wait(100);
		let mut player = OplPlayer::new(&writer.finish().unwrap(), RATE).unwrap();
		player
			.render(8_000)
			.as_chunks::<2>()
			.0
			.iter()
			.map(|[l, r]| (*l, *r))
			.unzip()
	}

	#[test]
	fn an_opl3_voice_can_sit_on_one_output_only() {
		let (left, right) = panned(0x10);
		assert!(peak(&left) > 0.05);
		assert_eq!(peak(&right), 0.0);
		let (left, right) = panned(0x20);
		assert_eq!(peak(&left), 0.0);
		assert!(peak(&right) > 0.05);
	}

	#[test]
	fn the_opl3_title_theme_is_stereo_with_signal_on_both_sides() {
		let (left, right) = whole(OPL3_TITLE);
		assert!(peak(&left) > 0.05 && peak(&right) > 0.05);
		assert!(peak(&left) <= 1.0 && peak(&right) <= 1.0);
		// Every note sits on both outputs, four steps quieter than a full-scale one, so the two
		// channels carry the same notes at the same level.
		assert!(left == right, "both outputs carry every note");
		let frames = left.len();
		assert_eq!(frames, right.len());
		assert!(frames > 60 * RATE as usize, "about 61.7 s and a tail");
	}

	#[test]
	fn refusals_are_readable() {
		assert!(
			OplPlayer::new(b"nonsense", RATE)
				.err()
				.unwrap()
				.starts_with("The stream is not valid")
		);
		let mut speaker = Writer::new(Chip::Speaker, 560);
		speaker.speaker(1_000).wait(5);
		let error = OplPlayer::new(&speaker.finish().unwrap(), RATE)
			.err()
			.unwrap();
		assert!(error.contains("PC speaker"), "{error}");
		assert!(OplPlayer::new(&note(Chip::Opl2, false), 0).is_err());
	}
}
