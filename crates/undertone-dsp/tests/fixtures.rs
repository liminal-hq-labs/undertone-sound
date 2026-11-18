// Checks the recorded Undertone graph against `fixtures/undertone/graph.json`, which the TypeScript wrote
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Each case in the file holds a score (one line an event, each distinct voice once) and the graph
//! Undertone's TypeScript booked for it on a recording context. The test reads the score back,
//! books it on a `LogGraph` and writes the graph as the recorder does; the whole graph must come
//! out byte for byte. When it does not, the test names the first case and the line that differs.

use std::fs;

use serde_json::Value;
use undertone_dsp::json::Json;
use undertone_dsp::log::LogGraph;
use undertone_dsp::render::{RenderRequest, book_score};

mod common;
use common::{fixture, read_score};

/// The recorded graph's text for a case, and the text the port writes for the same score.
fn both_graphs(case: &Value) -> (String, String) {
	let recorded = &case["render"];
	let request = RenderRequest {
		tail: recorded["tail"].as_f64().unwrap(),
		channels: recorded["channels"].as_f64().unwrap() as u32,
		seed: recorded["seed"].as_f64().unwrap(),
		master: recorded["master"].as_bool().unwrap(),
		..RenderRequest::EXPORT
	};
	let graph = book_score(&read_score(case), &request, |init| {
		LogGraph::new(init.channels, init.length, init.sample_rate)
	})
	.unwrap_or_else(|e| panic!("{}: {e}", case["id"]));
	(
		Json::from_value(&case["graph"]).stable(),
		graph.describe().stable(),
	)
}

/// The first line where two texts differ, with a little context, for a failure message.
fn first_difference(expected: &str, actual: &str) -> String {
	let expected: Vec<&str> = expected.lines().collect();
	let actual: Vec<&str> = actual.lines().collect();
	for (index, (e, a)) in expected.iter().zip(&actual).enumerate() {
		if e != a {
			let from = index.saturating_sub(8);
			return format!(
				"line {}:\n  expected: {e}\n  actual:   {a}\ncontext:\n{}",
				index + 1,
				expected[from..=index].join("\n")
			);
		}
	}
	format!(
		"lengths differ: {} lines expected, {} written",
		expected.len(),
		actual.len()
	)
}

#[test]
fn the_recorded_graph_matches_the_typescript() {
	let text = fs::read_to_string(fixture()).unwrap();
	let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
	assert!(cases.len() >= 3, "the fixture has its cases");
	for case in &cases {
		let (expected, actual) = both_graphs(case);
		assert!(
			expected == actual,
			"case {} differs at {}",
			case["id"],
			first_difference(&expected, &actual)
		);
	}
}
