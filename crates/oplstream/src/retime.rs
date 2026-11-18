// The re-timing step: moves a stream onto another grid and reports exactly what that cost.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::command::{Command, Decoder};
use crate::header::HEADER_LEN;
use crate::validate::{ValidationError, validate};
use crate::writer::{WriteError, Writer};
use std::vec::Vec;

/// Why a stream could not be re-timed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetimeError {
	/// The input is not a valid OPX file.
	Invalid(ValidationError),
	/// The new grid is zero.
	ZeroGrid,
	/// A tick falls beyond 4,294,967,295 on the new grid.
	Overflow,
	/// The re-timed stream is not valid, for example when the loop point and `END` land on the
	/// same tick.
	Write(WriteError),
}

/// What a re-timing did to the stream's timing. It is the whole cost: nothing is changed that
/// this report does not count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetimeReport {
	/// The grid the stream was on.
	pub from_grid_hz: u32,
	/// The grid it is on now.
	pub to_grid_hz: u32,
	/// Register and speaker writes in the stream.
	pub writes: usize,
	/// Writes that did not land exactly on a tick of the new grid and were moved to the nearest.
	pub writes_moved: usize,
	/// The largest distance any write moved, in seconds.
	pub largest_error_secs: f64,
	/// The sum of the distances every write moved, in seconds.
	pub total_error_secs: f64,
}

/// A re-timed OPX file and the report of what it cost.
#[derive(Clone, Debug, PartialEq)]
pub struct Retimed {
	/// The bytes of the new OPX file.
	pub opx: Vec<u8>,
	/// How far the writes moved.
	pub report: RetimeReport,
}

