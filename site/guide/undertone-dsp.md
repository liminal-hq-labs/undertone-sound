# undertone-dsp

`undertone-dsp` is a headless Undertone for native and WebAssembly. A score goes in, the crate books the Web Audio graph that Undertone's TypeScript engine would book, and an in-house renderer plays that graph offline or as a stream in blocks of any size. Nothing in it knows a host application's types: a host turns its own music into a `Score` and takes samples out.

The graph is checked against what the TypeScript recorded in `fixtures/undertone/graph.json`, and the streaming engine gives the same bits in any block size.

## Render a note

A voice is a patch of Undertone's parameters, a score is events on a tick grid, and `render_score` returns planar `f32` samples.

<<< @/../crates/undertone-dsp/examples/dsp_guide.rs#voice{rust}

## Streaming

`book_score_render` books a score and `Engine::process` fills blocks from an audio callback without allocating. Whatever the block size, the samples are the ones an offline render writes.

The [Rust API](/api/undertone_dsp/index.html) lists every public item, including the node kernels and the recorder.
