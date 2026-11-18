---
layout: home
title: Undertone Sound

hero:
  text: One sound grammar, a renderer per target
  tagline: From OPL2 and OPL3 chips to the browser, songs and effects share one grammar. It starts with OPX, a small, exact, loopable stream of timed OPL register writes.
  actions:
    - theme: brand
      text: Read the guide
      link: /guide/
    - theme: alt
      text: Live demo
      link: /demo
    - theme: alt
      text: OPX specification
      link: /opx
    - theme: alt
      text: Rust API
      link: /api

features:
  - title: Exact
    details: OPX time is a whole number of ticks on a declared grid. Importing IMF, DRO and VGM never changes a wait, and re-timing reports everything it moved.
  - title: Small and steppable
    details: The oplstream core is no_std and allocation-free, so a 560 Hz timer interrupt can step a stream in place. Writers and importers opt in with features.
  - title: One grammar, many targets
    details: The same crates compile for native builds and WebAssembly, so a browser renderer and a chip renderer can stay in step from one song.
---

## Crates

<CrateCards />

## Try it

```sh
cargo test --workspace
cargo run -p oplstream --example oplstream_guide
```

The crates are not published yet, and the project is experimental: any interface or file format may change between two commits. The [roadmap](/roadmap) lists what is built and what comes next.
