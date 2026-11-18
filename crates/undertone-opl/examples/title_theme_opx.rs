// Writes the Title Theme as OPX for the OPL2 and the OPL3, with the text twin of each.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Compiles `fixtures/song/inputs/title-theme-dos.song.json`, sequences it for each chip under the
//! game's profile, and writes `title-theme.opl2.opx`, `title-theme.opl3.opx` and a `.opx.txt` twin
//! beside each. The profile is the host's: channels 5 and 14 kept free, a 560 Hz grid, and the
//! game's level rule, headroom and drum holds (see `tests/support/mod.rs`).
//!
//! `cargo run -p undertone-opl --example title_theme_opx` writes into `fixtures/opx/`; pass a
//! directory to write somewhere else. `tests/title_theme_opx.rs` regenerates the same files and
//! requires them to match the ones in `fixtures/opx/` byte for byte.

#[path = "../tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
	let out = std::env::args()
		.nth(1)
		.map_or_else(|| support::fixtures().join("opx"), PathBuf::from);
	std::fs::create_dir_all(&out)?;
	for (name, bytes) in support::golden_files() {
		std::fs::write(out.join(&name), &bytes)?;
		println!(
			"wrote {} ({} bytes)",
			out.join(&name).display(),
			bytes.len()
		);
	}
	Ok(())
}
