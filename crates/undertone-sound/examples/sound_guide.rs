// The snippets of the `undertone-sound` guide page, compiled and run so they stay true.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Run with `cargo run -p undertone-sound --example sound_guide`. The regions are what
//! `site/guide/undertone-sound.md` shows.

use serde_json::Value;
use undertone_sound::compile::{CompileOptions, compile_song};
use undertone_sound::format::format_song;
use undertone_sound::model::{Author, AuthorKind, SONG_FORMAT};
use undertone_sound::validate::validate_song;

fn main() {
	// #region read
	// A song file is JSON. Validation names every problem by its path and returns the song when
	// there are no errors.
	let text = include_str!("../../../fixtures/song/inputs/blank.song.json");
	let raw: Value = serde_json::from_str(text).expect("JSON");
	let validation = validate_song(&raw);
	assert!(!validation.has_errors());
	let song = validation.song.expect("a valid song");
	assert_eq!(SONG_FORMAT, "undertone-sound/song");
	println!("{} at {} bpm", song.title, song.tempo);
	// #endregion read

	// #region author
	// Who wrote a take or a message is a kind, and an agent may carry a name. A host maps its own
	// words onto the two kinds.
	let person = Author::human();
	let model = Author::agent().named("Claude");
	assert_eq!(person.kind, AuthorKind::Human);
	println!("{}", serde_json::to_string(&model).expect("JSON"));
	// {"kind":"agent","name":"Claude"}
	// #endregion author

	// #region compile
	// Compiling lowers the arrangement to timed, engine-neutral events. An engine decides how to
	// voice them.
	let performance =
		compile_song(&song, &CompileOptions::default()).expect("a song that compiles");
	println!(
		"{} events over {} steps at {} steps a second",
		performance.events.len(),
		performance.length_steps,
		performance.steps_per_second
	);
	// #endregion compile

	// #region format
	// The same song always formats to the same text, so a diff shows only what changed.
	let formatted = format_song(&song);
	assert!(formatted.starts_with("{\n\t\"format\": \"undertone-sound/song\""));
	// #endregion format
}
