// Runs every case in fixtures/mini/ against the port
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The leaf rules (`word`, `pitch`, `drum`) live here, not in the crate: they are `checkPitchWord`
//! and `parsePitch` from the TypeScript the fixtures were recorded from, and a host owns its own.
//! This copy exists so the fixtures can run.

use std::path::PathBuf;

use serde_json::Value;
use undertone_mini::{Fraction, Hap, parse};

const DRUM_PIECES: [&str; 8] = [
	"crash", "openhat", "hat", "clap", "rim", "snare", "tom", "kick",
];

/// `parsePitch`: `^([a-g])(#|b)?(-?\d)$` with the `i` flag. The flag lets a capital `B` match as an
/// accidental, but `accidentalOffset` compares with a lowercase `b` only, so `cB4` is `c4`.
fn parse_pitch(word: &str) -> Option<i32> {
	let bytes = word.as_bytes();
	let natural = match bytes.first()?.to_ascii_lowercase() {
		b'c' => 0,
		b'd' => 2,
		b'e' => 4,
		b'f' => 5,
		b'g' => 7,
		b'a' => 9,
		b'b' => 11,
		_ => return None,
	};
	let mut rest = &bytes[1..];
	let mut offset = 0;
	if let Some(&mark) = rest.first()
		&& matches!(mark, b'#' | b'b' | b'B')
	{
		offset = match mark {
			b'#' => 1,
			b'b' => -1,
			_ => 0,
		};
		rest = &rest[1..];
	}
	let (negative, digits) = match rest.split_first() {
		Some((b'-', digits)) => (true, digits),
		_ => (false, rest),
	};
	let [digit] = digits else { return None };
	if !digit.is_ascii_digit() {
		return None;
	}
	let octave = i32::from(digit - b'0') * if negative { -1 } else { 1 };
	Some((octave + 1) * 12 + natural + offset)
}

/// `checkPitchWord(word, drums)`.
fn check_pitch_word(word: &str, drums: bool) -> Result<String, String> {
	if drums {
		if !DRUM_PIECES.contains(&word) {
			return Err(format!(
				"\"{word}\" is not a drum piece ({})",
				DRUM_PIECES.join(", ")
			));
		}
		return Ok(word.to_string());
	}
	let Some(midi) = parse_pitch(word) else {
		return Err(format!(
			"\"{word}\" is not a note name such as \"d4\", \"f#3\" or \"bb2\""
		));
	};
	if !(12..=120).contains(&midi) {
		return Err(format!("\"{word}\" is out of range (c0 to c9)"));
	}
	Ok(word.to_string())
}

fn leaf(rule: &str, word: &str) -> Result<String, String> {
	match rule {
		"word" => Ok(word.to_string()),
		"pitch" => check_pitch_word(word, false),
		"drum" => check_pitch_word(word, true),
		other => panic!("unknown leaf rule {other}"),
	}
}

fn fraction(text: &Value) -> Fraction {
	let text = text.as_str().expect("fraction text");
	let (n, d) = text.split_once('/').expect("n/d");
	Fraction::new(
		n.parse().expect("numerator"),
		d.parse().expect("denominator"),
	)
}

fn fraction_text(f: Fraction) -> String {
	format!("{}/{}", f.numer(), f.denom())
}

type SortKey = (Fraction, Fraction, Option<(Fraction, Fraction)>, String);

/// The fixture's hap order: part begin, part end, whole begin, whole end (a missing whole first),
/// then value.
fn sort_key(hap: &Hap<String>) -> SortKey {
	(
		hap.part.begin,
		hap.part.end,
		hap.whole.map(|w| (w.begin, w.end)),
		hap.value.clone(),
	)
}

fn hap_json(hap: &Hap<String>) -> Value {
	let span = |b: Fraction, e: Fraction| serde_json::json!([fraction_text(b), fraction_text(e)]);
	serde_json::json!({
		"part": span(hap.part.begin, hap.part.end),
		"value": hap.value,
		"whole": hap.whole.map(|w| span(w.begin, w.end)),
	})
}

fn run_case(case: &Value) -> Result<(), String> {
	let id = case["id"].as_str().unwrap();
	let source = case["pattern"].as_str().unwrap();
	let rule = case["leaf"].as_str().unwrap();
	let parsed = parse(source, |word| leaf(rule, word));
	if let Some(error) = case.get("error") {
		let err = parsed
			.err()
			.ok_or(format!("{id}: parsed but should fail"))?;
		let message = error["message"].as_str().unwrap();
		let position = error["position"].as_u64().map(|p| p as usize);
		if err.message != message || err.position != position {
			return Err(format!(
				"{id} {source:?}: got {:?} at {:?}, wanted {message:?} at {position:?}",
				err.message, err.position
			));
		}
		return Ok(());
	}
	let pattern = parsed.map_err(|e| format!("{id} {source:?}: unexpected error {}", e.message))?;
	for query in case["queries"].as_array().unwrap() {
		let span = query["span"].as_array().unwrap();
		let mut haps = pattern.query(fraction(&span[0]), fraction(&span[1]));
		haps.sort_by_key(sort_key);
		let got: Vec<Value> = haps.iter().map(hap_json).collect();
		let wanted = query["haps"].as_array().unwrap();
		if &got != wanted {
			return Err(format!(
				"{id} {source:?} over {span:?}: got {}, wanted {}",
				Value::Array(got),
				Value::Array(wanted.clone())
			));
		}
	}
	Ok(())
}

fn run_file(name: &str) {
	let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/mini")
		.join(name);
	let text = std::fs::read_to_string(&path).expect("fixture file");
	let cases: Vec<Value> = serde_json::from_str(&text).expect("fixture JSON");
	assert!(!cases.is_empty(), "{name} has no cases");
	let failures: Vec<String> = cases.iter().filter_map(|c| run_case(c).err()).collect();
	assert!(
		failures.is_empty(),
		"{} of {} cases in {name} fail:\n{}",
		failures.len(),
		cases.len(),
		failures.join("\n")
	);
	println!("{name}: {} cases pass", cases.len());
}

#[test]
fn songs() {
	run_file("songs.json");
}

#[test]
fn tests() {
	run_file("tests.json");
}

#[test]
fn edge() {
	run_file("edge.json");
}

#[test]
fn edge_pitch() {
	run_file("edge-pitch.json");
}

#[test]
fn edge_drum() {
	run_file("edge-drum.json");
}

#[test]
fn errors() {
	run_file("errors.json");
}

#[test]
fn errors_leaf() {
	run_file("errors-leaf.json");
}

/// Every fixture file is run by a test above.
#[test]
fn every_fixture_file_is_covered() {
	let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mini");
	let mut names: Vec<String> = std::fs::read_dir(dir)
		.unwrap()
		.map(|e| e.unwrap().file_name().into_string().unwrap())
		.collect();
	names.sort();
	assert_eq!(
		names,
		[
			"edge-drum.json",
			"edge-pitch.json",
			"edge.json",
			"errors-leaf.json",
			"errors.json",
			"songs.json",
			"tests.json"
		]
	);
}
