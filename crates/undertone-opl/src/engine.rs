// The three chip engines behind one interface: sequencing and rendering
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The chip engines a host plays a performance on: sequencing a song, and rendering a pass or a
//! track's share of the mix to samples. Playing the samples is the host's job.

use undertone_sound::compile::{CompileOptions, Performance, PerformanceEvent, compile_song};
use undertone_sound::model::{EngineId, Song};

use crate::chip::{RenderOptions, RenderedSound, render_sequence};
use crate::profile::OplProfile;
use crate::sequence::{OplChip, OplSequence, melodic_channels, sequence_opl};
use crate::speaker::render::render_speaker;
use crate::speaker::sequence::sequence_speaker;

/// The engines this crate plays. Undertone is the browser's synth and has no chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipEngine {
	/// The Yamaha YM3812: nine two-operator FM channels, mono.
	Opl2,
	/// The Yamaha YMF262: eighteen two-operator FM channels, stereo.
	Opl3,
	/// The PC speaker: one square-wave voice.
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

	/// The engine id this engine plays.
	pub fn id(self) -> EngineId {
		match self {
			ChipEngine::Opl2 => EngineId::Opl2,
			ChipEngine::Opl3 => EngineId::Opl3,
			ChipEngine::PcSpeaker => EngineId::PcSpeaker,
		}
	}

	/// The OPL chip, or `None` for the speaker.
	pub fn opl(self) -> Option<OplChip> {
		match self {
			ChipEngine::Opl2 => Some(OplChip::Opl2),
			ChipEngine::Opl3 => Some(OplChip::Opl3),
			ChipEngine::PcSpeaker => None,
		}
	}
}

/// The melodic voices an engine has under a profile, or `None` for the speaker, which reports no
/// limit.
pub fn capacity(engine: ChipEngine, profile: &OplProfile) -> Option<usize> {
	engine
		.opl()
		.map(|chip| melodic_channels(chip, profile).len())
}

/// The notes an engine cannot play of a performance, with its voice count.
pub fn limits(
	song: &Song,
	performance: &Performance,
	engine: ChipEngine,
	profile: &OplProfile,
) -> Option<(usize, Vec<PerformanceEvent>)> {
	let chip = engine.opl()?;
	Some((
		melodic_channels(chip, profile).len(),
		sequence_opl(song, performance, chip, profile).dropped_events,
	))
}

/// How to render a performance on any chip engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderRequest {
	/// The sample rate to render at.
	pub sample_rate: u32,
	/// Seconds after the last pass for releases to ring out (the OPL chips; the speaker pads silence).
	pub tail: f64,
	/// Fold the OPL tail back onto the start so the pass loops. The speaker has no tail to fold.
	pub wrap: bool,
}

impl RenderRequest {
	/// Settings for rendering a pass ahead of time, for looping.
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
	profile: &OplProfile,
	request: RenderRequest,
) -> RenderedSound {
	match engine.opl() {
		Some(chip) => {
			let sequence = sequence_opl(song, performance, chip, profile);
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

/// The sound one track adds to a render of a performance, with every other track playing as it
/// does in the mix: its share of the mix, not a render of it alone.
///
/// - On the OPL chips the performance is sequenced a second time with the track at the chip's
///   lowest level (`sequence_opl_hushing`), which keeps every channel choice, steal, and drop the
///   same, and `mix` minus that render is the track. A note the track loses to a steal is silent in
///   both renders, so it costs the track nothing here, as it costs it nothing in the mix. The
///   hushed track still sounds at about -47 dB (total level 63), so its share reads about 0.04 dB
///   low, and the two renders are the same length so the subtraction is sample for sample.
/// - On the PC speaker there is one voice and nothing adds, so the track is the frames it wins
///   (`sequence_speaker_track`) and silence elsewhere.
///
/// `mix` is the render of the whole `performance` at the same `request`.
pub fn render_track_in_mix(
	song: &Song,
	performance: &Performance,
	engine: ChipEngine,
	profile: &OplProfile,
	request: RenderRequest,
	track_id: &str,
	mix: &RenderedSound,
) -> RenderedSound {
	match engine.opl() {
		Some(chip) => {
			let hushed = crate::sequence::sequence_opl_hushing(
				song,
				performance,
				chip,
				profile,
				Some(track_id),
			);
			let without = render_sequence(
				&hushed,
				chip,
				RenderOptions {
					sample_rate: request.sample_rate,
					tail: request.tail,
					wrap: request.wrap,
				},
			);
			let subtract = |whole: &[f32], rest: &[f32]| -> Vec<f32> {
				whole
					.iter()
					.zip(rest)
					.map(|(whole, rest)| whole - rest)
					.collect()
			};
			RenderedSound {
				sample_rate: mix.sample_rate,
				left: subtract(&mix.left, &without.left),
				right: subtract(&mix.right, &without.right),
			}
		}
		None => {
			let frames =
				crate::speaker::sequence::sequence_speaker_track(song, performance, track_id);
			let mut pass = render_speaker(&frames, request.sample_rate);
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
	profile: &OplProfile,
	request: RenderRequest,
) -> Result<RenderedSound, String> {
	let performance = compile_song(song, options)?;
	Ok(render_performance(
		song,
		&performance,
		engine,
		profile,
		request,
	))
}

/// The register writes for a song or a section on an OPL chip, for tools that need the stream.
pub fn sequence_song(
	song: &Song,
	options: &CompileOptions,
	chip: OplChip,
	profile: &OplProfile,
) -> Result<OplSequence, String> {
	Ok(sequence_opl(
		song,
		&compile_song(song, options)?,
		chip,
		profile,
	))
}
