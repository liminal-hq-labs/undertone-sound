# Fixtures

Golden files the Rust crates are checked against. They are recorded from Undertone's TypeScript implementation and never edited by hand; a Rust port must reproduce them exactly.

- `mini/` — Undertone's `mini` run over every pattern a song or a test holds, plus edge cases and errors. One file per group, each an array of cases with an `id`, `group`, `from` (where the pattern came from), `pattern`, `leaf` and either `queries` or `error`. Run by `crates/undertone-mini/tests/fixtures.rs`. The `leaf` rule is `word` (accept every word), `pitch` (a note name whose MIDI number is 12 to 120) or `drum` (one of `crash openhat hat clap rim snare tom kick`).
- `undertone/graph.json` — the Web Audio graph Undertone's `playVoice`, the orbit buses and the master bus book for three scores, recorded call by call: every node made, every AudioParam call, every connection, start and stop times, and a summary of every buffer and periodic wave. Each case carries the score it was booked from, one line an event with each distinct voice listed once. `crates/undertone-dsp/tests/fixtures.rs` reads the score back, books it on `LogGraph` and requires the whole graph to match byte for byte. The scores are the cases `voices` and `corners` (96 ticks a second) and `title-theme` (112 ticks a second).

## Conventions

- **Readable JSON, tabs, a trailing newline.** Keys are sorted, and a flat object or array stays on one line up to 200 characters. Nothing carries a timestamp.
- **Numbers.** JSON numbers are doubles, written the way JavaScript prints them (`0.30000000000000004`, `1e+21`). A negative zero is written `0`.
- **JavaScript behaviours a port must copy.** `Math.round` rounds ties toward positive infinity (`-0.125` to `-0.12`, where Rust's `round` gives `-0.13`); string positions and lengths are UTF-16 code units; `trim()` and the `/i` flag follow JavaScript's rules. `undertone_mini::js` holds these.
- **Where they are recorded.** They come from the TypeScript reference implementation: Undertone's TypeScript library (`@liminal-hq/undertone`), which holds `playVoice`, `score()` and the mini-notation, driven by the recorder in the Fizz Audio Studio's `tools/fixtures/` (`undertone.ts` and `undertone/recorder.ts`, which wraps an `OfflineAudioContext` and writes each call down). The studio also writes the songs the scores were compiled from. Regenerating the files is done there, not here; the Rust crates only read them.
