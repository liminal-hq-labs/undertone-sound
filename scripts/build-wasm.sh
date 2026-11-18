#!/usr/bin/env bash
# Builds the `undertone-wasm` crate for the browser: the release `.wasm`, then the JavaScript glue.
#
# (c) Copyright 2026 Liminal HQ, Scott Morris
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

# Needs the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`) and a
# `wasm-bindgen` command-line tool at the version pinned in `crates/undertone-wasm/Cargo.toml`.
# The output is `site/.vitepress/theme/wasm/`, which the demo page imports and which is not
# committed.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out="site/.vitepress/theme/wasm"

pinned="$(sed -n 's/^wasm-bindgen = "=\(.*\)"$/\1/p' crates/undertone-wasm/Cargo.toml)"
installed="$(wasm-bindgen --version | awk '{print $2}')"
if [ "${pinned}" != "${installed}" ]; then
	echo "build-wasm: wasm-bindgen ${installed} is installed but the crate pins ${pinned}" >&2
	exit 1
fi

cargo build --release --target wasm32-unknown-unknown -p undertone-wasm --locked

rm -rf "${out}"
wasm-bindgen --target web --out-dir "${out}" --out-name undertone_wasm \
	target/wasm32-unknown-unknown/release/undertone_wasm.wasm

ls -l "${out}"
echo "build-wasm: ok (${out})"
