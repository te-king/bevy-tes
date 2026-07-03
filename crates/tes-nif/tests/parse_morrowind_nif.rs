//! Parse real Morrowind `.nif` models pulled out of `Morrowind.bsa`.
//!
//! The archive is the (gitignored, locally supplied) game data shared with the `tes3-bsa`
//! crate; the tests are skipped when it isn't present.

use std::path::Path;

use tes_nif::{Nif, VERSION_TES3};
use tes3_bsa::Bsa;

/// `Morrowind.bsa` lives in the sibling `tes3-bsa` crate's `tests` dir.
const BSA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tes3-bsa/tests/Morrowind.bsa"
);

fn open_bsa() -> Option<Bsa> {
    if !Path::new(BSA_PATH).exists() {
        eprintln!("skipping: {BSA_PATH} not present");
        return None;
    }
    Some(Bsa::open(BSA_PATH).expect("open bsa"))
}

#[test]
fn parses_a_static_mesh_with_geometry() {
    let Some(bsa) = open_bsa() else {
        return;
    };
    // Find the first `.nif` that fully parses and carries geometry. Most static meshes do;
    // animated/skinned/particle models use blocks this crate doesn't decode and are
    // skipped here.
    let parsed = bsa
        .files
        .iter()
        .filter(|f| f.name.decode().to_ascii_lowercase().ends_with(".nif"))
        .filter_map(|f| Nif::parse(bsa.bytes(f)).ok())
        .find(|nif| nif.tri_shapes().next().is_some())
        .expect("at least one .nif parses with geometry");

    assert_eq!(parsed.header.version, VERSION_TES3);
    let mesh = parsed.tri_shapes().next().unwrap().mesh;
    assert!(!mesh.vertices.is_empty(), "mesh has vertices");
    assert!(!mesh.triangles.is_empty(), "mesh has triangles");
    // Every triangle index must be in range for the vertex buffer.
    for tri in &mesh.triangles {
        for &i in tri {
            assert!((i as usize) < mesh.vertices.len(), "triangle index in range");
        }
    }
}

#[test]
fn most_static_meshes_parse() {
    let Some(bsa) = open_bsa() else {
        return;
    };
    let mut total = 0;
    let mut with_geometry = 0;
    for f in &bsa.files {
        if !f.name.decode().to_ascii_lowercase().ends_with(".nif") {
            continue;
        }
        total += 1;
        if let Ok(nif) = Nif::parse(bsa.bytes(f))
            && nif.tri_shapes().next().is_some()
        {
            with_geometry += 1;
        }
    }
    assert!(total > 0, "archive contains .nif files");
    // The static-mesh subset is the large majority of Morrowind's models.
    let ratio = with_geometry as f64 / total as f64;
    assert!(
        ratio > 0.75,
        "expected most .nif files to yield geometry, got {with_geometry}/{total}"
    );
}
