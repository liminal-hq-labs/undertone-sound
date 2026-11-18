// Undertone's voice graph in Rust: a song's pass booked node for node as the TypeScript engine does
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! A headless Undertone (`docs/research/UNDERTONE-HEADLESS.md`): a score in, the Web Audio graph
//! Undertone would book on the `graph::Graph` trait, and an in-house renderer that plays it, offline
//! or as a stream in blocks of any size.
//!
//! **The core**, which knows no studio type and could move to its own crate (`undertone-core`):
//!
//! - `score`: Undertone's `score()` at 12 ticks a step, and the pass's voices.
//! - `params`: Undertone's defaults and `resolveParams`.
//! - `voice`, `effects`, `master`: `playVoice`, the orbit buses and the master bus.
//! - `pitch`, `pitch_curve`, `noise`, `waves`, `random`, `math`: what those are built from,
//!   computed as the TypeScript computes it.
//! - `graph`, `log`, `json`: the trait, `LogGraph` (the recorder), and the fixture's JSON layout.
//! - `dsp`: the node kernels and the AudioParam timeline; `render_graph`: `RenderGraph` (the
//!   booking) and `Engine` (it prepared to stream, allocation-free); `render`: booking and
//!   rendering a score, the export and the per-part measurement.
//! - `reference`: the graphs compared with the `web-audio-api` crate (`tools/undertone-reference`).
//!
//! The core uses only `fizz_audio_song::js` from the studio's crates, JavaScript's number
//! formatting and rounding, which would move with it.
//!
//! **The studio glue**: `presets` (the studio's presets and drum pieces as Undertone voices, and a
//! song's overrides, the kit's flat `<piece>.<key>` keys included) and `studio` (a `Song` and its
//! `Performance` as score events, and the entry points `book`, `render_graph`,
//! `render_performance` and `render_tracks`).
//!
//! The crate's tests: `LogGraph` records the same text as the TypeScript recorder wrote in
//! `fixtures/undertone/graph.json`; each kernel against its analytic reference; the engine gives
//! the same bits in any block size and allocates nothing while it plays; and, when
//! `FIZZ_UNDERTONE_REFERENCE` names its renders, the comparison with `web-audio-api`.

pub mod dsp;
pub mod effects;
pub mod error;
pub mod graph;
pub mod json;
pub mod log;
pub mod master;
pub mod math;
pub mod noise;
pub mod params;
pub mod pitch;
pub mod pitch_curve;
pub mod presets;
pub mod random;
pub mod reference;
pub mod render;
pub mod render_graph;
pub mod score;
pub mod studio;
pub mod voice;
pub mod waves;
