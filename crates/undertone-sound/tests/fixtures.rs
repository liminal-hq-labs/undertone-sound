// Checks the song core against the recorded fixtures in `fixtures/song/`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each file in `fixtures/song/` holds cases the TypeScript recorded: validation problems, formatted
//! text, pitch and scale tables and song helpers. Each test reads one file and requires every case
//! to match; a failure names the case.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use undertone_sound::format::{format_document, format_song};
use undertone_sound::model::*;
use undertone_sound::music;
use undertone_sound::validate::{Problem, Severity, Validation, validate_song};

fn dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/song")
}

fn load(name: &str) -> Vec<Value> {
	let text = fs::read_to_string(dir().join(format!("{name}.json"))).unwrap();
	match serde_json::from_str::<Value>(&text).unwrap() {
		Value::Array(items) => items,
		other => vec![other],
	}
}

fn input_text(name: &str) -> String {
	fs::read_to_string(dir().join("inputs").join(format!("{name}.song.json"))).unwrap()
}

/// Reads a song document's text, as a host does: JSON first, then the song format's rules.
fn parse_song_text(text: &str) -> Validation {
	let raw: Value = serde_json::from_str(text).expect("a recorded song is JSON");
	validate_song(&raw)
}

fn input_song(name: &str) -> Song {
	let validation = parse_song_text(&input_text(name));
	validation
		.song
		.unwrap_or_else(|| panic!("{name} does not validate: {:?}", validation.problems))
}

