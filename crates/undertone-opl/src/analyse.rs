// Reads notes and drum hits back out of a register write log, or an OPX file.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal on purpose: it replays the chip's registers write by write and reports what sounded.
//! It does not group channels into tracks or turn the result into a song, so a stream from
//! anywhere can be opened and compared with another.

use oplstream::{Chip, Command, DecodeError, Decoder, HEADER_LEN, Header, HeaderError};

use crate::patches::{RHYTHM_DRUMS, RhythmDrum};
use crate::registers::OPERATORS;

/// A register write at a tick. A register of 0x100 and up is on the second bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickWrite {
	/// The tick the write happens at.
	pub tick: i64,
	/// The register: 0x100 and up are the OPL3's second bank.
	pub register: u16,
	/// The value written.
	pub value: u8,
}

/// A note from key-on to key-off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedNote {
	/// The chip channel, 0 to 17 (9 and up on the second bank).
	pub channel: usize,
	/// The tick of the key-on.
	pub on: i64,
	/// The tick of the key-off, or the end of the stream if the key was still down.
	pub off: i64,
	/// The pitch's F-number at key-on.
	pub fnum: u16,
	/// The pitch's block at key-on.
	pub block: u8,
	/// Registers 20, 40, 60, 80 and E0 of the modulator, then of the carrier, then C0, as they
	/// stood at key-on.
	pub patch: [u8; 11],
	/// The carrier's total level at key-on (0 loudest, 63 quietest).
	pub carrier_tl: u8,
}

/// A rising edge of a rhythm drum's bit in register BD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedHit {
	/// The tick of the hit.
	pub tick: i64,
	/// The drum struck.
	pub drum: RhythmDrum,
}

/// What a stream sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
	/// The length of the stream in ticks.
	pub ticks: i64,
	/// Notes in key-on order.
	pub notes: Vec<DecodedNote>,
	/// Rhythm hits in order.
	pub hits: Vec<DecodedHit>,
	/// Whether register BD ever switched the rhythm section on.
	pub rhythm: bool,
	/// Pitch changes made while a key was down.
	pub bends_while_keyed: usize,
}

/// Why an OPX file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyseError {
	/// The header does not parse.
	Header(HeaderError),
	/// A command does not decode.
	Command(DecodeError),
}

fn pitch_of(regs: &[u8; 512], bank: usize, channel: usize) -> (u16, u8) {
	let at = bank * 0x100;
	let high = regs[at + 0xb0 + channel];
	(
		u16::from(regs[at + 0xa0 + channel]) | (u16::from(high & 3) << 8),
		(high >> 2) & 7,
	)
}

/// Replays writes on a register file and collects what sounded. `ticks` is the stream's length.
pub fn decode_writes(writes: impl IntoIterator<Item = TickWrite>, ticks: i64) -> Decoded {
	let mut regs = [0u8; 512];
	let mut out = Decoded {
		ticks,
		notes: Vec::new(),
		hits: Vec::new(),
		rhythm: false,
		bends_while_keyed: 0,
	};
	// The open note on each channel: an index into `out.notes` and its pitch as last seen.
	let mut open: [Option<(usize, (u16, u8))>; 18] = [None; 18];
	for TickWrite {
		tick,
		register,
		value,
	} in writes
	{
		let (bank, low) = (usize::from(register >> 8), usize::from(register & 0xff));
		let before = regs[bank * 0x100 + low];
		regs[bank * 0x100 + low] = value;
		match low {
			0xa0..=0xa8 | 0xb0..=0xb8 => {
				let channel = low & 0x0f;
				let rhythm_channel =
					bank == 0 && (6..=8).contains(&channel) && regs[0xbd] & 0x20 != 0;
				if rhythm_channel {
					continue;
				}
				let keyed = regs[bank * 0x100 + 0xb0 + channel] & 0x20 != 0;
				let was_keyed = if low >= 0xb0 {
					before & 0x20 != 0
				} else {
					keyed
				};
				let pitch = pitch_of(&regs, bank, channel);
				let number = bank * 9 + channel;
				if keyed && !was_keyed {
					let (m, c) = OPERATORS[channel];
					let (m, c) = (bank * 0x100 + usize::from(m), bank * 0x100 + usize::from(c));
					let patch = [
						regs[0x20 + m],
						regs[0x40 + m],
						regs[0x60 + m],
						regs[0x80 + m],
						regs[0xe0 + m],
						regs[0x20 + c],
						regs[0x40 + c],
						regs[0x60 + c],
						regs[0x80 + c],
						regs[0xe0 + c],
						regs[bank * 0x100 + 0xc0 + channel],
					];
					out.notes.push(DecodedNote {
						channel: number,
						on: tick,
						off: -1,
						fnum: pitch.0,
						block: pitch.1,
						patch,
						carrier_tl: regs[0x40 + c] & 0x3f,
					});
					open[number] = Some((out.notes.len() - 1, pitch));
				} else if !keyed && was_keyed {
					if let Some((index, _)) = open[number].take() {
						out.notes[index].off = tick;
					}
				} else if keyed
					&& let Some((_, last)) = &mut open[number]
					&& *last != pitch
				{
					out.bends_while_keyed += 1;
					*last = pitch;
				}
			}
			0xbd if bank == 0 => {
				if value & 0x20 != 0 {
					out.rhythm = true;
				}
				let rising = value & !before & 0x1f;
				for drum in RHYTHM_DRUMS {
					if i32::from(rising) & drum.rhythm().bit != 0 {
						out.hits.push(DecodedHit { tick, drum });
					}
				}
			}
			_ => {}
		}
	}
	for note in &mut out.notes {
		if note.off < 0 {
			note.off = ticks;
		}
	}
	out
}

/// Reads an OPX file's register writes and replays them. The file's own `total_ticks` is the
/// length, so a stream that ends in silence keeps it.
pub fn decode_opx(file: &[u8]) -> Result<Decoded, AnalyseError> {
	let header = Header::parse(file).map_err(AnalyseError::Header)?;
	let chip = header.chip;
	if !matches!(chip, Chip::Opl2 | Chip::Opl3) {
		return Ok(Decoded {
			ticks: i64::from(header.total_ticks),
			notes: Vec::new(),
			hits: Vec::new(),
			rhythm: false,
			bends_while_keyed: 0,
		});
	}
	let mut writes = Vec::new();
	for step in Decoder::new(&file[HEADER_LEN..], chip) {
		let step = step.map_err(AnalyseError::Command)?;
		if let Command::Write { bank, reg, val } = step.command {
			writes.push(TickWrite {
				tick: i64::from(step.tick),
				register: u16::from(bank) << 8 | u16::from(reg),
				value: val,
			});
		}
	}
	Ok(decode_writes(writes, i64::from(header.total_ticks)))
}
