// The OPX writer: a builder that emits commands and fills in the derived header fields.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::command::{Command, Decoder};
use crate::header::{Chip, HEADER_LEN, Header, VERSION_MAJOR, VERSION_MINOR};
use crate::validate::{ValidationError, check};
use alloc::vec::Vec;

/// Why a stream could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
	/// A write named a bank other than 0 or 1.
	BankOutOfRange(u8),
	/// An extension opcode outside `0xF0` to `0xFF`.
	ExtensionOpcode(u8),
	/// An extension payload of more than 255 bytes.
	ExtensionTooLong(usize),
	/// A reserved channel number above 31.
	ChannelOutOfRange(u8),
	/// A command was added after `end`.
	CommandAfterEnd,
	/// The waits add up to more than 4,294,967,295 ticks.
	WaitOverrun,
	/// The command stream is longer than a `u32` can count.
	StreamTooLong,
	/// The version asked for is lower than the lowest that can express the stream.
	VersionTooLow {
		/// The version asked for, major then minor.
		asked: (u8, u8),
		/// The lowest version that can express the stream.
		lowest: (u8, u8),
	},
	/// The finished file breaks a rule of the specification.
	Invalid(ValidationError),
}

/// Builds an OPX file.
///
/// Add commands in stream order, then call [`finish`](Self::finish). A problem with a command is
/// remembered and reported by `finish`, so calls chain.
///
/// ```
/// use oplstream::{Chip, Writer};
///
/// let mut writer = Writer::new(Chip::Opl2, 560);
/// writer.write(0, 0xB0, 0x31).wait(16).write(0, 0xB0, 0x11);
/// let file = writer.finish().unwrap();
/// assert_eq!(&file[..4], b"OPX\x1A");
/// ```
#[derive(Clone, Debug)]
pub struct Writer {
	chip: Chip,
	grid_hz: u32,
	version: Option<(u8, u8)>,
	reserved_channels: u32,
	stream: Vec<u8>,
	pending_wait: u64,
	total_wait: u64,
	ended: bool,
	error: Option<WriteError>,
}

impl Writer {
	/// Starts a stream for `chip` on a grid of `grid_hz` ticks per second.
	pub fn new(chip: Chip, grid_hz: u32) -> Self {
		Self {
			chip,
			grid_hz,
			version: None,
			reserved_channels: 0,
			stream: Vec::new(),
			pending_wait: 0,
			total_wait: 0,
			ended: false,
			error: None,
		}
	}

	/// Writes this version instead of the lowest that can express the stream. A version below the
	/// lowest is an error at `finish`.
	pub fn version(&mut self, major: u8, minor: u8) -> &mut Self {
		self.version = Some((major, minor));
		self
	}

	/// Sets the whole `reserved_channels` mask.
	pub fn reserved_channels(&mut self, mask: u32) -> &mut Self {
		self.reserved_channels = mask;
		self
	}

	/// Promises that the stream never writes channel `channel` (0 to 31).
	pub fn reserve_channel(&mut self, channel: u8) -> &mut Self {
		if channel > 31 {
			self.fail(WriteError::ChannelOutOfRange(channel));
		} else {
			self.reserved_channels |= 1 << channel;
		}
		self
	}

	fn fail(&mut self, error: WriteError) {
		self.error.get_or_insert(error);
	}

	/// Flushes the pending wait in its shortest form, then makes room for a command.
	fn command(&mut self, bytes: &[u8]) -> &mut Self {
		if self.ended {
			self.fail(WriteError::CommandAfterEnd);
			return self;
		}
		self.flush_wait();
		self.stream.extend_from_slice(bytes);
		self
	}

	fn flush_wait(&mut self) {
		let mut left = core::mem::take(&mut self.pending_wait);
		while left > 0 {
			let n = left.min(0xFFFF) as u16;
			left -= u64::from(n);
			if n <= 112 {
				self.stream.push(0x80 + (n - 1) as u8);
			} else {
				let [lo, hi] = n.to_le_bytes();
				self.stream.extend_from_slice(&[0x06, lo, hi]);
			}
		}
	}

	/// A register write to `bank` 0 or 1 at the current tick.
	pub fn write(&mut self, bank: u8, reg: u8, val: u8) -> &mut Self {
		match bank {
			0 => self.command(&[0x01, reg, val]),
			1 => self.command(&[0x02, reg, val]),
			other => {
				self.fail(WriteError::BankOutOfRange(other));
				self
			}
		}
	}

