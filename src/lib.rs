// The song document for Fizz Audio Studio
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! Types, validation, formatting, editor files, albums, the text readout and change summaries.
//! The TypeScript in `apps/studio/src/song/` is the reference; `fixtures/song/` holds its output.

pub mod album;
pub mod compile;
pub mod describe;
pub mod file;
pub mod format;
pub mod js;
mod mini_bridge;
pub mod model;
pub mod music;
pub mod pan;
pub mod summarise;
pub mod takes;
pub mod validate;
