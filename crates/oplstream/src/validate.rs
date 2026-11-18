// The validator: every rule in the "Validity" section of the specification.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::command::{Command, DecodeError, Decoder};
use crate::header::{HEADER_LEN, Header, HeaderError, VERSION_MAJOR, VERSION_MINOR};

/// Why a file was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationError {
	/// The header is not readable: bad magic, unknown chip, a reserved field set or a zero grid.
	Header(HeaderError),
	/// The major version is not 0.
	UnknownMajor(u8),
	/// The minor version is higher than this crate knows, and the major version is 0.
	UnknownMinor(u8),
	/// The file is shorter than 32 plus `stream_len`.
	Truncated,
	/// The file is longer than 32 plus `stream_len`.
	TrailingBytes,
	/// A command could not be decoded, or the stream has no `END` or a wait overran the clock.
	Decode(DecodeError),
	/// There are bytes in the stream after the first `END`.
	BytesAfterEnd {
		/// Offset of the first byte after `END`.
		offset: usize,
	},
	/// The sum of the waits differs from `total_ticks`.
	TotalTicksMismatch {
		/// The header's `total_ticks`.
		header: u32,
		/// The clock at `END`.
		actual: u32,
	},
	/// A second `LOOP` command.
	MoreThanOneLoop {
		/// Offset of the second `LOOP`.
		offset: usize,
	},
	/// `loop_offset` is not the offset of the `LOOP` command (or not `0xFFFFFFFF` without one).
	LoopOffsetMismatch {
		/// The header's `loop_offset`.
		header: u32,
		/// The offset of the `LOOP` command, if the stream has one.
		actual: Option<usize>,
	},
	/// A `LOOP` while the tier is 1.
	LoopInExtras {
		/// Offset of the `LOOP`.
		offset: usize,
	},
	/// No tick passes between `LOOP` and `END`.
	NoTickAfterLoop,
	/// Flag bit 0 differs from whether the stream contains a `TIER 1`.
	ExtrasFlagMismatch,
	/// A core write to bank 1 in a stream whose flag bit 0 is set.
	CoreWriteToBankOne {
		/// Offset of the write.
		offset: usize,
	},
	/// Flag bit 1 differs from whether some write to 0xC0 to 0xC8 sets bit 4 or bit 5.
	StereoFlagMismatch,
	/// A write to a channel that `reserved_channels` reserves.
	ReservedChannelWritten {
		/// Offset of the write.
		offset: usize,
		/// The channel number, `bank * 9 + c`.
		channel: u8,
	},
	/// An `F1` segment extension that is not the first command, or whose payload is not 3 bytes.
	BadSegment {
		/// Offset of the extension.
		offset: usize,
	},
}

/// The chip channel (0 to 17) that a register of `bank` belongs to, or `None` for a global
/// register or one that belongs to no channel.
pub fn channel_of(bank: u8, reg: u8) -> Option<u8> {
	let c = match reg {
		0xA0..=0xA8 | 0xB0..=0xB8 | 0xC0..=0xC8 => reg & 0x0F,
		0x20..=0x35 | 0x40..=0x55 | 0x60..=0x75 | 0x80..=0x95 | 0xE0..=0xF5 => {
			let o = reg & 0x1F;
			if o % 8 >= 6 {
				return None;
			}
			(o / 8) * 3 + (o % 8) % 3
		}
		_ => return None,
	};
	Some(bank * 9 + c)
}

/// Checks `file` against every rule in the specification and returns its header.
///
/// It never panics, whatever the bytes.
pub fn validate(file: &[u8]) -> Result<Header, ValidationError> {
	check(file, true)
}

