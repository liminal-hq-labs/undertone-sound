// Shared helpers for the fixture tests: where the files are, the style they are written in and
// the bless mode that rewrites them
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The recorded files in `fixtures/song/` are written by this repository. Each test computes
//! what its file should hold from the file's own cases, writes that out in the fixture style and
//! requires the same bytes. With `UPDATE_FIXTURES=1` it writes the file instead of comparing.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use undertone_mini::js::{number_to_string, quote, utf16_len};

/// A flat object or array stays on one line up to this many columns.
const INLINE_LIMIT: usize = 200;

/// `fixtures/song/`.
pub fn song_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/song")
}

/// Whether `UPDATE_FIXTURES` asks the tests to write the files instead of checking them.
pub fn bless() -> bool {
	std::env::var("UPDATE_FIXTURES").is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Requires the file to hold exactly `text`, or writes it in bless mode. A mismatch says which
/// line differs first and how to bless.
pub fn expect_file(path: &Path, text: &str) {
	if bless() {
		if fs::read_to_string(path).ok().as_deref() != Some(text) {
			fs::write(path, text).unwrap();
			println!("wrote {}", path.display());
		}
		return;
	}
	let have = fs::read_to_string(path).unwrap_or_default();
	if have == text {
		return;
	}
	let line = have
		.lines()
		.zip(text.lines())
		.position(|(a, b)| a != b)
		.unwrap_or_else(|| have.lines().count().min(text.lines().count()));
	panic!(
		"{} is not what this crate writes (first difference at line {}):\n  file: {}\n  made: {}\nrun `UPDATE_FIXTURES=1 cargo test -p undertone-sound` to rewrite the recorded files",
		path.display(),
		line + 1,
		have.lines().nth(line).unwrap_or("<end of file>"),
		text.lines().nth(line).unwrap_or("<end of file>"),
	);
}

/// A value with every object's keys in sorted order, except the documents a case carries under
/// `input` and `value`, which keep the order they were written in.
pub fn sorted(value: &Value) -> Value {
	match value {
		Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
		Value::Object(entries) => {
			let mut keys: Vec<&String> = entries.keys().collect();
			keys.sort();
			let mut out = Map::new();
			for key in keys {
				let item = &entries[key];
				let kept = key == "input" || key == "value";
				out.insert(key.clone(), if kept { item.clone() } else { sorted(item) });
			}
			Value::Object(out)
		}
		other => other.clone(),
	}
}

/// The text of a fixture file: keys sorted (see `sorted`), tabs, a flat object or array on one line when it
/// fits, numbers as JavaScript prints them and a trailing newline.
pub fn fixture_text(value: &Value) -> String {
	format!("{}\n", write(&sorted(value), 0))
}

fn is_container(value: &Value) -> bool {
	matches!(value, Value::Array(_) | Value::Object(_))
}

fn primitive(value: &Value) -> String {
	match value {
		Value::Null => "null".to_string(),
		Value::Bool(flag) => flag.to_string(),
		Value::Number(number) => number_to_string(number.as_f64().unwrap_or(f64::NAN)),
		Value::String(text) => quote(text),
		_ => unreachable!("a primitive is not a container"),
	}
}

fn inline(value: &Value) -> Option<String> {
	match value {
		Value::Array(items) if !items.iter().any(is_container) => {
			let parts: Vec<String> = items.iter().map(primitive).collect();
			Some(format!("[{}]", parts.join(", ")))
		}
		Value::Object(entries) if !entries.values().any(is_container) => {
			if entries.is_empty() {
				return Some("{}".to_string());
			}
			let parts: Vec<String> = entries
				.iter()
				.map(|(key, item)| format!("{}: {}", quote(key), primitive(item)))
				.collect();
			Some(format!("{{ {} }}", parts.join(", ")))
		}
		_ => None,
	}
}

fn write(value: &Value, depth: usize) -> String {
	if !is_container(value) {
		return primitive(value);
	}
	if let Some(line) = inline(value)
		&& utf16_len(&line) + depth <= INLINE_LIMIT
	{
		return line;
	}
	let indent = "\t".repeat(depth + 1);
	let close = "\t".repeat(depth);
	match value {
		Value::Array(items) if items.is_empty() => "[]".to_string(),
		Value::Array(items) => {
			let body: Vec<String> = items
				.iter()
				.map(|item| format!("{indent}{}", write(item, depth + 1)))
				.collect();
			format!("[\n{}\n{close}]", body.join(",\n"))
		}
		Value::Object(entries) => {
			let body: Vec<String> = entries
				.iter()
				.map(|(key, item)| format!("{indent}{}: {}", quote(key), write(item, depth + 1)))
				.collect();
			format!("{{\n{}\n{close}}}", body.join(",\n"))
		}
		_ => unreachable!("containers only"),
	}
}
