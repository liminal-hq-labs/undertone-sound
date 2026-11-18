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
//! collections of songs, text readouts or how a sound engine voices a note. Those belong to the
//! layers above.
//!
//! The format id is [`model::SONG_FORMAT`], `undertone-sound/song`, at version
//! [`model::SONG_VERSION`]. `fixtures/song/` holds the recorded cases; this crate writes them
//! (`UPDATE_FIXTURES=1 cargo test -p undertone-sound -- --test-threads=1`) and its tests require
//! the files to hold exactly the bytes the code makes.
//!
//! The `ts` cargo feature (off by default) exports TypeScript bindings for the wire types, one
//! file per type, with `cargo test -p undertone-sound --features ts`. They go to `bindings/`, or to
//! the directory `TS_RS_EXPORT_DIR` names.

pub mod compile;
pub mod format;
mod mini_bridge;
pub mod model;
pub mod music;
pub mod pan;
pub mod takes;
pub mod validate;
