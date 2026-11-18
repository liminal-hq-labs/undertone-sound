// The DRO importer: DOSBox Raw OPL versions 0.1 (called v1) and 2.0 become a 1000 Hz stream.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::header::Chip;
use crate::import::{ImportError, ImportReport, Imported, Reader};
use crate::writer::Writer;

/// The grid of a DRO import: the file's delays are in milliseconds.
pub const DRO_GRID_HZ: u32 = 1000;

const SIGNATURE: &[u8; 8] = b"DBRAWOPL";

/// Imports a DRO file, version 1 (0.1) or 2.0, on a 1000 Hz grid with the delays copied exactly.
///
/// An OPL2 file becomes an OPL2 stream and an OPL3 file an OPL3 stream, with the second register
/// bank as `W1`. A dual-OPL2 file has no OPX form and is refused with [`ImportError::DualChip`].
pub fn import_dro(bytes: &[u8]) -> Result<Imported, ImportError> {
	let mut reader = Reader::new(bytes, 0);
	if reader.take(8)? != SIGNATURE {
		return Err(ImportError::BadSignature);
	}
	let major = reader.u16()?;
	let minor = reader.u16()?;
	let mut writer = match (major, minor) {
		(0, 1) => version_one(&mut reader)?,
		(2, 0) => version_two(&mut reader)?,
		_ => return Err(ImportError::UnsupportedVersion),
	};
	Ok(Imported {
		opx: writer.finish()?,
		report: ImportReport::default(),
	})
}

fn chip_of(hardware: u32) -> Result<Chip, ImportError> {
	match hardware {
		0 => Ok(Chip::Opl2),
		1 => Err(ImportError::DualChip),
		2 => Ok(Chip::Opl3),
		_ => Err(ImportError::UnsupportedFormat),
	}
}

/// Version 0.1: after the version, the length in milliseconds, the length in bytes and a
/// four-byte hardware type, then the data.
fn version_one(reader: &mut Reader<'_>) -> Result<Writer, ImportError> {
	let _millis = reader.u32()?;
	let length = usize::try_from(reader.u32()?).map_err(|_| ImportError::Truncated)?;
	let chip = chip_of(reader.u32()?)?;
	let mut data = Reader::new(reader.take(length)?, 0);
	let mut writer = Writer::new(chip, DRO_GRID_HZ);
	let mut bank = 0;
	while !data.is_empty() {
		match data.u8()? {
			0x00 => {
				writer.wait(u32::from(data.u8()?) + 1);
			}
			0x01 => {
				writer.wait(u32::from(data.u16()?) + 1);
			}
			0x02 => bank = 0,
			0x03 => {
				if chip != Chip::Opl3 {
					return Err(ImportError::BadData);
				}
				bank = 1;
			}
			code => {
				// 0x04 escapes a register number that would read as a code.
				let reg = if code == 0x04 { data.u8()? } else { code };
				let val = data.u8()?;
				writer.write(bank, reg, val);
			}
		}
	}
	Ok(writer)
}

