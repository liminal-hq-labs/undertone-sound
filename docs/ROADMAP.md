# Roadmap

The plan for Undertone Sound, kept current in the repository. Decisions live in [decisions.md](decisions.md) and behaviour in [SPEC.md](../SPEC.md). The studio design this follows is in the Fizz Audio Studio repository (`docs/plans/studio-next/shared-core.md`, the recommendation "one Rust grammar with a renderer per target").

## Now

- [x] Repository from the Labs template, public, with house settings, labels and topics
- [x] Scaffold merged: workspace, CI, hero banner, docs
- [x] `docs/OPX.md` moved here from Fizz 2
- [ ] Documentation site in `site/`, deployed to GitHub Pages from `main` (the Rust API is built into it)

## Next: `oplstream`

- [x] Header parse and write
- [x] Command decoder and stepper (`no_std`, no allocation)
- [x] Validator for the rules in the specification
- [x] Writer with `finish` filling the derived header fields (`alloc`)
- [x] Text twin: parser and printer
- [x] IMF, DRO and VGM import, and the explicit, reported re-timing step (`std`)
- [x] Title Theme, Fizz 1's song 0, as a real IMF fixture (`fixtures/opx/title-theme.imf`) with import, validation, twin and re-timing tests
- [ ] A conformance corpus of files, including the examples in the specification (the five examples and the nine refused files are tests; a corpus of files is not built)

## Then: the carve-out

Each crate moves from the studio with its history and fixtures, behind a version gate agreed with the studio. The studio depends on these crates by path or git dependency with a `[patch]` until then, and no wire type or song format changes before the gate.

- [ ] `undertone-opl`: OPL sequencing and allocation (from the studio's `fizz-audio-chips`)
- [x] `undertone-mini`: Undertone's mini-notation, exact fractions and JavaScript number semantics (from `fizz-audio-mini`, with `js` from `fizz-audio-song`), with its recorded fixtures
- [x] `undertone-dsp`: the in-house DSP and graph port (the Fizz-agnostic core of `fizz-audio-undertone`), with its recorded graph fixture. The studio glue and the `web-audio-api` comparison harness stay in the studio
- [ ] `undertone-sound`: the song model and compiler (from `fizz-audio-song`)
- [ ] First end-to-end proof: Title Theme to OPX OPL2 and OPL3, played by Fizz 2's own player

## Later

- [x] `undertone-mini` and `undertone-dsp` compile for `wasm32-unknown-unknown` (no bindings yet)
- [ ] WebAssembly bindings (`undertone-wasm`) and a browser demo page that imports Title Theme and plays it through an OPL emulator
- [ ] Parity canary against the TypeScript library
- [ ] Publishing plan: names, versions, release flow, then crates.io

## Not here

Fizz concepts, the studio's UI and the game's sound configuration.
