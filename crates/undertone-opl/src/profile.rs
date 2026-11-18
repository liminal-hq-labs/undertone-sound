// The host's choices the OPL sequencer does not make for itself
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A profile holds what belongs to the tool or game that plays the music, not to the chips:
//! which channels the music must never touch (a game keeps some free for sound effects), and the
//! patches it wants in place of the standard ones.

use undertone_sound::model::PresetId;

use crate::patches::PatchBank;
use crate::registers::Patch;

/// What a host decides about its chips: reserved channels and preset patches.
///
/// The default profile reserves nothing and holds the standard patches. A sound of the host's own
/// is a patch it supplies for the preset a song's instrument names; songs say `bass`, and the host
/// decides what `bass` sounds like.
///
/// ```
/// use undertone_opl::profile::OplProfile;
///
/// let profile = OplProfile::new().reserve([5, 14]);
/// assert_eq!(profile.reserved_channels(), [5, 14]);
/// assert_eq!(profile.reserved_mask(), 1 << 5 | 1 << 14);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OplProfile {
	reserved: Vec<usize>,
	patches: PatchBank,
}

impl OplProfile {
	/// A profile with nothing reserved and the standard patches.
	pub fn new() -> OplProfile {
		OplProfile::default()
	}

	/// Keeps these chip channels free: the sequencer never gives them a note. A channel is a
	/// number from 0 to 17, with 9 and up on the OPL3's second bank.
	pub fn reserve(mut self, channels: impl IntoIterator<Item = usize>) -> OplProfile {
		for channel in channels {
			if !self.reserved.contains(&channel) {
				self.reserved.push(channel);
			}
		}
		self.reserved.sort_unstable();
		self
	}

	/// Supplies the patch a preset plays, replacing the standard one. Every song that names the
	/// preset plays it, so a host with several sounds of one kind keeps them apart with the
	/// instrument's own `opl2` or `opl3` overrides.
	pub fn with_patch(mut self, preset: PresetId, patch: Patch) -> OplProfile {
		self.patches.insert(preset, patch);
		self
	}

	/// The reserved channels, in order.
	pub fn reserved_channels(&self) -> &[usize] {
		&self.reserved
	}

	/// The reserved channels as a bit mask, bit `n` for channel `n`, as an OPX header holds them.
	pub fn reserved_mask(&self) -> u32 {
		self.reserved
			.iter()
			.filter(|&&channel| channel < 32)
			.fold(0, |mask, &channel| mask | 1 << channel)
	}

	/// The patches presets play.
	pub fn patches(&self) -> &PatchBank {
		&self.patches
	}
}
