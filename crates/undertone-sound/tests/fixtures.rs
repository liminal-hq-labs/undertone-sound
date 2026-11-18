// Checks the song core against the files in `fixtures/song/`, which this crate writes
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each file in `fixtures/song/` holds cases: an input and what this crate makes of it. Each test
//! reads a file's cases, makes the results again and requires the file to hold exactly the text
//! that gives. With `UPDATE_FIXTURES=1` the tests write the files instead, which is how a change
//! to the format or the compiler reaches them; the diff then shows what the change did.

use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use undertone_sound::format::{format_document, format_song};
use undertone_sound::model::*;
use undertone_sound::music;
use undertone_sound::validate::{Problem, Severity, validate_song};

mod support;
use support::{bless, expect_file, fixture_text, song_dir};

/// `fixtures/chips/inputs/`, the songs the chip fixtures use that are not in `song/inputs/`.
pub fn chips_inputs_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/chips/inputs")
}

fn load(name: &str) -> Value {
	let text = fs::read_to_string(song_dir().join(format!("{name}.json"))).unwrap();
	serde_json::from_str(&text).unwrap()
}

fn cases(name: &str) -> Vec<Value> {
	match load(name) {
		Value::Array(items) => items,
		other => panic!("{name}.json holds {other}, not a list of cases"),
	}
}

fn input_text(name: &str) -> String {
	fs::read_to_string(song_dir().join("inputs").join(format!("{name}.song.json"))).unwrap()
}

fn input_song(name: &str) -> Song {
	let raw: Value = serde_json::from_str(&input_text(name)).unwrap();
	let validation = validate_song(&raw);
	validation
		.song
		.unwrap_or_else(|| panic!("{name} does not validate: {:?}", validation.problems))
}

fn id(case: &Value) -> &str {
	case["id"].as_str().unwrap()
}

/// The case with its result fields replaced.
fn with(case: &Value, results: &[(&str, Value)], drop: &[&str]) -> Value {
	let mut out: Map<String, Value> = case.as_object().unwrap().clone();
	for key in drop {
		out.shift_remove(*key);
	}
	for (key, value) in results {
		out.insert((*key).to_string(), value.clone());
	}
	Value::Object(out)
}

fn plain(problems: &[Problem]) -> Value {
	Value::Array(
		problems
			.iter()
			.map(|p| {
				json!({
					"severity": match p.severity { Severity::Error => "error", Severity::Warning => "warning" },
					"path": p.path,
					"message": p.message,
				})
			})
			.collect(),
	)
}

fn lines(text: &str) -> Value {
	Value::Array(text.split('\n').map(|line| json!(line)).collect())
}

/// Every song document in `song/inputs/` and `chips/inputs/` is written the way `format_document`
/// writes it, with its keys in document order.
#[test]
fn song_files_are_written_in_the_stable_style() {
	for dir in [song_dir().join("inputs"), chips_inputs_dir()] {
		let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
			.unwrap()
			.map(|entry| entry.unwrap().path())
			.filter(|path| path.to_string_lossy().ends_with(".song.json"))
			.collect();
		paths.sort();
		assert!(!paths.is_empty(), "no songs in {}", dir.display());
		for path in paths {
			let raw: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap())
				.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
			expect_file(&path, &format_document(&raw));
		}
	}
}

#[test]
fn validation() {
	let written: Vec<Value> = cases("validation")
		.iter()
		.map(|case| {
			let result = validate_song(&case["input"]);
			with(
				case,
				&[
					("ok", json!(result.song.is_some())),
					("problems", plain(&result.problems)),
				],
				&[],
			)
		})
		.collect();
	expect_file(
		&song_dir().join("validation.json"),
		&fixture_text(&Value::Array(written)),
	);
}

#[test]
fn formatted() {
	let written: Vec<Value> = cases("formatted")
		.iter()
		.map(|case| {
			if case["kind"] == "formatSong" {
				let name = case["input"].as_str().unwrap();
				let raw: Value = serde_json::from_str(&input_text(name)).unwrap();
				let validation = validate_song(&raw);
				let song = validation
					.song
					.as_ref()
					.unwrap_or_else(|| panic!("{} does not validate", id(case)));
				// The typed song writes the canonical key order and reads back the same song.
				let canonical = validate_song(&serde_json::from_str(&format_song(song)).unwrap());
				assert!(
					canonical.song.as_ref() == Some(song),
					"{}: the typed song does not survive format_song",
					id(case)
				);
				let text = format_document(validation.document.as_ref().unwrap());
				with(case, &[("bytes", json!(text.len()))], &[])
			} else {
				let text = format_document(&case["value"]);
				with(case, &[("text", lines(&text))], &[])
			}
		})
		.collect();
	expect_file(
		&song_dir().join("formatted.json"),
		&fixture_text(&Value::Array(written)),
	);
	// A song's file is the text its case names, so the two cannot drift apart.
	for case in cases("formatted") {
		if case["kind"] == "formatSong" {
			let name = case["input"].as_str().unwrap();
			assert_eq!(
				case["formatsTo"].as_str().unwrap(),
				format!("inputs/{name}.song.json")
			);
			if !bless() {
				assert_eq!(
					case["bytes"].as_u64().unwrap() as usize,
					input_text(name).len(),
					"{}",
					id(&case)
				);
			}
		}
	}
}

fn key_of(value: &Value) -> Key {
	serde_json::from_value(value.clone()).unwrap()
}

