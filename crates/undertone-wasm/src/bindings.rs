// The `wasm-bindgen` exports: thin wrappers that hand the crate's Rust logic to JavaScript.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Every function that can fail returns `Result<_, String>`, which JavaScript sees as a thrown
//! string with a readable message.

use wasm_bindgen::prelude::wasm_bindgen;

use crate::opx::{self, Format};
use crate::player::OplPlayer;
use crate::recorded::{self, RecordedScore};
use crate::voice;

/// What an OPX stream is: the answers the demo page shows.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct Summary {
	/// `OPL2`, `OPL3` or `PC speaker`.
	#[wasm_bindgen(getter_with_clone)]
	pub chip: String,
	/// Ticks a second.
	pub grid_hz: u32,
	/// The length in ticks.
	pub total_ticks: u32,
	/// The length in seconds.
	pub seconds: f64,
	/// The number of register writes.
	pub writes: u32,
	/// Whether the stream passes every rule of the specification.
	pub valid: bool,
	/// Why it does not, or an empty string.
	#[wasm_bindgen(getter_with_clone)]
	pub error: String,
	/// Whether the header names a loop point.
	pub has_loop: bool,
	/// The command stream's length in bytes.
	pub stream_len: u32,
	/// The file's length in bytes.
	pub file_len: u32,
}

/// Guesses a file's format: `opx`, `imf0`, `imf1`, `dro` or `vgm`.
#[wasm_bindgen(js_name = detectFormat)]
pub fn detect_format(bytes: &[u8]) -> String {
	match opx::detect_format(bytes) {
		Format::Opx => "opx",
		Format::Imf0 => "imf0",
		Format::Imf1 => "imf1",
		Format::Dro => "dro",
		Format::Vgm => "vgm",
	}
	.to_string()
}

/// Imports IMF, DRO or VGM bytes as an OPX file. `format` is what `detectFormat` returns, and
/// `grid_hz` is the rate an IMF song was written for (560, 700 or 280); zero means 560.
#[wasm_bindgen(js_name = importToOpx)]
pub fn import_to_opx(bytes: &[u8], format: &str, grid_hz: u32) -> Result<Vec<u8>, String> {
	let format = match format {
		"opx" => Format::Opx,
		"imf0" => Format::Imf0,
		"imf1" => Format::Imf1,
		"dro" => Format::Dro,
		"vgm" => Format::Vgm,
		other => {
			return Err(format!(
				"Unknown format \"{other}\": use opx, imf0, imf1, dro or vgm"
			));
		}
	};
	let grid_hz = if grid_hz == 0 {
		opx::DEFAULT_IMF_GRID_HZ
	} else {
		grid_hz
	};
	opx::import(bytes, format, grid_hz)
}

/// Validates an OPX file and summarizes it. A file that is not valid still returns a summary, with
/// `valid` false and the reason in `error`, when its header reads.
#[wasm_bindgen]
pub fn summarize(opx: &[u8]) -> Result<Summary, String> {
	let stats = opx::summarize(opx)?;
	Ok(Summary {
		chip: stats.chip.to_string(),
		grid_hz: stats.grid_hz,
		total_ticks: stats.total_ticks,
		seconds: stats.seconds,
		writes: stats.writes,
		valid: stats.valid,
		error: stats.error,
		has_loop: stats.has_loop,
		stream_len: stats.stream_len,
		file_len: stats.file_len,
	})
}

/// The header as a table: offset, size, bytes, field and value on each line.
#[wasm_bindgen(js_name = headerTable)]
pub fn header_table(opx: &[u8]) -> Result<String, String> {
	opx::header_table(opx)
}

/// The text twin of an OPX file.
#[wasm_bindgen(js_name = textTwin)]
pub fn text_twin(opx: &[u8]) -> Result<String, String> {
	opx::twin(opx)
}

