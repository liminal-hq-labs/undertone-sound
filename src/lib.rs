// Undertone's voice graph in Rust: a song's pass booked node for node as the TypeScript engine does
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! The first half of a headless Undertone (`docs/research/UNDERTONE-HEADLESS.md`, task 4.4): a song
//! and its compiled performance in, the Web Audio graph Undertone would book out, behind the
//! `graph::Graph` trait. Nothing here makes sound; a renderer that implements `Graph` (an in-house
//! DSP, or an adapter over the `web-audio-api` crate) is the next step, and either plugs into
//! `render::book`.
//!
//! - `presets`: the studio's presets and drum pieces as Undertone voices, and a song's overrides
//!   over them, the kit's flat `<piece>.<key>` keys included.
//! - `params`: Undertone's defaults and `resolveParams`.
//! - `score`: the performance as a score at 12 ticks a step, and the pass's voices.
//! - `voice`, `effects`, `master`: `playVoice`, the orbit buses and the master bus.
//! - `pitch`, `pitch_curve`, `noise`, `waves`, `random`, `math`: what those are built from,
//!   computed as the TypeScript computes it.
//! - `graph`, `log`, `json`: the trait, `LogGraph` (the recorder), and the fixture's JSON layout.
//! - `render`: `render_graph` and `book`, the entry points.
//!
//! The crate's test is that `LogGraph` records the same text as the TypeScript recorder wrote in
//! `fixtures/undertone/graph.json`.

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
pub mod render;
pub mod score;
pub mod voice;
pub mod waves;
