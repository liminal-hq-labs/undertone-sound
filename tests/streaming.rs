// Checks that the streaming engine allocates nothing while it plays, with a counting allocator
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: MIT

//! An audio callback must not allocate: `Engine::process` is meant to be called from one. This
//! test binary counts every allocation made while the engine plays a song with every node kind
//! Undertone books (FM, filters, the phaser, noise, the panner, the merger, the reverb, the echo
//! and the master bus), in blocks of several sizes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use fizz_audio_song::compile::{compile_song, CompileOptions};
use fizz_audio_undertone::render::RenderRequest;
use fizz_audio_undertone::render_graph::RenderGraph;
use fizz_audio_undertone::studio::book;

struct Counting;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn processing_allocates_and_frees_nothing() {
    for name in ["undertone-voices", "undertone-corners"] {
        let path = format!(
            "{}/../../fixtures/undertone/inputs/{name}.song.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(path).unwrap();
        let song = fizz_audio_song::file::parse_song_file(&text).song.unwrap();
        let performance = compile_song(&song, &CompileOptions::default()).unwrap();
        let graph = book(&song, &performance, &RenderRequest::EXPORT, |init| {
            RenderGraph::new(init.channels, init.length, init.sample_rate)
        })
        .unwrap();
        for block in [1usize, 100, 128, 480, 1024] {
            let mut engine = graph.clone().engine().unwrap();
            let mut left = vec![0f32; block];
            let mut right = vec![0f32; block];
            let mut sound = 0f32;
            ALLOCATIONS.store(0, Ordering::Relaxed);
            COUNTING.store(true, Ordering::Relaxed);
            // Past the end too, where the engine plays silence.
            for _ in 0..(engine.length() + 10_000).div_ceil(block) {
                engine.process(&mut left, &mut right);
                sound = sound.max(left[0].abs());
            }
            COUNTING.store(false, Ordering::Relaxed);
            assert_eq!(
                ALLOCATIONS.load(Ordering::Relaxed),
                0,
                "{name} in blocks of {block}"
            );
            assert!(sound > 0.0, "{name} made a sound");
        }
    }
}