fn lines(value: &Value) -> String {
	value
		.as_array()
		.unwrap()
		.iter()
		.map(|line| line.as_str().unwrap())
		.collect::<Vec<_>>()
		.join("\n")
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

/// Collects failures and prints the count for a fixture file.
struct Tally {
	file: &'static str,
	passed: usize,
	failed: Vec<String>,
}

impl Tally {
	fn new(file: &'static str) -> Tally {
		Tally {
			file,
			passed: 0,
			failed: Vec::new(),
		}
	}

	fn check(&mut self, id: &str, ok: bool, detail: impl FnOnce() -> String) {
		if ok {
			self.passed += 1;
		} else {
			self.failed.push(format!("{id}: {}", detail()));
		}
	}

	fn finish(self) {
		println!(
			"{}: {} passed, {} failed",
			self.file,
			self.passed,
			self.failed.len()
		);
		assert!(
			self.failed.is_empty(),
			"{} failures:\n{}",
			self.file,
			self.failed.join("\n")
		);
	}
}

fn id(case: &Value) -> &str {
	case["id"].as_str().unwrap()
}

/// A parse case's message may come from the JavaScript engine's JSON parser, so only its start is
/// compared.
#[test]
fn validation() {
	let mut tally = Tally::new("validation.json");
	for case in load("validation") {
		let result = validate_song(&case["input"]);
		let got = plain(&result.problems);
		let ok = result.song.is_some();
		tally.check(
			id(&case),
			got == case["problems"] && ok == case["ok"].as_bool().unwrap(),
			|| format!("got {got}\nwant {}", case["problems"]),
		);
	}
	tally.finish();
}

#[test]
fn formatted() {
	let mut tally = Tally::new("formatted.json");
	for case in load("formatted") {
		if case["kind"] == "formatSong" {
			let name = case["input"].as_str().unwrap();
			let want = input_text(name);
			let validation = parse_song_text(&input_text(name));
			let got = format_document(validation.document.as_ref().unwrap());
			// The typed song writes the canonical key order and reads back the same song.
			let canonical = parse_song_text(&format_song(validation.song.as_ref().unwrap()));
			tally.check(
				&format!("{}/canonical", id(&case)),
				canonical.song == validation.song,
				|| "typed song does not survive format_song".to_string(),
			);
			tally.check(
				id(&case),
				got == want && got.len() as u64 == case["bytes"].as_u64().unwrap(),
				|| format!("formatted text differs for {name}"),
			);
		} else {
			let got = format_document(&case["value"]);
			let want = lines(&case["text"]);
			tally.check(id(&case), got == want, || format!("{got:?} vs {want:?}"));
		}
	}
	tally.finish();
}

#[test]
fn music_fixtures() {
	let mut tally = Tally::new("music.json");
	let all = load("music");
	let data = &all[0];
	let key = |v: &Value| -> Key { serde_json::from_value(v.clone()).unwrap() };
	for (n, case) in data["parsePitch"].as_array().unwrap().iter().enumerate() {
		let got = music::parse_pitch(case["name"].as_str().unwrap());
		tally.check(
			&format!("parsePitch/{n}"),
			json!(got) == case["midi"],
			|| format!("{got:?}"),
		);
	}
	for (n, case) in data["pitchClass"].as_array().unwrap().iter().enumerate() {
		let got = music::pitch_class(case["name"].as_str().unwrap());
		tally.check(
			&format!("pitchClass/{n}"),
			json!(got) == case["semitone"],
			|| format!("{got:?}"),
		);
	}
	for (n, case) in data["isInKey"].as_array().unwrap().iter().enumerate() {
		let got = music::is_in_key(case["midi"].as_i64().unwrap() as i32, &key(&case["key"]));
		tally.check(
			&format!("isInKey/{n}"),
			got == case["inKey"].as_bool().unwrap(),
			String::new,
		);
	}
	for (n, case) in data["keyPitchClasses"]
		.as_array()
		.unwrap()
		.iter()
		.enumerate()
	{
		let mut got = music::key_pitch_classes(&key(&case["key"]));
		let mut want: Vec<i64> = case["classes"]
			.as_array()
			.unwrap()
			.iter()
			.map(|v| v.as_i64().unwrap())
			.collect();
		got.sort();
		want.sort();
		tally.check(
			&format!("keyPitchClasses/{n}"),
			got.iter().map(|v| i64::from(*v)).collect::<Vec<_>>() == want,
			|| format!("{got:?}"),
		);
	}
	for (n, case) in data["prefersFlats"]
		.as_array()
		.unwrap()
		.iter()
		.chain(data["prefersFlatsOddities"].as_array().unwrap())
		.enumerate()
	{
		let got = music::prefers_flats(&key(&case["key"]));
		tally.check(
			&format!("prefersFlats/{n}"),
			got == case["flats"].as_bool().unwrap(),
			String::new,
		);
	}
	for (n, case) in data["midiToName"].as_array().unwrap().iter().enumerate() {
		let k = if case["key"].is_null() {
			None
		} else {
			Some(key(&case["key"]))
		};
		let names: Vec<String> = case["names"]
			.as_array()
			.unwrap()
			.iter()
			.map(|v| v.as_str().unwrap().to_string())
			.collect();
		let got: Vec<String> = (0..names.len() as i32)
			.map(|m| music::midi_to_name(m, k.as_ref()))
			.collect();
		tally.check(&format!("midiToName/{n}"), got == names, String::new);
	}
	for (n, case) in data["midiToNameOutsideRange"]
		.as_array()
		.unwrap()
		.iter()
		.enumerate()
	{
		let got = music::midi_to_name(case["midi"].as_i64().unwrap() as i32, None);
		tally.check(
			&format!("midiToNameOutsideRange/{n}"),
			got == case["name"].as_str().unwrap(),
			|| got.clone(),
		);
	}
	for (n, case) in data["stepInKey"].as_array().unwrap().iter().enumerate() {
		let got = music::step_in_key(
			case["midi"].as_i64().unwrap() as i32,
			case["degrees"].as_i64().unwrap() as i32,
			&key(&case["key"]),
		);
		tally.check(
			&format!("stepInKey/{n}"),
			i64::from(got) == case["result"].as_i64().unwrap(),
			|| format!("{got}"),
		);
	}
	for (name, steps) in data["scales"].as_object().unwrap() {
		let scale = ScaleName::from_word(name).unwrap();
		let got: Vec<i64> = music::scale_intervals(scale)
			.iter()
			.map(|v| i64::from(*v))
			.collect();
		let want: Vec<i64> = steps
			.as_array()
			.unwrap()
			.iter()
			.map(|v| v.as_i64().unwrap())
			.collect();
		tally.check(&format!("scales/{name}"), got == want, String::new);
	}
	let c = &data["constants"];
	let words = |all: Vec<&str>| json!(all);
	tally.check(
		"constants/engines",
		words(EngineId::ALL.iter().map(|e| e.as_str()).collect()) == c["ENGINE_IDS"],
		String::new,
	);
	tally.check(
		"constants/presets",
		words(PresetId::ALL.iter().map(|e| e.as_str()).collect()) == c["PRESET_IDS"],
		String::new,
	);
	tally.check(
		"constants/roles",
		words(TrackRole::ALL.iter().map(|e| e.as_str()).collect()) == c["TRACK_ROLES"],
		String::new,
	);
	tally.check(
		"constants/drums",
		words(DrumPiece::ALL.iter().map(|e| e.as_str()).collect()) == c["DRUM_PIECES"],
		String::new,
	);
	tally.check(
		"constants/authors",
		words(Author::ALL.iter().map(|e| e.as_str()).collect()) == c["AUTHORS"],
		String::new,
	);
	tally.check(
		"constants/format",
		json!(SONG_FORMAT) == c["SONG_FORMAT"] && json!(SONG_VERSION) == c["SONG_VERSION"],
		String::new,
	);
	tally.finish();
}

#[test]
fn model_fixtures() {
	let mut tally = Tally::new("model.json");
	for case in load("model") {
		let song = input_song(case["input"].as_str().unwrap());
		let mut ok = song.steps_per_bar() as u64 == case["stepsPerBar"].as_u64().unwrap();
		for entry in case["sections"].as_array().unwrap() {
			let section = song.find_section(entry["id"].as_str().unwrap()).unwrap();
			ok &= u64::from(song.section_steps(section)) == entry["steps"].as_u64().unwrap();
		}
		for entry in case["findSection"].as_array().unwrap() {
			let found = song
				.find_section(entry["id"].as_str().unwrap())
				.map(|s| s.id.clone());
			ok &= json!(found) == entry["found"];
		}
		for entry in case["findTrack"].as_array().unwrap() {
			let found = song
				.find_track(entry["id"].as_str().unwrap())
				.map(|t| t.id.clone());
			ok &= json!(found) == entry["found"];
		}
		for entry in case["isDrumTrack"].as_array().unwrap() {
			let track = song.find_track(entry["id"].as_str().unwrap()).unwrap();
			ok &= song.is_drum_track(track) == entry["drums"].as_bool().unwrap();
		}
		for entry in case["activeTake"].as_array().unwrap() {
			let found = song
				.active_take(
					entry["track"].as_str().unwrap(),
					entry["section"].as_str().unwrap(),
				)
				.map(|t| t.id.clone());
			ok &= json!(found) == entry["take"];
		}
		tally.check(id(&case), ok, String::new);
	}
	tally.finish();
}
