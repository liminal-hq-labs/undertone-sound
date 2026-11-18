# Promoting the engine crates

The handover for moving the Undertone engine into the Undertone repository (`liminal-hq/undertone`). It describes the lab as it stands; the decisions are D41, D42 and D62 in [decisions.md](decisions.md), and [ROADMAP.md](ROADMAP.md) keeps the plan current.

## What moves

| Crate | Path | Depends on | History |
|---|---|---|---|
| `undertone-mini` | `crates/undertone-mini` | `serde_json` | 4 commits, imported from the studio's `fizz-audio-mini` as a subtree, plus the JavaScript number semantics (`js`) added from the studio's `fizz-audio-song` |
| `undertone-dsp` | `crates/undertone-dsp` | `undertone-mini`, `serde_json`, `indexmap` | 7 commits, imported from the studio's `fizz-audio-undertone` as a subtree, with the studio glue removed |

Both are `Apache-2.0 OR MIT` (relicensed from MIT on import, D11), Fizz-agnostic, `no unsafe`, and compile for `wasm32-unknown-unknown`. The names change at the promotion: `undertone-pattern` and `undertone-engine`.

The layering rule is what makes the move a path filter: these two crates depend on each other and on registry crates only. `scripts/check-layering.sh` fails when either gains a dependency on another crate of this workspace, and the `Crate layering` CI job runs it. Take the script and its job with the crates, with the engine list updated for the new names.

## Fixtures that move with them

- `fixtures/mini/`: the recorded results of Undertone's mini-notation, run by `crates/undertone-mini/tests/fixtures.rs`
- `fixtures/undertone/graph.json`: the recorded Web Audio graph for three scores, run by `crates/undertone-dsp/tests/fixtures.rs` (and `tests/streaming.rs` for block-size independence)
- `fixtures/undertone/scores/title-theme.score.json`: an events-only recorded score, read by the demo's `ScorePlayer` and by `crates/undertone-wasm` tests; it moves with the engine only if the bindings do
- The engine part of `fixtures/README.md`

Both are recorded from the TypeScript reference implementation and checked byte for byte. They are not written here.

## What stays in the lab

- `oplstream`, `undertone-opl`, `undertone-sound` (the song model and compiler, with Rust-written fixtures in `fixtures/song/`), the Title Theme proof (`fixtures/opx/`), and the OPX bindings now inside `undertone-wasm`
- Fizz concepts, the studio's UI and the game's sound configuration (they are the studio's)

`undertone-wasm` mixes the engine with OPX today. At the promotion its engine bindings go to the Undertone repository and the OPX bindings stay here.

## Checks to carry over

The CI is one job per check in the shared `ci-rust` image, pinned by digest (D18). For the engine crates these are the ones that matter: `Rust format`, `Rust Clippy`, `Rust tests`, `Rust doc tests`, `Rust docs`, `WebAssembly build`, `Dependency licences` with `deny.toml` (permissive licences only), `Security advisories` and `Crate layering`. Test results post to each pull request with `pr-results` (D40).

## After the move

The lab depends on Undertone for the engine (a git or path dependency, then crates.io when publishing is decided), and the studio keeps pinning the lab by git revision. Nothing is published until the maintainer says so (D9).

## Open pull requests

None at the time of writing. Check `gh pr list` in the lab before starting.
