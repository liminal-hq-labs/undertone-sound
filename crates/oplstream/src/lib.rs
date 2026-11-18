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

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

mod command;
#[cfg(feature = "std")]
mod dro;
#[cfg(test)]
mod examples;
#[cfg(test)]
mod fuzz;
mod header;
#[cfg(feature = "std")]
mod imf;
#[cfg(feature = "std")]
mod import;
#[cfg(feature = "std")]
mod retime;
mod twin;
mod validate;
#[cfg(feature = "std")]
mod vgm;
#[cfg(feature = "alloc")]
mod writer;

pub use command::{Command, DecodeError, DecodeErrorKind, Decoder, Step, Stepper, TickState};
#[cfg(feature = "std")]
pub use dro::{DRO_GRID_HZ, import_dro};
pub use header::{Chip, HEADER_LEN, Header, HeaderError, MAGIC, VERSION_MAJOR, VERSION_MINOR};
#[cfg(feature = "std")]
pub use imf::{ImfKind, import_imf};
#[cfg(feature = "std")]
pub use import::{ImportError, ImportReport, Imported};
#[cfg(feature = "std")]
pub use retime::{RetimeError, RetimeReport, Retimed, retime};
pub use twin::{PrintError, print_twin};
#[cfg(feature = "alloc")]
pub use twin::{TwinError, TwinErrorKind, assemble};
pub use validate::{ValidationError, channel_of, validate};
#[cfg(feature = "std")]
pub use vgm::{VGM_GRID_HZ, import_vgm};
#[cfg(feature = "alloc")]
pub use writer::{WriteError, Writer};
