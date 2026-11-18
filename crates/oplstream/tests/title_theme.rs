// Imports a real Type-1 IMF chunk, Fizz 1's Title Theme, and checks it end to end.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The chunk in `fixtures/opx/title-theme.imf` is song 0 of the Fizz 1 DOS game's sound bundle, at
//! 560 Hz. These tests hold the importer to a file nobody wrote by hand: the import must be exact,
//! the result must validate, the text twin must rebuild the same bytes, and re-timing must report
//! what it moved.

use oplstream::{
	Chip, Command, Decoder, HEADER_LEN, Header, ImfKind, Step, assemble, import_imf, print_twin,
	retime, validate,
};

const CHUNK: &[u8] = include_bytes!("../../../fixtures/opx/title-theme.imf");

/// The chunk's records: its 13,248 body bytes at four bytes a record.
const RECORDS: usize = 13_248 / 4;

/// The song's length at 560 Hz, as the studio's comparison with the DOS game found it.
const TICKS: u32 = 34_560;

fn imported() -> Vec<u8> {
	import_imf(CHUNK, ImfKind::Type1, 560)
		.expect("the chunk imports")
		.opx
}

#[test]
fn imports_exactly_and_validates() {
	let opx = imported();
	let header = validate(&opx).expect("the import validates");
	assert_eq!(header.chip, Chip::Opl2);
	assert_eq!(header.grid_hz, 560);
	assert_eq!(header.total_ticks, TICKS);
	assert_eq!(header.loop_offset, u32::MAX);

	let mut writes = 0;
	for step in Decoder::new(&opx[HEADER_LEN..], Chip::Opl2) {
		if let Step {
			command: Command::Write { .. },
			..
		} = step.expect("every command decodes")
		{
			writes += 1;
		}
	}
	assert_eq!(writes, RECORDS, "one write per IMF record");
}

#[test]
fn the_twin_rebuilds_the_same_bytes() {
	let opx = imported();
	let mut text = String::new();
	print_twin(&opx, &mut text).expect("prints");
	assert_eq!(assemble(&text).expect("assembles"), opx);
}

#[test]
fn retiming_to_the_same_grid_changes_nothing() {
	let opx = imported();
	let same = retime(&opx, 560).expect("re-times");
	assert_eq!(same.report.writes_moved, 0);
	assert_eq!(Header::parse(&same.opx).expect("parses").total_ticks, TICKS);
}
