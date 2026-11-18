// The command decoder and the tick stepper: no allocation, no panics on any input.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::header::{Chip, HEADER_LEN, Header, HeaderError};

/// A decoded command. Waits carry their tick count; extensions borrow their payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'a> {
	/// `END` (`00`): the stream ends here.
	End,
	/// `W0` or `W1` (`01`, `02`): write `val` to register `reg` of `bank` (0 or 1).
	Write {
		/// The register bank, 0 or 1.
		bank: u8,
		/// The register within the bank.
		reg: u8,
		/// The value written.
		val: u8,
	},
	/// `TIER` (`05`): 0 is core and 1 is extras.
	Tier(u8),
	/// `WAIT` or `LWAIT` (`80` to `EF`, `06`): advance the clock by this many ticks (1 to 65,535).
	Wait(u16),
	/// `LOOP` (`08`): the loop point.
	Loop,
	/// `MARK` (`09`): a marker reported to the host.
	Mark(u8),
	/// `SPK` (`0A`): set the PC speaker divisor; 0 is gate off.
	Speaker(u16),
	/// An extension (`F0` to `FF`) with its payload; it has no effect on the chip or the clock.
	Extension {
		/// The extension opcode, `0xF0` to `0xFF`.
		opcode: u8,
		/// The payload bytes (0 to 255).
		payload: &'a [u8],
	},
}

/// One decoded command with where and when it happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step<'a> {
	/// Byte offset of the command from the first byte of the command stream.
	pub offset: usize,
	/// The clock, in ticks, when the command runs. For a wait this is the tick before it.
	pub tick: u32,
	/// The tier in effect when the command runs (for a `TIER` command, the tier it replaces).
	pub tier: u8,
	/// The command.
	pub command: Command<'a>,
}

/// Why a command could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
	/// The stream ran out before the command's operands did.
	Truncated,
	/// The stream ran out before an `END` command.
	MissingEnd,
	/// An opcode that is reserved or unknown and is not an extension.
	UnknownOpcode(u8),
	/// A `W1` command on an OPL2 stream.
	BankOneOnOpl2,
	/// An `SPK` command on an OPL stream.
	SpeakerCommandOnOpl,
	/// A `W0`, `W1` or `TIER` command on a speaker stream.
	OplCommandOnSpeaker,
	/// An `LWAIT` of zero ticks.
	ZeroLongWait,
	/// A `TIER` value above 1.
	BadTier(u8),
	/// The clock would pass 4,294,967,295 ticks.
	WaitOverrun,
}

/// A decoding error and the offset of the command that caused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeError {
	/// Byte offset of the command, or the stream length for [`DecodeErrorKind::MissingEnd`].
	pub offset: usize,
	/// What was wrong.
	pub kind: DecodeErrorKind,
}

impl core::fmt::Display for DecodeError {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		let offset = self.offset;
		match self.kind {
			DecodeErrorKind::Truncated => {
				write!(
					f,
					"the stream ends in the middle of the command at byte {offset}"
				)
			}
			DecodeErrorKind::MissingEnd => {
				write!(
					f,
					"the stream has no END command (it stops at byte {offset})"
				)
			}
			DecodeErrorKind::UnknownOpcode(opcode) => {
				write!(
					f,
					"byte {offset} is the reserved or unknown opcode {opcode:#04X}"
				)
			}
			DecodeErrorKind::BankOneOnOpl2 => {
				write!(f, "byte {offset} writes to bank 1 on an OPL2 stream")
			}
			DecodeErrorKind::SpeakerCommandOnOpl => {
				write!(f, "byte {offset} is a speaker command in an OPL stream")
			}
			DecodeErrorKind::OplCommandOnSpeaker => {
				write!(f, "byte {offset} is an OPL command in a speaker stream")
			}
			DecodeErrorKind::ZeroLongWait => {
				write!(f, "byte {offset} is a long wait of zero ticks")
			}
			DecodeErrorKind::BadTier(tier) => write!(f, "byte {offset} sets the tier to {tier}"),
			DecodeErrorKind::WaitOverrun => {
				write!(f, "the waits at byte {offset} pass the largest clock value")
			}
		}
	}
}

