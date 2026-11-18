// The host's side of the Title Theme export: its profile, its level rule and its grid.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The crate makes no choice a host should make. The game the Title Theme comes from makes these:
//! it keeps channels 5 and 14 free for sound effects, plays its music on a 560 Hz grid, sets each
//! melodic note's level against the loudest melodic note, strikes every drum at full level, hands
//! channels to the most important part first when notes start together, leaves six steps of
//! headroom on every carrier, and holds each rhythm drum for a fixed number of ticks. Everything
//! here is that host's data, passed to the crate's functions; none of it is in the crate.

use std::path::PathBuf;

use undertone_opl::opx::{DrumHolds, KeyOffs, OpxExport, OpxOptions, export_opx};
use undertone_opl::profile::OplProfile;
use undertone_opl::sequence::{OplChip, sequence_opl};
use undertone_sound::compile::{CompileOptions, Performance, compile_song};
use undertone_sound::model::Song;
use undertone_sound::validate::validate_song;

/// The repository's `fixtures` directory.
pub fn fixtures() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The Title Theme song: the 36-bar arrangement the game's chunk was made from.
pub fn title_theme() -> Song {
	let path = fixtures().join("song/inputs/title-theme-dos.song.json");
	let raw = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
	let validation = validate_song(&raw);
	validation.song.unwrap_or_else(|| {
		panic!(
			"the Title Theme does not validate: {:?}",
			validation.problems
		)
	})
}

/// The channels the game keeps for its sound effects: 5, and 14 beside it on the OPL3.
pub fn game_profile() -> OplProfile {
	OplProfile::new().reserve([5, 14])
}

/// The game's grid, in ticks a second.
pub const GRID_HZ: u32 = 560;

/// How the game turns a sequence into a stream: six steps of headroom, a key-off before the next
/// key-on on a channel, and fixed drum holds in ticks (bass, snare, tom, cymbal, hat).
pub fn game_options() -> OpxOptions {
	OpxOptions {
		grid_hz: GRID_HZ,
		headroom: 6,
		key_offs: KeyOffs::BeforeNextKeyOn,
		drum_holds: DrumHolds::Ticks([40, 30, 40, 60, 14]),
	}
}

/// The game's gain rule, applied to the performance before it is sequenced: each melodic note is
/// set against the loudest melodic note, and a drum hit is struck at full level.
fn apply_levels(performance: &mut Performance) {
	let loudest = performance
		.events
		.iter()
		.filter(|e| e.drum.is_none())
		.map(|e| e.velocity)
		.fold(0.01, f64::max);
	for event in &mut performance.events {
		event.velocity = if event.drum.is_some() {
			1.0
		} else {
			event.velocity / loudest
		};
	}
}

/// Puts the notes that start together in the order the game gives them channels: the most
/// important track first, keeping the song's own order among equals.
fn rank_simultaneous_notes(song: &Song, performance: &mut Performance) {
	let rank = |track_id: &str| {
		song.tracks
			.iter()
			.find(|track| track.id == track_id)
			.map_or(0, |track| track.effective_priority())
	};
	performance.events.sort_by(|a, b| {
		a.step
			.total_cmp(&b.step)
			.then_with(|| rank(&b.track_id).cmp(&rank(&a.track_id)))
	});
}

/// The song as the game's host plays it: compiled, then levelled and ranked.
pub fn game_performance(song: &Song) -> Performance {
	let mut performance =
		compile_song(song, &CompileOptions::default()).expect("the song compiles");
	apply_levels(&mut performance);
	rank_simultaneous_notes(song, &mut performance);
	performance
}

/// The Title Theme as an OPX file for a chip, made the game's way.
pub fn export(song: &Song, chip: OplChip) -> OpxExport {
	let profile = game_profile();
	let sequence = sequence_opl(song, &game_performance(song), chip, &profile);
	export_opx(&sequence, chip, &profile, &game_options()).expect("the stream exports")
}

/// The text twin of an OPX file.
pub fn twin(opx: &[u8]) -> String {
	let mut text = String::new();
	oplstream::print_twin(opx, &mut text).expect("a stream we wrote prints");
	text
}

/// Every file the fixtures hold for the Title Theme: the name inside `fixtures/opx/`, and the
/// bytes. Each stream is followed by its text twin.
pub fn golden_files() -> Vec<(String, Vec<u8>)> {
	let song = title_theme();
	let mut files = Vec::new();
	for (name, chip) in [("opl2", OplChip::Opl2), ("opl3", OplChip::Opl3)] {
		let opx = export(&song, chip).bytes;
		files.push((
			format!("title-theme.{name}.opx.txt"),
			twin(&opx).into_bytes(),
		));
		files.push((format!("title-theme.{name}.opx"), opx));
	}
	files
}
