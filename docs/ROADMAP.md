# Roadmap

The plan for Undertone Sound, kept current in the repository. Decisions live in [decisions.md](decisions.md) and behaviour in [SPEC.md](../SPEC.md). The studio design this follows is in the Fizz Audio Studio repository (`docs/plans/studio-next/shared-core.md`, the recommendation "one Rust grammar with a renderer per target").

The work runs on two tracks, and the first leads. Undertone's own audio engine is the core product; the second track extends it to chips and DOS, and to other synths one day (D41).

## Track 1: the Undertone engine in Rust (primary)

The goal is a Rust Undertone with the full API, native rendering without Web Audio, and a JavaScript surface pulled through WebAssembly, so the library works without TypeScript or a browser when that is wanted. Undertone's TypeScript library, and City Sim 1000 with its frozen `VoiceParams` field names and builder method names, stay working throughout. This project is the testing ground.

Done:

- [x] `undertone-mini`: mini-notation, exact fractions and pattern combinators, with recorded fixtures
- [x] `undertone-dsp`: voices, effects, master bus, DSP kernels, the `Graph` trait and an in-house renderer that plays offline or streams in blocks of any size, matching the TypeScript library's recorded Web Audio graph byte for byte
- [x] `undertone-mini` and `undertone-dsp` compile for `wasm32-unknown-unknown`
- [x] `undertone-wasm`: first bindings, with a browser demo (the demo plays Title Theme as the recorded Undertone score, an OPL2 file and an OPL3 file, side by side; one `undertone-dsp` voice renders through the same bindings)

Open:

- [ ] A design for the Rust-first Undertone: API mapping, WebAssembly strategy, native output, parity, and how the studio and the chip backends fit as consumers (written in the studio repository, to be adopted here)
- [ ] The full API in Rust: the pattern, control, signal and scheduling API that the TypeScript library exposes, not only the renderer
- [ ] Parity with the 0.4.0 features: the autopan LFO, the pan law, the master ceiling, windowed booking, signals and `panPath`
- [ ] Native output without Web Audio, and streaming through an AudioWorklet in the browser
- [ ] The JavaScript API through WebAssembly, with generated TypeScript typings
- [ ] A parity canary against the TypeScript library in CI, with City Sim 1000's API frozen and checked
- [ ] Where the Undertone repository itself goes: Rust first, or the TypeScript library on a Rust core. Not decided. Until then the engine crates incubate here and stay promotable with their history (D42), and the engine's own bindings become a crate separate from `undertone-wasm`

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

New OPL-only work waits until Track 1's design is settled. Work already in flight finishes.

## Across both

- [x] Documentation site in `site/`, deployed to GitHub Pages from `main`, with the Rust API built into it
- [x] CI: one job per check, with `pr-results` publishing the results to each pull request
- [ ] A demo that streams through an AudioWorklet instead of rendering the whole song first
- [ ] Publishing plan: names, versions, release flow, then crates.io. Nothing is published until the maintainer says so

## Not here

Fizz concepts, the studio's UI and the game's sound configuration.
