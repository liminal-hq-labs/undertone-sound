// The three chip engines behind one interface: info, placement, parameters, sequencing and rendering
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! What `opl/engine.ts` and `speaker/engine.ts` offer outside the browser: the descriptions,
//! where a pan lands, voice parameters, the one-note preview, and rendering a pass or a section
//! to samples. Playing the samples is the app's job.

use fizz_audio_song::compile::{compile_song, CompileOptions, Performance, PerformanceEvent};
use fizz_audio_song::model::{DrumPiece, EngineId, Mix, Song};
use fizz_audio_song::music::parse_pitch;
use serde_json::{Map, Value};

use crate::chip::{render_sequence, RenderOptions, RenderedSound};
use crate::params::{
    opl_voice_params, opl_voice_values, speaker_values, speaker_voice_params, VoiceParam,
};
use crate::sequence::{melodic_channels, sequence_opl, OplChip, OplSequence};
use crate::speaker::render::render_speaker;
use crate::speaker::sequence::sequence_speaker;
use crate::stereo::{opl_placement, DEFAULT_PAN_LAW};

/// The engines this crate plays. Undertone is the browser's synth and has no chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipEngine {
    Opl2,
    Opl3,
    PcSpeaker,
}

impl ChipEngine {
    /// The chip engine an engine id names, or `None` for Undertone.
    pub fn from_id(id: EngineId) -> Option<ChipEngine> {
        match id {
            EngineId::Opl2 => Some(ChipEngine::Opl2),
            EngineId::Opl3 => Some(ChipEngine::Opl3),
            EngineId::PcSpeaker => Some(ChipEngine::PcSpeaker),
            EngineId::Undertone => None,
        }
    }

    pub fn id(self) -> EngineId {
        match self {
            ChipEngine::Opl2 => EngineId::Opl2,
            ChipEngine::Opl3 => EngineId::Opl3,
            ChipEngine::PcSpeaker => EngineId::PcSpeaker,
        }
    }

    fn opl(self) -> Option<OplChip> {
        match self {
            ChipEngine::Opl2 => Some(OplChip::Opl2),
            ChipEngine::Opl3 => Some(OplChip::Opl3),
            ChipEngine::PcSpeaker => None,
        }
    }
}

/// What the studio lists for an engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    pub id: EngineId,
    pub name: &'static str,
    pub description: &'static str,
}

pub fn engine_info(engine: ChipEngine) -> EngineInfo {
    match engine {
        ChipEngine::Opl2 => EngineInfo {
            id: EngineId::Opl2,
            name: "OPL2",
            description: "Yamaha YM3812 (AdLib): five two-operator FM channels plus the rhythm section, mono; channel 5 is kept for sound effects.",
        },
        ChipEngine::Opl3 => EngineInfo {
            id: EngineId::Opl3,
            name: "OPL3",
            description: "Yamaha YMF262 (Sound Blaster 16): thirteen FM channels plus rhythm, stereo with off-centre notes on paired channels; channels 5 and 14 are kept for sound effects.",
        },
        ChipEngine::PcSpeaker => EngineInfo {
            id: EngineId::PcSpeaker,
            name: "PC Speaker",
            description: "One square-wave voice: the most important part wins, chords become arpeggios, drums become noise and sweeps.",
        },
    }
}

/// Where an engine puts a track in the stereo field, for the studio to show beside its pan.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// A short readout: `L`, `C` or `R` for a chip that snaps, `L85·R15` for a pair, or `mono`.
    pub label: String,
    /// A sentence on how the engine places sound, for a tooltip.
    pub detail: String,
    /// The pan the engine plays, from -1 to 1, or `None` when everything comes out in mono.
    pub heard: Option<f64>,
}

fn mono_placement(detail: &str) -> Placement {
    Placement {
        label: "mono".to_string(),
        detail: detail.to_string(),
        heard: None,
    }
}

/// How this engine places a track authored at `pan`, under the song's mix settings.
pub fn placement(engine: ChipEngine, pan: f64, mix: Option<&Mix>) -> Placement {
    match engine {
        ChipEngine::Opl2 => {
            mono_placement("The OPL2 is a mono chip, so every track plays in the centre.")
        }
        ChipEngine::PcSpeaker => {
            mono_placement("The PC speaker is one mono speaker, so pan has no effect.")
        }
        ChipEngine::Opl3 => {
            let law = mix.and_then(|m| m.pan_law).unwrap_or(DEFAULT_PAN_LAW);
            let placed = opl_placement(OplChip::Opl3, pan, law);
            // A pair plays the pan as written; one channel can only be left, both sides or right.
            let heard = if placed.channels == 2 {
                pan
            } else if placed.label == "L" {
                -1.0
            } else if placed.label == "R" {
                1.0
            } else {
                0.0
            };
            Placement {
                label: placed.label,
                detail: placed.detail,
                heard: Some(heard),
            }
        }
    }
}

/// The adjustable parameters of an instrument on an engine.
pub fn voice_params(song: &Song, instrument_id: &str, engine: ChipEngine) -> Vec<VoiceParam> {
    match engine.opl() {
        Some(chip) => opl_voice_params(song, instrument_id, chip),
        None => speaker_voice_params(song, instrument_id),
    }
}

