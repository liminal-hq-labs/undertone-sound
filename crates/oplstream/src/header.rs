// The 32-byte OPX header: parse and write.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// The first four bytes of every OPX file: `OPX` then 0x1A, which stops a DOS `type` command.
pub const MAGIC: [u8; 4] = [b'O', b'P', b'X', 0x1A];

/// The header's length in bytes; the command stream follows it.
pub const HEADER_LEN: usize = 32;

/// The sound chip a stream drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
	/// Yamaha YM3812.
	Opl2,
	/// Yamaha YMF262.
	Opl3,
	/// The PC speaker, driven by timer divisors.
	Speaker,
}

/// Why a header was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
	/// Fewer than [`HEADER_LEN`] bytes.
	Truncated,
	/// The magic bytes are wrong.
	BadMagic,
	/// The `chip` byte is not 0, 1 or 2.
	UnknownChip(u8),
	/// A reserved byte or flag bit is not zero.
	ReservedNotZero,
	/// `grid_hz` is zero.
	ZeroGrid,
}

/// A parsed OPX header. Field names follow the specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
	/// Format major version.
	pub major: u8,
	/// Format minor version.
	pub minor: u8,
	/// The chip the stream drives.
	pub chip: Chip,
	/// Bit 0: extras tier present. Bit 1: OPL3 stereo bits authored.
	pub flags: u16,
	/// Ticks per second; never zero.
	pub grid_hz: u32,
	/// The tick count at `END`.
	pub total_ticks: u32,
	/// Byte offset of the `LOOP` command in the stream, or `u32::MAX` for none.
	pub loop_offset: u32,
	/// Length in bytes of the command stream.
	pub stream_len: u32,
	/// Mask of chip channels the stream promises never to write.
	pub reserved_channels: u32,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
	u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
	u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl Header {
	/// Parses the first [`HEADER_LEN`] bytes of `bytes`.
	pub fn parse(bytes: &[u8]) -> Result<Self, HeaderError> {
		if bytes.len() < HEADER_LEN {
			return Err(HeaderError::Truncated);
		}
		if bytes[0..4] != MAGIC {
			return Err(HeaderError::BadMagic);
		}
		let chip = match bytes[6] {
			0 => Chip::Opl2,
			1 => Chip::Opl3,
			2 => Chip::Speaker,
			other => return Err(HeaderError::UnknownChip(other)),
		};
		let flags = u16_at(bytes, 8);
		if bytes[7] != 0 || flags & !0b11 != 0 || u16_at(bytes, 10) != 0 {
			return Err(HeaderError::ReservedNotZero);
		}
		let grid_hz = u32_at(bytes, 12);
		if grid_hz == 0 {
			return Err(HeaderError::ZeroGrid);
		}
		Ok(Self {
			major: bytes[4],
			minor: bytes[5],
			chip,
			flags,
			grid_hz,
			total_ticks: u32_at(bytes, 16),
			loop_offset: u32_at(bytes, 20),
			stream_len: u32_at(bytes, 24),
			reserved_channels: u32_at(bytes, 28),
		})
	}

	/// Writes the header as its 32 bytes.
	pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
		let mut out = [0u8; HEADER_LEN];
		out[0..4].copy_from_slice(&MAGIC);
		out[4] = self.major;
		out[5] = self.minor;
		out[6] = match self.chip {
			Chip::Opl2 => 0,
			Chip::Opl3 => 1,
			Chip::Speaker => 2,
		};
		out[8..10].copy_from_slice(&self.flags.to_le_bytes());
		out[12..16].copy_from_slice(&self.grid_hz.to_le_bytes());
		out[16..20].copy_from_slice(&self.total_ticks.to_le_bytes());
		out[20..24].copy_from_slice(&self.loop_offset.to_le_bytes());
		out[24..28].copy_from_slice(&self.stream_len.to_le_bytes());
		out[28..32].copy_from_slice(&self.reserved_channels.to_le_bytes());
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample() -> Header {
		Header {
			major: 0,
			minor: 1,
			chip: Chip::Opl3,
			flags: 0b10,
			grid_hz: 560,
			total_ticks: 1120,
			loop_offset: u32::MAX,
			stream_len: 7,
			reserved_channels: (1 << 5) | (1 << 14),
		}
	}

	#[test]
	fn round_trips() {
		let header = sample();
		assert_eq!(Header::parse(&header.to_bytes()), Ok(header));
	}

	#[test]
	fn refuses_bad_input() {
		let good = sample().to_bytes();
		assert_eq!(Header::parse(&good[..31]), Err(HeaderError::Truncated));

		let mut bad = good;
		bad[0] = b'X';
		assert_eq!(Header::parse(&bad), Err(HeaderError::BadMagic));

		let mut bad = good;
		bad[6] = 3;
		assert_eq!(Header::parse(&bad), Err(HeaderError::UnknownChip(3)));

		let mut bad = good;
		bad[8] = 0b100;
		assert_eq!(Header::parse(&bad), Err(HeaderError::ReservedNotZero));

		let mut bad = good;
		bad[12..16].fill(0);
		assert_eq!(Header::parse(&bad), Err(HeaderError::ZeroGrid));
	}
}
