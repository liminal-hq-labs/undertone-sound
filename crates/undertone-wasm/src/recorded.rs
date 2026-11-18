// Reads a recorded Undertone score and plays it through `undertone-dsp`'s streaming engine.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `fixtures/undertone/graph.json` and `fixtures/undertone/scores/title-theme.score.json` hold, for each case, the score Undertone's TypeScript booked
//! (one line an event, each distinct voice listed once) beside the graph it made. The browser
//! demo reads the same score the native tests read, books it on the in-house renderer and streams
//! it a block at a time, so the Undertone version of Title Theme is the recorded one and nothing
//! is copied into the site.
//!
//! `fixtures/undertone/scores/title-theme.score.json` is the whole rich Title Theme, events only,
//! with no graph beside it. Its extra header fields (`bars`, `input`, `lengthTicks`, `render`) are
//! not needed to play it: the render is the export's, and the length is the recorded seconds at
//! the tick rate. A drum layer's `note c1` is a note name the engine resolves.
//!
//! The files do not record the tick rate. It is the tempo times four steps a beat times twelve
//! ticks a step, over 60 seconds, and the caller supplies it: 112 for both Title Theme cases
//! (140 bpm).

use serde_json::Value;
use undertone_dsp::params::Patch;
use undertone_dsp::pitch::Pitch;
use undertone_dsp::render::{RenderRequest, book_score_render};
use undertone_dsp::render_graph::Engine;
use undertone_dsp::score::{Score, ScoreEvent, ScorePitch, score};

/// The ticks a second `title-theme` was recorded at: 140 bpm, four steps a beat, twelve ticks a step.
pub const TITLE_THEME_TICKS_PER_SECOND: f64 = 112.0;

/// The case of `fixtures/undertone/scores/title-theme.score.json`: the rich Title Theme, whole.
pub const FULL_ID: &str = "title-theme-full";

/// A case of the recorded file, read back.
#[derive(Debug, Clone)]
pub struct RecordedScore {
	/// The compiled score, one pass.
	pub score: Score,
	/// How many events the score holds.
	pub events: usize,
	/// How many distinct voices they play.
	pub voices: usize,
	/// How many bars of the song the case holds, when the file says.
	pub bars: Option<u32>,
	/// What the case is, in the recorder's words.
	pub from: String,
}

/// One `tick … voice vN` line as an event of the score.
fn parse_event(line: &str, voices: &[Patch], source: usize) -> Result<ScoreEvent, String> {
	let mut tick = None;
	let mut pitch = None;
	let mut gate = None;
	let mut level = None;
	let mut pan = None;
	let mut voice = None;
	let mut words = line.split(' ');
	while let Some(key) = words.next() {
		let value = words
			.next()
			.ok_or_else(|| format!("The event \"{line}\" ends at \"{key}\""))?;
		let number = || {
			value
				.parse::<f64>()
				.map_err(|_| format!("The event \"{line}\" has \"{value}\" where a number belongs"))
		};
		match key {
			"tick" => tick = Some(number()?),
			"midi" => pitch = Some(ScorePitch::Midi(number()?)),
			"note" => {
				let note = match value.parse::<f64>() {
					Ok(hz) => Pitch::Hz(hz),
					Err(_) => Pitch::Name(value.to_string()),
				};
				pitch = Some(ScorePitch::Note(note));
			}
			"gate" => gate = Some(number()?),
			"level" => level = Some(number()?),
			"pan" => pan = Some(number()?),
			"voice" => {
				let index: usize = value
					.trim_start_matches('v')
					.parse()
					.map_err(|_| format!("The event \"{line}\" names the voice \"{value}\""))?;
				voice = Some(
					voices
						.get(index)
						.ok_or_else(|| {
							format!("The event \"{line}\" names a voice the case lacks")
						})?
						.clone(),
				);
			}
			other => {
				return Err(format!(
					"The event \"{line}\" has an unknown field \"{other}\""
				));
			}
		}
	}
	let missing = |name: &str| format!("The event \"{line}\" has no {name}");
	Ok(ScoreEvent {
		tick: tick.ok_or_else(|| missing("tick"))?,
		pitch: pitch.ok_or_else(|| missing("pitch"))?,
		gate: gate.ok_or_else(|| missing("gate"))?,
		level,
		pan,
		voice: voice.ok_or_else(|| missing("voice"))?,
		source,
	})
}

