// WebAssembly bindings for the Undertone Sound crates.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What a web page can do with the crates: import IMF, DRO and VGM bytes as OPX, validate and
//! summarize a stream, read its header and text twin, list its register writes, play it through an
//! OPL emulator in blocks, and render one Undertone voice.
//!
//! The logic is plain Rust in [`opx`], [`player`] and [`voice`] and is tested natively. `bindings`
//! is the thin layer that `wasm-bindgen` exports. Errors cross the boundary as readable strings.

mod bindings;
pub mod opx;
pub mod player;
pub mod voice;

pub use bindings::{Player, Summary};
