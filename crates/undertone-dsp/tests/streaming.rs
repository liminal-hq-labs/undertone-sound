// Checks that the streaming engine gives the same samples in blocks of any size
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `Engine::process` is meant to be called from an audio callback, which asks for a block at a
//! time. Whatever the block size, the samples must be the ones an offline render writes. The
//! scores are the recorded ones, which between them use every node kind Undertone books (FM,
//! filters, the phaser, noise, the panner, the merger, the reverb, the echo and the master bus).

use std::fs;

use serde_json::Value;
use undertone_dsp::render::{RenderRequest, book_score};
use undertone_dsp::render_graph::RenderGraph;

mod common;
use common::{fixture, read_score};

#[test]
fn blocks_of_any_size_give_the_offline_render() {
	let text = fs::read_to_string(fixture()).unwrap();
	let cases: Vec<Value> = serde_json::from_str(&text).unwrap();
	for case in &cases {
		let id = case["id"].as_str().unwrap();
		let graph = book_score(&read_score(case), &RenderRequest::EXPORT, |init| {
			RenderGraph::new(init.channels, init.length, init.sample_rate)
		})
		.unwrap();
		let offline = graph.render().unwrap();
		assert!(
			offline.iter().flatten().any(|sample| sample.abs() > 0.0),
			"{id} made a sound"
		);
		for block in [100usize, 128, 480, 1024] {
			let mut engine = graph.clone().engine().unwrap();
			let length = engine.length();
			let mut left = vec![0f32; length];
			let mut right = vec![0f32; length];
			let mut at = 0;
			while at < length {
				let end = (at + block).min(length);
				engine.process(&mut left[at..end], &mut right[at..end]);
				at = end;
			}
			assert!(
				offline[0]
					.iter()
					.zip(&left)
					.all(|(a, b)| a.to_bits() == b.to_bits()),
				"{id}: the left channel differs in blocks of {block}"
			);
			assert!(
				offline[1]
					.iter()
					.zip(&right)
					.all(|(a, b)| a.to_bits() == b.to_bits()),
				"{id}: the right channel differs in blocks of {block}"
			);
		}
	}
}