	/// Advances the clock by `ticks`. Consecutive waits merge, and a wait is written in its
	/// shortest form: one byte up to 112 ticks, `LWAIT` above, split at 65,535.
	pub fn wait(&mut self, ticks: u32) -> &mut Self {
		if self.ended {
			self.fail(WriteError::CommandAfterEnd);
			return self;
		}
		self.pending_wait += u64::from(ticks);
		self.total_wait += u64::from(ticks);
		if self.total_wait > u64::from(u32::MAX) {
			self.fail(WriteError::WaitOverrun);
		}
		self
	}

	/// Sets the tier of the writes that follow: 0 core, 1 extras.
	pub fn tier(&mut self, tier: u8) -> &mut Self {
		self.command(&[0x05, tier])
	}

	/// Marks the loop point at the current position.
	pub fn loop_point(&mut self) -> &mut Self {
		self.command(&[0x08])
	}

	/// A marker with an 8-bit id, reported to the host at its tick.
	pub fn marker(&mut self, id: u8) -> &mut Self {
		self.command(&[0x09, id])
	}

	/// Sets the PC speaker divisor; 0 gates it off.
	pub fn speaker(&mut self, divisor: u16) -> &mut Self {
		let [lo, hi] = divisor.to_le_bytes();
		self.command(&[0x0A, lo, hi])
	}

	/// An extension command, `opcode` in `0xF0` to `0xFF` and a payload of at most 255 bytes.
	pub fn extension(&mut self, opcode: u8, payload: &[u8]) -> &mut Self {
		if opcode < 0xF0 {
			self.fail(WriteError::ExtensionOpcode(opcode));
			return self;
		}
		let Ok(len) = u8::try_from(payload.len()) else {
			self.fail(WriteError::ExtensionTooLong(payload.len()));
			return self;
		};
		self.command(&[opcode, len]);
		if !self.ended {
			self.stream.extend_from_slice(payload);
		}
		self
	}

	/// Ends the stream. Any wait before it stays: it is the silence at the end.
	pub fn end(&mut self) -> &mut Self {
		self.command(&[0x00]);
		self.ended = true;
		self
	}