/// Version 2.0: pairs, a hardware type, a layout, a compression, the two delay codes and a
/// codemap that turns the pairs' first byte into a register.
fn version_two(reader: &mut Reader<'_>) -> Result<Writer, ImportError> {
	let pairs = usize::try_from(reader.u32()?).map_err(|_| ImportError::Truncated)?;
	let _millis = reader.u32()?;
	let chip = chip_of(u32::from(reader.u8()?))?;
	let layout = reader.u8()?;
	let compression = reader.u8()?;
	if layout != 0 || compression != 0 {
		return Err(ImportError::UnsupportedFormat);
	}
	let short_delay = reader.u8()?;
	let long_delay = reader.u8()?;
	let codemap_len = usize::from(reader.u8()?);
	let codemap = reader.take(codemap_len)?;
	let len = pairs.checked_mul(2).ok_or(ImportError::Truncated)?;
	let mut data = Reader::new(reader.take(len)?, 0);
	let mut writer = Writer::new(chip, DRO_GRID_HZ);
	while !data.is_empty() {
		let index = data.u8()?;
		let val = data.u8()?;
		if index == short_delay {
			writer.wait(u32::from(val) + 1);
		} else if index == long_delay {
			writer.wait((u32::from(val) + 1) << 8);
		} else {
			let reg = *codemap
				.get(usize::from(index & 0x7F))
				.ok_or(ImportError::BadData)?;
			let bank = index >> 7;
			if bank == 1 && chip != Chip::Opl3 {
				return Err(ImportError::BadData);
			}
			writer.write(bank, reg, val);
		}
	}
	Ok(writer)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::fuzz::for_each_mutation;
	use crate::twin::assemble;
	use crate::validate::validate;
	use std::vec::Vec;

	fn v1(hardware: u32, data: &[u8]) -> Vec<u8> {
		let mut f = SIGNATURE.to_vec();
		f.extend_from_slice(&[0, 0, 1, 0]);
		f.extend_from_slice(&0u32.to_le_bytes());
		f.extend_from_slice(&(data.len() as u32).to_le_bytes());
		f.extend_from_slice(&hardware.to_le_bytes());
		f.extend_from_slice(data);
		f
	}

	fn v2(
		hardware: u8,
		layout: u8,
		compression: u8,
		short: u8,
		long: u8,
		codemap: &[u8],
		pairs: &[u8],
	) -> Vec<u8> {
		let mut f = SIGNATURE.to_vec();
		f.extend_from_slice(&[2, 0, 0, 0]);
		f.extend_from_slice(&((pairs.len() / 2) as u32).to_le_bytes());
		f.extend_from_slice(&0u32.to_le_bytes());
		f.extend_from_slice(&[
			hardware,
			layout,
			compression,
			short,
			long,
			codemap.len() as u8,
		]);
		f.extend_from_slice(codemap);
		f.extend_from_slice(pairs);
		f
	}

	#[test]
	fn version_one_writes_and_delays_in_order() {
		// reg 0x20 val 1; delay 16; reg 0xB0 val 0x31; delay 300; escaped reg 1 val 0x7F.
		let data = [
			0x20, 0x01, 0x00, 15, 0xB0, 0x31, 0x01, 0x2B, 0x01, 0x04, 0x01, 0x7F,
		];
		let imported = import_dro(&v1(0, &data)).unwrap();
		let expected = assemble(
			"OPX 0.1\nchip opl2\ngrid 1000\nstream\nW0 20 01\nT 16\nW0 B0 31\nT 300\nW0 01 7F\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
	}

	#[test]
	fn version_one_opl3_banks() {
		let data = [0x03, 0x05, 0x01, 0x02, 0x20, 0x01, 0x03, 0xB0, 0x31];
		let imported = import_dro(&v1(2, &data)).unwrap();
		let expected =
			assemble("OPX 0.1\nchip opl3\ngrid 1000\nstream\nW1 05 01\nW0 20 01\nW1 B0 31\nEND\n")
				.unwrap();
		assert_eq!(imported.opx, expected);
		// A bank switch to the second chip is not valid on an OPL2.
		assert_eq!(import_dro(&v1(0, &[0x03, 1, 2])), Err(ImportError::BadData));
	}

	#[test]
	fn version_two_with_a_codemap() {
		// Codemap: index 0 is register 0x20, 1 is 0xB0, 2 is 0x05. Short delay code 0x7E, long 0x7F.
		let codemap = [0x20, 0xB0, 0x05];
		let pairs = [
			0,
			0x01,
			1,
			0x31,
			0x7E,
			15,
			0x80 | 1,
			0x11,
			0x7F,
			1,
			0x80 | 2,
			0x01,
		];
		let imported = import_dro(&v2(2, 0, 0, 0x7E, 0x7F, &codemap, &pairs)).unwrap();
		let expected = assemble(
			"OPX 0.1\nchip opl3\ngrid 1000\nstream\nW0 20 01\nW0 B0 31\nT 16\nW1 B0 11\nT 512\nW1 05 01\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
	}

	#[test]
	fn version_two_opl2() {
		let imported = import_dro(&v2(0, 0, 0, 0x7E, 0x7F, &[0x01], &[0, 0x20, 0x7E, 3])).unwrap();
		let header = validate(&imported.opx).unwrap();
		assert_eq!((header.chip, header.total_ticks), (Chip::Opl2, 4));
	}

	#[test]
	fn refuses_what_it_cannot_hold() {
		assert_eq!(import_dro(&v1(1, &[])), Err(ImportError::DualChip));
		assert_eq!(
			import_dro(&v2(1, 0, 0, 0x7E, 0x7F, &[], &[])),
			Err(ImportError::DualChip)
		);
		assert_eq!(
			import_dro(&v2(0, 1, 0, 0x7E, 0x7F, &[], &[])),
			Err(ImportError::UnsupportedFormat)
		);
		assert_eq!(
			import_dro(&v2(0, 0, 1, 0x7E, 0x7F, &[], &[])),
			Err(ImportError::UnsupportedFormat)
		);
		assert_eq!(
			import_dro(&v2(7, 0, 0, 0x7E, 0x7F, &[], &[])),
			Err(ImportError::UnsupportedFormat)
		);
		assert_eq!(
			import_dro(&v2(0, 0, 0, 0x7E, 0x7F, &[1], &[5, 5])),
			Err(ImportError::BadData)
		);
		assert_eq!(
			import_dro(&v2(0, 0, 0, 0x7E, 0x7F, &[1], &[0x80, 5])),
			Err(ImportError::BadData)
		);
		assert_eq!(import_dro(b"DBRAWOPX"), Err(ImportError::BadSignature));
		assert_eq!(import_dro(b"DBRAW"), Err(ImportError::Truncated));
		let mut other = v1(0, &[]);
		other[8] = 9;
		assert_eq!(import_dro(&other), Err(ImportError::UnsupportedVersion));
		let mut short = v1(0, &[1, 2]);
		short.pop();
		assert_eq!(import_dro(&short), Err(ImportError::Truncated));
		assert_eq!(import_dro(&v1(0, &[0x20])), Err(ImportError::Truncated));
	}

	#[test]
	fn never_panics_on_arbitrary_bytes() {
		for_each_mutation(
			&v1(2, &[0x03, 0x05, 0x01, 0x04, 0x20, 0x01, 0x00, 5]),
			|b| {
				let _ = import_dro(b);
			},
		);
		for_each_mutation(
			&v2(
				2,
				0,
				0,
				0x7E,
				0x7F,
				&[0x20, 0xB0],
				&[0, 1, 0x81, 2, 0x7E, 3, 0x7F, 1],
			),
			|b| {
				let _ = import_dro(b);
			},
		);
	}
}
