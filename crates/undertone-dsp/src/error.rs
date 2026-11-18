// What can stop a score being booked on Undertone
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

/// A score or a voice Undertone refuses, with the TypeScript's message word for word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
	/// The TypeScript's message.
	pub message: String,
}

impl Error {
	/// An error with `message`.
	pub fn new(message: impl Into<String>) -> Self {
		Error {
			message: message.into(),
		}
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for Error {}
