//! Parse real Morrowind `.nif` models pulled out of `Morrowind.bsa`.
//!
//! The archive is (gitignored, locally supplied) game data; the tests are skipped when it
//! isn't present.

use tes_nif::{Nif, VERSION_TES3};
use tes3_bsa::Bsa;

fn open_bsa() -> Option<Bsa> {
    Some(Bsa::open(tes_testdata::fixture("Morrowind.bsa")?).expect("open bsa"))
}

#[test]
fn parses_a_static_mesh_with_geometry() {
    let Some(bsa) = open_bsa() else {
        return;
    };
    // Find the first `.nif` that carries drawable geometry (a handful are pure particle
    // effects with none).
    let parsed = bsa
        .files
        .iter()
        .filter(|f| f.name.decode().to_ascii_lowercase().ends_with(".nif"))
        .filter_map(|f| Nif::parse(bsa.bytes(f)).ok())
        .find(|nif| !nif.instances().is_empty())
        .expect("at least one .nif parses with geometry");

    assert_eq!(parsed.header.version, VERSION_TES3);
    let mesh = parsed.instances().into_iter().next().unwrap().mesh;
    assert!(!mesh.vertices.is_empty(), "mesh has vertices");
    assert!(!mesh.triangles.is_empty(), "mesh has triangles");
    // Every triangle index must be in range for the vertex buffer.
    for tri in &mesh.triangles {
        for &i in tri {
            assert!(
                (i as usize) < mesh.vertices.len(),
                "triangle index in range"
            );
        }
    }
}

#[test]
fn every_archived_mesh_parses() {
    let Some(bsa) = open_bsa() else {
        return;
    };
    let mut total = 0;
    let mut with_geometry = 0;
    for f in &bsa.files {
        let name = f.name.decode();
        if !name.to_ascii_lowercase().ends_with(".nif") {
            continue;
        }
        total += 1;
        let nif = Nif::parse(bsa.bytes(f)).unwrap_or_else(|e| panic!("parse {name}: {e}"));
        if !nif.instances().is_empty() {
            with_geometry += 1;
        }
    }
    assert!(total > 0, "archive contains .nif files");
    // Nearly everything is drawable; the rest are pure particle effects and the like.
    let ratio = with_geometry as f64 / total as f64;
    assert!(
        ratio > 0.95,
        "expected nearly all .nif files to yield geometry, got {with_geometry}/{total}"
    );
}
