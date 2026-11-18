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
- `fixtures/undertone/scores/title-theme.score.json`: the full Title Theme as an events-only score, rendered by the demo and by `undertone-wasm`'s tests
- The engine part of `fixtures/README.md`, which mixes engine and lab sections and needs splitting

These are recorded from the TypeScript reference implementation and are not written here. `fixtures/undertone/inputs/title-theme-full.song.json` is different: it is a song in the studio's format, with its old format id, so it stays out of the engine.

The fixtures have no earlier history under another path. They were copied into the lab by `5160686` (`fixtures/mini`), `9d685c9` (`graph.json`) and `43ee96a` (the score), and their origin is `fixtures/mini` and `fixtures/undertone` in the studio repository.

The demo and `undertone-wasm`'s native tests read `graph.json` and the score file. After the move the lab keeps its own copies of those two files as demo data, and the voice and recorded-score player stay as lab demo code over Undertone's crates. The Undertone repository's copies are canonical, and the lab refreshes its copies from them.

## Selecting the history

Select the history by commit, not by path alone, and check it by subject lines. The early commits of all four carved crates sit at the repository root in the lab (`Cargo.toml`, `src/`, `tests/`, `examples/`), because they were subtree-imported, so a filter on root `src/` mixes their histories. `git subtree split` does not isolate a crate here: its output keeps unrelated lab commits.

The studio's original commits (their paths in the studio repository were `crates/fizz-audio-mini/` and `crates/fizz-audio-undertone/`, so the studio is the cleanest source):

- `undertone-mini`: `feat(mini): port Undertone's mini-notation as the fizz-audio-mini crate` (`8d8c7fb` in the lab)
- `undertone-dsp`: `feat(undertone): port Undertone's voice graph to Rust behind a Graph trait` (`2423833`), `feat(undertone): render the booked graph in-house, offline or as a stream` (`6ad23b4`) and `feat(undertone): measure loudness and width without the master bus in render_songs` (`68ba969`)

The lab-era commits that touch the crates (a path filter on `crates/undertone-mini` and `crates/undertone-dsp` selects these correctly):

- `undertone-mini`: `5160686` (rename and relicense, with `fixtures/mini`), `4baaa41` (the `js` module) and `9fd386f` (the guide example)
- `undertone-dsp`: `79b48e0` (remove the studio glue and the reference harness), `9d685c9` (rename, relicense and `graph.json`), `4a4dfcd` (the provenance wording) and `9fd386f` (the guide example). The placeholder crate's commits (`b326ebf`, `bd55623`) are not part of it

Then verify by subject lines: the studio's original subjects first, then the lab-era ones in order.

## What stays in the lab

- `oplstream`, `undertone-opl`, `undertone-sound` (the song model and compiler, with Rust-written fixtures in `fixtures/song/`), the Title Theme proof (`fixtures/opx/`), and the OPX bindings now inside `undertone-wasm`
- Fizz concepts, the studio's UI and the game's sound configuration (they are the studio's)

`undertone-wasm` mixes the engine with OPX today. At the promotion the OPX player and importer stay in the lab as `undertone-sound-wasm`, and the Undertone repository builds its own bindings. The scripts, CI jobs and site that name `undertone-wasm` (`scripts/build-wasm.sh`, `scripts/build-site.sh`, `WebAssembly build`, `WebAssembly bindings`, the Pages workflow) follow the rename.

## Checks to carry over

The CI is one job per check in the shared `ci-rust` image, pinned by digest (D18). For the engine crates these are the ones that matter: `Rust format`, `Rust Clippy`, `Rust tests`, `Rust doc tests`, `Rust docs`, `WebAssembly build`, `Dependency licences` with `deny.toml` (permissive licences only), `Security advisories` and `Crate layering`. Test results post to each pull request with `pr-results` (D40).

## After the move

The lab depends on Undertone for the engine (a git or path dependency, then crates.io when publishing is decided), and the studio keeps pinning the lab by git revision. Nothing is published until the maintainer says so (D9).

## Open pull requests

None at the time of writing (`main` at `26bbb05`, a candidate snapshot). Check `gh pr list` in the lab before starting.
