// Exports the Title Theme as OPX and compares it with the game's own chunk.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Title Theme song is compiled, sequenced for the OPL2 and the OPL3, and written as OPX on a
//! 560 Hz grid. The streams must validate, must equal the golden files in `fixtures/opx/` byte for
//! byte, and the OPL2 stream must sound what the game's own chunk
//! (`fixtures/opx/title-theme.imf`, imported with `import_imf`) sounds: the same length, the same
//! drum hits, and the same notes by start, end, pitch and patch, with channels ignored. Four
//! carrier levels differ and are asserted as such. The OPL3 stream is compared with the OPL2's.
//!
//! `cargo run -p undertone-opl --example title_theme_opx` rewrites the golden files.

mod support;

use std::collections::BTreeMap;

use oplstream::{Chip, Command, Decoder, HEADER_LEN, ImfKind, assemble, import_imf, validate};
use support::{fixtures, game_profile, golden_files, title_theme};
use undertone_opl::analyse::{Decoded, decode_opx};
use undertone_opl::sequence::OplChip;

/// A note as the comparison sees it: its start, end, pitch and patch, channel ignored and the
/// carrier's level apart.
type Compared = (i64, i64, u16, u8, [u8; 11]);

/// The notes of a stream, sorted, each with its carrier's level. `c0_mask` keeps the bits of
/// register C0 the comparison cares about: the OPL3's output bits are left out when comparing
/// with a chip that has none.
fn compared(decoded: &Decoded, c0_mask: u8) -> Vec<(Compared, u8)> {
	let mut notes: Vec<(Compared, u8)> = decoded
		.notes
		.iter()
		.map(|n| {
			let mut patch = n.patch;
			// The carrier's level is compared apart; its key scale bits stay.
			patch[6] &= 0xc0;
			patch[10] &= c0_mask;
			((n.on, n.off, n.fnum, n.block, patch), n.carrier_tl)
		})
		.collect();
	notes.sort_unstable();
	notes
}

fn hits(decoded: &Decoded) -> Vec<(i64, &'static str)> {
	let mut list: Vec<(i64, &str)> = decoded
		.hits
		.iter()
		.map(|h| (h.tick, h.drum.as_str()))
		.collect();
	list.sort_unstable();
	list
}

/// The game's chunk, imported to OPX at the grid it was written for.
fn game_opx() -> Vec<u8> {
	let chunk = std::fs::read(fixtures().join("opx/title-theme.imf")).unwrap();
	import_imf(&chunk, ImfKind::Type1, 560)
		.expect("the chunk imports")
		.opx
}

fn stream(chip: OplChip) -> Vec<u8> {
	support::export(&title_theme(), chip).bytes
}

#[test]
fn the_streams_validate_and_carry_the_hosts_choices_in_their_headers() {
	for (chip, expect, reserved) in [
		(OplChip::Opl2, Chip::Opl2, 1u32 << 5),
		(OplChip::Opl3, Chip::Opl3, 1 << 5 | 1 << 14),
	] {
		let bytes = stream(chip);
		let header = validate(&bytes).expect("the stream validates");
		assert_eq!(header.chip, expect);
		assert_eq!(header.grid_hz, 560);
		assert_eq!(header.total_ticks, 34_560, "{chip:?}: 36 bars at 140 bpm");
		assert_eq!(header.reserved_channels, reserved);
		assert_eq!(game_profile().reserved_channels(), [5, 14]);
		assert_eq!(header.loop_offset, u32::MAX);
		// Registers 0x104 and 0x105 belong to the host's audio service.
		for step in Decoder::new(&bytes[HEADER_LEN..], expect) {
			if let Command::Write {
				bank: 1,
				reg: 4 | 5,
				..
			} = step.unwrap().command
			{
				panic!("{chip:?}: the stream writes the OPL3's mode registers");
			}
		}
	}
}

#[test]
fn the_streams_and_their_twins_are_the_golden_files() {
	let dir = fixtures().join("opx");
	for (name, bytes) in golden_files() {
		let golden = std::fs::read(dir.join(&name))
			.unwrap_or_else(|_| panic!("{name} is missing; run the title_theme_opx example"));
		assert!(
			golden == bytes,
			"{name} changed; run the title_theme_opx example to rewrite it"
		);
	}
}

#[test]
fn a_twin_assembles_to_the_same_stream() {
	for chip in [OplChip::Opl2, OplChip::Opl3] {
		let bytes = stream(chip);
		let twin = support::twin(&bytes);
		assert_eq!(assemble(&twin).expect("the twin assembles"), bytes);
	}
}

