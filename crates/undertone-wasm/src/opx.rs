// OPX import, validation, summary, header table and register-write list for the bindings.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything here takes and returns plain Rust values, so it is tested natively and the bindings
//! only translate.

use core::fmt::Write as _;
use oplstream::{
	Chip, Command, Decoder, HEADER_LEN, Header, ImfKind, import_dro, import_imf, import_vgm,
	print_twin, validate,
};

/// The grid an IMF or DRO file is imported at when the caller does not say: 560 Hz, the rate of
/// Fizz 1's songs.
pub const DEFAULT_IMF_GRID_HZ: u32 = 560;

/// The file formats the importer reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
	/// An OPX file, taken as it is.
	Opx,
	/// An IMF file with no length prefix.
	Imf0,
	/// An IMF file with a `u16` length prefix.
	Imf1,
	/// A DOSBox raw OPL capture.
	Dro,
	/// A VGM log.
	Vgm,
}

/// Guesses a file's format from its bytes: the signatures of OPX, DRO and VGM, and for anything
/// else IMF, whose Type-1 files start with the byte length of their records.
pub fn detect_format(bytes: &[u8]) -> Format {
	if bytes.starts_with(&oplstream::MAGIC) {
		return Format::Opx;
	}
	if bytes.starts_with(b"DBRAWOPL") {
		return Format::Dro;
	}
	if bytes.starts_with(b"Vgm ") || bytes.starts_with(&[0x1f, 0x8b]) {
		return Format::Vgm;
	}
	if let [lo, hi, ..] = bytes {
		let length = usize::from(u16::from_le_bytes([*lo, *hi]));
		if length != 0 && length % 4 == 0 && length + 2 <= bytes.len() {
			return Format::Imf1;
		}
	}
	Format::Imf0
}

/// Imports `bytes` as OPX. An OPX file is returned unchanged. `grid_hz` is the rate an IMF file was
/// written for; DRO and VGM carry their own.
pub fn import(bytes: &[u8], format: Format, grid_hz: u32) -> Result<Vec<u8>, String> {
	let imported = match format {
		Format::Opx => return Ok(bytes.to_vec()),
		Format::Imf0 => import_imf(bytes, ImfKind::Type0, grid_hz),
		Format::Imf1 => import_imf(bytes, ImfKind::Type1, grid_hz),
		Format::Dro => import_dro(bytes),
		Format::Vgm => import_vgm(bytes),
	};
	imported
		.map(|imported| imported.opx)
		.map_err(|error| format!("The file could not be imported: {error}"))
}

/// What a stream is, for a page to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
	/// `OPL2`, `OPL3` or `PC speaker`.
	pub chip: &'static str,
	/// Ticks a second.
	pub grid_hz: u32,
	/// The length in ticks.
	pub total_ticks: u32,
	/// The length in seconds.
	pub seconds: f64,
	/// Register writes (speaker divisors are not counted).
	pub writes: u32,
	/// Whether the file passes every rule of the specification.
	pub valid: bool,
	/// The validator's message when it does not, else empty.
	pub error: String,
	/// Whether the header names a loop point.
	pub has_loop: bool,
	/// The command stream's length in bytes.
	pub stream_len: u32,
	/// The file's length in bytes.
	pub file_len: u32,
}

fn chip_name(chip: Chip) -> &'static str {
	match chip {
		Chip::Opl2 => "OPL2",
		Chip::Opl3 => "OPL3",
		Chip::Speaker => "PC speaker",
	}
}

/// Validates `file` and summarizes it. An invalid file still reports what its header says when the
/// header parses.
pub fn summarize(file: &[u8]) -> Result<Stats, String> {
	let header = Header::parse(file).map_err(|error| format!("Not an OPX file: {error}"))?;
	let (valid, error) = match validate(file) {
		Ok(_) => (true, String::new()),
		Err(error) => (false, error.to_string()),
	};
	Ok(Stats {
		chip: chip_name(header.chip),
		grid_hz: header.grid_hz,
		total_ticks: header.total_ticks,
		seconds: f64::from(header.total_ticks) / f64::from(header.grid_hz),
		writes: u32::try_from(register_writes(file).len() / 3).unwrap_or(u32::MAX),
		valid,
		error,
		has_loop: header.loop_offset != u32::MAX,
		stream_len: header.stream_len,
		file_len: u32::try_from(file.len()).unwrap_or(u32::MAX),
	})
}

/// The stream's register writes as a flat list of triples: the tick, the register (`0x100` added
/// for bank 1) and the value. A stream that stops decoding returns the writes before the problem.
pub fn register_writes(file: &[u8]) -> Vec<u32> {
	let mut out = Vec::new();
	let Ok((header, _)) = Decoder::from_file(file) else {
		return out;
	};
	let stream = &file[HEADER_LEN.min(file.len())..];
	for step in Decoder::new(stream, header.chip) {
		let Ok(step) = step else { break };
		if let Command::Write { bank, reg, val } = step.command {
			out.extend([
				step.tick,
				u32::from(bank) << 8 | u32::from(reg),
				u32::from(val),
			]);
		}
	}
	out
}