	/// Ends the stream if it is not ended, fills in `flags`, `total_ticks`, `loop_offset` and
	/// `stream_len`, picks the lowest version, checks the result against the specification and
	/// returns the file's bytes.
	pub fn finish(&mut self) -> Result<Vec<u8>, WriteError> {
		if !self.ended && self.error.is_none() {
			self.end();
		}
		if let Some(error) = self.error {
			return Err(error);
		}
		let stream_len = u32::try_from(self.stream.len()).map_err(|_| WriteError::StreamTooLong)?;

		let mut flags = 0;
		let mut total_ticks = 0;
		let mut loop_offset = None;
		for step in Decoder::new(&self.stream, self.chip).map_while(Result::ok) {
			match step.command {
				Command::Tier(1) => flags |= 1,
				Command::Write {
					reg: 0xC0..=0xC8,
					val,
					..
				} if val & 0x30 != 0 => flags |= 2,
				Command::Loop => {
					loop_offset.get_or_insert(step.offset);
				}
				Command::End => total_ticks = step.tick,
				_ => {}
			}
		}
		// Only the first LOOP is recorded; a second one fails validation below.
		let loop_offset = loop_offset.map_or(u32::MAX, |o| o as u32);

		let lowest = (VERSION_MAJOR, VERSION_MINOR);
		let (major, minor) = self.version.unwrap_or(lowest);
		if (major, minor) < lowest {
			return Err(WriteError::VersionTooLow {
				asked: (major, minor),
				lowest,
			});
		}
		let header = Header {
			major,
			minor,
			chip: self.chip,
			flags,
			grid_hz: self.grid_hz,
			total_ticks,
			loop_offset,
			stream_len,
			reserved_channels: self.reserved_channels,
		};
		let mut file = Vec::with_capacity(HEADER_LEN + self.stream.len());
		file.extend_from_slice(&header.to_bytes());
		file.extend_from_slice(&self.stream);
		check(&file, false).map_err(WriteError::Invalid)?;
		Ok(file)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::examples::{FIVE, FOUR, ONE, THREE, TWO};
	use crate::validate::validate;

	#[test]
	fn writes_example_one() {
		let mut w = Writer::new(Chip::Opl2, 560);
		w.write(0, 0x01, 0x20)
			.write(0, 0xA0, 0x41)
			.write(0, 0xB0, 0x31)
			.wait(16)
			.write(0, 0xB0, 0x11)
			.wait(4)
			.end();
		assert_eq!(w.finish().unwrap(), ONE);
	}

	#[test]
	fn writes_example_two_with_its_loop() {
		let mut w = Writer::new(Chip::Opl2, 560);
		w.reserve_channel(5)
			.write(0, 0x01, 0x20)
			.wait(4)
			.loop_point()
			.write(0, 0xB0, 0x31)
			.wait(16)
			.write(0, 0xB0, 0x11)
			.wait(16);
		assert_eq!(w.finish().unwrap(), TWO);
	}

	#[test]
	fn writes_example_three_with_flags_and_tiers() {
		let mut w = Writer::new(Chip::Opl3, 560);
		w.write(0, 0xC0, 0x31)
			.write(0, 0xB0, 0x31)
			.tier(1)
			.write(1, 0xC0, 0x31)
			.write(1, 0xB0, 0x31)
			.tier(0)
			.wait(16)
			.marker(7)
			.tier(1)
			.write(1, 0xB0, 0x11)
			.tier(0)
			.write(0, 0xB0, 0x11);
		assert_eq!(w.finish().unwrap(), THREE);
	}

	#[test]
	fn writes_example_four_for_the_speaker() {
		let mut w = Writer::new(Chip::Speaker, 140);
		w.speaker(1193)
			.wait(4)
			.speaker(0)
			.wait(2)
			.speaker(607)
			.wait(3)
			.speaker(0);
		assert_eq!(w.finish().unwrap(), FOUR);
	}

	#[test]
	fn writes_example_five_with_extensions() {
		let mut w = Writer::new(Chip::Opl2, 560);
		w.write(0, 0x01, 0x20)
			.extension(0xF8, &[0xAA, 0xBB, 0xCC])
			.extension(0xF9, &[])
			.write(0, 0xB0, 0x31)
			.wait(4);
		assert_eq!(w.finish().unwrap(), FIVE);
	}

	fn stream_of(f: impl FnOnce(&mut Writer)) -> Vec<u8> {
		let mut w = Writer::new(Chip::Opl2, 560);
		f(&mut w);
		let file = w.finish().unwrap();
		validate(&file).unwrap();
		file[HEADER_LEN..].to_vec()
	}

	#[test]
	fn waits_merge_and_use_the_shortest_form() {
		assert_eq!(
			stream_of(|w| {
				w.wait(3).wait(4);
			}),
			[0x86, 0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(1);
			}),
			[0x80, 0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(112);
			}),
			[0xEF, 0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(113);
			}),
			[0x06, 113, 0, 0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(60).wait(60);
			}),
			[0x06, 120, 0, 0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(0).wait(0);
			}),
			[0x00]
		);
		assert_eq!(
			stream_of(|w| {
				w.wait(65_535);
			}),
			[0x06, 0xFF, 0xFF, 0x00]
		);
	}

	#[test]
	fn long_waits_split_at_65535() {
		// 65,535 + 34,465: the remainder is above 112, so it is an LWAIT.
		assert_eq!(
			stream_of(|w| {
				w.wait(100_000);
			}),
			[0x06, 0xFF, 0xFF, 0x06, 0xA1, 0x86, 0x00]
		);
		// 65,535 + 5: the remainder fits one short wait.
		assert_eq!(
			stream_of(|w| {
				w.wait(65_540);
			}),
			[0x06, 0xFF, 0xFF, 0x84, 0x00]
		);
		let file = {
			let mut w = Writer::new(Chip::Opl2, 560);
			w.wait(u32::MAX);
			w.finish().unwrap()
		};
		assert_eq!(Header::parse(&file).unwrap().total_ticks, u32::MAX);
	}

	#[test]
	fn a_wait_flushes_before_a_loop_point() {
		let file = {
			let mut w = Writer::new(Chip::Opl2, 560);
			w.wait(2).loop_point().wait(2);
			w.finish().unwrap()
		};
		let header = validate(&file).unwrap();
		assert_eq!(header.loop_offset, 1);
		assert_eq!(header.total_ticks, 4);
	}

	#[test]
	fn finish_ends_the_stream_and_the_header_is_derived() {
		let mut w = Writer::new(Chip::Opl3, 700);
		w.write(1, 0x05, 1).wait(10);
		let file = w.finish().unwrap();
		let header = validate(&file).unwrap();
		assert_eq!((header.major, header.minor), (0, 1));
		assert_eq!(header.stream_len as usize, file.len() - HEADER_LEN);
		assert_eq!(header.total_ticks, 10);
		assert_eq!(header.loop_offset, u32::MAX);
		assert_eq!(header.flags, 0);
	}

	#[test]
	fn reports_problems_at_finish() {
		let fails = |f: &dyn Fn(&mut Writer)| {
			let mut w = Writer::new(Chip::Opl3, 560);
			f(&mut w);
			w.finish().unwrap_err()
		};
		assert_eq!(
			fails(&|w| {
				w.write(2, 0, 0);
			}),
			WriteError::BankOutOfRange(2)
		);
		assert_eq!(
			fails(&|w| {
				w.extension(0x10, &[]);
			}),
			WriteError::ExtensionOpcode(0x10)
		);
		assert_eq!(
			fails(&|w| {
				w.extension(0xF0, &[0; 256]);
			}),
			WriteError::ExtensionTooLong(256)
		);
		assert_eq!(
			fails(&|w| {
				w.reserve_channel(32);
			}),
			WriteError::ChannelOutOfRange(32)
		);
		assert_eq!(
			fails(&|w| {
				w.end().wait(1);
			}),
			WriteError::CommandAfterEnd
		);
		assert_eq!(
			fails(&|w| {
				w.wait(u32::MAX).wait(1);
			}),
			WriteError::WaitOverrun
		);
		assert_eq!(
			fails(&|w| {
				w.version(0, 0);
			}),
			WriteError::VersionTooLow {
				asked: (0, 0),
				lowest: (0, 1)
			}
		);
		// The first problem wins.
		assert_eq!(
			fails(&|w| {
				w.write(5, 0, 0).write(6, 0, 0);
			}),
			WriteError::BankOutOfRange(5)
		);
	}

	#[test]
	fn refuses_streams_that_break_the_specification() {
		let invalid = |f: &dyn Fn(&mut Writer), chip| {
			let mut w = Writer::new(chip, 560);
			f(&mut w);
			matches!(w.finish(), Err(WriteError::Invalid(_)))
		};
		assert!(invalid(
			&|w| {
				w.write(1, 0, 0);
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.speaker(1);
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.loop_point();
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.loop_point().wait(1).loop_point().wait(1);
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.tier(2);
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.reserve_channel(0).write(0, 0xA0, 1);
			},
			Chip::Opl2
		));
		assert!(invalid(
			&|w| {
				w.tier(1).write(0, 0, 0).tier(0).write(1, 0, 0);
			},
			Chip::Opl3
		));
		assert!(invalid(
			&|w| {
				w.extension(0xF1, &[0; 3]);
				w.extension(0xF1, &[0; 3]);
			},
			Chip::Opl2
		));
		let mut w = Writer::new(Chip::Opl2, 0);
		assert!(matches!(w.finish(), Err(WriteError::Invalid(_))));
	}

	#[test]
	fn a_later_version_is_written_as_asked() {
		let mut w = Writer::new(Chip::Opl2, 560);
		w.version(0, 2);
		let file = w.finish().unwrap();
		assert_eq!(file[5], 2);
		// A reader of version 0.1 refuses it.
		assert_eq!(validate(&file), Err(ValidationError::UnknownMinor(2)));
	}

	#[test]
	fn written_files_round_trip_through_the_decoder() {
		let mut w = Writer::new(Chip::Opl3, 560);
		w.write(0, 1, 2).wait(200).write(1, 3, 4).marker(9).wait(5);
		let file = w.finish().unwrap();
		let (_, decoder) = Decoder::from_file(&file).unwrap();
		let waits: u32 = decoder
			.filter_map(|s| match s.unwrap().command {
				Command::Wait(n) => Some(u32::from(n)),
				_ => None,
			})
			.sum();
		assert_eq!(waits, 205);
	}
}
