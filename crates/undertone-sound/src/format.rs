// Writes a song (or any document in the song's style) as stable, readable JSON
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A song written the same way every time, so a diff shows only what changed.

use serde_json::Value;

use crate::model::Song;
use undertone_mini::js::{number_to_string, quote, utf16_len};

const INLINE_LIMIT: usize = 90;

/// Formats a song the same way every time, so a diff shows only what changed. Small flat objects
/// and arrays (a note, a key, the arrangement) stay on one line; everything else is indented with
/// tabs. Key order follows the document.
pub fn format_song(song: &Song) -> String {
	format_document(&serde_json::to_value(song).expect("a song is always JSON"))
}

/// Formats any JSON document in the song file's stable style: tabs, with each small flat object on one line. Reads
/// keys in the order the value holds them.
pub fn format_document(value: &Value) -> String {
	format!("{}\n", format_value(value, 0))
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
		Value::Array(_) | Value::Object(_) => unreachable!("a primitive is not a container"),
	}
}

fn format_value(value: &Value, depth: usize) -> String {
	if !is_container(value) {
		return primitive(value);
	}
	if let Some(inline) = inline_form(value)
		&& utf16_len(&inline) + depth <= INLINE_LIMIT
	{
		return inline;
	}
	let indent = "\t".repeat(depth + 1);
	let close = "\t".repeat(depth);
	match value {
		Value::Array(items) => {
			if items.is_empty() {
				return "[]".to_string();
			}
			let body: Vec<String> = items
				.iter()
				.map(|item| format!("{indent}{}", format_value(item, depth + 1)))
				.collect();
			format!("[\n{}\n{close}]", body.join(",\n"))
		}
		Value::Object(entries) => {
			if entries.is_empty() {
				return "{}".to_string();
			}
			let body: Vec<String> = entries
				.iter()
				.map(|(key, item)| {
					format!("{indent}{}: {}", quote(key), format_value(item, depth + 1))
				})
				.collect();
			format!("{{\n{}\n{close}}}", body.join(",\n"))
		}
		_ => unreachable!("containers only"),
	}
}

/// One-line form for an object or array whose members are all primitives.
fn inline_form(value: &Value) -> Option<String> {
	match value {
		Value::Array(items) => {
			if items.iter().any(is_container) {
				return None;
			}
			let parts: Vec<String> = items.iter().map(primitive).collect();
			Some(format!("[{}]", parts.join(", ")))
		}
		Value::Object(entries) => {
			if entries.values().any(is_container) {
				return None;
			}
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