/// Reads the case `id` of `fixtures/undertone/graph.json` (the whole file's text) as a score at
/// `ticks_per_second`. The pass lasts the seconds the case recorded.
pub fn read_case(text: &str, id: &str, ticks_per_second: f64) -> Result<RecordedScore, String> {
	let cases: Value = serde_json::from_str(text)
		.map_err(|error| format!("The score file is not JSON: {error}"))?;
	let case = cases
		.as_array()
		.and_then(|all| all.iter().find(|case| case["id"] == id))
		.ok_or_else(|| format!("The score file has no case named \"{id}\""))?;
	let recorded = &case["score"];
	let voices: Vec<Patch> = recorded["voices"]
		.as_array()
		.ok_or("The case has no voices")?
		.iter()
		.map(|voice| {
			voice
				.as_object()
				.map(|fields| {
					fields
						.iter()
						.filter(|(name, _)| name.as_str() != "id")
						.map(|(name, value)| (name.clone(), value.clone()))
						.collect::<Patch>()
				})
				.ok_or_else(|| "A voice of the case is not an object".to_string())
		})
		.collect::<Result<_, _>>()?;
	let events: Vec<ScoreEvent> = recorded["events"]
		.as_array()
		.ok_or("The case has no events")?
		.iter()
		.enumerate()
		.map(|(source, line)| {
			parse_event(
				line.as_str().ok_or("An event is not a line of text")?,
				&voices,
				source,
			)
		})
		.collect::<Result<_, _>>()?;
	let seconds = recorded["seconds"]
		.as_f64()
		.ok_or("The case does not say how long it lasts")?;
	let length_ticks = (seconds * ticks_per_second).round();
	let compiled =
		score(ticks_per_second, length_ticks, &events).map_err(|error| error.to_string())?;
	Ok(RecordedScore {
		score: compiled,
		events: events.len(),
		voices: voices.len(),
		bars: case["bars"]
			.as_u64()
			.and_then(|bars| u32::try_from(bars).ok()),
		from: case["from"].as_str().unwrap_or_default().to_string(),
	})
}

/// A score booked and prepared to stream: stereo, through the master bus, with the export's
/// two-second tail.
pub struct ScorePlayer {
	engine: Engine,
	sample_rate: u32,
	/// The pass in seconds, without the tail.
	pass_seconds: f64,
	events: usize,
	voices: usize,
}

impl ScorePlayer {
	/// Books `recorded` at `sample_rate` and prepares the engine. Every allocation happens here.
	pub fn new(recorded: &RecordedScore, sample_rate: u32) -> Result<Self, String> {
		if sample_rate == 0 {
			return Err("The sample rate must be above zero".to_string());
		}
		let request = RenderRequest {
			sample_rate: f64::from(sample_rate),
			..RenderRequest::EXPORT
		};
		let (graph, _) =
			book_score_render(&recorded.score, &request).map_err(|error| error.to_string())?;
		let engine = graph.engine().map_err(|error| error.to_string())?;
		Ok(Self {
			engine,
			sample_rate,
			pass_seconds: recorded.score.seconds,
			events: recorded.events,
			voices: recorded.voices,
		})
	}

	/// Renders the next `frames` frames as interleaved stereo `f32`. Past the end it is silence.
	pub fn render(&mut self, frames: usize) -> Vec<f32> {
		let mut out = vec![0f32; frames * 2];
		self.engine.process_interleaved(&mut out);
		out
	}

	/// The frames of the pass and its tail.
	pub fn total_frames(&self) -> usize {
		self.engine.length()
	}

	/// The sample rate it was booked at.
	pub fn sample_rate(&self) -> u32 {
		self.sample_rate
	}

	/// The score's pass in seconds, before the tail.
	pub fn pass_seconds(&self) -> f64 {
		self.pass_seconds
	}

	/// The events of the score.
	pub fn events(&self) -> usize {
		self.events
	}

