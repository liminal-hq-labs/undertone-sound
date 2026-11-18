# undertone-sound

The Undertone song document and what a host does with it before any sound is made: the types (`model`), validation that names every problem by its path (`validate`), stable formatting (`format`), pitch names and scales (`music`), pan (`pan`), the notes a take plays (`takes`) and a compiler to timed, engine-neutral events (`compile`). It depends on `undertone-mini` and on nothing else in this workspace, and it knows no host: files on disk, albums and text readouts belong to the layers above. Behaviour is in [SPEC.md](../../SPEC.md) and the reasons are in [decisions.md](../../docs/decisions.md).

The format id is `undertone-sound/song`, and the version is 1.

## Authors

`Take.by` and `Message.by` are an `Author`: an object with a `kind` of `human` or `agent` and, on an agent, an optional free-text `name`.

```json
{ "kind": "human" }
{ "kind": "agent" }
{ "kind": "agent", "name": "Claude" }
```

In TypeScript that is `{ kind: AuthorKind, name?: string }` with `AuthorKind = "human" | "agent"`. A name on a human, an empty name and an unknown kind are errors (`bad-author`).

## TypeScript bindings

The `ts` cargo feature (off by default) derives [`ts-rs`](https://crates.io/crates/ts-rs) bindings for the wire types. To generate them:

```sh
cargo test -p undertone-sound --features ts
```

Each type's export test writes its file into `crates/undertone-sound/bindings/` (ignored by git). To write them somewhere else, set `TS_RS_EXPORT_DIR`; a relative path is taken from the crate's directory, and the directory is created if it is missing:

```sh
TS_RS_EXPORT_DIR=/path/to/generated cargo test -p undertone-sound --features ts
```

The layout is `ts-rs`'s own: one file per type, named after it (`Song.ts`, `Take.ts`, `Author.ts`, `AuthorKind.ts`, `PresetId.ts`, `ProblemCode.ts`, `Validation.ts`, `Performance.ts` and the rest, 33 files today), each starting with a generated-file comment, importing the types it uses with `import type { Author } from "./Author";` and exporting one `type`. There is no `index.ts`: a consumer imports from the file it needs, or writes its own index. `tests/bindings.rs` exports the song, validation and performance types to a scratch directory and checks the layout and the types a host depends on.

## Fixtures

`fixtures/song/` is written by this crate. `UPDATE_FIXTURES=1 cargo test -p undertone-sound -- --test-threads=1` rewrites the recorded files from the cases they hold, and a plain `cargo test -p undertone-sound` requires each file to hold exactly the bytes the code makes. Review the diff of a rewrite: it is what the change did to the format.