fn map_list(list: &Value, make: impl Fn(&Value) -> Vec<(&'static str, Value)>) -> Value {
	Value::Array(
		list.as_array()
			.unwrap()
			.iter()
			.map(|case| with(case, &make(case), &[]))
			.collect(),
	)
}

#[test]
fn music_fixtures() {
	let data = load("music");
	let mut out = data.as_object().unwrap().clone();
	let mut set = |name: &str, value: Value| {
		out.insert(name.to_string(), value);
	};
	set(
		"parsePitch",
		map_list(&data["parsePitch"], |c| {
			vec![(
				"midi",
				json!(music::parse_pitch(c["name"].as_str().unwrap())),
			)]
		}),
	);
	set(
		"pitchClass",
		map_list(&data["pitchClass"], |c| {
			vec![(
				"semitone",
				json!(music::pitch_class(c["name"].as_str().unwrap())),
			)]
		}),
	);
	set(
		"isInKey",
		map_list(&data["isInKey"], |c| {
			let midi = c["midi"].as_i64().unwrap() as i32;
			vec![("inKey", json!(music::is_in_key(midi, &key_of(&c["key"]))))]
		}),
	);
	set(
		"keyPitchClasses",
		map_list(&data["keyPitchClasses"], |c| {
			let mut classes = music::key_pitch_classes(&key_of(&c["key"]));
			classes.sort();
			vec![("classes", json!(classes))]
		}),
	);
	for table in ["prefersFlats", "prefersFlatsOddities"] {
		set(
			table,
			map_list(&data[table], |c| {
				vec![("flats", json!(music::prefers_flats(&key_of(&c["key"]))))]
			}),
		);
	}
	set(
		"midiToName",
		map_list(&data["midiToName"], |c| {
			let key = (!c["key"].is_null()).then(|| key_of(&c["key"]));
			let count = c["names"].as_array().unwrap().len() as i32;
			let names: Vec<String> = (0..count)
				.map(|midi| music::midi_to_name(midi, key.as_ref()))
				.collect();
			vec![("names", json!(names))]
		}),
	);
	set(
		"midiToNameOutsideRange",
		map_list(&data["midiToNameOutsideRange"], |c| {
			let midi = c["midi"].as_i64().unwrap() as i32;
			vec![("name", json!(music::midi_to_name(midi, None)))]
		}),
	);
	set(
		"stepInKey",
		map_list(&data["stepInKey"], |c| {
			let result = music::step_in_key(
				c["midi"].as_i64().unwrap() as i32,
				c["degrees"].as_i64().unwrap() as i32,
				&key_of(&c["key"]),
			);
			vec![("result", json!(result))]
		}),
	);
	let mut scales = Map::new();
	for (name, _) in data["scales"].as_object().unwrap() {
		let scale = ScaleName::from_word(name).unwrap_or_else(|| panic!("unknown scale {name}"));
		scales.insert(name.clone(), json!(music::scale_intervals(scale)));
	}
	set("scales", Value::Object(scales));
	let words = |all: Vec<&str>| json!(all);
	set(
		"constants",
		json!({
			"ENGINE_IDS": words(EngineId::ALL.iter().map(|e| e.as_str()).collect()),
			"PRESET_IDS": words(PresetId::ALL.iter().map(|e| e.as_str()).collect()),
			"TRACK_ROLES": words(TrackRole::ALL.iter().map(|e| e.as_str()).collect()),
			"DRUM_PIECES": words(DrumPiece::ALL.iter().map(|e| e.as_str()).collect()),
			"AUTHOR_KINDS": words(AuthorKind::ALL.iter().map(|e| e.as_str()).collect()),
			"SONG_FORMAT": SONG_FORMAT,
			"SONG_VERSION": SONG_VERSION,
		}),
	);
	expect_file(
		&song_dir().join("music.json"),
		&fixture_text(&Value::Object(out)),
	);
}

#[test]
fn model_fixtures() {
	let written: Vec<Value> = cases("model")
		.iter()
		.map(|case| {
			let song = input_song(case["input"].as_str().unwrap());
			let text = |entry: &Value, key: &str| entry[key].as_str().unwrap().to_string();
			let list = |name: &str, make: &dyn Fn(&Value) -> Vec<(&'static str, Value)>| {
				map_list(&case[name], make)
			};
			let sections = list("sections", &|e| {
				let section = song.find_section(&text(e, "id")).unwrap();
				vec![("steps", json!(song.section_steps(section)))]
			});
			let find_section = list("findSection", &|e| {
				let found = song.find_section(&text(e, "id")).map(|s| s.id.clone());
				vec![("found", json!(found))]
			});
			let find_track = list("findTrack", &|e| {
				let found = song.find_track(&text(e, "id")).map(|t| t.id.clone());
				vec![("found", json!(found))]
			});
			let is_drum_track = list("isDrumTrack", &|e| {
				let track = song.find_track(&text(e, "id")).unwrap();
				vec![("drums", json!(song.is_drum_track(track)))]
			});
			let active_take = list("activeTake", &|e| {
				let take = song
					.active_take(&text(e, "track"), &text(e, "section"))
					.map(|t| t.id.clone());
				vec![("take", json!(take))]
			});
			with(
				case,
				&[
					("stepsPerBar", json!(song.steps_per_bar())),
					("sections", sections),
					("findSection", find_section),
					("findTrack", find_track),
					("isDrumTrack", is_drum_track),
					("activeTake", active_take),
				],
				&[],
			)
		})
		.collect();
	expect_file(
		&song_dir().join("model.json"),
		&fixture_text(&Value::Array(written)),
	);
}
