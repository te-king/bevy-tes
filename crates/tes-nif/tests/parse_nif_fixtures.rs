//! Parse the real Morrowind `.nif` fixtures kept in this crate's `tests` dir.
//!
//! The files are (gitignored, locally supplied) game data; each test skips when its file
//! isn't present, so a fresh checkout without the assets still passes.

use std::path::PathBuf;

use tes_nif::{Block, Nif, VERSION_TES3};

/// Resolve a fixture path next to this test file.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests")).join(name)
}

/// Read a fixture's bytes, or `None` (with a skip notice) when it isn't present.
fn read_fixture(name: &str) -> Option<Vec<u8>> {
    let path = fixture(name);
    if !path.exists() {
        eprintln!("skipping: {} not present", path.display());
        return None;
    }
    Some(std::fs::read(path).expect("read fixture"))
}

/// Assert a fixture parses fully as a v4.0.0.2 NIF with the expected block count, and that
/// its first block is of the expected variant.
fn check(name: &str, expected_blocks: usize, first_is_node: bool) {
    let Some(bytes) = read_fixture(name) else {
        return;
    };
    let nif = Nif::parse(&bytes).unwrap_or_else(|e| panic!("parse {name}: {e}"));
    assert_eq!(nif.header.version, VERSION_TES3, "{name} version");
    assert_eq!(
        nif.header.ident, "NetImmerse File Format, Version 4.0.0.2",
        "{name} ident"
    );
    assert_eq!(nif.blocks.len(), expected_blocks, "{name} block count");
    assert_eq!(
        nif.header.num_blocks as usize, expected_blocks,
        "{name} header block count"
    );
    match (&nif.blocks[0], first_is_node) {
        (Block::Node { .. }, true) | (Block::TriShape { .. }, false) => {}
        (other, _) => panic!("{name} unexpected first block: {other:?}"),
    }
}

#[test]
fn parses_cursor() {
    // A lone NiTriShape with its properties and geometry.
    check("cursor.nif", 5, false);
}

#[test]
fn parses_raindrop() {
    // A NiNode root with one child NiTriShape.
    check("Raindrop.nif", 7, true);
}

#[test]
fn cursor_has_geometry() {
    let Some(bytes) = read_fixture("cursor.nif") else {
        return;
    };
    let nif = Nif::parse(&bytes).expect("parse cursor");
    let shapes: Vec<_> = nif.tri_shapes().collect();
    assert_eq!(shapes.len(), 1, "cursor has one tri shape");
    let (_, mesh) = shapes[0];
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.triangles.len(), 2);
    assert_eq!(mesh.normals.len(), 4);
}

#[test]
fn fire_small_is_a_particle_system() {
    // fire_small.nif is a particle effect, not a static mesh: it uses particle/controller
    // blocks this crate intentionally does not decode. Parsing should fail cleanly naming
    // the unsupported block rather than mis-parsing.
    let Some(bytes) = read_fixture("fire_small.nif") else {
        return;
    };
    let err = Nif::parse(&bytes).expect_err("fire_small should not fully parse");
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported block type"),
        "expected an unsupported-block error, got: {msg}"
    );
}
