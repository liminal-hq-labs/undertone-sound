// Undertone's voices, Web Audio graph and in-house renderer in Rust
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A headless Undertone for native and WebAssembly: a score in, the Web Audio graph Undertone's
//! TypeScript engine would book, and an in-house renderer that plays that graph offline or as a
//! stream in blocks of any size. Nothing here knows a host application's types; a host turns its
//! own music into a [`score::Score`] and takes samples out.
//!
//! The pieces:
//!
//! - [`score`]: Undertone's `score()` at 12 ticks a step, and the pass's voices.
//! - [`params`]: Undertone's defaults and `resolveParams`. A voice's field names are shared with
//!   other Undertone ports and stay as they are.
//! - [`voice`], [`effects`], [`master`]: `playVoice`, the orbit buses and the master bus.
//! - [`pitch`], [`pitch_curve`], [`noise`], [`waves`], [`random`], [`math`]: what those are built
//!   from, computed as the TypeScript computes it.
//! - [`graph`], [`log`], [`json`]: the `Graph` trait that voices are booked on, `LogGraph` (a
//!   recorder that writes the calls down), and the fixture's JSON layout.
//! - [`dsp`]: the node kernels and the AudioParam timeline. [`render_graph`]: `RenderGraph` (the
//!   booking) and `Engine` (it prepared to stream, allocation-free). [`render`]: booking and
//!   rendering a score, the export and the per-part measurement.
//!
//! JavaScript's number formatting and rounding come from [`undertone_mini::js`].
//!
//! The crate's tests: `LogGraph` records the same text as the TypeScript recorder wrote in
//! `fixtures/undertone/graph.json`; each kernel against its analytic reference; and the engine
//! gives the same bits in any block size.

pub mod dsp;
pub mod effects;
/// The error a refused score or voice carries.
pub mod error;
pub mod graph;
pub mod json;
pub mod log;
pub mod master;
pub mod math;
/// Undertone's seeded noise buffers.
pub mod noise;
pub mod params;
/// Pitch names, MIDI numbers and frequencies.
pub mod pitch;
pub mod pitch_curve;
pub mod random;
pub mod render;
pub mod render_graph;
pub mod score;
pub mod voice;
pub mod waves;
