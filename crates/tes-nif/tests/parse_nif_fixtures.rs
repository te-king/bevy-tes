//! Parse the real Morrowind `.nif` fixtures kept under the workspace `data/meshes` dir.
//!
//! The files are (gitignored, locally supplied) game data; each test skips when its file
//! isn't present, so a fresh checkout without the assets still passes.

use tes_nif::{Block, Nif, VERSION_TES3};

/// Read a mesh fixture's bytes from the workspace `data/meshes` directory, or `None`
/// (with a skip notice) when it isn't present.
fn read_fixture(name: &str) -> Option<Vec<u8>> {
    tes_testdata::read(&format!("meshes/{name}"))
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
        (Block::Node(_), true) | (Block::TriShape(_), false) => {}
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
fn parses_beer_barrel() {
    // A NiNode root over a single NiTriShape (lidded stein: body, lid and handle).
    check("BeerBarrel.NIF", 6, true);
}

#[test]
fn beer_barrel_has_geometry() {
    let Some(bytes) = read_fixture("BeerBarrel.NIF") else {
        return;
    };
    let nif = Nif::parse(&bytes).expect("parse beer barrel");
    let shapes = nif.instances();
    assert_eq!(shapes.len(), 1, "beer barrel has one tri shape");
    let mesh = shapes[0].mesh;
    assert_eq!(mesh.vertices.len(), 398);
    assert_eq!(mesh.triangles.len(), 511);
    // One normal per vertex.
    assert_eq!(mesh.normals.len(), mesh.vertices.len());
    // A UV per vertex so the base texture can be applied.
    assert_eq!(mesh.uvs.len(), mesh.vertices.len());
}

#[test]
fn beer_barrel_references_its_texture() {
    let Some(bytes) = read_fixture("BeerBarrel.NIF") else {
        return;
    };
    let nif = Nif::parse(&bytes).expect("parse beer barrel");
    let shape = nif.instances().into_iter().next().expect("one tri shape");
    let texture = shape
        .base_texture
        .expect("beer barrel resolves a base texture")
        .decode();
    assert_eq!(texture, "Tx_BeerStein.dds");
}

#[test]
fn cursor_has_geometry() {
    let Some(bytes) = read_fixture("cursor.nif") else {
        return;
    };
    let nif = Nif::parse(&bytes).expect("parse cursor");
    let shapes = nif.instances();
    assert_eq!(shapes.len(), 1, "cursor has one tri shape");
    let mesh = shapes[0].mesh;
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.triangles.len(), 2);
    assert_eq!(mesh.normals.len(), 4);
}

#[test]
fn shack_has_multiple_textured_parts() {
    // A multi-part static: several NiTriShapes under a NiNode root, each with its own
    // texture. This exercises scene traversal (composed transforms) and per-shape texture
    // resolution — the reason `instances()` replaced the flat shape list.
    let Some(bytes) = read_fixture("i/In_De_Shack_01.nif") else {
        return;
    };
    let nif = Nif::parse(&bytes).expect("parse shack");
    let shapes = nif.instances();
    assert!(
        shapes.len() > 1,
        "expected several parts, got {}",
        shapes.len()
    );
    let distinct: std::collections::BTreeSet<_> = shapes
        .iter()
        .filter_map(|s| s.base_texture.map(|t| t.decode().into_owned()))
        .collect();
    assert!(
        distinct.len() > 1,
        "expected several distinct textures, got {distinct:?}"
    );
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
