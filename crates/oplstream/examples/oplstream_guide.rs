// The snippets of the `oplstream` guide page, compiled and run so they stay true.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Run with `cargo run -p oplstream --example oplstream_guide`. The regions are what `site/guide/oplstream.md`
//! shows.

use oplstream::{
	Chip, Command, Decoder, HEADER_LEN, ImfKind, Writer, assemble, import_imf, print_twin, validate,
};

const TITLE_THEME: &[u8] = include_bytes!("../../../fixtures/opx/title-theme.imf");

fn main() {
	// #region import
	// A Type-1 IMF chunk written for a 560 Hz timer becomes an exact OPX file.
	let imported = import_imf(TITLE_THEME, ImfKind::Type1, 560).expect("an IMF chunk");
	let opx = imported.opx;

	// The validator checks the header against the bytes and every rule in the specification.
	let header = validate(&opx).expect("a valid stream");
	assert_eq!(header.chip, Chip::Opl2);
	assert_eq!(header.total_ticks, 34_560);
	// #endregion import

	// #region decode
	// The decoder walks the command stream with its tick, offset and tier.
	let mut writes = 0;
	for step in Decoder::new(&opx[HEADER_LEN..], Chip::Opl2) {
		let step = step.expect("every command decodes");
		if let Command::Write { .. } = step.command {
			writes += 1;
		}
	}
	assert_eq!(writes, 3_312);
	// #endregion decode

	// #region twin
	// The text twin is the same stream as readable text, and it assembles back to the same bytes.
	let mut text = String::new();
	print_twin(&opx, &mut text).expect("prints");
	assert_eq!(assemble(&text).expect("assembles"), opx);
	// #endregion twin

	// #region write
	// The writer fills in the derived header fields (length, ticks, loop point) when it finishes.
	let mut writer = Writer::new(Chip::Opl2, 560);
	writer
		.write(0, 0xA0, 0x41) // channel 0 frequency, low byte
		.write(0, 0xB0, 0x31) // key on, block 4
		.wait(280) // half a second at 560 Hz
		.write(0, 0xB0, 0x11); // key off
	let file = writer.finish().expect("a valid stream");
	assert_eq!(validate(&file).expect("valid").total_ticks, 280);
	// #endregion write

	println!(
		"{} ticks, {writes} writes, {} bytes of OPX",
		34_560,
		opx.len()
	);
}