/// As [`validate`], but accepts any version; the writer uses it for a version the caller chose.
pub(crate) fn check(file: &[u8], check_version: bool) -> Result<Header, ValidationError> {
	use ValidationError as E;
	let header = Header::parse(file).map_err(E::Header)?;
	if check_version {
		if header.major != VERSION_MAJOR {
			return Err(E::UnknownMajor(header.major));
		}
		if header.minor > VERSION_MINOR {
			return Err(E::UnknownMinor(header.minor));
		}
	}
	let stream_len = u64::from(header.stream_len);
	let available = (file.len() - HEADER_LEN) as u64;
	if available < stream_len {
		return Err(E::Truncated);
	}
	if available > stream_len {
		return Err(E::TrailingBytes);
	}
	let stream = &file[HEADER_LEN..];

	let extras_flag = header.flags & 1 != 0;
	let stereo_flag = header.flags & 2 != 0;
	let mut seen_extras = false;
	let mut seen_stereo = false;
	let mut loop_at = None;
	let mut loop_tick = 0;
	let mut end = None;
	let mut decoder = Decoder::new(stream, header.chip);
	for step in &mut decoder {
		let step = step.map_err(E::Decode)?;
		let offset = step.offset;
		match step.command {
			Command::End => end = Some((offset, step.tick)),
			Command::Tier(1) => {
				if !extras_flag {
					return Err(E::ExtrasFlagMismatch);
				}
				seen_extras = true;
			}
			Command::Loop => {
				if loop_at.is_some() {
					return Err(E::MoreThanOneLoop { offset });
				}
				if step.tier == 1 {
					return Err(E::LoopInExtras { offset });
				}
				loop_at = Some(offset);
				loop_tick = step.tick;
			}
			Command::Write { bank, reg, val } => {
				if extras_flag && step.tier == 0 && bank == 1 {
					return Err(E::CoreWriteToBankOne { offset });
				}
				if matches!(reg, 0xC0..=0xC8) && val & 0x30 != 0 {
					seen_stereo = true;
				}
				if let Some(channel) = channel_of(bank, reg)
					&& header.reserved_channels & (1 << channel) != 0
				{
					return Err(E::ReservedChannelWritten { offset, channel });
				}
			}
			Command::Extension {
				opcode: 0xF1,
				payload,
			} if offset != 0 || payload.len() != 3 => {
				return Err(E::BadSegment { offset });
			}
			_ => {}
		}
	}
	// The decoder returns an error for a missing `END`, so `end` is set here.
	let Some((end_offset, end_tick)) = end else {
		return Err(E::Decode(DecodeError {
			offset: stream.len(),
			kind: crate::command::DecodeErrorKind::MissingEnd,
		}));
	};
	if end_offset + 1 != stream.len() {
		return Err(E::BytesAfterEnd {
			offset: end_offset + 1,
		});
	}
	if end_tick != header.total_ticks {
		return Err(E::TotalTicksMismatch {
			header: header.total_ticks,
			actual: end_tick,
		});
	}
	let loop_matches = match loop_at {
		Some(offset) => u32::try_from(offset).is_ok_and(|o| o == header.loop_offset),
		None => header.loop_offset == u32::MAX,
	};
	if !loop_matches {
		return Err(E::LoopOffsetMismatch {
			header: header.loop_offset,
			actual: loop_at,
		});
	}
	if loop_at.is_some() && end_tick <= loop_tick {
		return Err(E::NoTickAfterLoop);
	}
	if extras_flag != seen_extras {
		return Err(E::ExtrasFlagMismatch);
	}
	if stereo_flag != seen_stereo {
		return Err(E::StereoFlagMismatch);
	}
	Ok(header)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::command::DecodeErrorKind;
	use crate::examples::{FIVE, FOUR, ONE, THREE, TWO};
	use crate::header::Chip;
	use std::vec::Vec;

	/// Builds a file around `stream` with the given header fields, filling `stream_len`.
	fn file(
		chip: Chip,
		flags: u16,
		total: u32,
		loop_offset: u32,
		reserved: u32,
		stream: &[u8],
	) -> Vec<u8> {
		let header = Header {
			major: 0,
			minor: 1,
			chip,
			flags,
			grid_hz: 560,
			total_ticks: total,
			loop_offset,
			stream_len: stream.len() as u32,
			reserved_channels: reserved,
		};
		let mut out = header.to_bytes().to_vec();
		out.extend_from_slice(stream);
		out
	}

	fn opl2(total: u32, stream: &[u8]) -> Vec<u8> {
		file(Chip::Opl2, 0, total, u32::MAX, 0, stream)
	}

	#[test]
	fn the_examples_are_valid() {
		for (name, bytes) in crate::examples::ALL {
			assert!(validate(bytes).is_ok(), "example {name}");
		}
	}

	// The nine files that the specification says a reader must refuse.

	#[test]
	fn refuses_1_bad_magic() {
		let mut f = ONE;
		f[0] = 0x51;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Header(HeaderError::BadMagic))
		);
	}

	#[test]
	fn refuses_2_unknown_major() {
		let mut f = ONE;
		f[4] = 1;
		assert_eq!(validate(&f), Err(ValidationError::UnknownMajor(1)));
	}

	#[test]
	fn refuses_3_truncated() {
		assert_eq!(validate(&ONE[..44]), Err(ValidationError::Truncated));
	}

	#[test]
	fn refuses_4_missing_end() {
		let mut f = ONE[..46].to_vec();
		f[24] = 0x0E;
		let err = validate(&f).unwrap_err();
		assert!(matches!(
			err,
			ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::MissingEnd,
				..
			})
		));
	}

	#[test]
	fn refuses_5_wait_sum_differs_from_total() {
		let mut f = ONE;
		f[32 + 9] = 0x91;
		assert_eq!(
			validate(&f),
			Err(ValidationError::TotalTicksMismatch {
				header: 20,
				actual: 22
			})
		);
	}

	#[test]
	fn refuses_6_reserved_opcode() {
		let mut f = ONE;
		f[32 + 9] = 0x07;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				offset: 9,
				kind: DecodeErrorKind::UnknownOpcode(7)
			}))
		);
	}

	#[test]
	fn refuses_7_bank_one_on_opl2() {
		let mut f = ONE;
		f[32 + 3] = 0x02;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				offset: 3,
				kind: DecodeErrorKind::BankOneOnOpl2
			}))
		);
	}

	#[test]
	fn refuses_8_loop_offset_not_at_loop() {
		let mut f = TWO;
		f[20] = 5;
		assert_eq!(
			validate(&f),
			Err(ValidationError::LoopOffsetMismatch {
				header: 5,
				actual: Some(4)
			})
		);
	}

	#[test]
	fn refuses_9_tier_without_flag() {
		let mut f = THREE;
		f[8] = 0x02;
		assert_eq!(validate(&f), Err(ValidationError::ExtrasFlagMismatch));
	}

	// The other rules in the Validity section.

	#[test]
	fn refuses_header_faults() {
		let mut f = ONE;
		f[6] = 9;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Header(HeaderError::UnknownChip(9)))
		);
		let mut f = ONE;
		f[7] = 1;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Header(HeaderError::ReservedNotZero))
		);
		let mut f = ONE;
		f[9] = 0x80;
		assert_eq!(
			validate(&f),
			Err(ValidationError::Header(HeaderError::ReservedNotZero))
		);
		let mut f = ONE;
		f[12..16].fill(0);
		assert_eq!(
			validate(&f),
			Err(ValidationError::Header(HeaderError::ZeroGrid))
		);
		let mut f = ONE;
		f[5] = 2;
		assert_eq!(validate(&f), Err(ValidationError::UnknownMinor(2)));
		assert_eq!(
			validate(&ONE[..10]),
			Err(ValidationError::Header(HeaderError::Truncated))
		);
	}

	#[test]
	fn refuses_trailing_bytes() {
		let mut f = ONE.to_vec();
		f.push(0);
		assert_eq!(validate(&f), Err(ValidationError::TrailingBytes));
	}

	#[test]
	fn refuses_bytes_after_end() {
		let f = opl2(0, &[0x00, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::BytesAfterEnd { offset: 1 })
		);
	}

	#[test]
	fn refuses_a_stream_with_no_end() {
		let f = opl2(0, &[]);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::MissingEnd,
				..
			}))
		));
	}

	#[test]
	fn refuses_chip_mismatches() {
		let f = file(Chip::Opl3, 0, 0, u32::MAX, 0, &[0x0A, 1, 0, 0x00]);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::SpeakerCommandOnOpl,
				..
			}))
		));
		let f = file(Chip::Speaker, 0, 0, u32::MAX, 0, &[0x01, 1, 0, 0x00]);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::OplCommandOnSpeaker,
				..
			}))
		));
		let f = file(Chip::Speaker, 0, 0, u32::MAX, 0, &[0x05, 0, 0x00]);
		assert!(validate(&f).is_err());
	}

	#[test]
	fn refuses_a_zero_long_wait_and_a_bad_tier() {
		let f = opl2(0, &[0x06, 0, 0, 0x00]);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::ZeroLongWait,
				..
			}))
		));
		let f = opl2(0, &[0x05, 2, 0x00]);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::BadTier(2),
				..
			}))
		));
	}

	#[test]
	fn refuses_a_wait_overrun() {
		let mut stream = Vec::new();
		for _ in 0..65_538 {
			stream.extend_from_slice(&[0x06, 0xFF, 0xFF]);
		}
		stream.push(0);
		let f = opl2(0, &stream);
		assert!(matches!(
			validate(&f),
			Err(ValidationError::Decode(DecodeError {
				kind: DecodeErrorKind::WaitOverrun,
				..
			}))
		));
		// Exactly 4,294,967,295 ticks is not an overrun.
		let mut stream = Vec::new();
		for _ in 0..65_537 {
			stream.extend_from_slice(&[0x06, 0xFF, 0xFF]);
		}
		stream.push(0);
		assert!(validate(&opl2(u32::MAX, &stream)).is_ok());
	}

	#[test]
	fn refuses_loop_faults() {
		let f = file(Chip::Opl2, 0, 2, 0, 0, &[0x08, 0x08, 0x81, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::MoreThanOneLoop { offset: 1 })
		);

		let f = file(
			Chip::Opl2,
			1,
			1,
			2,
			0,
			&[0x05, 0x01, 0x08, 0x05, 0x00, 0x80, 0x00],
		);
		assert_eq!(
			validate(&f),
			Err(ValidationError::LoopInExtras { offset: 2 })
		);

		let f = file(Chip::Opl2, 0, 1, 0, 0, &[0x08, 0x80, 0x00]);
		assert!(validate(&f).is_ok());
		let f = file(Chip::Opl2, 0, 1, u32::MAX, 0, &[0x08, 0x80, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::LoopOffsetMismatch {
				header: u32::MAX,
				actual: Some(0)
			})
		);
		let f = file(Chip::Opl2, 0, 1, 0, 0, &[0x80, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::LoopOffsetMismatch {
				header: 0,
				actual: None
			})
		);
		let f = file(Chip::Opl2, 0, 1, 1, 0, &[0x80, 0x08, 0x00]);
		assert_eq!(validate(&f), Err(ValidationError::NoTickAfterLoop));
	}

	#[test]
	fn refuses_flag_faults() {
		// Flag bit 0 set with no TIER 1.
		let f = file(Chip::Opl3, 1, 0, u32::MAX, 0, &[0x00]);
		assert_eq!(validate(&f), Err(ValidationError::ExtrasFlagMismatch));
		// A core write to bank 1 when the flag is set.
		let f = file(Chip::Opl3, 1, 0, u32::MAX, 0, &[0x02, 1, 1, 0x05, 1, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::CoreWriteToBankOne { offset: 0 })
		);
		// Bank 1 core writes are fine without extras.
		let f = file(Chip::Opl3, 0, 0, u32::MAX, 0, &[0x02, 1, 1, 0x00]);
		assert!(validate(&f).is_ok());
		// Stereo bits authored but flag clear, and the reverse.
		let f = file(Chip::Opl3, 0, 0, u32::MAX, 0, &[0x01, 0xC0, 0x10, 0x00]);
		assert_eq!(validate(&f), Err(ValidationError::StereoFlagMismatch));
		let f = file(Chip::Opl3, 2, 0, u32::MAX, 0, &[0x01, 0xC0, 0x01, 0x00]);
		assert_eq!(validate(&f), Err(ValidationError::StereoFlagMismatch));
		let f = file(Chip::Opl3, 2, 0, u32::MAX, 0, &[0x02, 0xC8, 0x20, 0x00]);
		assert!(validate(&f).is_ok());
		// A bit-4 or bit-5 write to a register outside 0xC0 to 0xC8 is not stereo.
		let f = file(Chip::Opl3, 0, 0, u32::MAX, 0, &[0x01, 0xC9, 0x30, 0x00]);
		assert!(validate(&f).is_ok());
	}

	#[test]
	fn refuses_writes_to_reserved_channels() {
		// Example 2 reserves channel 5; 0xB5 is its frequency high byte.
		let mut f = TWO;
		f[32 + 6] = 0xB5;
		assert_eq!(
			validate(&f),
			Err(ValidationError::ReservedChannelWritten {
				offset: 5,
				channel: 5
			})
		);
		// Bank 1 channel 5 is channel 14.
		let f = file(Chip::Opl3, 0, 0, u32::MAX, 1 << 14, &[0x02, 0xB5, 0, 0x00]);
		assert_eq!(
			validate(&f),
			Err(ValidationError::ReservedChannelWritten {
				offset: 0,
				channel: 14
			})
		);
		// Global registers belong to no channel.
		let f = file(
			Chip::Opl3,
			0,
			0,
			u32::MAX,
			u32::MAX,
			&[0x02, 0x05, 1, 0x01, 0xBD, 0, 0x00],
		);
		assert!(validate(&f).is_ok());
	}

	#[test]
	fn checks_the_segment_extension() {
		let ok = opl2(0, &[0xF1, 3, 0, 2, 0xFF, 0x00]);
		assert!(validate(&ok).is_ok());
		let late = opl2(0, &[0x01, 1, 1, 0xF1, 3, 0, 2, 0xFF, 0x00]);
		assert_eq!(
			validate(&late),
			Err(ValidationError::BadSegment { offset: 3 })
		);
		let short = opl2(0, &[0xF1, 2, 0, 2, 0x00]);
		assert_eq!(
			validate(&short),
			Err(ValidationError::BadSegment { offset: 0 })
		);
	}

	#[test]
	fn maps_registers_to_channels() {
		assert_eq!(channel_of(0, 0xA0), Some(0));
		assert_eq!(channel_of(0, 0xB8), Some(8));
		assert_eq!(channel_of(1, 0xC4), Some(13));
		assert_eq!(channel_of(0, 0xA9), None);
		assert_eq!(channel_of(0, 0xBD), None);
		assert_eq!(channel_of(0, 0x20), Some(0));
		assert_eq!(channel_of(0, 0x23), Some(0));
		assert_eq!(channel_of(0, 0x21), Some(1));
		assert_eq!(channel_of(0, 0x25), Some(2));
		assert_eq!(channel_of(0, 0x26), None);
		assert_eq!(channel_of(0, 0x27), None);
		assert_eq!(channel_of(0, 0x28), Some(3));
		assert_eq!(channel_of(0, 0x35), Some(8));
		assert_eq!(channel_of(0, 0x36), None);
		assert_eq!(channel_of(0, 0xF5), Some(8));
		assert_eq!(channel_of(1, 0xF5), Some(17));
		assert_eq!(channel_of(0, 0x01), None);
		assert_eq!(channel_of(1, 0x04), None);
		assert_eq!(channel_of(0, 0x55), Some(8));
		assert_eq!(channel_of(0, 0x56), None);
		assert_eq!(channel_of(0, 0xFF), None);
	}

	#[test]
	fn examples_four_and_five_are_valid_for_their_chips() {
		assert_eq!(validate(&FOUR).unwrap().chip, Chip::Speaker);
		assert_eq!(validate(&FIVE).unwrap().total_ticks, 4);
	}

	#[test]
	fn never_panics_on_arbitrary_bytes() {
		crate::fuzz::for_each_input(|bytes| {
			let _ = validate(bytes);
		});
	}
}
