// Writes and checks `fixtures/song/compile.json`, the compiler's results for the named songs
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each case in `compile.json` names a song and what to ask of the compiler, and holds what the
//! compiler gives back, every event, section and length, bit for bit. The test makes the results
//! again and requires the file to hold exactly that text; with `UPDATE_FIXTURES=1` it writes the
//! file instead.

use std::fs;

use serde_json::{Map, Value, json};
use undertone_sound::compile::*;
use undertone_sound::model::Song;
use undertone_sound::takes::take_notes;

mod support;
use support::{expect_file, fixture_text, song_dir};

fn song(name: &str) -> Song {
	let text =
		fs::read_to_string(song_dir().join("inputs").join(format!("{name}.song.json"))).unwrap();
	serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn options(value: &Value) -> CompileOptions {
	CompileOptions {
		only: value["only"].as_i64().map(|n| n as i32),
		section: value["section"].as_str().map(str::to_string),
		takes: value["takes"]
			.as_object()
			.map(|map| {
				map.iter()
					.map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
					.collect()
			})
			.unwrap_or_default(),
	}
}

fn value<T: serde::Serialize>(result: &T) -> Value {
	serde_json::to_value(result).unwrap()
}

/// What a case asks for, as the fields it adds to the case.
fn results(case: &Value, song: &Song) -> Vec<(&'static str, Value)> {
	let id = case["id"].as_str().unwrap();
	let kind = case["kind"].as_str().unwrap_or("compileSong");
	match kind {
		"compileSong" => match compile_song(song, &options(&case["options"])) {
			Ok(performance) => vec![("performance", value(&performance))],
			Err(message) => vec![("error", json!(message))],
		},
		"rotatePerformance" => {
			let base = compile_song(song, &CompileOptions::default()).unwrap();
			let rotated = rotate_performance(&base, case["fromStep"].as_f64().unwrap());
			vec![("performance", value(&rotated))]
		}
		"sectionAtStep" => {
			let base = compile_song(song, &CompileOptions::default()).unwrap();
			let section = section_at_step(&base, case["step"].as_f64().unwrap());
			vec![("section", value(&section))]
		}
		"audibleTracks" => {
			let tracks: Vec<&str> = audible_tracks(song)
				.iter()
				.map(|track| track.id.as_str())
				.collect();
			vec![("tracks", json!(tracks))]
		}
		"takeNotes" => {
			let track = song.find_track(case["track"].as_str().unwrap()).unwrap();
			let section = song
				.find_section(case["section"].as_str().unwrap())
				.unwrap();
			let take = song.clips[&track.id][&section.id]
				.takes
				.iter()
				.find(|take| take.id == case["take"].as_str().unwrap())
				.unwrap();
			match take_notes(song, track, section, take) {
				Ok(notes) => vec![("notes", value(&notes))],
				Err(message) => vec![("error", json!(message))],
			}
		}
		other => panic!("{id}: unknown case kind {other}"),
	}
}

#[test]
fn compile_fixtures() {
	let text = fs::read_to_string(song_dir().join("compile.json")).unwrap();
	let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
	let written: Vec<Value> = cases
		.iter()
		.map(|case| {
			let song = song(case["input"].as_str().unwrap());
			let mut out: Map<String, Value> = case.as_object().unwrap().clone();
			// A `takeNotes` case's `section` is an input, a `sectionAtStep` case's is a result.
			let own: &[&str] = match case["kind"].as_str().unwrap_or("compileSong") {
				"sectionAtStep" => &["section"],
				"audibleTracks" => &["tracks"],
				"takeNotes" => &["notes", "error"],
				_ => &["performance", "error"],
			};
			for key in own {
				out.shift_remove(*key);
			}
			for (key, result) in results(case, &song) {
				out.insert(key.to_string(), result);
			}
			Value::Object(out)
		})
		.collect();
	expect_file(
		&song_dir().join("compile.json"),
		&fixture_text(&Value::Array(written)),
	);
}