	/// The distinct voices of the score.
	pub fn voices(&self) -> usize {
		self.voices
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const GRAPH: &str = include_str!("../../../fixtures/undertone/graph.json");
	const FULL: &str = include_str!("../../../fixtures/undertone/scores/title-theme.score.json");

	fn peak(samples: &[f32]) -> f32 {
		samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
	}

	#[test]
	fn the_title_theme_case_reads_back_as_a_two_bar_score() {
		let recorded = read_case(GRAPH, "title-theme", TITLE_THEME_TICKS_PER_SECOND).unwrap();
		assert_eq!(recorded.events, 46);
		assert_eq!(recorded.voices, 6);
		assert_eq!(recorded.bars, Some(2));
		assert!((recorded.score.seconds - 3.428_571).abs() < 1e-5);
		assert!(recorded.from.contains("Title Theme"));
	}

	#[test]
	fn it_streams_stereo_sound_the_length_of_the_pass_and_tail() {
		let recorded = read_case(GRAPH, "title-theme", TITLE_THEME_TICKS_PER_SECOND).unwrap();
		let mut player = ScorePlayer::new(&recorded, 48_000).unwrap();
		// 3.43 s of pass and the export's 2 s tail.
		assert!((player.total_frames() as f64 / 48_000.0 - 5.4286).abs() < 0.01);
		let all = player.render(player.total_frames());
		assert_eq!(all.len(), 2 * player.total_frames());
		assert!(peak(&all) > 0.05, "the score is not silent");
		assert!(peak(&all) <= 1.0, "the master bus keeps it in range");
		let (left, right): (Vec<f32>, Vec<f32>) =
			all.as_chunks::<2>().0.iter().map(|[l, r]| (*l, *r)).unzip();
		assert!(peak(&left) > 0.0 && peak(&right) > 0.0);
		// The last half second is the tail's end: quieter than the loudest of the pass.
		assert!(peak(&all[all.len() - 48_000..]) < peak(&all));
	}

	#[test]
	fn blocks_of_any_size_give_the_same_samples() {
		let recorded = read_case(GRAPH, "title-theme", TITLE_THEME_TICKS_PER_SECOND).unwrap();
		let mut whole = ScorePlayer::new(&recorded, 44_100).unwrap();
		let frames = whole.total_frames();
		let all = whole.render(frames);
		let mut parts = ScorePlayer::new(&recorded, 44_100).unwrap();
		let mut joined = Vec::new();
		while joined.len() < all.len() {
			joined.extend(parts.render(1_000.min((all.len() - joined.len()) / 2)));
		}
		assert!(joined == all);
	}

	#[test]
	fn the_other_cases_read_and_bad_input_is_refused_with_a_reason() {
		assert!(read_case(GRAPH, "voices", 96.0).is_ok());
		assert!(
			read_case(GRAPH, "nothing", 96.0)
				.unwrap_err()
				.contains("nothing")
		);
		assert!(read_case("not json", "voices", 96.0).is_err());
		assert!(ScorePlayer::new(&read_case(GRAPH, "voices", 96.0).unwrap(), 0).is_err());
	}

	fn full() -> RecordedScore {
		read_case(FULL, FULL_ID, TITLE_THEME_TICKS_PER_SECOND).unwrap()
	}

	#[test]
	fn the_full_title_theme_reads_back() {
		let recorded = full();
		assert_eq!(recorded.events, 1_560);
		assert_eq!(recorded.voices, 12);
		assert_eq!(recorded.bars, None);
		assert!(recorded.from.contains("rich Title Theme"));
		assert_eq!(recorded.score.length_ticks, 6_912.0);
		assert_eq!(recorded.score.ticks_per_second, 112.0);
		assert!((recorded.score.seconds - 61.714_285).abs() < 1e-5);
	}

	#[test]
	fn the_full_title_theme_renders_the_pass_and_tail_in_range() {
		let mut player = ScorePlayer::new(&full(), 48_000).unwrap();
		assert!((player.total_frames() as f64 / 48_000.0 - 63.7143).abs() < 0.001);
		let all = player.render(player.total_frames());
		let top = peak(&all);
		assert!(top > 0.5 && top < 1.0, "peak {top}");
		let (left, right): (Vec<f32>, Vec<f32>) =
			all.as_chunks::<2>().0.iter().map(|[l, r]| (*l, *r)).unzip();
		assert!(peak(&left) > 0.1 && peak(&right) > 0.1);
		// The tail has died away by its end.
		assert!(peak(&all[all.len() - 48_000..]) < 0.01);
	}

	#[test]
	fn the_full_title_theme_matches_the_studios_level() {
		// The studio's in-house render reports a peak of -2.0 dBFS and an RMS of -18.1 dBFS.
		let mut player = ScorePlayer::new(&full(), 48_000).unwrap();
		let all = player.render(player.total_frames());
		let db = |linear: f64| 20.0 * linear.log10();
		let top = db(f64::from(peak(&all)));
		let mean_square = all.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / all.len() as f64;
		let rms = db(mean_square.sqrt());
		eprintln!("measured peak {top:.3} dBFS rms {rms:.3} dBFS");
		assert!((top + 2.0).abs() < 0.2, "peak {top:.2} dBFS");
		assert!((rms + 18.1).abs() < 0.5, "rms {rms:.2} dBFS");
	}

	#[test]
	fn the_full_title_theme_is_the_same_in_blocks_of_any_size() {
		let recorded = full();
		let mut whole = ScorePlayer::new(&recorded, 44_100).unwrap();
		let frames = whole.total_frames();
		let all = whole.render(frames);
		for block in [128, 4_096] {
			let mut parts = ScorePlayer::new(&recorded, 44_100).unwrap();
			let mut joined = Vec::with_capacity(all.len());
			while joined.len() < all.len() {
				joined.extend(parts.render(block.min((all.len() - joined.len()) / 2)));
			}
			assert!(joined == all, "block size {block}");
		}
	}
}