/// The header as a table of its fields: offset, size, the bytes in hex, the field name and its
/// value, one line each.
pub fn header_table(file: &[u8]) -> Result<String, String> {
	let header = Header::parse(file).map_err(|error| format!("Not an OPX file: {error}"))?;
	let hex = |range: core::ops::Range<usize>| {
		file[range]
			.iter()
			.map(|byte| format!("{byte:02X}"))
			.collect::<Vec<_>>()
			.join(" ")
	};
	let fields: [(usize, usize, &str, String); 11] = [
		(0, 4, "magic", "OPX, then 1A".to_string()),
		(4, 1, "major", header.major.to_string()),
		(5, 1, "minor", header.minor.to_string()),
		(6, 1, "chip", chip_name(header.chip).to_string()),
		(7, 1, "reserved", "0".to_string()),
		(8, 2, "flags", format!("{:#06b}", header.flags)),
		(10, 2, "reserved", "0".to_string()),
		(12, 4, "grid_hz", header.grid_hz.to_string()),
		(16, 4, "total_ticks", header.total_ticks.to_string()),
		(
			20,
			4,
			"loop_offset",
			if header.loop_offset == u32::MAX {
				"none".to_string()
			} else {
				header.loop_offset.to_string()
			},
		),
		(24, 4, "stream_len", header.stream_len.to_string()),
	];
	let mut out = String::new();
	for (offset, size, name, value) in fields {
		let _ = writeln!(
			out,
			"{offset:>2}  {size}  {:<24}  {name:<12} {value}",
			hex(offset..offset + size)
		);
	}
	let _ = writeln!(
		out,
		"28  4  {:<24}  {:<12} {:#010X}",
		hex(28..32),
		"reserved_channels",
		header.reserved_channels
	);
	Ok(out)
}

/// The text twin of `file`.
pub fn twin(file: &[u8]) -> Result<String, String> {
	let mut text = String::new();
	print_twin(file, &mut text)
		.map_err(|error| format!("The twin could not be printed: {error:?}"))?;
	Ok(text)
}

#[cfg(test)]
mod tests {
	use super::*;

	const TITLE_THEME: &[u8] = include_bytes!("../../../fixtures/opx/title-theme.imf");

	#[test]
	fn title_theme_is_detected_as_a_type_1_imf_and_imports() {
		assert_eq!(detect_format(TITLE_THEME), Format::Imf1);
		let opx = import(TITLE_THEME, Format::Imf1, DEFAULT_IMF_GRID_HZ).unwrap();
		assert_eq!(detect_format(&opx), Format::Opx);
		let stats = summarize(&opx).unwrap();
		assert_eq!(stats.chip, "OPL2");
		assert_eq!(stats.grid_hz, 560);
		assert_eq!(stats.total_ticks, 34_560);
		assert_eq!(stats.writes, 3_312);
		assert!(stats.valid, "{}", stats.error);
		assert!(!stats.has_loop);
		assert!((stats.seconds - 61.714).abs() < 0.001);
	}

	#[test]
	fn opx_passes_through_the_importer_unchanged() {
		let opx = import(TITLE_THEME, Format::Imf1, 560).unwrap();
		assert_eq!(import(&opx, Format::Opx, 0).unwrap(), opx);
	}

	#[test]
	fn the_other_formats_are_recognized_by_their_signatures() {
		assert_eq!(detect_format(b"DBRAWOPL\0\0\0\0"), Format::Dro);
		assert_eq!(detect_format(b"Vgm \0\0\0\0"), Format::Vgm);
		assert_eq!(detect_format(&[0, 0, 0x20, 0x01, 0, 0]), Format::Imf0);
	}

	#[test]
	fn a_file_that_is_not_an_imf_is_refused_with_a_readable_message() {
		let error = import(&[1, 2, 3], Format::Imf0, 560).unwrap_err();
		assert!(
			error.starts_with("The file could not be imported:"),
			"{error}"
		);
	}

	#[test]
	fn the_write_list_has_three_numbers_a_write_in_tick_order() {
		let opx = import(TITLE_THEME, Format::Imf1, 560).unwrap();
		let writes = register_writes(&opx);
		assert_eq!(writes.len(), 3 * 3_312);
		let ticks: Vec<u32> = writes.chunks(3).map(|write| write[0]).collect();
		assert!(ticks.windows(2).all(|pair| pair[0] <= pair[1]));
		assert!(
			writes
				.chunks(3)
				.all(|write| write[1] < 0x100 && write[2] < 0x100)
		);
	}

	#[test]
	fn the_header_table_names_every_field() {
		let opx = import(TITLE_THEME, Format::Imf1, 560).unwrap();
		let table = header_table(&opx).unwrap();
		assert_eq!(table.lines().count(), 12);
		assert!(table.contains("4F 50 58 1A"), "{table}");
		assert!(table.contains("grid_hz"));
		assert!(table.contains("560"));
		assert!(table.contains("34560"));
	}

	#[test]
	fn the_twin_has_the_stream_in_text() {
		let opx = import(TITLE_THEME, Format::Imf1, 560).unwrap();
		let text = twin(&opx).unwrap();
		assert!(text.contains("END"), "{}", &text[..200.min(text.len())]);
		assert_eq!(oplstream::assemble(&text).unwrap(), opx);
	}

	#[test]
	fn a_broken_stream_is_summarized_as_invalid_with_the_reason() {
		let mut opx = import(TITLE_THEME, Format::Imf1, 560).unwrap();
		opx.truncate(opx.len() - 1);
		let stats = summarize(&opx).unwrap();
		assert!(!stats.valid);
		assert!(!stats.error.is_empty());
		assert!(summarize(b"not opx").is_err());
	}
}