/// Decodes a command stream one command at a time and keeps the clock.
///
/// It is an iterator of [`Step`]s. It stops after `END` and after the first error.
#[derive(Clone, Debug)]
pub struct Decoder<'a> {
	stream: &'a [u8],
	chip: Chip,
	pos: usize,
	now: u32,
	tier: u8,
	done: bool,
}

impl<'a> Decoder<'a> {
	/// Starts decoding `stream`, the bytes after the header, for a stream written for `chip`.
	pub fn new(stream: &'a [u8], chip: Chip) -> Self {
		Self {
			stream,
			chip,
			pos: 0,
			now: 0,
			tier: 0,
			done: false,
		}
	}

	/// Parses the header of `file` and decodes the command stream that follows it.
	///
	/// The stream is cut to `stream_len` bytes when the file is longer. This does not validate;
	/// use [`validate`](crate::validate) for that.
	pub fn from_file(file: &'a [u8]) -> Result<(Header, Self), HeaderError> {
		let header = Header::parse(file)?;
		let rest = &file[HEADER_LEN..];
		let len = usize::try_from(header.stream_len).map_or(rest.len(), |n| n.min(rest.len()));
		Ok((header, Self::new(&rest[..len], header.chip)))
	}

	/// The offset of the next command.
	pub fn offset(&self) -> usize {
		self.pos
	}

	/// The clock: the ticks that the commands decoded so far have waited.
	pub fn now(&self) -> u32 {
		self.now
	}

	/// The tier in effect.
	pub fn tier(&self) -> u8 {
		self.tier
	}

	/// Continues from `offset`, a command boundary, with the tier reset to 0 as a loop jump does.
	/// The clock is not changed.
	pub fn seek(&mut self, offset: usize) {
		self.pos = offset;
		self.tier = 0;
		self.done = false;
	}

	fn fail(
		&mut self,
		offset: usize,
		kind: DecodeErrorKind,
	) -> Option<Result<Step<'a>, DecodeError>> {
		self.done = true;
		Some(Err(DecodeError { offset, kind }))
	}
}

impl<'a> Iterator for Decoder<'a> {
	type Item = Result<Step<'a>, DecodeError>;

	fn next(&mut self) -> Option<Self::Item> {
		use DecodeErrorKind as E;
		if self.done {
			return None;
		}
		let at = self.pos;
		let Some(&opcode) = self.stream.get(at) else {
			return self.fail(self.stream.len(), E::MissingEnd);
		};
		let rest = &self.stream[at + 1..];
		let chip = self.chip;
		// The operand count, then the command, or the reason it is not valid on this chip.
		let (len, command) = match opcode {
			0x00 => (1, Command::End),
			0x01 | 0x02 => {
				let &[reg, val, ..] = rest else {
					return self.fail(at, E::Truncated);
				};
				if chip == Chip::Speaker {
					return self.fail(at, E::OplCommandOnSpeaker);
				}
				let bank = opcode - 1;
				if bank == 1 && chip == Chip::Opl2 {
					return self.fail(at, E::BankOneOnOpl2);
				}
				(3, Command::Write { bank, reg, val })
			}
			0x05 => {
				let &[tier, ..] = rest else {
					return self.fail(at, E::Truncated);
				};
				if chip == Chip::Speaker {
					return self.fail(at, E::OplCommandOnSpeaker);
				}
				if tier > 1 {
					return self.fail(at, E::BadTier(tier));
				}
				(2, Command::Tier(tier))
			}
			0x06 => {
				let &[lo, hi, ..] = rest else {
					return self.fail(at, E::Truncated);
				};
				let ticks = u16::from_le_bytes([lo, hi]);
				if ticks == 0 {
					return self.fail(at, E::ZeroLongWait);
				}
				(3, Command::Wait(ticks))
			}
			0x08 => (1, Command::Loop),
			0x09 => {
				let &[id, ..] = rest else {
					return self.fail(at, E::Truncated);
				};
				(2, Command::Mark(id))
			}
			0x0A => {
				let &[lo, hi, ..] = rest else {
					return self.fail(at, E::Truncated);
				};
				if chip != Chip::Speaker {
					return self.fail(at, E::SpeakerCommandOnOpl);
				}
				(3, Command::Speaker(u16::from_le_bytes([lo, hi])))
			}
			0x80..=0xEF => (1, Command::Wait(u16::from(opcode - 0x80) + 1)),
			0xF0..=0xFF => {
				let Some(&n) = rest.first() else {
					return self.fail(at, E::Truncated);
				};
				let n = usize::from(n);
				let Some(payload) = rest.get(1..1 + n) else {
					return self.fail(at, E::Truncated);
				};
				(2 + n, Command::Extension { opcode, payload })
			}
			other => return self.fail(at, E::UnknownOpcode(other)),
		};
		let step = Step {
			offset: at,
			tick: self.now,
			tier: self.tier,
			command,
		};
		match command {
			Command::End => self.done = true,
			Command::Tier(t) => self.tier = t,
			Command::Wait(n) => match self.now.checked_add(u32::from(n)) {
				Some(now) => self.now = now,
				None => return self.fail(at, E::WaitOverrun),
			},
			_ => {}
		}
		self.pos = at + len;
		Some(Ok(step))
	}
}

