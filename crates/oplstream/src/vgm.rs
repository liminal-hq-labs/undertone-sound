// The VGM importer: YM3812 and YMF262 writes and sample waits become a 44,100 Hz stream.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::header::Chip;
use crate::import::{ImportError, ImportReport, Imported, Reader, u32_at};
use crate::writer::Writer;
use std::vec::Vec;

/// The grid of a VGM import: the file counts waits in samples at 44,100 Hz.
pub const VGM_GRID_HZ: u32 = 44_100;

/// The first VGM version with the YM3812 and YMF262 clock fields.
const FIRST_OPL_VERSION: u32 = 0x151;

/// The clock's flag for a second chip of the same kind.
const DUAL_CHIP: u32 = 0x4000_0000;

enum Event {
	Write { bank: u8, reg: u8, val: u8 },
	Wait(u32),
	Loop,
}

/// Imports a VGM file that drives one YM3812 (OPL2) or one YMF262 (OPL3), on a 44,100 Hz grid with
/// the waits copied exactly. The loop offset becomes `LOOP`.
///
/// A file whose header names neither chip, both chips, or a second chip is refused. Commands for
/// other chips that sit in an OPL file are dropped and counted in the report; their waits still
/// count. A loop point with no tick after it is dropped and reported, because OPX forbids it.
pub fn import_vgm(bytes: &[u8]) -> Result<Imported, ImportError> {
	if bytes.get(..4) != Some(b"Vgm ") {
		return Err(ImportError::BadSignature);
	}
	let version = u32_at(bytes, 8).ok_or(ImportError::Truncated)?;
	let data_start = match u32_at(bytes, 0x34) {
		Some(rel) if version >= 0x150 && rel != 0 => 0x34usize
			.checked_add(rel as usize)
			.ok_or(ImportError::Truncated)?,
		_ => 0x40,
	};
	// A clock field exists only if the version has it and the header is long enough to hold it.
	let clock = |at: usize| -> u32 {
		if version >= FIRST_OPL_VERSION && data_start >= at + 4 {
			u32_at(bytes, at).unwrap_or(0)
		} else {
			0
		}
	};
	let (ym3812, ymf262) = (clock(0x50), clock(0x5C));
	if (ym3812 | ymf262) & DUAL_CHIP != 0 {
		return Err(ImportError::DualChip);
	}
	let chip = match (ym3812 != 0, ymf262 != 0) {
		(true, false) => Chip::Opl2,
		(false, true) => Chip::Opl3,
		(true, true) => return Err(ImportError::BothOplChips),
		(false, false) => return Err(ImportError::NoOplChip),
	};
	let loop_at = match u32_at(bytes, 0x1C) {
		Some(rel) if rel != 0 => Some(
			0x1Cusize
				.checked_add(rel as usize)
				.ok_or(ImportError::BadLoop)?,
		),
		_ => None,
	};

	let mut events = Vec::new();
	let mut dropped = 0;
	let mut loop_seen = false;
	let mut reader = Reader::new(bytes, data_start);
	loop {
		if loop_at == Some(reader.pos()) {
			events.push(Event::Loop);
			loop_seen = true;
		}
		let op = reader.u8()?;
		match op {
			0x66 => break,
			0x5A if chip == Chip::Opl2 => {
				let (reg, val) = (reader.u8()?, reader.u8()?);
				events.push(Event::Write { bank: 0, reg, val });
			}
			0x5E | 0x5F if chip == Chip::Opl3 => {
				let (reg, val) = (reader.u8()?, reader.u8()?);
				events.push(Event::Write {
					bank: op - 0x5E,
					reg,
					val,
				});
			}
			0x61 => events.push(Event::Wait(u32::from(reader.u16()?))),
			0x62 => events.push(Event::Wait(735)),
			0x63 => events.push(Event::Wait(882)),
			0x70..=0x7F => events.push(Event::Wait(u32::from(op & 0x0F) + 1)),
			// A YM2612 sample write that also waits.
			0x80..=0x8F => {
				events.push(Event::Wait(u32::from(op & 0x0F)));
				dropped += 1;
			}
			0x67 => {
				// A data block: 0x66, a type, a size, then the data.
				let _ = (reader.u8()?, reader.u8()?);
				let size = reader.u32()? & 0x7FFF_FFFF;
				reader.take(size as usize)?;
				dropped += 1;
			}
			other => {
				// Everything else is another chip's command, of a fixed length.
				let operands = match other {
					0x30..=0x3F | 0x4F | 0x50 => 1,
					0x40..=0x4E | 0x51..=0x5F | 0xA0..=0xBF => 2,
					0xC0..=0xDF => 3,
					0x90 | 0x91 | 0x95 | 0xE0..=0xFF => 4,
					0x92 => 5,
					0x93 => 10,
					0x94 => 1,
					0x68 => 11,
					_ => return Err(ImportError::UnsupportedCommand(other)),
				};
				reader.take(operands)?;
				dropped += 1;
			}
		}
	}
	if loop_at.is_some() && !loop_seen {
		return Err(ImportError::BadLoop);
	}

	// OPX forbids a loop with no tick after it.
	let mut loop_dropped = false;
	if let Some(at) = events.iter().position(|e| matches!(e, Event::Loop)) {
		let after: u64 = events[at..]
			.iter()
			.map(|e| {
				if let Event::Wait(n) = e {
					u64::from(*n)
				} else {
					0
				}
			})
			.sum();
		if after == 0 {
			events.remove(at);
			loop_dropped = true;
		}
	}

	let mut writer = Writer::new(chip, VGM_GRID_HZ);
	for event in events {
		match event {
			Event::Write { bank, reg, val } => writer.write(bank, reg, val),
			Event::Wait(n) => writer.wait(n),
			Event::Loop => writer.loop_point(),
		};
	}
	Ok(Imported {
		opx: writer.finish()?,
		report: ImportReport {
			dropped_commands: dropped,
			loop_dropped,
		},
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::fuzz::for_each_mutation;
	use crate::twin::assemble;
	use crate::validate::validate;

	/// A 0x100-byte version 1.71 header with the given OPL clocks, followed by `data`.
	fn vgm(ym3812: u32, ymf262: u32, loop_at: Option<usize>, data: &[u8]) -> Vec<u8> {
		let mut f = std::vec![0u8; 0x100];
		f[..4].copy_from_slice(b"Vgm ");
		f[8..12].copy_from_slice(&0x171u32.to_le_bytes());
		f[0x34..0x38].copy_from_slice(&0xCCu32.to_le_bytes());
		f[0x50..0x54].copy_from_slice(&ym3812.to_le_bytes());
		f[0x5C..0x60].copy_from_slice(&ymf262.to_le_bytes());
		if let Some(at) = loop_at {
			f[0x1C..0x20].copy_from_slice(&((0x100 + at - 0x1C) as u32).to_le_bytes());
		}
		f.extend_from_slice(data);
		let eof = (f.len() - 4) as u32;
		f[4..8].copy_from_slice(&eof.to_le_bytes());
		f
	}

	#[test]
	fn opl2_writes_and_sample_waits_are_exact() {
		let data = [
			0x5A, 0x20, 0x01, 0x5A, 0xB0, 0x31, 0x62, 0x63, 0x61, 0x34, 0x12, 0x73, 0x5A, 0xB0,
			0x11, 0x66,
		];
		let imported = import_vgm(&vgm(3_579_545, 0, None, &data)).unwrap();
		// The waits are 735 + 882 + 0x1234 (4660) + 4 = 6281 samples, copied as they are.
		let expected = assemble(
			"OPX 0.1\nchip opl2\ngrid 44100\nstream\nW0 20 01\nW0 B0 31\nT 6281\nW0 B0 11\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
		let header = validate(&imported.opx).unwrap();
		assert_eq!(
			(header.chip, header.grid_hz, header.total_ticks),
			(Chip::Opl2, 44_100, 6281)
		);
		assert_eq!(imported.report, ImportReport::default());
	}

	#[test]
	fn opl3_ports_become_banks() {
		let data = [
			0x5E, 0x20, 0x01, 0x5F, 0x05, 0x01, 0x70, 0x5F, 0xB0, 0x31, 0x66,
		];
		let imported = import_vgm(&vgm(0, 14_318_180, None, &data)).unwrap();
		let expected = assemble(
			"OPX 0.1\nchip opl3\ngrid 44100\nstream\nW0 20 01\nW1 05 01\nT 1\nW1 B0 31\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
	}

	#[test]
	fn the_loop_offset_becomes_loop() {
		let data = [
			0x5A, 0x20, 0x01, 0x61, 0x10, 0x00, 0x5A, 0xB0, 0x31, 0x61, 0x20, 0x00, 0x66,
		];
		// The loop starts at the second write: offset 6 in the data.
		let imported = import_vgm(&vgm(3_579_545, 0, Some(6), &data)).unwrap();
		let expected = assemble(
			"OPX 0.1\nchip opl2\ngrid 44100\nstream\nW0 20 01\nT 16\nLOOP\nW0 B0 31\nT 32\nEND\n",
		)
		.unwrap();
		assert_eq!(imported.opx, expected);
		assert_eq!(validate(&imported.opx).unwrap().loop_offset, 4);
	}

	#[test]
	fn a_loop_with_no_tick_after_it_is_dropped_and_reported() {
		let data = [0x61, 0x10, 0x00, 0x5A, 0x01, 0x02, 0x66];
		let imported = import_vgm(&vgm(3_579_545, 0, Some(3), &data)).unwrap();
		assert!(imported.report.loop_dropped);
		assert_eq!(validate(&imported.opx).unwrap().loop_offset, u32::MAX);
		// A loop at the end command itself is the same.
		let imported = import_vgm(&vgm(3_579_545, 0, Some(6), &data)).unwrap();
		assert!(imported.report.loop_dropped);
		// A loop after the end command is not at a command.
		assert_eq!(
			import_vgm(&vgm(3_579_545, 0, Some(7), &data)),
			Err(ImportError::BadLoop)
		);
	}

	#[test]
	fn a_loop_inside_a_command_is_refused() {
		let data = [0x5A, 0x01, 0x02, 0x66];
		assert_eq!(
			import_vgm(&vgm(3_579_545, 0, Some(1), &data)),
			Err(ImportError::BadLoop)
		);
		assert_eq!(
			import_vgm(&vgm(3_579_545, 0, Some(50), &data)),
			Err(ImportError::BadLoop)
		);
	}

	#[test]
	fn other_chips_are_dropped_counted_and_still_wait() {
		let data = [
			0x50, 0x9F, // SN76489
			0x52, 0x28, 0xF0, // YM2612
			0x5E, 0x01, 0x02, // YMF262 in an OPL2 file
			0x81, // YM2612 sample and wait 1
			0x67, 0x66, 0x00, 0x02, 0x00, 0x00, 0x00, 0xAA, 0xBB, // data block
			0xB5, 0x01, 0x02, // an A0 to BF command
			0x5A, 0x20, 0x01, 0x66,
		];
		let imported = import_vgm(&vgm(3_579_545, 0, None, &data)).unwrap();
		assert_eq!(imported.report.dropped_commands, 6);
		let header = validate(&imported.opx).unwrap();
		assert_eq!(header.total_ticks, 1);
		let expected =
			assemble("OPX 0.1\nchip opl2\ngrid 44100\nstream\nT 1\nW0 20 01\nEND\n").unwrap();
		assert_eq!(imported.opx, expected);
	}

	#[test]
	fn refuses_files_it_cannot_import_exactly() {
		let end = [0x66];
		assert_eq!(
			import_vgm(&vgm(0, 0, None, &end)),
			Err(ImportError::NoOplChip)
		);
		assert_eq!(
			import_vgm(&vgm(1, 1, None, &end)),
			Err(ImportError::BothOplChips)
		);
		assert_eq!(
			import_vgm(&vgm(1 | DUAL_CHIP, 0, None, &end)),
			Err(ImportError::DualChip)
		);
		assert_eq!(
			import_vgm(&vgm(0, 1 | DUAL_CHIP, None, &end)),
			Err(ImportError::DualChip)
		);
		assert_eq!(
			import_vgm(&vgm(1, 0, None, &[0x04, 0x66])),
			Err(ImportError::UnsupportedCommand(4))
		);
		assert_eq!(
			import_vgm(&vgm(1, 0, None, &[0x64, 0, 0, 0, 0x66])),
			Err(ImportError::UnsupportedCommand(0x64))
		);
		assert_eq!(
			import_vgm(&vgm(1, 0, None, &[0x5A, 1])),
			Err(ImportError::Truncated)
		);
		assert_eq!(
			import_vgm(&vgm(1, 0, None, &[])),
			Err(ImportError::Truncated)
		);
		assert_eq!(import_vgm(b"Vgx "), Err(ImportError::BadSignature));
		assert_eq!(import_vgm(b"Vgm "), Err(ImportError::Truncated));
		// A version before the OPL clock fields.
		let mut old = vgm(1, 0, None, &end);
		old[8..12].copy_from_slice(&0x150u32.to_le_bytes());
		assert_eq!(import_vgm(&old), Err(ImportError::NoOplChip));
	}

	#[test]
	fn an_old_header_starts_data_at_0x40() {
		let mut f = std::vec![0u8; 0x40];
		f[..4].copy_from_slice(b"Vgm ");
		f[8..12].copy_from_slice(&0x110u32.to_le_bytes());
		f.push(0x66);
		assert_eq!(import_vgm(&f), Err(ImportError::NoOplChip));
	}

	#[test]
	fn never_panics_on_arbitrary_bytes() {
		let data = [
			0x5A, 0x20, 0x01, 0x61, 0x10, 0x00, 0x52, 0x28, 0xF0, 0x67, 0x66, 0, 1, 0, 0, 0, 7,
			0x70, 0x66,
		];
		for_each_mutation(&vgm(3_579_545, 0, Some(3), &data), |b| {
			let _ = import_vgm(b);
		});
		for_each_mutation(&vgm(0, 3_579_545, None, &data), |b| {
			let _ = import_vgm(b);
		});
	}
}
