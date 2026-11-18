// A voice's parameters: Undertone's defaults, and a partial patch filled out to a full set
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `DEFAULT_PARAMS` and `resolveParams` from `engine.ts` in `@liminal-hq/undertone`. A patch (a
//! `ControlPatch`) is the plain data a preset, a host's overrides and a score event carry: field
//! names to JSON values, in the order they were set. `resolve_params` reads it into typed
//! parameters. Where the TypeScript would carry a value of the wrong type into its arithmetic (a
//! string where a number belongs), the port takes the default instead; songs that validate never
//! have one.

use indexmap::IndexMap;
use serde_json::Value;

use crate::pitch::Pitch;

/// A partial voice, `ControlPatch`: field names to values, in the order they were set.
pub type Patch = IndexMap<String, Value>;

/// The waveform or noise a voice sounds with (`soundType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundType {
	/// A sine wave.
	Sine,
	/// A square wave.
	Square,
	/// A sawtooth wave.
	Sawtooth,
	/// A triangle wave.
	Triangle,
	/// A sine with its negative half removed.
	HalfSine,
	/// The absolute value of a sine.
	AbsSine,
	/// A sine with its second half of each cycle silent.
	PulseSine,
	/// White noise.
	White,
	/// Pink noise.
	Pink,
	/// Brown noise.
	Brown,
}

impl SoundType {
	/// The type a word names. A word Web Audio does not know plays as a sine, as a browser's
	/// oscillator ignores an unknown `type`.
	pub fn from_word(word: &str) -> SoundType {
		match word {
			"square" => SoundType::Square,
			"sawtooth" => SoundType::Sawtooth,
			"triangle" => SoundType::Triangle,
			"halfsine" => SoundType::HalfSine,
			"abssine" => SoundType::AbsSine,
			"pulsesine" => SoundType::PulseSine,
			"white" => SoundType::White,
			"pink" => SoundType::Pink,
			"brown" => SoundType::Brown,
			_ => SoundType::Sine,
		}
	}

	/// The word for the type, as `soundType` spells it.
	pub fn as_str(self) -> &'static str {
		match self {
			SoundType::Sine => "sine",
			SoundType::Square => "square",
			SoundType::Sawtooth => "sawtooth",
			SoundType::Triangle => "triangle",
			SoundType::HalfSine => "halfsine",
			SoundType::AbsSine => "abssine",
			SoundType::PulseSine => "pulsesine",
			SoundType::White => "white",
			SoundType::Pink => "pink",
			SoundType::Brown => "brown",
		}
	}

	/// White, pink and brown noise play from a buffer, not an oscillator.
	pub fn is_noise(self) -> bool {
		matches!(self, SoundType::White | SoundType::Pink | SoundType::Brown)
	}

	/// The shaped waves are periodic waves (`waves.ts`).
	pub fn is_shaped(self) -> bool {
		matches!(
			self,
			SoundType::HalfSine | SoundType::AbsSine | SoundType::PulseSine
		)
	}
}

/// How an FM envelope ramps (`fmEnvCurve`): anything but `"exp"` ramps linearly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
	/// A linear ramp.
	Linear,
	/// An exponential ramp.
	Exponential,
}

