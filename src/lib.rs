// The OPL2, OPL3 and PC speaker engines in Rust, with `oplon` linked natively
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Port of `apps/studio/src/engines/opl/`, `apps/studio/src/engines/speaker/` and
//! `apps/studio/src/engines/limits.ts`. The TypeScript is the reference: the golden files in
//! `fixtures/chips/` hold its register writes, speaker frames, voice parameters, lost notes and
//! rendered samples, and this crate matches them.
//!
//! - [`sequence`]: a compiled performance to timed OPL register writes ([`sequence::sequence_opl`]).
//! - [`chip`]: those writes to samples through `oplon` ([`chip::render_sequence`]).
//! - [`speaker`]: the PC speaker's frame frequencies and its square-wave render.
//! - [`params`]: voice parameters and values for the instrument editor.
//! - [`limits`]: lost notes ([`limits::analyse_limits`]).
//! - [`engine`]: the three engines behind one interface, with placement, the preview and
//!   rendering a pass or a section.

pub mod chip;
pub mod engine;
pub mod limits;
pub mod numbers;
pub mod params;
pub mod patches;
pub mod registers;
pub mod sequence;
pub mod speaker;
pub mod stereo;
