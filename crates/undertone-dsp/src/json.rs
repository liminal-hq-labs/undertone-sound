// A JSON value written the way the golden fixtures are, so a recording compares byte for byte
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `stableJson` from `tools/fixtures/lib.ts`: tabs, a trailing newline, an object's keys sorted, a
//! flat object or array on one line while it fits in 200 characters with its indent, and numbers
//! written as JavaScript's `String(n)` does. `serde_json` cannot be told to do either, and whether
//! its map sorts depends on features other crates turn on, so this is a small tree of its own.

use fizz_audio_song::js::{number_to_string, quote};
use serde_json::Value;

/// A JSON value. An object keeps its keys in the order they were added; the writer sorts them.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// A flat value or line longer than this goes on lines of its own.
const INLINE_LIMIT: usize = 200;

impl Json {
    /// An object from `(key, value)` pairs.
    pub fn object<K: Into<String>>(entries: impl IntoIterator<Item = (K, Json)>) -> Json {
        Json::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// A string.
    pub fn text(text: impl Into<String>) -> Json {
        Json::String(text.into())
    }

    /// A `serde_json` value, keeping its key order.
    pub fn from_value(value: &Value) -> Json {
        match value {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Number(n) => Json::Number(n.as_f64().unwrap_or(f64::NAN)),
            Value::String(s) => Json::String(s.clone()),
            Value::Array(items) => Json::Array(items.iter().map(Json::from_value).collect()),
            Value::Object(map) => Json::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), Json::from_value(v)))
                    .collect(),
            ),
        }
    }

    /// `JSON.stringify(value)`: compact, keys in their own order.
    pub fn compact(&self) -> String {
        match self {
            Json::Null => "null".into(),
            Json::Bool(b) => b.to_string(),
            Json::Number(n) => number(*n),
            Json::String(s) => quote(s),
            Json::Array(items) => {
                let inner: Vec<String> = items.iter().map(Json::compact).collect();
                format!("[{}]", inner.join(","))
            }
            Json::Object(entries) => {
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}:{}", quote(k), v.compact()))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    /// `stableJson(value)`: the fixture's layout, with a trailing newline.
    pub fn stable(&self) -> String {
        format!("{}\n", write(self, 0))
    }

    fn is_container(&self) -> bool {
        matches!(self, Json::Array(_) | Json::Object(_))
    }
}

/// A number as `JSON.stringify` writes it: `String(n)`, and `null` for what JSON cannot hold.
fn number(value: f64) -> String {
    if value.is_finite() {
        number_to_string(value)
    } else {
        "null".into()
    }
}

/// `JSON.stringify` of a string, with the characters a reader cannot see written as escapes.
fn string(text: &str) -> String {
    quote(text)
        .chars()
        .map(|c| match c as u32 {
            0x7f..=0xa0 | 0x200b..=0x200f | 0x2028 | 0x2029 | 0xfeff => {
                format!("\\u{:04x}", c as u32)
            }
            _ => c.to_string(),
        })
        .collect()
}

fn sorted(entries: &[(String, Json)]) -> Vec<&(String, Json)> {
    let mut sorted: Vec<&(String, Json)> = entries.iter().collect();
    // JavaScript compares strings by UTF-16 code units; the fixtures' keys are ASCII.
    sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    sorted
}

fn write(value: &Json, depth: usize) -> String {
    match value {
        Json::Null | Json::Bool(_) | Json::Number(_) => value.compact(),
        Json::String(s) => string(s),
        Json::Array(items) => {
            if items.iter().all(|item| !item.is_container()) {
                let inner: Vec<String> = items.iter().map(|v| write(v, 0)).collect();
                let text = format!("[{}]", inner.join(", "));
                if text.chars().count() + depth <= INLINE_LIMIT {
                    return text;
                }
            }
            if items.is_empty() {
                return "[]".into();
            }
            let indent = "\t".repeat(depth + 1);
            let lines: Vec<String> = items
                .iter()
                .map(|v| format!("{indent}{}", write(v, depth + 1)))
                .collect();
            format!("[\n{}\n{}]", lines.join(",\n"), "\t".repeat(depth))
        }
        Json::Object(entries) => {
            let entries = sorted(entries);
            if entries.iter().all(|(_, v)| !v.is_container()) {
                let text = if entries.is_empty() {
                    "{}".to_string()
                } else {
                    let inner: Vec<String> = entries
                        .iter()
                        .map(|(k, v)| format!("{}: {}", quote(k), write(v, 0)))
                        .collect();
                    format!("{{ {} }}", inner.join(", "))
                };
                if text.chars().count() + depth <= INLINE_LIMIT {
                    return text;
                }
            }
            if entries.is_empty() {
                return "{}".into();
            }
            let indent = "\t".repeat(depth + 1);
            let lines: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{indent}{}: {}", quote(k), write(v, depth + 1)))
                .collect();
            format!("{{\n{}\n{}}}", lines.join(",\n"), "\t".repeat(depth))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_values_stay_on_one_line_and_keys_sort() {
        let value = Json::object([
            ("b", Json::Number(2.0)),
            ("a", Json::Array(vec![Json::text("x"), Json::Number(0.5)])),
        ]);
        assert_eq!(value.stable(), "{\n\t\"a\": [\"x\", 0.5],\n\t\"b\": 2\n}\n");
        assert_eq!(
            Json::object([("b", Json::Number(1e21)), ("a", Json::Null)]).stable(),
            "{ \"a\": null, \"b\": 1e+21 }\n"
        );
    }

    #[test]
    fn a_long_flat_array_breaks_into_lines() {
        let long = Json::Array((0..60).map(|n| Json::Number(f64::from(n))).collect());
        assert!(long.stable().starts_with("[\n\t0,\n\t1,"));
        assert_eq!(Json::Array(vec![]).stable(), "[]\n");
    }

    #[test]
    fn compact_keeps_key_order() {
        let value = Json::object([("b", Json::Number(-0.0)), ("a", Json::Bool(true))]);
        assert_eq!(value.compact(), "{\"b\":0,\"a\":true}");
    }
}
