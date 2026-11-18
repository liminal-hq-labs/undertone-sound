# AGENTS.md

## Table of contents

- [Project status](#project-status)
- [Spelling and writing](#spelling-and-writing)
- [Markdown formatting](#markdown-formatting)
- [Authoring voice](#authoring-voice)
- [Commit messages](#commit-messages)
- [Pull requests](#pull-requests)
- [Git workflow](#git-workflow)
- [Local tooling](#local-tooling)
- [Licence headers](#licence-headers)
- [Architecture rules](#architecture-rules)
- [Testing](#testing)
- [Documentation](#documentation)
- [Repository layout](#repository-layout)

## Project status

Undertone Sound is a [Liminal HQ Labs](https://github.com/liminal-hq-labs) experiment: Shared Rust sound core for Undertone. It is **experimental**, so interfaces, file formats and behaviour may change between any two commits. `SPEC.md` is the source of truth for behaviour, and `docs/decisions.md` records why. The [Labs charter](https://github.com/liminal-hq-labs/.github/blob/main/docs/labs-charter.md) covers what that status means and how a project graduates.

## Spelling and writing

**REQUIREMENT:** Use **Canadian English** in UI strings, comments, commit messages, pull request text and documentation: `colour`, `centre`, `behaviour`, `cancelled`, `licence` (noun) and `license` (verb), and `-ize` endings such as `customize`. Keep American spellings where an API, crate, CSS property or protocol requires them (`color`, `background-color`).

- Use a real em dash (`—`) in prose, never `--`. A command-line flag such as `--check` is not prose.
- Sentence case in the UI and in headings. Title Case for window titles.
- Write tight. No padding, no restating the heading.

## Markdown formatting

**REQUIREMENT:** Do not hard-wrap markdown prose. Write each paragraph or bullet as one line and let the renderer reflow it. This applies to `README.md`, `SPEC.md`, `docs/` and pull request descriptions. Commit message bodies are the one exception: hard-wrap them at roughly 72 characters. Code comments wrap normally.

## Authoring voice

**REQUIREMENT:** Ship the result, not how it was arrived at. Write comments, identifiers, docs and pull request text as the author of the artifact, for the person who meets it later.

- No "this PR", reviewer names or commit hashes in comments or docs. State the fact or the reasoning directly.
- When a comment has been edited more than once, rewrite it as one clean explanation.
- Commit messages and a pull request's `## Test plan` are the exceptions: they are the right place to record why a change happened and how it was checked.

## Commit messages

**Format:** Conventional Commits (`feat`, `fix`, `docs`, `test`, `ci`, `build`, `chore`, `refactor`, `perf`, `style`), with an optional scope.

- The body explains what and why, not how. Use markdown with **bold labels** and no headings. Hard-wrap.
- Backtick every code-level reference (identifiers, paths, config keys, flags, crate names). User-facing strings use quotes.
- End with a `Co-Authored-By:` trailer for the model that wrote the change.
- Write the message to a file with a single-quoted heredoc and commit with `git commit -F <file>`, so backticks and `$()` are not expanded. Check the result with `git log -1 --pretty=fuller`.

## Pull requests

- **Title:** capitalized, imperative, about 70 characters, no Conventional Commit prefix, outcome-focused.
- **Body:** `## Summary` with flat bullets that open with a **bold lead-in**, then `## Test plan` with `- [x]` or `- [ ]` items naming the commands run. Finish with what was not verified.
- **Labels:** one of `enhancement`, `bug`, `documentation`, `testing`, `ci`, `build`, `chore`, plus scope labels where useful.
- Fix a bug with a test that fails before the fix, and state exactly what was and was not run.

## Git workflow

- `main` is the default branch. Work on topic branches (`feat/`, `fix/issue-<n>-…`, `docs/`, `chore/`, `ci/`, `build/`). Never commit to `main`.
- **Merge commits only.** No squash and no rebase merges. Branches are deleted on merge.
- **Never** push, tag, publish or dispatch workflows unless asked.
- **Never** run `git add -A` or `git commit -a`, because checkouts may be shared. Stage explicit paths.
- Prefer the `gh` CLI for GitHub work.
- Planning files such as `PLAN.md` stay out of git.

## Local tooling

- **Editor settings:** `.editorconfig` is authoritative: tabs everywhere except YAML, which uses spaces.
- **Rust**, if the project uses it, is formatted with rustfmt using hard tabs (`rustfmt.toml`). Strictness comes from CI: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and the test suite.
- **Shared CI images** come from [`liminal-hq/.github`](https://github.com/liminal-hq/.github) (`ci-rust`, `ci-web`, `tauri-ci-desktop` and others). Pick the leanest tier that fits and pin it by digest. A project image belongs here only when no shared tier covers it.
- **Dependabot** is off. Add `.github/dependabot.yml` only when the project opts in (see `docs/new-project.md`).

- **Toolchain** is pinned in `rust-toolchain.toml`. The local gate is `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`.

## Licence headers

Every source, script, YAML, TOML and SVG file starts with a one-line summary, a blank comment line and the copyright block:

```rust
// One-line summary of the file.
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT
```

Use `#` comments for shell, YAML, TOML and Dockerfiles, and an XML comment for SVG and `.ui` files. Markdown, JSON and lockfiles don't need one. `scripts/check-headers.sh` enforces it. The project is licensed `Apache-2.0 OR MIT`.

## Architecture rules

- **Fizz-agnostic.** Fizz concepts (rooms, role pans, the level rule, captions, cue sheets, mix profiles) stay out of every crate. Reserved channels and similar host choices are passed in.
- **`oplstream` core is `no_std`** and allocation-free. Writers need `alloc` and importers need `std`.
- **No `unsafe`.** The workspace forbids it.
- **Frozen names.** Undertone's `VoiceParams` field names and builder method names do not change.
- **Nothing is published** without the maintainer's say: every crate keeps `publish = false`.
- **No barrel files.** A `lib.rs` declares modules and re-exports only the public API.
- **SPEC first.** A change to behaviour, interfaces, settings or defaults updates `SPEC.md` in the same change.

## Testing

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo nextest run --workspace --locked` and `cargo test --workspace --doc --locked` — Rust
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked` — docs
- `cargo test -p undertone-sound --features ts --locked` — the song crate's TypeScript bindings
- `cargo test -p oplstream --no-default-features --locked` and again with `--features alloc` — the feature tiers
- `cargo build -p oplstream --no-default-features --target thumbv7em-none-eabi --locked` — the `no_std` core on a bare-metal target (`rustup target add thumbv7em-none-eabi`)
- `cargo build --workspace --release --target wasm32-unknown-unknown --locked` — WebAssembly (`rustup target add wasm32-unknown-unknown`)
- `cargo deny check licenses bans sources` and `cargo deny check advisories` — dependency policy (`deny.toml`)
- `bash scripts/check-headers.sh` — SPDX headers
- `bash scripts/check-banner.sh` — hero banner rules and text fit (needs `xmllint`, `rsvg-convert`, `python3` and Chromium; `--shots DIR` writes frames to look at)
- `docker run --rm -v "$PWD:/workdir" davidanson/markdownlint-cli2:v0.17.2 "**/*.md"` — markdown
- `docker run --rm -v "$PWD:/mnt" -w /mnt koalaman/shellcheck:stable scripts/*.sh` — shell scripts
- `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint` — workflows

Pull requests get one comment and a check run per suite from [pr-results](https://github.com/liminal-hq-labs/pr-results). Each CI job runs its check through `scripts/step-report.sh`, which writes a JUnit step report to `results/steps.xml` and keeps the command's exit status, and uploads it as `steps-test-results-<job>`. The `Rust tests` job also uploads nextest's JUnit (`cargo nextest run --profile ci`, the `ci` profile in `.config/nextest.toml`) as `rust-test-results`, and the `Licence headers` job uploads the `event-file`. `.github/workflows/publish-test-results.yml` runs after CI from the default branch, with the write tokens and none of the pull request's code, and reads those artefacts. Settings are in `.github/pr-results.toml`. Keep the job names stable, because check names come from them, and keep a new job's results in the same shape. To see the comment locally, run `pr-results render --config .github/pr-results.toml <junit files> --out DIR`.

State exactly what was and was not run when reporting results.

## Documentation

- Update `SPEC.md` when behaviour changes, and add or amend a `docs/decisions.md` entry when a decision is made. A change that alters behaviour without touching the doc that describes it is incomplete.
- `README.md` is for people who want to use or understand the project: short, scannable, no padding. Keep architecture and history out of it, and keep the `> **Status:**` line honest.
- `CLAUDE.md` is a summary of this file. Keep them in sync.

## Repository layout

```text
README.md SPEC.md AGENTS.md CLAUDE.md LICENSE-APACHE LICENSE-MIT
.editorconfig .markdownlint.jsonc .gitignore rustfmt.toml
Cargo.toml crates/  workspace and crates (oplstream, undertone-dsp, undertone-mini, undertone-opl, undertone-sound, undertone-wasm)
site/          documentation site and browser demo (VitePress and Bun)
assets/        hero banner
fixtures/      golden files recorded from the TypeScript implementation
docs/          decisions, roadmap and the OPX specification
scripts/       header check, banner check and its Chromium text-fit harness, site and WebAssembly builds
.github/       CI workflow, issue and pull request templates, release notes configuration
```