/// What a call to [`Stepper::tick`] left the stream doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickState {
	/// The stream is playing and wants more ticks.
	Playing,
	/// The stream reached `END` and does not loop.
	Finished,
}

/// Plays a command stream tick by tick, as a timer interrupt would.
///
/// Each call to [`tick`](Self::tick) is one tick of the grid. It runs every command due on that
/// tick, in stream order, and stops at the next wait. It never allocates and never panics.
#[derive(Clone, Debug)]
pub struct Stepper<'a> {
	decoder: Decoder<'a>,
	loop_offset: Option<usize>,
	wait_left: u32,
	progressed: bool,
	finished: bool,
}

impl<'a> Stepper<'a> {
	/// Plays `stream` once from the start.
	pub fn new(stream: &'a [u8], chip: Chip) -> Self {
		Self {
			decoder: Decoder::new(stream, chip),
			loop_offset: None,
			wait_left: 0,
			progressed: false,
			finished: false,
		}
	}

	/// Plays `file`, looping at its header's loop point when it has one.
	pub fn from_file(file: &'a [u8]) -> Result<Self, HeaderError> {
		let (header, decoder) = Decoder::from_file(file)?;
		let loop_offset = match header.loop_offset {
			u32::MAX => None,
			offset => usize::try_from(offset).ok(),
		};
		Ok(Self {
			decoder,
			loop_offset,
			wait_left: 0,
			progressed: false,
			finished: false,
		})
	}

	/// Loops at the `LOOP` command at `offset` in the command stream, a header's `loop_offset`.
	/// Playing continues with the command after it, with the tier reset to 0.
	pub fn with_loop(mut self, offset: usize) -> Self {
		self.loop_offset = Some(offset);
		self
	}

	/// The clock, in ticks, at the last command decoded.
	pub fn now(&self) -> u32 {
		self.decoder.now()
	}

