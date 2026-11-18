// A voice's parameters: Undertone's defaults, and a partial patch filled out to a full set
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! `DEFAULT_PARAMS` and `resolveParams` from `engine.ts` in `@liminal-hq/undertone`. A patch (a
//! `ControlPatch`) is the plain data a preset, a song's overrides and a score event carry: field
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
    Sine,
    Square,
    Sawtooth,
    Triangle,
    HalfSine,
    AbsSine,
    PulseSine,
    White,
    Pink,
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
    Linear,
    Exponential,
}

/// A full set of voice parameters, `VoiceParams`.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub sound_type: SoundType,
    pub pitch: Option<Pitch>,
    pub sample_name: Option<String>,
    pub sample_bank: Option<String>,
    pub gain_level: f64,
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
    pub filter_cutoff: Option<f64>,
    pub filter_env_amount: f64,
    pub filter_attack: f64,
    pub filter_decay: f64,
    pub filter_sustain: f64,
    pub filter_release: f64,
    pub hpf_cutoff: Option<f64>,
    pub phaser_rate: Option<f64>,
    pub room_level: Option<f64>,
    pub room_size: f64,
    pub delay_level: Option<f64>,
    pub delay_time: f64,
    pub delay_feedback: f64,
    pub orbit: Option<f64>,
    pub slide_time: f64,
    pub pitch_env_amount: Option<f64>,
    pub pitch_attack: Option<f64>,
    pub pitch_decay: Option<f64>,
    pub pitch_steps: Option<Vec<f64>>,
    pub pitch_step_rate: Option<f64>,
    pub fm_index: Option<f64>,
    pub fm_harmonicity: Option<f64>,
    pub fm_attack: Option<f64>,
    pub fm_decay: Option<f64>,
    pub fm_sustain: Option<f64>,
    pub fm_release: Option<f64>,
    pub fm_env_curve: Option<Curve>,
    pub fm_wave: Option<SoundType>,
    pub nudge_time: f64,
    pub pan: Option<f64>,
    pub channel_gains: Option<Vec<f64>>,
    pub duration: Option<f64>,
    pub legato: Option<f64>,
    pub velocity: Option<f64>,
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
