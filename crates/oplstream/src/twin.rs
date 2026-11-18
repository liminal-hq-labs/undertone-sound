// The text twin: a printer for any valid file and, with `alloc`, an assembler back to bytes.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::command::{Command, Decoder};
use crate::header::{Chip, HEADER_LEN};
use crate::validate::{ValidationError, validate};
use core::fmt;

/// Why a file could not be printed as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintError {
	/// The file breaks a rule of the specification, so it has no twin.
	Invalid(ValidationError),
	/// The output refused a write.
	Output(fmt::Error),
}

impl From<fmt::Error> for PrintError {
	fn from(error: fmt::Error) -> Self {
		Self::Output(error)
	}
}

/// Writes the canonical text twin of `file` to `out`.
///
/// The file is validated first. The twin has the header lines in the order `OPX`, `chip`, `grid`,
/// `reserved`, then `stream`, then one command per line, with each run of waits as one `T` line
/// before the next command, and a final newline. It does not allocate.
pub fn print_twin<W: fmt::Write>(file: &[u8], out: &mut W) -> Result<(), PrintError> {
	let header = validate(file).map_err(PrintError::Invalid)?;
	writeln!(out, "OPX {}.{}", header.major, header.minor)?;
	let chip = match header.chip {
		Chip::Opl2 => "opl2",
		Chip::Opl3 => "opl3",
		Chip::Speaker => "spkr",
	};
	writeln!(out, "chip {chip}")?;
	writeln!(out, "grid {}", header.grid_hz)?;
	if header.reserved_channels == 0 {
		writeln!(out, "reserved none")?;
	} else {
		write!(out, "reserved")?;
		for channel in 0..32 {
			if header.reserved_channels & (1 << channel) != 0 {
				write!(out, " {channel}")?;
			}
		}
		writeln!(out)?;
	}
	writeln!(out, "stream")?;

	let mut pending: u32 = 0;
	for step in Decoder::new(&file[HEADER_LEN..], header.chip).map_while(Result::ok) {
		if let Command::Wait(n) = step.command {
			// The validator has bounded the sum by `total_ticks`.
			pending = pending.saturating_add(u32::from(n));
			continue;
		}
		if pending > 0 {
			writeln!(out, "T {pending}")?;
			pending = 0;
		}
		match step.command {
			Command::End => writeln!(out, "END")?,
			Command::Write { bank, reg, val } => writeln!(out, "W{bank} {reg:02X} {val:02X}")?,
			Command::Tier(t) => writeln!(out, "TIER {t}")?,
			Command::Wait(_) => {}
			Command::Loop => writeln!(out, "LOOP")?,
			Command::Mark(id) => writeln!(out, "MARK {id}")?,
			Command::Speaker(divisor) => writeln!(out, "SPK {divisor}")?,
			Command::Extension { opcode, payload } => {
				write!(out, "X {opcode:02X}")?;
				for byte in payload {
					write!(out, " {byte:02X}")?;
				}
				writeln!(out)?;
			}
		}
	}
	Ok(())
}

#[cfg(feature = "alloc")]
pub use assemble::{TwinError, TwinErrorKind, assemble};

#[cfg(feature = "alloc")]
mod assemble {
	use crate::header::Chip;
	use crate::writer::{WriteError, Writer};
	use alloc::vec::Vec;

	/// Why a twin could not be assembled.
	#[derive(Clone, Copy, Debug, PartialEq, Eq)]
	pub enum TwinErrorKind {
		/// A line starts with something that is not a mnemonic that applies here.
		UnknownMnemonic,
		/// A line has too few or too many fields.
		FieldCount,
		/// A decimal field is not a number in range.
		BadNumber,
		/// A hex field is not one or two hex digits.
		BadHex,
		/// `chip` is not `opl2`, `opl3` or `spkr`.
		BadChip,
		/// `OPX` is not followed by `major.minor`.
		BadVersion,
		/// A header line appears twice.
		DuplicateHeader,
		/// A required header line (`OPX`, `chip` or `grid`) is missing.
		MissingHeader,
		/// There is no `stream` line.
		MissingStream,
		/// The twin has no `END`.
		MissingEnd,
		/// A line follows `END`.
		AfterEnd,
		/// A reserved channel above 31.
		ChannelOutOfRange,
		/// `T 0`.
		ZeroWait,
		/// An extension opcode outside F0 to FF.
		ExtensionOpcode,
		/// An extension payload of more than 255 bytes.
		ExtensionTooLong,
		/// The stream does not make a valid file; the line is 0.
		Write(WriteError),
	}

