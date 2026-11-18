// The snippets of the `undertone-dsp` guide page, compiled and run so they stay true.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Run with `cargo run -p undertone-dsp --example dsp_guide`. The regions are what
//! `site/guide/undertone-dsp.md` shows.

use serde_json::json;
use undertone_dsp::params::Patch;
use undertone_dsp::render::{RenderRequest, render_score};
use undertone_dsp::score::{ScoreEvent, ScorePitch, score};

fn main() {
	// #region voice
	// A voice is a patch: Undertone's parameter names, any of them optional.
	let mut voice = Patch::new();
	voice.insert("soundType".into(), json!("triangle"));
	voice.insert("attack".into(), json!(0.005));
	voice.insert("decay".into(), json!(0.3));
	voice.insert("sustain".into(), json!(0.4));
	voice.insert("release".into(), json!(0.2));

	// A score is events at 12 ticks a step. This one plays A4 for half a second at 96 ticks a second.
	let events = [ScoreEvent {
		tick: 0.0,
		pitch: ScorePitch::Midi(69.0),
		gate: 48.0,
		level: Some(0.8),
		pan: Some(0.0),
		voice,
		source: 0,
	}];
	let one_pass = score(96.0, 96.0, &events).expect("a valid score");

	// Render it offline: stereo, 48 kHz, through the master bus, with a short tail.
	let request = RenderRequest {
		tail: 0.5,
		..RenderRequest::EXPORT
	};
	let sound = render_score(&one_pass, &request).expect("a render");
	assert!(sound.peak() > 0.01, "the note made a sound");
	// #endregion voice

	println!("{} frames, peak {:.3}", sound.frames(), sound.peak());
}
