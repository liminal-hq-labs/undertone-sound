# Roadmap

The plan for Undertone Sound, kept current in the repository. Decisions live in [decisions.md](decisions.md) and behaviour in [SPEC.md](../SPEC.md). The studio design this follows is in the Fizz Audio Studio repository (`docs/plans/studio-next/shared-core.md`, the recommendation "one Rust grammar with a renderer per target").

The work runs on two tracks, and the first leads. Undertone's own audio engine is the core product; the second track extends it to chips and DOS, and to other synths one day (D41).

## Track 1: the Undertone engine in Rust (primary)

The next version of Undertone is Rust. The Undertone repository (`liminal-hq/undertone`) is where it lives: a Rust API, with the TypeScript API bridging WebAssembly and JavaScript. The plan is the full migration, and it does not wait on its consumers. There is no staging gate (D62).

This lab incubated the engine crates and is the testing ground until the move. The engine crates are promoted into the Undertone repository early, with their history, and the lab then depends on Undertone and never the reverse. The renames that come with the promotion (`undertone-mini` to `undertone-pattern`, `undertone-dsp` to `undertone-engine`) happen there, not here. [The promotion handover](https://github.com/liminal-hq-labs/undertone-sound/blob/main/docs/promotion.md) is the handover: what moves, what stays, and the checks that guard the move.

Done in the lab:

- [x] `undertone-mini`: mini-notation, exact fractions and pattern combinators, with recorded fixtures
- [x] `undertone-dsp`: voices, effects, master bus, DSP kernels, the `Graph` trait and an in-house renderer that plays offline or streams in blocks of any size, matching the TypeScript library's recorded Web Audio graph byte for byte
- [x] `undertone-mini` and `undertone-dsp` compile for `wasm32-unknown-unknown`
- [x] `undertone-wasm`: first bindings, with a browser demo that plays Title Theme through the Undertone engine (a two-bar excerpt for now), the OPL2 chip and the OPL3 chip
- [x] The layering rule (D42), enforced by `scripts/check-layering.sh`: the engine crates depend on nothing else in the lab, so the promotion is a path filter

Next:

- [ ] The execution plan for the Rust-first Undertone, written in the Undertone repository
- [ ] The promotion of the engine crates into the Undertone repository, with their history and fixtures
- [ ] The demo's Undertone card plays the full Title Theme, from a recorded events-only score the studio is producing
- [ ] The lab depends on Undertone for the engine, and the engine's bindings leave `undertone-wasm` (the OPX bindings stay in the lab)

No new engine-crate work starts in the lab: schedulers, renames and the bindings split happen after the promotion, in the Undertone repository.

## Track 2: synth backends, OPX and DOS (supporting)

Backends extend the engine to hardware sound chips, and the interface should be general: OPL is the first backend, and other synths can follow. Fizz 2's DOS build only plays exported OPX files, so everything is lowered offline and OPX carries what DOS needs (the loop point, section markers and the reserved-channel mask).

Done:

- [x] `oplstream`: the OPX reader, writer, stepper, validator, text twin, and IMF, DRO and VGM import with a reported re-timing step, tested against Fizz 1's real Title Theme chunk (`fixtures/opx/title-theme.imf`)
- [x] `undertone-sound`: the song model, validation, formatting and compiler (the generic part of the studio's `fizz-audio-song`), with fixtures this repository writes. The format is its own: `undertone-sound/song` at version 1, a neutral author type and no product-specific presets
- [x] `undertone-opl`: OPL2 and OPL3 sequencing and allocation, lost notes, the PC speaker and the `oplon` renderer. The reserved channels and the game's own patches are the host's, passed in as a profile
- [x] First end-to-end proof, built: Title Theme to OPX for the OPL2 and OPL3 (`fixtures/opx/title-theme.opl2.opx` and `.opl3.opx`). The OPL2 stream matches the DOS game's own chunk note for note, with four carrier levels apart

Open:

- [ ] First end-to-end proof, played: the same files loaded by Fizz 2's own player, which is the studio's loading test
- [ ] A backend interface, so the engine drives OPL as one synth among others (to be designed with Track 1's API)
- [ ] A conformance corpus of OPX files, including the examples in the specification (the five examples and the nine refused files are tests; a corpus of files is not built)
- [ ] Real DRO and VGM captures to test those readers against (none exist; low priority)

New OPL-only work waits until the Undertone repository's plan lands. Work already in flight finishes.

## Across both

- [x] Documentation site in `site/`, deployed to GitHub Pages from `main`, with the Rust API built into it
- [x] CI: one job per check, with `pr-results` publishing the results to each pull request
- [ ] A demo that streams through an AudioWorklet instead of rendering the whole song first
- [ ] Publishing plan: names, versions, release flow, then crates.io. Nothing is published until the maintainer says so

## Not here

Fizz concepts, the studio's UI and the game's sound configuration.