	/// A twin error and the 1-based line it is on.
	#[derive(Clone, Copy, Debug, PartialEq, Eq)]
	pub struct TwinError {
		/// The line number, counting from 1; 0 when the error is about the whole file.
		pub line: usize,
		/// What was wrong.
		pub kind: TwinErrorKind,
	}

	fn decimal(field: &str) -> Option<u32> {
		if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
			return None;
		}
		field.parse().ok()
	}

	fn hex(field: &str) -> Option<u8> {
		if field.is_empty() || field.len() > 2 || !field.bytes().all(|b| b.is_ascii_hexdigit()) {
			return None;
		}
		u8::from_str_radix(field, 16).ok()
	}

	/// Assembles a text twin into the bytes of an OPX file.
	///
	/// Mnemonics are case-insensitive, `;` starts a comment, and consecutive `T` lines add up.
	/// The file is checked against the specification before it is returned.
	pub fn assemble(text: &str) -> Result<Vec<u8>, TwinError> {
		use TwinErrorKind as K;
		let err = |line, kind| TwinError { line, kind };

		let mut version = None;
		let mut chip = None;
		let mut grid = None;
		let mut reserved: Option<u32> = None;
		let mut writer: Option<Writer> = None;
		let mut ended = false;

		for (index, raw) in text.lines().enumerate() {
			let line = index + 1;
			let content = raw.split(';').next().unwrap_or("");
			let mut fields = content.split_ascii_whitespace();
			let Some(word) = fields.next() else { continue };
			let args: Vec<&str> = fields.collect();
			let word = word.to_ascii_lowercase();

			if ended {
				return Err(err(line, K::AfterEnd));
			}

			let Some(w) = writer.as_mut() else {
				// The header block.
				match word.as_str() {
					"opx" => {
						if version.is_some() {
							return Err(err(line, K::DuplicateHeader));
						}
						let [field] = args[..] else {
							return Err(err(line, K::FieldCount));
						};
						let parsed = field.split_once('.').and_then(|(major, minor)| {
							Some((
								u8::try_from(decimal(major)?).ok()?,
								u8::try_from(decimal(minor)?).ok()?,
							))
						});
						version = Some(parsed.ok_or(err(line, K::BadVersion))?);
					}
					"chip" => {
						if chip.is_some() {
							return Err(err(line, K::DuplicateHeader));
						}
						let [field] = args[..] else {
							return Err(err(line, K::FieldCount));
						};
						chip = Some(match field.to_ascii_lowercase().as_str() {
							"opl2" => Chip::Opl2,
							"opl3" => Chip::Opl3,
							"spkr" => Chip::Speaker,
							_ => return Err(err(line, K::BadChip)),
						});
					}
					"grid" => {
						if grid.is_some() {
							return Err(err(line, K::DuplicateHeader));
						}
						let [field] = args[..] else {
							return Err(err(line, K::FieldCount));
						};
						grid = Some(decimal(field).ok_or(err(line, K::BadNumber))?);
					}
					"reserved" => {
						if reserved.is_some() {
							return Err(err(line, K::DuplicateHeader));
						}
						if args.is_empty() {
							return Err(err(line, K::FieldCount));
						}
						let mut mask = 0;
						if !(args.len() == 1 && args[0].eq_ignore_ascii_case("none")) {
							for field in &args {
								let channel = decimal(field).ok_or(err(line, K::BadNumber))?;
								if channel > 31 {
									return Err(err(line, K::ChannelOutOfRange));
								}
								mask |= 1 << channel;
							}
						}
						reserved = Some(mask);
					}
					"stream" => {
						if !args.is_empty() {
							return Err(err(line, K::FieldCount));
						}
						let (Some((major, minor)), Some(chip), Some(grid)) = (version, chip, grid)
						else {
							return Err(err(line, K::MissingHeader));
						};
						let mut new = Writer::new(chip, grid);
						new.version(major, minor)
							.reserved_channels(reserved.unwrap_or(0));
						writer = Some(new);
					}
					_ => return Err(err(line, K::UnknownMnemonic)),
				}
				continue;
			};

			let expect = |n: usize| {
				if args.len() == n {
					Ok(())
				} else {
					Err(err(line, K::FieldCount))
				}
			};
			match word.as_str() {
				"w0" | "w1" => {
					expect(2)?;
					let reg = hex(args[0]).ok_or(err(line, K::BadHex))?;
					let val = hex(args[1]).ok_or(err(line, K::BadHex))?;
					w.write(u8::from(word == "w1"), reg, val);
				}
				"t" => {
					expect(1)?;
					match decimal(args[0]) {
						Some(0) => return Err(err(line, K::ZeroWait)),
						Some(n) => w.wait(n),
						None => return Err(err(line, K::BadNumber)),
					};
				}
				"tier" => {
					expect(1)?;
					let t = decimal(args[0]).and_then(|t| u8::try_from(t).ok());
					w.tier(t.ok_or(err(line, K::BadNumber))?);
				}
				"loop" => {
					expect(0)?;
					w.loop_point();
				}
				"mark" => {
					expect(1)?;
					let id = decimal(args[0]).and_then(|t| u8::try_from(t).ok());
					w.marker(id.ok_or(err(line, K::BadNumber))?);
				}
				"spk" => {
					expect(1)?;
					let n = decimal(args[0]).and_then(|t| u16::try_from(t).ok());
					w.speaker(n.ok_or(err(line, K::BadNumber))?);
				}
				"x" => {
					let Some((opcode, payload)) = args.split_first() else {
						return Err(err(line, K::FieldCount));
					};
					let opcode = hex(opcode).ok_or(err(line, K::BadHex))?;
					if opcode < 0xF0 {
						return Err(err(line, K::ExtensionOpcode));
					}
					if payload.len() > 255 {
						return Err(err(line, K::ExtensionTooLong));
					}
					let mut bytes = Vec::with_capacity(payload.len());
					for field in payload {
						bytes.push(hex(field).ok_or(err(line, K::BadHex))?);
					}
					w.extension(opcode, &bytes);
				}
				"end" => {
					expect(0)?;
					w.end();
					ended = true;
				}
				_ => return Err(err(line, K::UnknownMnemonic)),
			}
		}

		match writer {
			None => Err(err(
				0,
				if version.is_none() || chip.is_none() || grid.is_none() {
					K::MissingHeader
				} else {
					K::MissingStream
				},
			)),
			Some(_) if !ended => Err(err(0, K::MissingEnd)),
			Some(mut w) => w.finish().map_err(|e| err(0, K::Write(e))),
		}
	}
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
	use super::*;
	use crate::examples::{ALL, FIVE, FOUR, ONE, THREE, TWO};
	use crate::fuzz::{Rng, for_each_input};
	use crate::header::Header;
	use crate::writer::{WriteError, Writer};
	use std::string::String;
	use std::vec::Vec;

	fn print(file: &[u8]) -> String {
		let mut out = String::new();
		print_twin(file, &mut out).unwrap();
		out
	}

	const TWIN_ONE: &str = "OPX 0.1\nchip opl2\ngrid 560\nreserved none\nstream\nW0 01 20\nW0 A0 41\nW0 B0 31\nT 16\nW0 B0 11\nT 4\nEND\n";
	const TWIN_TWO: &str = "OPX 0.1\nchip opl2\ngrid 560\nreserved 5\nstream\nW0 01 20\nT 4\nLOOP\nW0 B0 31\nT 16\nW0 B0 11\nT 16\nEND\n";
	const TWIN_THREE: &str = "OPX 0.1\nchip opl3\ngrid 560\nreserved none\nstream\nW0 C0 31\nW0 B0 31\nTIER 1\nW1 C0 31\nW1 B0 31\nTIER 0\nT 16\nMARK 7\nTIER 1\nW1 B0 11\nTIER 0\nW0 B0 11\nEND\n";
	const TWIN_FOUR: &str = "OPX 0.1\nchip spkr\ngrid 140\nreserved none\nstream\nSPK 1193\nT 4\nSPK 0\nT 2\nSPK 607\nT 3\nSPK 0\nEND\n";
	const TWIN_FIVE: &str = "OPX 0.1\nchip opl2\ngrid 560\nreserved none\nstream\nW0 01 20\nX F8 AA BB CC\nX F9\nW0 B0 31\nT 4\nEND\n";

	#[test]
	fn prints_the_specifications_twins() {
		assert_eq!(print(&ONE), TWIN_ONE);
		assert_eq!(print(&TWO), TWIN_TWO);
		assert_eq!(print(&THREE), TWIN_THREE);
		assert_eq!(print(&FOUR), TWIN_FOUR);
		assert_eq!(print(&FIVE), TWIN_FIVE);
	}

	#[test]
	fn assembles_the_specifications_twins_to_its_bytes() {
		assert_eq!(assemble(TWIN_ONE).unwrap(), ONE);
		assert_eq!(assemble(TWIN_TWO).unwrap(), TWO);
		assert_eq!(assemble(TWIN_THREE).unwrap(), THREE);
		assert_eq!(assemble(TWIN_FOUR).unwrap(), FOUR);
		assert_eq!(assemble(TWIN_FIVE).unwrap(), FIVE);
	}

	#[test]
	fn binary_to_twin_to_binary_is_exact_for_canonical_files() {
		for (name, file) in ALL {
			let twin = print(file);
			assert_eq!(assemble(&twin).unwrap(), file, "example {name}");
			assert_eq!(print(&assemble(&twin).unwrap()), twin, "example {name}");
		}
	}

	#[test]
	fn a_non_canonical_file_gets_its_canonical_form() {
		// Waits 2 + 2 become one wait of 4; an LWAIT of 3 becomes a short wait.
		let mut w = Writer::new(Chip::Opl2, 560);
		w.write(0, 1, 2);
		let mut file = w.finish().unwrap();
		file.truncate(HEADER_LEN + 3);
		file.extend_from_slice(&[0x81, 0x81, 0x06, 3, 0, 0x00]);
		file[24] = 9;
		file[16] = 7;
		let twin = print(&file);
		assert!(twin.contains("T 7\n"));
		let canonical = assemble(&twin).unwrap();
		assert_eq!(&canonical[HEADER_LEN + 3..], [0x86, 0x00]);
		assert_eq!(print(&canonical), twin);
	}

	#[test]
	fn long_waits_in_a_twin_are_canonical() {
		let twin = "OPX 0.1\nchip opl2\ngrid 560\nstream\nT 100000\nEND\n";
		let file = assemble(twin).unwrap();
		assert_eq!(
			&file[HEADER_LEN..],
			[0x06, 0xFF, 0xFF, 0x06, 0xA1, 0x86, 0x00]
		);
		assert!(print(&file).contains("T 100000\n"));
		let twin = "OPX 0.1\nchip opl2\ngrid 560\nstream\nT 4294967295\nEND\n";
		let file = assemble(twin).unwrap();
		assert!(print(&file).contains("T 4294967295\n"));
	}

	#[test]
	fn input_is_forgiving_where_the_grammar_says_so() {
		let twin = "; a comment\n\n  opx 0.1 ; version\nCHIP\tOPL3\nGrid  560\nreserved 14 5\nStream\n\
			w0 b0 1 ; short hex\nt 2\nT 2\n\tX f0 41\nend\n";
		let file = assemble(twin).unwrap();
		let canonical = print(&file);
		assert_eq!(
			canonical,
			"OPX 0.1\nchip opl3\ngrid 560\nreserved 5 14\nstream\nW0 B0 01\nT 4\nX F0 41\nEND\n"
		);
		// `reserved` is optional.
		let file = assemble("OPX 0.1\nchip opl2\ngrid 1\nstream\nEND\n").unwrap();
		assert_eq!(Header::parse(&file).unwrap().reserved_channels, 0);
	}

	fn kind(twin: &str) -> (usize, TwinErrorKind) {
		let e = assemble(twin).unwrap_err();
		(e.line, e.kind)
	}

	#[test]
	fn refuses_bad_twins_with_line_numbers() {
		use TwinErrorKind as K;
		let head = "OPX 0.1\nchip opl2\ngrid 560\nstream\n";
		let with = |body: &str| std::format!("{head}{body}");
		assert_eq!(kind(&with("W0 01\nEND\n")), (5, K::FieldCount));
		assert_eq!(kind(&with("W0 01 GG\nEND\n")), (5, K::BadHex));
		assert_eq!(kind(&with("W0 001 00\nEND\n")), (5, K::BadHex));
		assert_eq!(kind(&with("T 0\nEND\n")), (5, K::ZeroWait));
		assert_eq!(kind(&with("T -1\nEND\n")), (5, K::BadNumber));
		assert_eq!(kind(&with("T 4294967296\nEND\n")), (5, K::BadNumber));
		assert_eq!(kind(&with("MARK 256\nEND\n")), (5, K::BadNumber));
		assert_eq!(kind(&with("SPK 65536\nEND\n")), (5, K::BadNumber));
		assert_eq!(kind(&with("TIER 300\nEND\n")), (5, K::BadNumber));
		assert_eq!(kind(&with("X 10\nEND\n")), (5, K::ExtensionOpcode));
		assert_eq!(kind(&with("X\nEND\n")), (5, K::FieldCount));
		assert_eq!(kind(&with("JUMP\nEND\n")), (5, K::UnknownMnemonic));
		assert_eq!(kind(&with("chip opl3\nEND\n")), (5, K::UnknownMnemonic));
		assert_eq!(kind(&with("END\nEND\n")), (6, K::AfterEnd));
		assert_eq!(kind(&with("T 1\n")), (0, K::MissingEnd));
		assert_eq!(
			kind(&with("END ; ok\n; the end\n\nW0 1 1\n")),
			(8, K::AfterEnd)
		);
		let long = (0..256).map(|_| "00").collect::<Vec<_>>().join(" ");
		assert_eq!(
			kind(&with(&std::format!("X F0 {long}\nEND\n"))),
			(5, K::ExtensionTooLong)
		);
		assert_eq!(
			kind("OPX 0.1\nchip opl2\nstream\nEND\n"),
			(3, K::MissingHeader)
		);
		assert_eq!(kind("OPX 0.1\nchip opl2\ngrid 5\n"), (0, K::MissingStream));
		assert_eq!(kind(""), (0, K::MissingHeader));
		assert_eq!(kind("OPX 0.1\nOPX 0.1\n"), (2, K::DuplicateHeader));
		assert_eq!(kind("chip opl2\nchip opl2\n"), (2, K::DuplicateHeader));
		assert_eq!(kind("grid 1\ngrid 1\n"), (2, K::DuplicateHeader));
		assert_eq!(kind("reserved none\nreserved 1\n"), (2, K::DuplicateHeader));
		assert_eq!(kind("OPX 0\n"), (1, K::BadVersion));
		assert_eq!(kind("OPX 0.256\n"), (1, K::BadVersion));
		assert_eq!(kind("OPX 0.1 x\n"), (1, K::FieldCount));
		assert_eq!(kind("chip opl4\n"), (1, K::BadChip));
		assert_eq!(kind("grid x\n"), (1, K::BadNumber));
		assert_eq!(kind("reserved 32\n"), (1, K::ChannelOutOfRange));
		assert_eq!(kind("reserved\n"), (1, K::FieldCount));
		assert_eq!(kind("reserved none 1\n"), (1, K::BadNumber));
	}

	#[test]
	fn refuses_twins_that_do_not_make_a_valid_file() {
		use TwinErrorKind as K;
		let (line, k) = kind("OPX 0.1\nchip opl2\ngrid 560\nstream\nW1 01 01\nEND\n");
		assert_eq!(line, 0);
		assert!(matches!(k, K::Write(WriteError::Invalid(_))));
		let (_, k) = kind("OPX 0.0\nchip opl2\ngrid 560\nstream\nEND\n");
		assert!(matches!(k, K::Write(WriteError::VersionTooLow { .. })));
		let (_, k) = kind("OPX 0.1\nchip opl2\ngrid 0\nstream\nEND\n");
		assert!(matches!(k, K::Write(WriteError::Invalid(_))));
		let (_, k) = kind("OPX 0.1\nchip opl2\ngrid 1\nstream\nT 4294967295\nT 1\nEND\n");
		assert_eq!(k, K::Write(WriteError::WaitOverrun));
	}

	#[test]
	fn the_printer_refuses_invalid_files() {
		let mut out = String::new();
		assert!(matches!(
			print_twin(&ONE[..40], &mut out),
			Err(PrintError::Invalid(_))
		));
		assert!(out.is_empty());
	}

	#[test]
	fn random_valid_streams_round_trip_through_text() {
		let mut rng = Rng::new(0xC0FF_EE11);
		for case in 0..300 {
			let chip = [Chip::Opl2, Chip::Opl3, Chip::Speaker][case % 3];
			let mut w = Writer::new(chip, 1 + rng.below(1000) as u32);
			let mut looped = false;
			for _ in 0..rng.below(40) {
				match (chip, rng.below(8)) {
					(Chip::Speaker, 0..=3) => w.speaker(rng.next() as u16),
					(_, 0..=3) => {
						let bank = u8::from(chip == Chip::Opl3 && rng.below(2) == 0);
						w.write(bank, rng.next() as u8, rng.next() as u8)
					}
					(_, 4) => w.marker(rng.next() as u8),
					(_, 5) => w.wait(rng.below(200_000) as u32),
					(_, 6) => {
						let opcode = 0xF0 + rng.below(16) as u8;
						let len = rng.below(5);
						w.extension(opcode, &rng.bytes(len))
					}
					_ if !looped => {
						looped = true;
						w.loop_point().wait(1)
					}
					_ => w.wait(1),
				};
			}
			let Ok(file) = w.finish() else { continue };
			let twin = print(&file);
			assert_eq!(assemble(&twin).unwrap(), file, "case {case}\n{twin}");
		}
	}

	#[test]
	fn never_panics_on_arbitrary_input() {
		for_each_input(|bytes| {
			let mut out = String::new();
			let _ = print_twin(bytes, &mut out);
			let _ = assemble(&String::from_utf8_lossy(bytes));
		});
		let mut rng = Rng::new(7);
		let words = [
			"OPX 0.1",
			"chip opl3",
			"grid 560",
			"reserved 5 14",
			"stream",
			"W0 B0 31",
			"W1 1 2",
			"T 4",
			"TIER 1",
			"LOOP",
			"MARK 3",
			"SPK 9",
			"X F0 41",
			"END",
			";",
			"T",
			"W0",
			"\t",
			"zz",
		];
		for _ in 0..3000 {
			let mut text = String::new();
			for _ in 0..rng.below(16) {
				text.push_str(words[rng.below(words.len())]);
				text.push('\n');
			}
			let _ = assemble(&text);
		}
	}
}