/// Moves `file` onto a grid of `to_grid_hz` ticks per second.
///
/// Each command goes to the nearest tick of the new grid (halves round up), so no write is ever
/// reordered, writes may share a tick that they did not share before, and the song may sound
/// different. Everything that moves is counted in the report. This is never done as part of an
/// import or by a player; it makes a new file.
pub fn retime(file: &[u8], to_grid_hz: u32) -> Result<Retimed, RetimeError> {
	let header = validate(file).map_err(RetimeError::Invalid)?;
	if to_grid_hz == 0 {
		return Err(RetimeError::ZeroGrid);
	}
	let old = u128::from(header.grid_hz);
	let new = u128::from(to_grid_hz);
	let mut writer = Writer::new(header.chip, to_grid_hz);
	writer
		.version(header.major, header.minor)
		.reserved_channels(header.reserved_channels);

	let mut report = RetimeReport {
		from_grid_hz: header.grid_hz,
		to_grid_hz,
		writes: 0,
		writes_moved: 0,
		largest_error_secs: 0.0,
		total_error_secs: 0.0,
	};
	let mut at = 0u32;
	for step in Decoder::new(&file[HEADER_LEN..], header.chip).map_while(Result::ok) {
		if let Command::Wait(_) = step.command {
			continue;
		}
		// The old tick t is t * new / old new ticks; round it to the nearest.
		let exact = u128::from(step.tick) * new;
		let tick =
			u32::try_from((2 * exact + old) / (2 * old)).map_err(|_| RetimeError::Overflow)?;
		if tick > at {
			writer.wait(tick - at);
			at = tick;
		}
		match step.command {
			Command::Write { bank, reg, val } => {
				writer.write(bank, reg, val);
			}
			Command::Speaker(divisor) => {
				writer.speaker(divisor);
			}
			Command::Tier(t) => {
				writer.tier(t);
			}
			Command::Loop => {
				writer.loop_point();
			}
			Command::Mark(id) => {
				writer.marker(id);
			}
			Command::Extension { opcode, payload } => {
				writer.extension(opcode, payload);
			}
			Command::End => {
				writer.end();
			}
			Command::Wait(_) => {}
		}
		if matches!(step.command, Command::Write { .. } | Command::Speaker(_)) {
			report.writes += 1;
			let moved = (u128::from(tick) * old).abs_diff(exact);
			if moved != 0 {
				report.writes_moved += 1;
				// `moved` is in units of 1 / (old * new) seconds.
				let secs = moved as f64 / (old * new) as f64;
				report.largest_error_secs = report.largest_error_secs.max(secs);
				report.total_error_secs += secs;
			}
		}
	}
	let opx = writer.finish().map_err(RetimeError::Write)?;
	Ok(Retimed { opx, report })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::examples::{ONE, THREE, TWO};
	use crate::fuzz::{Rng, for_each_input};
	use crate::twin::{assemble, print_twin};
	use std::string::String;

	fn twin(file: &[u8]) -> String {
		let mut out = String::new();
		print_twin(file, &mut out).unwrap();
		out
	}

	#[test]
	fn the_same_grid_changes_nothing() {
		for file in [&ONE[..], &TWO[..], &THREE[..]] {
			let retimed = retime(file, 560).unwrap();
			assert_eq!(retimed.opx, file);
			assert_eq!(retimed.report.writes_moved, 0);
			assert_eq!(retimed.report.total_error_secs, 0.0);
		}
	}

	#[test]
	fn doubling_the_grid_is_exact() {
		let retimed = retime(&ONE, 1120).unwrap();
		assert_eq!(
			twin(&retimed.opx),
			"OPX 0.1\nchip opl2\ngrid 1120\nreserved none\nstream\nW0 01 20\nW0 A0 41\nW0 B0 31\nT 32\nW0 B0 11\nT 8\nEND\n"
		);
		assert_eq!(retimed.report.writes, 4);
		assert_eq!(retimed.report.writes_moved, 0);
		assert_eq!(retimed.report.largest_error_secs, 0.0);
	}

	#[test]
	fn a_vgm_grid_onto_560_reports_what_moved() {
		// Ticks at 44,100 Hz: 0, 100, 1000, 44,100 and the end at 44,200.
		let source = assemble(
			"OPX 0.1\nchip opl2\ngrid 44100\nstream\nW0 01 01\nT 100\nW0 02 02\nT 900\nW0 03 03\nT 43100\nW0 04 04\nT 100\nEND\n",
		)
		.unwrap();
		let retimed = retime(&source, 560).unwrap();
		// 100 * 560 / 44,100 = 1.2698 -> 1; 1000 -> 12.698 -> 13; 44,100 -> 560; the end 44,200 -> 561.27 -> 561.
		assert_eq!(
			twin(&retimed.opx),
			"OPX 0.1\nchip opl2\ngrid 560\nreserved none\nstream\nW0 01 01\nT 1\nW0 02 02\nT 12\nW0 03 03\nT 547\nW0 04 04\nT 1\nEND\n"
		);
		let r = retimed.report;
		assert_eq!(
			(r.from_grid_hz, r.to_grid_hz, r.writes, r.writes_moved),
			(44_100, 560, 4, 2)
		);
		let e1: f64 = (100.0 * 560.0 / 44_100.0 - 1.0) / 560.0;
		let e2: f64 = (13.0 - 1000.0 * 560.0 / 44_100.0) / 560.0;
		assert!((r.largest_error_secs - e1.max(e2)).abs() < 1e-12);
		assert!((r.total_error_secs - (e1 + e2)).abs() < 1e-12);
	}

	#[test]
	fn writes_can_merge_but_never_reorder() {
		let source = assemble(
			"OPX 0.1\nchip opl2\ngrid 44100\nstream\nW0 01 01\nT 10\nW0 02 02\nT 10\nW0 03 03\nT 44080\nEND\n",
		)
		.unwrap();
		let retimed = retime(&source, 560).unwrap();
		assert_eq!(
			twin(&retimed.opx),
			"OPX 0.1\nchip opl2\ngrid 560\nreserved none\nstream\nW0 01 01\nW0 02 02\nW0 03 03\nT 560\nEND\n"
		);
		assert_eq!(retimed.report.writes_moved, 2);
	}

	#[test]
	fn the_loop_marker_tier_and_extensions_come_along() {
		let source = assemble(
			"OPX 0.1\nchip opl3\ngrid 700\nreserved 5 14\nstream\nW0 01 01\nT 70\nLOOP\nMARK 3\nX F0 41\nTIER 1\nW1 B0 01\nTIER 0\nT 70\nEND\n",
		)
		.unwrap();
		let retimed = retime(&source, 560).unwrap();
		assert_eq!(
			twin(&retimed.opx),
			"OPX 0.1\nchip opl3\ngrid 560\nreserved 5 14\nstream\nW0 01 01\nT 56\nLOOP\nMARK 3\nX F0 41\nTIER 1\nW1 B0 01\nTIER 0\nT 56\nEND\n"
		);
		assert_eq!(retimed.report.writes_moved, 0);
	}

	#[test]
	fn speaker_writes_count_too() {
		let retimed = retime(&crate::examples::FOUR, 100).unwrap();
		// 140 Hz to 100 Hz: 4 -> 2.857 -> 3, 6 -> 4.286 -> 4, 9 -> 6.43 -> 6.
		assert_eq!(
			twin(&retimed.opx),
			"OPX 0.1\nchip spkr\ngrid 100\nreserved none\nstream\nSPK 1193\nT 3\nSPK 0\nT 1\nSPK 607\nT 2\nSPK 0\nEND\n"
		);
		assert_eq!(retimed.report.writes, 4);
		assert_eq!(retimed.report.writes_moved, 3);
	}

	#[test]
	fn refuses_what_it_cannot_do() {
		assert_eq!(retime(&ONE, 0), Err(RetimeError::ZeroGrid));
		assert!(matches!(
			retime(&ONE[..10], 560),
			Err(RetimeError::Invalid(_))
		));
		// A loop and END that land on the same tick.
		let source = assemble("OPX 0.1\nchip opl2\ngrid 1000\nstream\nLOOP\nT 1\nEND\n").unwrap();
		assert!(matches!(retime(&source, 100), Err(RetimeError::Write(_))));
		// A tick beyond what the new grid can count.
		let source = assemble("OPX 0.1\nchip opl2\ngrid 1\nstream\nT 4294967295\nEND\n").unwrap();
		assert_eq!(retime(&source, 2), Err(RetimeError::Overflow));
	}

	#[test]
	fn never_panics_and_always_yields_a_valid_file() {
		for_each_input(|bytes| {
			let _ = retime(bytes, 560);
		});
		let mut rng = Rng::new(99);
		for _ in 0..200 {
			let grid = 1 + rng.below(100_000) as u32;
			if let Ok(retimed) = retime(&THREE, grid) {
				assert!(validate(&retimed.opx).is_ok());
			}
		}
	}
}
