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