/// A full set of voice parameters, `VoiceParams`.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
	/// The waveform or noise (`soundType`).
	pub sound_type: SoundType,
	/// The pitch (`pitch`).
	pub pitch: Option<Pitch>,
	/// A sample's name (`sampleName`).
	pub sample_name: Option<String>,
	/// A sample's bank (`sampleBank`).
	pub sample_bank: Option<String>,
	/// The peak level (`gainLevel`).
	pub gain_level: f64,
	/// Seconds of attack (`attack`).
	pub attack: f64,
	/// Seconds of decay (`decay`).
	pub decay: f64,
	/// The sustain level as a fraction of the peak (`sustain`).
	pub sustain: f64,
	/// Seconds of release (`release`).
	pub release: f64,
	/// The low-pass cutoff in hertz (`filterCutoff`).
	pub filter_cutoff: Option<f64>,
	/// How far the filter envelope opens the cutoff (`filterEnvAmount`).
	pub filter_env_amount: f64,
	/// Seconds of filter attack (`filterAttack`).
	pub filter_attack: f64,
	/// Seconds of filter decay (`filterDecay`).
	pub filter_decay: f64,
	/// The filter's sustain level (`filterSustain`).
	pub filter_sustain: f64,
	/// Seconds of filter release (`filterRelease`).
	pub filter_release: f64,
	/// The high-pass cutoff in hertz (`hpfCutoff`).
	pub hpf_cutoff: Option<f64>,
	/// The phaser's rate in hertz (`phaserRate`).
	pub phaser_rate: Option<f64>,
	/// The reverb send level (`roomLevel`).
	pub room_level: Option<f64>,
	/// The reverb's size (`roomSize`).
	pub room_size: f64,
	/// The echo send level (`delayLevel`).
	pub delay_level: Option<f64>,
	/// Seconds between echoes (`delayTime`).
	pub delay_time: f64,
	/// How much of each echo is fed back (`delayFeedback`).
	pub delay_feedback: f64,
	/// The effect bus (orbit) it sends to (`orbit`).
	pub orbit: Option<f64>,
	/// Seconds a pitch glide takes (`slideTime`).
	pub slide_time: f64,
	/// Semitones the pitch envelope moves (`pitchEnvAmount`).
	pub pitch_env_amount: Option<f64>,
	/// Seconds of pitch attack (`pitchAttack`).
	pub pitch_attack: Option<f64>,
	/// Seconds of pitch decay (`pitchDecay`).
	pub pitch_decay: Option<f64>,
	/// Semitone offsets the pitch steps through (`pitchSteps`).
	pub pitch_steps: Option<Vec<f64>>,
	/// Steps a second (`pitchStepRate`).
	pub pitch_step_rate: Option<f64>,
	/// The modulation depth (`fmIndex`).
	pub fm_index: Option<f64>,
	/// The modulator's frequency as a multiple of the carrier's (`fmHarmonicity`).
	pub fm_harmonicity: Option<f64>,
	/// Seconds of modulation attack (`fmAttack`).
	pub fm_attack: Option<f64>,
	/// Seconds of modulation decay (`fmDecay`).
	pub fm_decay: Option<f64>,
	/// The modulation sustain level (`fmSustain`).
	pub fm_sustain: Option<f64>,
	/// Seconds of modulation release (`fmRelease`).
	pub fm_release: Option<f64>,
	/// How the modulation envelope ramps (`fmEnvCurve`).
	pub fm_env_curve: Option<Curve>,
	/// The modulator's waveform (`fmWave`).
	pub fm_wave: Option<SoundType>,
	/// Seconds the voice is moved in time (`nudgeTime`).
	pub nudge_time: f64,
	/// Stereo position from -1 to 1 (`pan`).
	pub pan: Option<f64>,
	/// A gain for each output channel, folded to fit the output (`channelGains`).
	pub channel_gains: Option<Vec<f64>>,
	/// Seconds the gate stays open (`duration`); without it the envelope is percussive.
	pub duration: Option<f64>,
	/// Read from a patch as Undertone does (`legato`); the renderer does not use it.
	pub legato: Option<f64>,
	/// How hard it is played (`velocity`).
	pub velocity: Option<f64>,
	/// Voices in one cut group on one orbit cut each other off (`cutGroup`).
	pub cut_group: Option<f64>,
}

/// Undertone's `DEFAULT_PARAMS`: what a voice is before a patch says otherwise.
pub const DEFAULT_PARAMS: Params = Params {
	sound_type: SoundType::Sine,
	pitch: None,
	sample_name: None,
	sample_bank: None,
	gain_level: 0.8,
	attack: 0.01,
	decay: 0.1,
	sustain: 0.0,
	release: 0.05,
	filter_cutoff: None,
	filter_env_amount: 0.0,
	filter_attack: 0.0,
	filter_decay: 0.0,
	filter_sustain: 1.0,
	filter_release: 0.0,
	hpf_cutoff: None,
	phaser_rate: None,
	room_level: None,
	room_size: 3.0,
	delay_level: None,
	delay_time: 0.3,
	delay_feedback: 0.35,
	orbit: None,
	slide_time: 0.0,
	pitch_env_amount: None,
	pitch_attack: None,
	pitch_decay: None,
	pitch_steps: None,
	pitch_step_rate: None,
	fm_index: None,
	fm_harmonicity: None,
	fm_attack: None,
	fm_decay: None,
	fm_sustain: None,
	fm_release: None,
	fm_env_curve: None,
	fm_wave: None,
	nudge_time: 0.0,
	pan: None,
	channel_gains: None,
	duration: None,
	legato: None,
	velocity: None,
	cut_group: None,
};

/// A sample voice holds by default, where a synth voice is percussive (`SAMPLE_DEFAULT_SUSTAIN`).
const SAMPLE_DEFAULT_SUSTAIN: f64 = 1.0;

fn number(patch: &Patch, field: &str) -> Option<f64> {
	patch.get(field).and_then(Value::as_f64)
}

fn text(patch: &Patch, field: &str) -> Option<String> {
	patch.get(field).and_then(Value::as_str).map(str::to_string)
}

fn numbers(patch: &Patch, field: &str) -> Option<Vec<f64>> {
	let items = patch.get(field)?.as_array()?;
	Some(
		items
			.iter()
			.map(|v| v.as_f64().unwrap_or(f64::NAN))
			.collect(),
	)
}

