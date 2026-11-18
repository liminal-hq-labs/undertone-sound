// Shared types for the importers: the result, the report, the error and a bounds-checked reader.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::writer::WriteError;
use core::fmt;
use std::vec::Vec;

/// An imported OPX file and what the import left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
	/// The bytes of the OPX file.
	pub opx: Vec<u8>,
	/// Anything the import did not carry over.
	pub report: ImportReport,
}

/// What an import dropped. An import never changes a wait; it only leaves things out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
	/// Commands for chips other than the OPL (VGM only), counted and dropped.
	pub dropped_commands: usize,
	/// The source's loop point was dropped because no tick follows it before the end.
	pub loop_dropped: bool,
}

/// Why a file was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportError {
	/// The caller's grid is zero.
	ZeroGrid,
	/// The file ends before its own data does.
	Truncated,
	/// The signature or magic is wrong.
	BadSignature,
	/// The format version is not one this importer reads.
	UnsupportedVersion,
	/// A DRO file for two OPL2 chips, which OPX cannot hold (it is two chips).
	DualChip,
	/// A DRO file whose layout is not the interleaved, uncompressed one.
	UnsupportedFormat,
	/// An IMF payload whose length is not a multiple of four.
	BadLength,
	/// The data contradicts the header: a bad index, or a second bank on an OPL2.
	BadData,
	/// A VGM file whose header names no YM3812 and no YMF262.
	NoOplChip,
	/// A VGM file that names both a YM3812 and a YMF262.
	BothOplChips,
	/// A VGM command that this importer does not know the length of.
	UnsupportedCommand(u8),
	/// A VGM loop offset that is not at the start of a command.
	BadLoop,
	/// The result is not a valid OPX file (for example the waits overran the clock).
	Write(WriteError),
}

impl fmt::Display for ImportError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::ZeroGrid => f.write_str("the grid rate must not be zero"),
			Self::Truncated => f.write_str("the file ends before its data does"),
			Self::BadSignature => f.write_str("the file's signature is wrong"),
			Self::UnsupportedVersion => f.write_str("this version of the format is not supported"),
			Self::DualChip => f.write_str("two OPL chips cannot be held in one OPX stream"),
			Self::UnsupportedFormat => {
				f.write_str("only the interleaved, uncompressed DRO layout is supported")
			}
			Self::BadLength => {
				f.write_str("the IMF data is not a whole number of four-byte records")
			}
			Self::BadData => f.write_str("the data contradicts the file's header"),
			Self::NoOplChip => f.write_str("the VGM file has no YM3812 or YMF262 to import"),
			Self::BothOplChips => {
				f.write_str("the VGM file has both a YM3812 and a YMF262; OPX holds one")
			}
			Self::UnsupportedCommand(op) => write!(f, "VGM command 0x{op:02X} is not supported"),
			Self::BadLoop => f.write_str("the VGM loop offset is not at the start of a command"),
			Self::Write(error) => write!(f, "the result is not a valid OPX file ({error:?})"),
		}
	}
}

impl core::error::Error for ImportError {}

impl From<WriteError> for ImportError {
	fn from(error: WriteError) -> Self {
		Self::Write(error)
	}
}

/// A cursor over a byte slice whose reads are all checked.
pub(crate) struct Reader<'a> {
	bytes: &'a [u8],
	pos: usize,
}

impl<'a> Reader<'a> {
	pub(crate) fn new(bytes: &'a [u8], pos: usize) -> Self {
		Self { bytes, pos }
	}

	pub(crate) fn pos(&self) -> usize {
		self.pos
	}

	pub(crate) fn is_empty(&self) -> bool {
		self.pos >= self.bytes.len()
	}

	pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
		let end = self.pos.checked_add(n).ok_or(ImportError::Truncated)?;
		let slice = self
			.bytes
			.get(self.pos..end)
			.ok_or(ImportError::Truncated)?;
		self.pos = end;
		Ok(slice)
	}

	pub(crate) fn u8(&mut self) -> Result<u8, ImportError> {
		Ok(self.take(1)?[0])
	}

	pub(crate) fn u16(&mut self) -> Result<u16, ImportError> {
		let b = self.take(2)?;
		Ok(u16::from_le_bytes([b[0], b[1]]))
	}

	pub(crate) fn u32(&mut self) -> Result<u32, ImportError> {
		let b = self.take(4)?;
		Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
	}
}

/// The little-endian `u32` at `at`, if the bytes reach that far.
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
	let b = bytes.get(at..at.checked_add(4)?)?;
	Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