/// Every parameter's current value: the song's override, else the preset's, else the engine default.
pub fn voice_values(song: &Song, instrument_id: &str, engine: ChipEngine) -> Map<String, Value> {
    match engine.opl() {
        Some(chip) => opl_voice_values(song, instrument_id, chip),
        None => speaker_values(song, instrument_id),
    }
}

/// The melodic voices an engine has, or `None` for the speaker, which reports no limit.
pub fn capacity(engine: ChipEngine) -> Option<usize> {
    engine.opl().map(|chip| melodic_channels(chip).len())
}

/// The notes an engine cannot play of a performance, with its voice count.
pub fn limits(
    song: &Song,
    performance: &Performance,
    engine: ChipEngine,
) -> Option<(usize, Vec<PerformanceEvent>)> {
    let chip = engine.opl()?;
    Some((
        melodic_channels(chip).len(),
        sequence_opl(song, performance, chip).dropped_events,
    ))
}

/// How to render a performance on any chip engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
    pub sample_rate: u32,
    /// Seconds after the last pass for releases to ring out (the OPL chips; the speaker pads silence).
    pub tail: f64,
    /// Fold the OPL tail back onto the start so the pass loops. The speaker has no tail to fold.
    pub wrap: bool,
}

impl RenderRequest {
    /// The settings the studio uses to render a pass ahead of time, for looping.
    pub const LOOPING: RenderRequest = RenderRequest {
        sample_rate: crate::chip::SAMPLE_RATE,
        tail: crate::chip::LOOP_TAIL,
        wrap: true,
    };
}

/// Renders a compiled performance on an engine.
pub fn render_performance(
    song: &Song,
    performance: &Performance,
    engine: ChipEngine,
    request: RenderRequest,
) -> RenderedSound {
    match engine.opl() {
        Some(chip) => {
            let sequence = sequence_opl(song, performance, chip);
            render_sequence(
                &sequence,
                chip,
                RenderOptions {
                    sample_rate: request.sample_rate,
                    tail: request.tail,
                    wrap: request.wrap,
                },
            )
        }
        None => {
            let mut pass =
                render_speaker(&sequence_speaker(song, performance), request.sample_rate);
            if !request.wrap {
                let length = pass.left.len()
                    + crate::numbers::round(request.tail * f64::from(request.sample_rate)) as usize;
                pass.left.resize(length, 0.0);
                pass.right.clone_from(&pass.left);
            }
            pass
        }
    }
}

/// Compiles a song (the whole arrangement, one entry or one section, as the options say) and
/// renders it on an engine.
pub fn render_song(
    song: &Song,
    options: &CompileOptions,
    engine: ChipEngine,
    request: RenderRequest,
) -> Result<RenderedSound, String> {
    let performance = compile_song(song, options)?;
    Ok(render_performance(song, &performance, engine, request))
}

/// The register writes for a song or a section on an OPL chip, for tools that need the stream.
pub fn sequence_song(
    song: &Song,
    options: &CompileOptions,
    chip: OplChip,
) -> Result<OplSequence, String> {
    Ok(sequence_opl(song, &compile_song(song, options)?, chip))
}

/// A one-note performance on a song's instrument, for auditioning while editing.
pub fn preview_performance(
    song: &Song,
    instrument_id: &str,
    pitch: &str,
    engine: ChipEngine,
) -> Performance {
    let drum = DrumPiece::from_word(pitch);
    let midi = if drum.is_some() {
        None
    } else {
        parse_pitch(pitch)
    };
    let event = |track_id: &str, velocity: f64, pan: f64, len: f64| PerformanceEvent {
        step: 0.0,
        len,
        track_id: track_id.to_string(),
        instrument_id: instrument_id.to_string(),
        pitch: pitch.to_string(),
        midi,
        drum,
        velocity,
        pan,
    };
    if engine == ChipEngine::PcSpeaker {
        return Performance {
            steps_per_second: 8.0,
            length_steps: 4.0,
            sections: Vec::new(),
            events: vec![event("preview", 1.0, 0.0, 2.0)],
        };
    }
    let track = song
        .tracks
        .iter()
        .find(|track| track.instrument == instrument_id);
    let track_id = track.map_or("preview", |track| track.id.as_str());
    let pan = song.find_track(track_id).map_or(0.0, |track| track.pan);
    Performance {
        steps_per_second: 8.0,
        length_steps: 8.0,
        sections: Vec::new(),
        events: vec![event(track_id, 0.8, pan, 2.0)],
    }
}

/// Renders the one-note preview of an instrument, as the studio plays it: the OPL chips with a
/// 0.3 s tail and no wrap, the speaker as it is.
pub fn render_preview(
    song: &Song,
    instrument_id: &str,
    pitch: &str,
    engine: ChipEngine,
    sample_rate: u32,
) -> RenderedSound {
    let performance = preview_performance(song, instrument_id, pitch, engine);
    render_performance(
        song,
        &performance,
        engine,
        RenderRequest {
            sample_rate,
            tail: if engine == ChipEngine::PcSpeaker {
                0.0
            } else {
                0.3
            },
            wrap: false,
        },
    )
}