/// The register writes as a flat list of triples: tick, register (`0x100` added for bank 1), value.
#[wasm_bindgen(js_name = registerWrites)]
pub fn register_writes(opx: &[u8]) -> Vec<u32> {
	opx::register_writes(opx)
}

/// Plays an OPX stream through an OPL emulator, a block at a time.
#[wasm_bindgen]
pub struct Player {
	inner: OplPlayer,
}

#[wasm_bindgen]
impl Player {
	/// Prepares to play a valid OPL2 or OPL3 stream at `sample_rate` frames a second.
	#[wasm_bindgen(constructor)]
	pub fn new(opx: &[u8], sample_rate: u32) -> Result<Player, String> {
		Ok(Player {
			inner: OplPlayer::new(opx, sample_rate)?,
		})
	}

	/// Renders `frames` frames as interleaved stereo `f32`.
	pub fn render(&mut self, frames: u32) -> Vec<f32> {
		self.inner.render(frames as usize)
	}

	/// Whether the stream has reached its end; later blocks are the chip's tail.
	pub fn ended(&self) -> bool {
		self.inner.ended()
	}

	/// The stream's length in frames at this sample rate.
	#[wasm_bindgen(js_name = totalFrames)]
	pub fn total_frames(&self) -> f64 {
		self.inner.total_frames() as f64
	}

	/// Sets whether the stream restarts at its loop point.
	#[wasm_bindgen(js_name = setLooping)]
	pub fn set_looping(&mut self, looping: bool) {
		self.inner.set_looping(looping);
	}
}

/// Renders one Undertone voice: `sound` is `sine`, `triangle`, `square` or `sawtooth`, `midi` the
/// note number. Returns interleaved stereo `f32`, with half a second of tail.
#[wasm_bindgen(js_name = renderVoice)]
pub fn render_voice(
	sound: &str,
	midi: f64,
	seconds: f64,
	sample_rate: f64,
) -> Result<Vec<f32>, String> {
	voice::render_note(sound, midi, seconds, sample_rate)
}

/// A recorded Undertone score streamed through `undertone-dsp`'s engine, a block at a time.
#[wasm_bindgen]
pub struct ScorePlayer {
	inner: recorded::ScorePlayer,
	recorded: RecordedScore,
}

#[wasm_bindgen]
impl ScorePlayer {
	/// Reads the case `case_id` from the text of `fixtures/undertone/graph.json` (`title-theme`
	/// is recorded at 112 ticks a second) and books it at `sample_rate` frames a second.
	#[wasm_bindgen(constructor)]
	pub fn new(
		graph_json: &str,
		case_id: &str,
		ticks_per_second: f64,
		sample_rate: u32,
	) -> Result<ScorePlayer, String> {
		let recorded = recorded::read_case(graph_json, case_id, ticks_per_second)?;
		Ok(ScorePlayer {
			inner: recorded::ScorePlayer::new(&recorded, sample_rate)?,
			recorded,
		})
	}

	/// Renders the next `frames` frames as interleaved stereo `f32`; silence past the end.
	pub fn render(&mut self, frames: u32) -> Vec<f32> {
		self.inner.render(frames as usize)
	}

	/// The frames of the pass and its two-second tail.
	#[wasm_bindgen(js_name = totalFrames)]
	pub fn total_frames(&self) -> f64 {
		self.inner.total_frames() as f64
	}

	/// The score's length in seconds, without the tail.
	#[wasm_bindgen(js_name = passSeconds)]
	pub fn pass_seconds(&self) -> f64 {
		self.inner.pass_seconds()
	}

	/// The events of the score.
	pub fn events(&self) -> u32 {
		self.inner.events() as u32
	}

	/// The distinct voices of the score.
	pub fn voices(&self) -> u32 {
		self.inner.voices() as u32
	}

	/// The bars of the song the case holds, or zero when the file does not say.
	pub fn bars(&self) -> u32 {
		self.recorded.bars.unwrap_or(0)
	}
}