	/// Runs one tick. `sink` is called for each command due on this tick, `END` included when it
	/// ends the stream; a loop jump itself is not reported.
	///
	/// A stream that loops without a tick between the loop point and `END` finishes instead of
	/// spinning.
	pub fn tick(&mut self, mut sink: impl FnMut(&Step<'a>)) -> Result<TickState, DecodeError> {
		if self.finished {
			return Ok(TickState::Finished);
		}
		if self.wait_left > 0 {
			self.wait_left -= 1;
			return Ok(TickState::Playing);
		}
		loop {
			let Some(step) = self.decoder.next() else {
				self.finished = true;
				return Ok(TickState::Finished);
			};
			let step = match step {
				Ok(step) => step,
				Err(error) => {
					self.finished = true;
					return Err(error);
				}
			};
			match step.command {
				Command::Wait(n) => {
					self.progressed = true;
					self.wait_left = u32::from(n) - 1;
					return Ok(TickState::Playing);
				}
				Command::End => match self.loop_offset {
					Some(offset) if self.progressed => {
						self.progressed = false;
						self.decoder.seek(offset.saturating_add(1));
					}
					_ => {
						sink(&step);
						self.finished = true;
						return Ok(TickState::Finished);
					}
				},
				_ => sink(&step),
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::examples;
	use std::vec::Vec;

	fn decode(file: &[u8]) -> Vec<(u32, Command<'_>)> {
		let (_, decoder) = Decoder::from_file(file).unwrap();
		decoder
			.map(|s| s.unwrap())
			.map(|s| (s.tick, s.command))
			.collect()
	}

	#[test]
	fn example_one_decodes_as_listed() {
		let steps = decode(&examples::ONE);
		let w = |reg, val| Command::Write { bank: 0, reg, val };
		assert_eq!(
			steps,
			[
				(0, w(0x01, 0x20)),
				(0, w(0xA0, 0x41)),
				(0, w(0xB0, 0x31)),
				(0, Command::Wait(16)),
				(16, w(0xB0, 0x11)),
				(16, Command::Wait(4)),
				(20, Command::End),
			]
		);
	}

	#[test]
	fn example_three_tracks_tiers() {
		let (_, decoder) = Decoder::from_file(&examples::THREE).unwrap();
		let extras: Vec<_> = decoder
			.map(|s| s.unwrap())
			.filter(|s| s.tier == 1 && matches!(s.command, Command::Write { .. }))
			.map(|s| (s.tick, s.command))
			.collect();
		assert_eq!(
			extras,
			[
				(
					0,
					Command::Write {
						bank: 1,
						reg: 0xC0,
						val: 0x31
					}
				),
				(
					0,
					Command::Write {
						bank: 1,
						reg: 0xB0,
						val: 0x31
					}
				),
				(
					16,
					Command::Write {
						bank: 1,
						reg: 0xB0,
						val: 0x11
					}
				),
			]
		);
	}

	#[test]
	fn example_four_and_five() {
		let steps = decode(&examples::FOUR);
		assert_eq!(steps[0], (0, Command::Speaker(1193)));
		assert_eq!(steps[steps.len() - 1], (9, Command::End));
		let steps = decode(&examples::FIVE);
		assert_eq!(
			steps[1],
			(
				0,
				Command::Extension {
					opcode: 0xF8,
					payload: &[0xAA, 0xBB, 0xCC]
				}
			)
		);
		assert_eq!(
			steps[2],
			(
				0,
				Command::Extension {
					opcode: 0xF9,
					payload: &[]
				}
			)
		);
	}

	#[test]
	fn errors_carry_offsets() {
		use DecodeErrorKind as E;
		let run = |chip, bytes: &[u8]| Decoder::new(bytes, chip).find_map(Result::err);
		let err = |offset, kind| Some(DecodeError { offset, kind });
		assert_eq!(run(Chip::Opl2, &[0x01, 0x01]), err(0, E::Truncated));
		assert_eq!(run(Chip::Opl2, &[0x80]), err(1, E::MissingEnd));
		assert_eq!(run(Chip::Opl2, &[0x80, 0x07]), err(1, E::UnknownOpcode(7)));
		assert_eq!(run(Chip::Opl2, &[0x03]), err(0, E::UnknownOpcode(3)));
		assert_eq!(run(Chip::Opl2, &[0x0B]), err(0, E::UnknownOpcode(0x0B)));
		assert_eq!(run(Chip::Opl2, &[0x7F]), err(0, E::UnknownOpcode(0x7F)));
		assert_eq!(run(Chip::Opl2, &[0x02, 0, 0, 0]), err(0, E::BankOneOnOpl2));
		assert_eq!(run(Chip::Opl3, &[0x02, 0, 0, 0]), None);
		assert_eq!(
			run(Chip::Opl3, &[0x0A, 0, 0, 0]),
			err(0, E::SpeakerCommandOnOpl)
		);
		assert_eq!(
			run(Chip::Speaker, &[0x01, 0, 0, 0]),
			err(0, E::OplCommandOnSpeaker)
		);
		assert_eq!(
			run(Chip::Speaker, &[0x05, 0, 0]),
			err(0, E::OplCommandOnSpeaker)
		);
		assert_eq!(run(Chip::Opl2, &[0x06, 0, 0, 0]), err(0, E::ZeroLongWait));
		assert_eq!(run(Chip::Opl2, &[0x05, 2, 0]), err(0, E::BadTier(2)));
		assert_eq!(run(Chip::Opl2, &[0xF0, 3, 1, 2]), err(0, E::Truncated));
		assert_eq!(run(Chip::Opl2, &[0xF0]), err(0, E::Truncated));
		// Speaker streams still accept markers, waits, loops and extensions.
		assert_eq!(run(Chip::Speaker, &[0x09, 1, 0x08, 0xF0, 0, 0x81, 0]), None);
	}

	#[test]
	fn long_waits_overrun() {
		let mut stream = std::vec![];
		for _ in 0..65_538 {
			stream.extend_from_slice(&[0x06, 0xFF, 0xFF]);
		}
		stream.push(0);
		let err = Decoder::new(&stream, Chip::Opl2)
			.find_map(Result::err)
			.unwrap();
		assert_eq!(err.kind, DecodeErrorKind::WaitOverrun);
		assert_eq!(err.offset, 65_537 * 3);
	}

	#[test]
	fn decoder_stops_after_end_and_error() {
		let mut decoder = Decoder::new(&[0, 0, 0], Chip::Opl2);
		assert!(decoder.next().unwrap().is_ok());
		assert!(decoder.next().is_none());
		let mut decoder = Decoder::new(&[0x07, 0], Chip::Opl2);
		assert!(decoder.next().unwrap().is_err());
		assert!(decoder.next().is_none());
	}

	fn play(file: &[u8], ticks: u32) -> Vec<(u32, Command<'_>)> {
		let mut stepper = Stepper::from_file(file).unwrap();
		let mut out = Vec::new();
		for t in 0..ticks {
			let state = stepper
				.tick(|s| {
					if !matches!(s.command, Command::End) {
						out.push((t, s.command));
					}
				})
				.unwrap();
			if state == TickState::Finished {
				out.push((t, Command::End));
				break;
			}
		}
		out
	}

	#[test]
	fn stepper_matches_the_decoded_listing() {
		let w = |reg, val| Command::Write { bank: 0, reg, val };
		assert_eq!(
			play(&examples::ONE, 100),
			[
				(0, w(0x01, 0x20)),
				(0, w(0xA0, 0x41)),
				(0, w(0xB0, 0x31)),
				(16, w(0xB0, 0x11)),
				(20, Command::End),
			]
		);
	}

	#[test]
	fn stepper_loops_and_the_clock_continues() {
		let w = |reg, val| Command::Write { bank: 0, reg, val };
		let steps = play(&examples::TWO, 80);
		assert_eq!(
			steps,
			[
				(0, w(0x01, 0x20)),
				(4, Command::Loop),
				(4, w(0xB0, 0x31)),
				(20, w(0xB0, 0x11)),
				// The pass ends at 36 and the next one starts there, after the LOOP command.
				(36, w(0xB0, 0x31)),
				(52, w(0xB0, 0x11)),
				(68, w(0xB0, 0x31)),
			]
		);
	}

	#[test]
	fn stepper_does_not_spin_on_a_loop_without_ticks() {
		// LOOP, a write, END: no tick after the loop point.
		let stream = [0x08, 0x01, 1, 2, 0x00];
		let mut stepper = Stepper::new(&stream, Chip::Opl2).with_loop(0);
		let mut seen = 0;
		let mut state = TickState::Playing;
		for _ in 0..4 {
			state = stepper.tick(|_| seen += 1).unwrap();
		}
		assert_eq!(state, TickState::Finished);
		assert_eq!(seen, 3);
	}

	#[test]
	fn stepper_reports_errors_once() {
		let mut stepper = Stepper::new(&[0x07], Chip::Opl2);
		assert!(stepper.tick(|_| {}).is_err());
		assert_eq!(stepper.tick(|_| {}), Ok(TickState::Finished));
	}

	#[test]
	fn stepper_survives_a_bad_loop_offset() {
		let mut stepper = Stepper::new(&[0x81, 0x00], Chip::Opl2).with_loop(usize::MAX);
		for _ in 0..8 {
			let _ = stepper.tick(|_| {});
		}
	}
}
