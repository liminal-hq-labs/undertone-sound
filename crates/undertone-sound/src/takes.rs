// The notes a take plays
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The notes a take plays, with a mini-notation take expanded over its section.

use crate::mini_bridge;
use crate::model::{Note, Section, Song, Take, Track};

/// The notes a take plays, with a `mini` take expanded over the section's bars. A take with
/// neither notes nor a pattern plays nothing. Fails with the notation's message when the pattern
/// does not parse.
pub fn take_notes(
	song: &Song,
	track: &Track,
	section: &Section,
	take: &Take,
) -> Result<Vec<Note>, String> {
	if let Some(notes) = &take.notes {
		return Ok(notes.clone());
	}
	let Some(source) = &take.mini else {
		return Ok(Vec::new());
	};
	let drums = song.is_drum_track(track);
	let per_bar = f64::from(song.steps_per_bar());
	let mut notes: Vec<Note> = mini_bridge::onsets(source, drums, section.bars)?
		.into_iter()
		.map(|onset| Note {
			pan: None,
			pan_motion: None,
			at: onset.begin * per_bar,
			pitch: onset.value,
			len: (onset.end - onset.begin) * per_bar,
			vel: None,
		})
		.collect();
	notes.sort_by(|a, b| a.at.total_cmp(&b.at));
	Ok(notes)
}
