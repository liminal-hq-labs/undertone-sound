// Checks the TypeScript bindings the `ts` feature generates for the song types
//
// (c) Copyright 2026 Liminal HQ, Scott Morris
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Exports the wire types to a scratch directory and reads the files back: one file per type,
//! named after it, each importing the types it uses, and no index. Building with `--features ts`
//! also writes the same files to `bindings/` (or to `TS_RS_EXPORT_DIR`) as each type's own export
//! test runs.

#![cfg(feature = "ts")]

use std::fs;
use std::path::PathBuf;

use ts_rs::{Config, TS};
use undertone_sound::compile::Performance;
use undertone_sound::model::Song;
use undertone_sound::validate::Validation;

fn scratch() -> PathBuf {
	let dir = std::env::temp_dir().join(format!("undertone-sound-bindings-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir
}

fn read(dir: &std::path::Path, name: &str) -> String {
	fs::read_to_string(dir.join(format!("{name}.ts")))
		.unwrap_or_else(|e| panic!("{name}.ts was not written: {e}"))
}

#[test]
fn the_wire_types_export_one_file_each_with_no_index() {
	let dir = scratch();
	let config = Config::new().with_out_dir(&dir);
	Song::export_all(&config).unwrap();
	Validation::export_all(&config).unwrap();
	Performance::export_all(&config).unwrap();

	let mut files: Vec<String> = fs::read_dir(&dir)
		.unwrap()
		.map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
		.collect();
	files.sort();
	for name in [
		"Song.ts",
		"Take.ts",
		"Author.ts",
		"AuthorKind.ts",
		"PresetId.ts",
		"Validation.ts",
		"Problem.ts",
		"ProblemCode.ts",
		"Performance.ts",
	] {
		assert!(files.iter().any(|file| file == name), "{name} in {files:?}");
	}
	assert!(
		!files.iter().any(|file| file.starts_with("index")),
		"the export writes no index: {files:?}"
	);

	let song = read(&dir, "Song");
	assert!(song.contains("format: \"undertone-sound/song\""), "{song}");
	assert!(song.contains("import type { Take }") || read(&dir, "ClipSlot").contains("Take"));
	let take = read(&dir, "Take");
	assert!(take.contains("import type { Author } from \"./Author\";"));
	let author = read(&dir, "Author");
	assert!(author.contains("kind: AuthorKind"), "{author}");
	assert!(author.contains("name?: string"), "{author}");
	assert!(read(&dir, "AuthorKind").contains("\"human\" | \"agent\""));
	let preset = read(&dir, "PresetId");
	assert!(preset.contains("\"kit\""));
	for retired in ["bass-saw", "pad-bright", "drone"] {
		assert!(!preset.contains(retired), "{retired} in {preset}");
	}
	let code = read(&dir, "ProblemCode");
	assert!(code.contains("\"bad-author\""));
	assert!(!code.contains("comment"), "{code}");
	let _ = fs::remove_dir_all(&dir);
}