/// `resolveParams`: a patch filled out with the defaults. A sample voice with no `sustain` holds.
pub fn resolve_params(patch: &Patch) -> Params {
	let d = DEFAULT_PARAMS;
	let or = |field: &str, fallback: f64| number(patch, field).unwrap_or(fallback);
	let sample_name = text(patch, "sampleName");
	let sustain = match number(patch, "sustain") {
		Some(sustain) => sustain,
		None if sample_name.is_some() => SAMPLE_DEFAULT_SUSTAIN,
		None => d.sustain,
	};
	Params {
		sound_type: text(patch, "soundType").map_or(d.sound_type, |w| SoundType::from_word(&w)),
		pitch: patch.get("pitch").and_then(Pitch::from_value),
		sample_name,
		sample_bank: text(patch, "sampleBank"),
		gain_level: or("gainLevel", d.gain_level),
		attack: or("attack", d.attack),
		decay: or("decay", d.decay),
		sustain,
		release: or("release", d.release),
		filter_cutoff: number(patch, "filterCutoff"),
		filter_env_amount: or("filterEnvAmount", d.filter_env_amount),
		filter_attack: or("filterAttack", d.filter_attack),
		filter_decay: or("filterDecay", d.filter_decay),
		filter_sustain: or("filterSustain", d.filter_sustain),
		filter_release: or("filterRelease", d.filter_release),
		hpf_cutoff: number(patch, "hpfCutoff"),
		phaser_rate: number(patch, "phaserRate"),
		room_level: number(patch, "roomLevel"),
		room_size: or("roomSize", d.room_size),
		delay_level: number(patch, "delayLevel"),
		delay_time: or("delayTime", d.delay_time),
		delay_feedback: or("delayFeedback", d.delay_feedback),
		orbit: number(patch, "orbit"),
		slide_time: or("slideTime", d.slide_time),
		pitch_env_amount: number(patch, "pitchEnvAmount"),
		pitch_attack: number(patch, "pitchAttack"),
		pitch_decay: number(patch, "pitchDecay"),
		pitch_steps: numbers(patch, "pitchSteps"),
		pitch_step_rate: number(patch, "pitchStepRate"),
		fm_index: number(patch, "fmIndex"),
		fm_harmonicity: number(patch, "fmHarmonicity"),
		fm_attack: number(patch, "fmAttack"),
		fm_decay: number(patch, "fmDecay"),
		fm_sustain: number(patch, "fmSustain"),
		fm_release: number(patch, "fmRelease"),
		fm_env_curve: text(patch, "fmEnvCurve").map(|w| {
			if w == "exp" {
				Curve::Exponential
			} else {
				Curve::Linear
			}
		}),
		fm_wave: text(patch, "fmWave").map(|w| SoundType::from_word(&w)),
		nudge_time: or("nudgeTime", d.nudge_time),
		pan: number(patch, "pan"),
		channel_gains: numbers(patch, "channelGains"),
		duration: number(patch, "duration"),
		legato: number(patch, "legato"),
		velocity: number(patch, "velocity"),
		cut_group: number(patch, "cutGroup"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn patch(value: Value) -> Patch {
		serde_json::from_value(value).unwrap()
	}

	#[test]
	fn an_empty_patch_is_the_defaults() {
		assert_eq!(resolve_params(&Patch::new()), DEFAULT_PARAMS);
	}

	#[test]
	fn a_patch_wins_over_the_defaults() {
		let params = resolve_params(&patch(json!({
			"soundType": "sawtooth", "attack": 0.2, "filterCutoff": 900, "pitch": "a2", "fmEnvCurve": "lin",
			"pitchSteps": [12, 0]
		})));
		assert_eq!(params.sound_type, SoundType::Sawtooth);
		assert_eq!(params.attack, 0.2);
		assert_eq!(params.decay, 0.1);
		assert_eq!(params.filter_cutoff, Some(900.0));
		assert_eq!(params.pitch, Some(Pitch::Name("a2".into())));
		assert_eq!(params.fm_env_curve, Some(Curve::Linear));
		assert_eq!(params.pitch_steps, Some(vec![12.0, 0.0]));
	}

	#[test]
	fn a_sample_voice_holds_unless_told_otherwise() {
		assert_eq!(
			resolve_params(&patch(json!({ "sampleName": "x" }))).sustain,
			1.0
		);
		assert_eq!(
			resolve_params(&patch(json!({ "sampleName": "x", "sustain": 0.2 }))).sustain,
			0.2
		);
	}

	#[test]
	fn an_unknown_wave_plays_as_a_sine() {
		assert_eq!(SoundType::from_word("buzz"), SoundType::Sine);
		assert!(SoundType::Pink.is_noise());
		assert!(SoundType::AbsSine.is_shaped());
	}
}
