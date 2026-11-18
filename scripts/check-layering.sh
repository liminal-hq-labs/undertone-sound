#!/usr/bin/env bash
# Checks that the engine crates depend on no supporting crate in this workspace.
#
# (c) Copyright 2026 Liminal HQ, Scott Morris
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

# The engine crates can be promoted to the Undertone repository with their history, so they may
# depend on each other and on registry crates only. Dependencies point from the supporting crates
# (song, OPL, OPX, the bindings) to the engine, never back.
engine=(undertone-mini undertone-dsp)
allowed=" ${engine[*]} "

cd "$(git rev-parse --show-toplevel 2>/dev/null || dirname "$(dirname "${BASH_SOURCE[0]}")")"

bad=()
for crate in "${engine[@]}"; do
	# Captured first so that a failing `cargo tree` stops the script instead of reading as no dependencies.
	tree="$(cargo tree -p "$crate" --edges normal,build --prefix none --locked)"
	while IFS= read -r dep; do
		case "$dep" in
		undertone-* | oplstream)
			[[ "$allowed" == *" $dep "* ]] || bad+=("$crate -> $dep")
			;;
		esac
	done < <(awk 'NR > 1 { print $1 }' <<<"$tree" | sort -u)
done

if [ "${#bad[@]}" -gt 0 ]; then
	echo "check-layering: an engine crate depends on a supporting crate:" >&2
	printf '  %s\n' "${bad[@]}" >&2
	exit 1
fi
echo "check-layering: ok (${engine[*]} depend on no supporting crate)"
