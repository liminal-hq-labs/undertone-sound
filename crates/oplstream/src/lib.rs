// Reader, writer and stepper for OPX, a timed OPL2, OPL3 and PC speaker register stream.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The reference implementation of OPX; the format is specified in `docs/OPX.md`.
//!
//! The core is `no_std` and allocation-free: the header, the command decoder and stepper, the
//! validator and the text twin printer. The `alloc` feature adds the writer and the twin
//! assembler, and the `std` feature adds the importers and the re-timing step.

#![no_std]

#[cfg(test)]
extern crate std;

mod command;
#[cfg(test)]
mod examples;
#[cfg(test)]
mod fuzz;
mod header;
mod validate;

pub use command::{Command, DecodeError, DecodeErrorKind, Decoder, Step, Stepper, TickState};
pub use header::{Chip, HEADER_LEN, Header, HeaderError, MAGIC, VERSION_MAJOR, VERSION_MINOR};
pub use validate::{ValidationError, channel_of, validate};
