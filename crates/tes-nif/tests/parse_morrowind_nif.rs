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
    // Find the first `.nif` that fully parses and carries geometry. Most static meshes do;
    // animated/skinned/particle models use blocks this crate doesn't decode and are
    // skipped here.
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
            && !nif.instances().is_empty()
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
