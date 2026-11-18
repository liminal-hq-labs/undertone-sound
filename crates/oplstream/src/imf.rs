// The IMF importer: Type-0 and Type-1 files become a W0 and wait stream at the caller's rate.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::header::Chip;
use crate::import::{ImportError, ImportReport, Imported, Reader};
use crate::writer::Writer;

/// Which IMF layout a file has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImfKind {
	/// Records start at byte 0 and run to the end of the file.
	Type0,
	/// A `u16` payload length comes first; trailing tag bytes after the payload are dropped.
	Type1,
}

/// Imports an IMF file as an OPL2 stream.
///
/// The file does not say how fast it plays, so `grid_hz` is the rate the song was written for
/// (560, 700 or 280). Each record becomes a `W0` and its delay becomes waits, unchanged, so the
/// import is exact. The file's last delay is the wait before `END`.
pub fn import_imf(bytes: &[u8], kind: ImfKind, grid_hz: u32) -> Result<Imported, ImportError> {
	if grid_hz == 0 {
		return Err(ImportError::ZeroGrid);
	}
	let mut reader = Reader::new(bytes, 0);
	let records = match kind {
		ImfKind::Type0 => bytes,
		ImfKind::Type1 => {
			let len = usize::from(reader.u16()?);
			reader.take(len)?
		}
	};
	if records.len() % 4 != 0 {
		return Err(ImportError::BadLength);
	}
	let mut writer = Writer::new(Chip::Opl2, grid_hz);
	for &[reg, val, lo, hi] in records.as_chunks::<4>().0 {
		writer.write(0, reg, val);
		writer.wait(u32::from(u16::from_le_bytes([lo, hi])));
	}
	Ok(Imported {
		opx: writer.finish()?,
		report: ImportReport::default(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::fuzz::for_each_mutation;
	use crate::twin::assemble;
	use crate::validate::validate;
	use std::vec::Vec;

	fn record(reg: u8, val: u8, delay: u16) -> [u8; 4] {
		let [lo, hi] = delay.to_le_bytes();
		[reg, val, lo, hi]
	}

	fn type0() -> Vec<u8> {
		[
			record(0x01, 0x20, 0),
			record(0xB0, 0x31, 700),
			record(0xB0, 0x11, 3),
		]
		.concat()
	}

	#[test]
	fn type_zero_is_exact() {
		let imported = import_imf(&type0(), ImfKind::Type0, 700).unwrap();
		let expected = assemble(
			"OPX 0.1\nchip opl2\ngrid 700\nstream\nW0 01 20\nW0 B0 31\nT 700\nW0 B0 11\nT 3\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
		assert_eq!(imported.report, ImportReport::default());
		let header = validate(&imported.opx).unwrap();
		assert_eq!((header.grid_hz, header.total_ticks), (700, 703));
	}

	#[test]
	fn type_one_drops_the_length_and_trailing_tags() {
		let payload = type0();
		let mut file = (payload.len() as u16).to_le_bytes().to_vec();
		file.extend_from_slice(&payload);
		file.extend_from_slice(b"Title\0Composer\0");
		let imported = import_imf(&file, ImfKind::Type1, 700).unwrap();
		assert_eq!(
			imported.opx,
			import_imf(&payload, ImfKind::Type0, 700).unwrap().opx
		);
	}

	#[test]
	fn zero_delays_share_a_tick_and_long_delays_split() {
		let file = [
			record(1, 1, 0),
			record(2, 2, 0),
			record(3, 3, 0xFFFF),
			record(4, 4, 0xFFFF),
		]
		.concat();
		let opx = import_imf(&file, ImfKind::Type0, 560).unwrap().opx;
		let header = validate(&opx).unwrap();
		assert_eq!(header.total_ticks, 2 * 0xFFFF);
	}

	#[test]
	fn an_empty_file_is_an_empty_stream() {
		let opx = import_imf(&[], ImfKind::Type0, 560).unwrap().opx;
		assert_eq!(validate(&opx).unwrap().total_ticks, 0);
	}

	#[test]
	fn refuses_what_it_cannot_read() {
		assert_eq!(
			import_imf(&type0(), ImfKind::Type0, 0),
			Err(ImportError::ZeroGrid)
		);
		assert_eq!(
			import_imf(&type0()[..11], ImfKind::Type0, 560),
			Err(ImportError::BadLength)
		);
		assert_eq!(
			import_imf(&[], ImfKind::Type1, 560),
			Err(ImportError::Truncated)
		);
		assert_eq!(
			import_imf(&[8, 0, 1, 2, 3, 4], ImfKind::Type1, 560),
			Err(ImportError::Truncated)
		);
		assert_eq!(
			import_imf(&[3, 0, 1, 2, 3], ImfKind::Type1, 560),
			Err(ImportError::BadLength)
		);
		// The writer's check shows through: waits past the clock's end.
		let mut big = Vec::new();
		for _ in 0..65_538 {
			big.extend_from_slice(&record(1, 1, 0xFFFF));
		}
		assert!(matches!(
			import_imf(&big, ImfKind::Type0, 560),
			Err(ImportError::Write(_))
		));
	}

	#[test]
	fn never_panics_on_arbitrary_bytes() {
		for_each_mutation(&type0(), |bytes| {
			let _ = import_imf(bytes, ImfKind::Type0, 560);
			let _ = import_imf(bytes, ImfKind::Type1, 560);
		});
	}
}
