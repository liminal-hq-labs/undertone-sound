// The Undertone song core: the song document, its validation and its compiler.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The song document and what a host does with it before any sound is made.
//!
//! - [`model`] — the document's types and wire format.
//! - [`validate`] — checks a parsed document and names every problem by its path.
//! - [`mod@format`] — writes a document as stable, readable JSON.
//! - [`music`] — pitch names, MIDI numbers and scales.
//! - [`pan`] — a note's stereo position and the path a pan motion draws.
//! - [`takes`] — the notes a take plays, with a mini-notation take expanded.
//! - [`compile`] — lowers a song to a [`compile::Performance`]: timed, engine-neutral events.
//!
//! The crate knows no host. It reads and writes the document, and says nothing about files on disk,
//! albums, text readouts or how a sound engine voices a note. Those belong to the layers above.
//!
//! The TypeScript implementation is the reference, and `fixtures/song/` holds what it recorded.
//! The `ts` cargo feature exports TypeScript bindings for the wire types; it is off by default.

pub mod compile;
pub mod format;
mod mini_bridge;
pub mod model;
pub mod music;
pub mod pan;
pub mod takes;
pub mod validate;
