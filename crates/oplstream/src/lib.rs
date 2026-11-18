// Reader, writer and stepper for OPX, a timed OPL2, OPL3 and PC speaker register stream.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The reference implementation of OPX; the format is specified in `docs/OPX.md`.
//!
//! The core is `no_std`. The `alloc` feature adds the writer and the `std` feature adds the
//! importers and the re-timing step. Only the header exists so far.

#![no_std]

mod header;

pub use header::{Chip, HEADER_LEN, Header, HeaderError, MAGIC};
