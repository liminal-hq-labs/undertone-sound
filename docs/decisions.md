# Decisions

Each decision is numbered and stays here even when superseded, with a note on what replaced it. Open questions move here once they are answered.

| # | Decision | Choice | Why |
|---|---|---|---|
| D1 | Home | A Liminal HQ Labs experiment in `liminal-hq-labs/undertone-sound`, public from the first commit | Try the idea in the open before it earns a place in Liminal HQ |
| D2 | Licence | `Apache-2.0 OR MIT` | House licence, compatible with both ecosystems |
| D3 | Dependency updates | No Dependabot configuration until the project opts in | Labs repositories stay quiet by default. See the [Labs charter](https://github.com/liminal-hq-labs/.github/blob/main/docs/labs-charter.md#dependabot-is-opt-in) |
| D4 | Family and names | Undertone is a family: `undertone` (the TypeScript library, its own repository), `undertone-dsp`, `undertone-opl` and `undertone-sound` (crates here), and `oplstream`, which stays unprefixed | `oplstream` names a format library that is useful without Undertone. Nothing is reserved on crates.io yet |
| D5 | First crate | `oplstream` is the reference implementation of OPX: reader, writer, stepper, validator, text twin, and IMF, DRO and VGM import | OPX is the DOS sound format and the core's register-stream format |
| D6 | OPX specification | `docs/OPX.md` lives here and moves with the crate; Fizz 2's docs point at this repository | The spec and its reference code change together. Maintainer's call, 2026-11-18 |
| D7 | Fizz-agnostic core | Fizz concepts (rooms, role pans, the level rule, captions, cue sheets, mix profiles) live in a separate Fizz layer outside this repository | A second consumer, City Sim 1000, shares Undertone. VoiceParams field names and builder method names are frozen, and any Rust or WebAssembly backend is opt-in |
| D8 | Neutral wording | No format, field, crate or type name uses the word "enhanced" | It names a variant only; use neutral terms |
| D9 | Publishing | Every crate has `publish = false` until the maintainer says otherwise. Names are planned for crates.io but not reserved | Publish deliberately, with versions and a release flow decided first |
| D10 | Sound-effect channels | Channels 5 and 14 are the sound-effect channels on OPL2 and OPL3, passed in as reserved channels rather than hard-coded in the core | Fizz's choice; the core stays agnostic |
| D14 | Twin assembler needs `alloc` | `print_twin` is in the `no_std` core; `assemble` (twin text to bytes) needs `alloc` | A parser that produces bytes has to allocate. The specification lists both in the core |
| D15 | Other chips in a VGM file | A VGM file is refused when its header names no YM3812 or YMF262, names both, or sets the dual-chip flag. Other chips' commands inside an OPL file are dropped, counted in the import report, and their waits kept. An unknown VGM opcode is refused | The specification says other chips' commands are "dropped and reported" |
| D16 | Reader versions | A reader accepts major 0 with minor 0 or 1 and refuses a higher minor. `LOOP` while the tier is 1, and core writes to bank 1 when flag bit 0 is set, are refused. A loop point that lands on the `END` tick is refused | The most literal reading of the specification. Raise a question in the specification when a real file disagrees |
| D17 | Re-timing | Halves round up. "Writes moved" counts register and speaker writes whose tick did not map exactly. Markers and the loop point are re-timed but not counted | One rule, reported, never silent |
| D18 | CI shape | One job per check in the shared `ci-rust` image pinned by digest (format, Clippy, tests, doc tests, docs, `oplstream` feature tiers, `no_std` build, WebAssembly build, dependency licences, advisories), plus markdown, headers, shell, workflow and banner checks on the runner | A failing pull request names exactly what broke. No single validate job. Dependabot stays off |
