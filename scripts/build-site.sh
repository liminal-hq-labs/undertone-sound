#!/usr/bin/env bash
# Builds the documentation site: the VitePress pages, then the Rust API pages beside them under `api/`.
#
# (c) Copyright 2026 Liminal HQ, Scott Morris
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

# Needs Bun and the pinned Rust toolchain. The result is `site/.vitepress/dist`, which is what
# GitHub Pages serves. Set `SKIP_RUST_DOCS=1` to leave the API pages out (the pages still build).

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

dist="site/.vitepress/dist"

(
	cd site
	bun install --frozen-lockfile
	bun run typecheck
	bun run build
)

if [ "${SKIP_RUST_DOCS:-0}" != "1" ]; then
	RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
	rm -rf "${dist}/api"
	mkdir -p "${dist}/api"
	cp -R target/doc/. "${dist}/api/"
fi

# Pages serves files as they are; this stops it running Jekyll over the rustdoc output.
touch "${dist}/.nojekyll"

echo "build-site: ok (${dist})"
