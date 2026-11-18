// Checks the performance compiler against `fixtures/song/compile.json`
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each case holds a song and compile options, and the performance the TypeScript compiled from
//! them. The test requires every event, section and length to match, bit for bit.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use undertone_sound::compile::*;
use undertone_sound::model::Song;
use undertone_sound::takes::take_notes;

fn dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/song")
}

fn song(name: &str) -> Song {
	let text = fs::read_to_string(dir().join("inputs").join(format!("{name}.song.json"))).unwrap();
	serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Equality that treats `96` and `96.0` as the same number, as JSON does.
fn same(a: &Value, b: &Value) -> bool {
	match (a, b) {
		(Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
		(Value::Array(x), Value::Array(y)) => {
			x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
		}
		(Value::Object(x), Value::Object(y)) => {
			x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
		}
		_ => a == b,
	}
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

#[test]
fn compile_fixtures() {
	let text = fs::read_to_string(dir().join("compile.json")).unwrap();
	let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
	let mut counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
	let mut failures = Vec::new();
	for case in &cases {
		let kind = case["kind"].as_str().unwrap_or("compileSong").to_string();
		let id = case["id"].as_str().unwrap();
		let song = song(case["input"].as_str().unwrap());
		let outcome: Result<(), String> = (|| match kind.as_str() {
			"compileSong" => {
				let got = compile_song(&song, &options(&case["options"]));
				match (got, case.get("error")) {
					(Err(message), Some(want)) => {
						if want.as_str() == Some(message.as_str()) {
							Ok(())
						} else {
							Err(format!("error {message:?} != {want}"))
						}
					}
					(Ok(performance), None) => {
						let value = serde_json::to_value(&performance).unwrap();
						if same(&value, &case["performance"]) {
							Ok(())
						} else {
							Err("performance differs".to_string())
						}
					}
					(got, _) => Err(format!("outcome differs: {:?}", got.map(|_| "ok"))),
				}
			}
			"rotatePerformance" => {
				let base = compile_song(&song, &CompileOptions::default())?;
				let got = rotate_performance(&base, case["fromStep"].as_f64().unwrap());
				let value = serde_json::to_value(&got).unwrap();
				if same(&value, &case["performance"]) {
					Ok(())
				} else {
					Err("rotated performance differs".to_string())
				}
			}
			"sectionAtStep" => {
				let base = compile_song(&song, &CompileOptions::default())?;
				let got = section_at_step(&base, case["step"].as_f64().unwrap());
				let value = serde_json::to_value(got).unwrap();
				if same(&value, &case["section"]) {
					Ok(())
				} else {
					Err(format!("{value} != {}", case["section"]))
				}
			}
			"audibleTracks" => {
				let got: Vec<&str> = audible_tracks(&song)
					.iter()
					.map(|track| track.id.as_str())
					.collect();
				let want: Vec<&str> = case["tracks"]
					.as_array()
					.unwrap()
					.iter()
					.map(|v| v.as_str().unwrap())
					.collect();
				if got == want {
					Ok(())
				} else {
					Err(format!("{got:?} != {want:?}"))
				}
			}
			"takeNotes" => {
				let track = song.find_track(case["track"].as_str().unwrap()).unwrap();
				let section = song
					.find_section(case["section"].as_str().unwrap())
					.unwrap();
				let slot = &song.clips[&track.id][&section.id];
				let take = slot
					.takes
					.iter()
					.find(|t| t.id == case["take"].as_str().unwrap())
					.unwrap();
				match (take_notes(&song, track, section, take), case.get("error")) {
					(Ok(notes), None) => {
						let value = serde_json::to_value(&notes).unwrap();
						if same(&value, &case["notes"]) {
							Ok(())
						} else {
							Err("notes differ".to_string())
						}
					}
					(Err(message), Some(want)) if want.as_str() == Some(message.as_str()) => Ok(()),
					(other, _) => Err(format!("outcome differs: {:?}", other.map(|_| "ok"))),
				}
			}
			other => Err(format!("unknown case kind {other}")),
		})();
		let entry = counts.entry(kind).or_default();
		match outcome {
			Ok(()) => entry.0 += 1,
			Err(detail) => {
				entry.1 += 1;
				failures.push(format!("{id}: {detail}"));
			}
		}
	}
	for (kind, (passed, failed)) in &counts {
		println!("compile.json {kind}: {passed} passed, {failed} failed");
	}
	assert!(failures.is_empty(), "failures:\n{}", failures.join("\n"));
}