#[test]
fn the_export_is_deterministic() {
	assert!(stream(OplChip::Opl2) == stream(OplChip::Opl2));
	assert!(stream(OplChip::Opl3) == stream(OplChip::Opl3));
}

#[test]
fn the_opl2_stream_sounds_what_the_games_own_chunk_sounds() {
	let song = title_theme();
	let export = support::export(&song, OplChip::Opl2);
	let ours = decode_opx(&export.bytes).unwrap();
	let game = decode_opx(&game_opx()).unwrap();

	assert_eq!(ours.ticks, 34_560);
	assert_eq!(ours.ticks, game.ticks, "ticks");
	assert_eq!(ours.notes.len(), 644);
	assert_eq!(ours.notes.len(), game.notes.len(), "notes");
	assert_eq!(export.notes, 644);
	assert_eq!(export.dropped, 32, "notes the five channels cannot carry");
	assert_eq!(ours.hits.len(), 316);
	assert_eq!(export.drum_hits, 316);
	assert_eq!(hits(&ours), hits(&game), "drum hits");
	assert_eq!(ours.bends_while_keyed, 0);

	let (a, b) = (compared(&ours, 0xff), compared(&game, 0xff));
	let notes = |list: &[(Compared, u8)]| list.iter().map(|(n, _)| *n).collect::<Vec<_>>();
	assert_eq!(
		notes(&a),
		notes(&b),
		"the note list (start, end, pitch, patch)"
	);

	// The game's converter takes a part's gain from the first section it plays in, so the
	// breakdown's bass plays at full gain there and at half gain here: four notes whose carrier is
	// eight steps quieter in the stream than in the game's chunk.
	let levels: Vec<(i64, u8, u8)> = a
		.iter()
		.zip(&b)
		.filter(|(x, y)| x.1 != y.1)
		.map(|(x, y)| (x.0.0, x.1, y.1))
		.collect();
	assert_eq!(
		levels,
		[
			(23_040, 14, 6),
			(24_000, 14, 6),
			(24_960, 14, 6),
			(25_920, 14, 6)
		]
	);
}

#[test]
fn the_opl3_stream_adds_the_notes_the_opl2_dropped_and_places_every_note_in_the_centre() {
	let song = title_theme();
	let opl2 = decode_opx(&stream(OplChip::Opl2)).unwrap();
	let export = support::export(&song, OplChip::Opl3);
	let opl3 = decode_opx(&export.bytes).unwrap();
	let game = decode_opx(&game_opx()).unwrap();

	assert_eq!(opl3.ticks, opl2.ticks);
	assert_eq!(hits(&opl3), hits(&game), "drum hits");
	assert_eq!(export.dropped, 0, "thirteen channels carry every note");
	assert_eq!(
		opl3.notes.len(),
		676,
		"the game's 644 and the 32 it dropped"
	);

	// Every track is centred, so the pan law sends every note to both outputs, each at -3 dB.
	assert!(opl3.notes.iter().all(|n| n.patch[10] & 0x30 == 0x30));
	assert!(opl2.notes.iter().all(|n| n.patch[10] & 0x30 == 0));
	assert!(
		opl3.notes.iter().any(|n| n.channel >= 9),
		"the second bank plays"
	);

	// Against the OPL2 stream, levels aside and output bits ignored: every OPL2 note is there
	// with a carrier four steps quieter, except one the OPL2 cut short, which here rings out.
	let mut left: Vec<(Compared, u8)> = compared(&opl3, 0x0f);
	let mut missing: Vec<(Compared, u8)> = Vec::new();
	let mut deltas = BTreeMap::<i32, usize>::new();
	for note in compared(&opl2, 0x0f) {
		match left.iter().position(|other| other.0 == note.0) {
			Some(at) => {
				*deltas
					.entry(i32::from(left[at].1) - i32::from(note.1))
					.or_default() += 1;
				left.remove(at);
			}
			None => missing.push(note),
		}
	}
	assert_eq!(deltas, BTreeMap::from([(4, 643)]));
	assert_eq!(missing.len(), 1);
	assert_eq!(left.len(), 33, "32 dropped notes and the one that was cut");
	let cut = missing[0].0;
	assert!(
		left.iter()
			.any(|extra| extra.0.0 == cut.0 && extra.0.1 > cut.1 && extra.0.2 == cut.2),
		"the OPL2's cut note is whole on the OPL3"
	);
}
