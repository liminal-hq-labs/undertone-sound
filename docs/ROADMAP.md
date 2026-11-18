# Roadmap

The plan for Undertone Sound, kept current in the repository. Decisions live in [decisions.md](decisions.md) and behaviour in [SPEC.md](../SPEC.md). The studio design this follows is in the Fizz Audio Studio repository (`docs/plans/studio-next/shared-core.md`, the recommendation "one Rust grammar with a renderer per target").

## Now

- [x] Repository from the Labs template, public, with house settings, labels and topics
- [ ] Scaffold merged: workspace, CI, hero banner, docs
- [ ] `docs/OPX.md` moved here from Fizz 2

## Next: `oplstream`

- [x] Header parse and write
- [ ] Command decoder and stepper (`no_std`, no allocation)
- [ ] Validator for the rules in the specification
- [ ] Writer with `finish` filling the derived header fields (`alloc`)
- [ ] Text twin: parser and printer
- [ ] IMF, DRO and VGM import, and the explicit, reported re-timing step (`std`)
- [ ] Fixtures and a conformance corpus, including the examples in the specification

## Then: the carve-out

Each crate moves from the studio with its history and fixtures, behind a version gate agreed with the studio. The studio depends on these crates by path or git dependency with a `[patch]` until then, and no wire type or song format changes before the gate.

- [ ] `undertone-opl`: OPL sequencing and allocation (from the studio's `fizz-audio-chips`)
- [ ] `undertone-dsp`: the in-house DSP and graph port (from `fizz-audio-undertone`)
- [ ] `undertone-sound`: the song model and compiler (from `fizz-audio-song`)
- [ ] First end-to-end proof: Title Theme to OPX OPL2 and OPL3, played by Fizz 2's own player

## Later

- [ ] WebAssembly build and parity canary against the TypeScript library
- [ ] Publishing plan: names, versions, release flow, then crates.io

## Not here

Fizz concepts, the studio's UI and the game's sound configuration.
