// Reads the recorded scores of `fixtures/undertone/graph.json` back, for the tests that book them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::path::PathBuf;

use indexmap::IndexMap;
use serde_json::Value;
use undertone_dsp::params::Patch;
use undertone_dsp::pitch::Pitch;
use undertone_dsp::score::{Score, ScoreEvent, ScorePitch, score};

pub fn fixture() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/undertone/graph.json")
}

/// The ticks a second each case was recorded at: the tempo (120 for the first two cases, 140 for
/// the third) over 60, four steps a beat, twelve ticks a step.
fn ticks_per_second(id: &str) -> f64 {
	match id {
		"voices" | "corners" => 96.0,
		"title-theme" => 112.0,
		other => panic!("the fixture has a case, {other}, the test does not know the tempo of"),
	}
}

/// One `tick … voice vN` line as an event of the score.
fn parse_event(line: &str, voices: &[Patch], source: usize) -> ScoreEvent {
	let words: Vec<&str> = line.split(' ').collect();
	let mut tick = None;
	let mut pitch = None;
	let mut gate = None;
	let mut level = None;
	let mut pan = None;
	let mut voice = None;
	let mut words = words.into_iter();
	while let Some(key) = words.next() {
		let value = words
			.next()
			.unwrap_or_else(|| panic!("`{line}` ends at `{key}`"));
		let number = || {
			value
				.parse::<f64>()
				.unwrap_or_else(|_| panic!("`{line}`: `{value}`"))
		};
		match key {
			"tick" => tick = Some(number()),
			"midi" => pitch = Some(ScorePitch::Midi(number())),
			"note" => {
				let note = match value.parse::<f64>() {
					Ok(hz) => Pitch::Hz(hz),
					Err(_) => Pitch::Name(value.to_string()),
				};
				pitch = Some(ScorePitch::Note(note));
			}
			"gate" => gate = Some(number()),
			"level" => level = Some(number()),
			"pan" => pan = Some(number()),
			"voice" => {
				let index: usize = value.trim_start_matches('v').parse().unwrap();
				voice = Some(voices[index].clone());
			}
			other => panic!("`{line}` has an unknown field `{other}`"),
		}
	}
	ScoreEvent {
		tick: tick.unwrap(),
		pitch: pitch.unwrap(),
		gate: gate.unwrap(),
		level,
		pan,
		voice: voice.unwrap(),
		source,
	}
}

/// The score a case recorded: its events, and the pass's length from the seconds it took.
pub fn read_score(case: &Value) -> Score {
	let id = case["id"].as_str().unwrap();
	let recorded = &case["score"];
	let voices: Vec<Patch> = recorded["voices"]
		.as_array()
		.unwrap()
		.iter()
		.map(|voice| {
			let fields = voice.as_object().unwrap();
			fields
				.iter()
				.filter(|(name, _)| name.as_str() != "id")
				.map(|(name, value)| (name.clone(), value.clone()))
				.collect::<IndexMap<String, Value>>()
		})
		.collect();
	let events: Vec<ScoreEvent> = recorded["events"]
		.as_array()
		.unwrap()
		.iter()
		.enumerate()
		.map(|(source, line)| parse_event(line.as_str().unwrap(), &voices, source))
		.collect();
	let tps = ticks_per_second(id);
	let length_ticks = (recorded["seconds"].as_f64().unwrap() * tps).round();
	score(tps, length_ticks, &events).unwrap_or_else(|e| panic!("{id}: {e}"))
}
