// OPL2, OPL3 and PC speaker sequencing for Undertone: songs to register writes and samples.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A compiled [`undertone_sound`] performance becomes timed OPL register writes, channel
//! allocation and lost-note accounting, and those writes become samples through `oplon`. The
//! PC speaker's frames and its square-wave render sit beside them.
//!
//! The crate knows nothing about the host that plays the music. What a host decides — channels
//! its music must never touch, and the patches of presets this crate does not hold — arrives as an
//! [`profile::OplProfile`].
//!
//! The TypeScript implementation is the reference: `fixtures/chips/` holds its register writes,
//! speaker frames, lost notes and rendered samples, and this crate matches them.
//!
//! - [`profile`]: the host's reserved channels and preset patches.
//! - [`sequence`]: a compiled performance to timed OPL register writes ([`sequence::sequence_opl`]).
//! - [`chip`]: those writes to samples through `oplon` ([`chip::render_sequence`]).
//! - [`speaker`]: the PC speaker's frame frequencies and its square-wave render.
//! - [`limits`]: lost notes ([`limits::analyse_limits`]).
//! - [`engine`]: the three engines behind one interface, rendering a pass or a track's share of
//!   the mix.
//! - [`patches`], [`registers`], [`stereo`] and [`numbers`]: the tables and arithmetic under the
//!   sequencer.

pub mod chip;
pub mod engine;
pub mod limits;
pub mod numbers;
pub mod patches;
pub mod profile;
pub mod registers;
pub mod sequence;
/// The PC speaker engine.
pub mod speaker;
pub mod stereo;
