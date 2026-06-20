//! End-to-end tests parsing the bundled BSA archives.

use beth_rs::Bsa;

const MORROWIND_BSA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Morrowind.bsa");
const BLOODMOON_BSA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Bloodmoon.bsa");

#[test]
fn parses_morrowind_archive() {
    let bytes = std::fs::read(MORROWIND_BSA).expect("Morrowind.bsa should be readable");
    let bsa = Bsa::parse(&bytes).unwrap();
    assert_eq!(bsa.version, 0x100);
    assert_eq!(bsa.files.len(), 11_090);

    // Reference entry from an independent directory scan.
    let probe = bsa
        .get(r"meshes\m\probe_journeyman_01.nif")
        .expect("known mesh should be present");
    assert_eq!(probe.data.len(), 6276);

    // Lookup is case-insensitive and separator-tolerant.
    assert!(bsa.get("MESHES/M/PROBE_JOURNEYMAN_01.NIF").is_some());
}

#[test]
fn parses_bloodmoon_archive() {
    let bytes = std::fs::read(BLOODMOON_BSA).expect("Bloodmoon.bsa should be readable");
    let bsa = Bsa::parse(&bytes).unwrap();
    assert_eq!(bsa.version, 0x100);
    assert_eq!(bsa.files.len(), 1_545);

    let tex = bsa
        .get(r"textures\c_nordic02_upperarm.dds")
        .expect("known texture should be present");
    assert_eq!(tex.data.len(), 43_856);
}

#[test]
fn entry_data_is_in_bounds_and_named() {
    let bytes = std::fs::read(MORROWIND_BSA).expect("Morrowind.bsa should be readable");
    let bsa = Bsa::parse(&bytes).unwrap();
    let base = bytes.as_ptr() as usize;
    let end = base + bytes.len();
    for f in &bsa.files {
        assert!(!f.name.is_empty(), "every entry should have a name");
        // The borrowed data slice points inside the archive buffer.
        let start = f.data.as_ptr() as usize;
        assert!(start >= base && start + f.data.len() <= end);
    }
}

#[test]
fn dds_textures_have_the_dds_magic() {
    let bytes = std::fs::read(MORROWIND_BSA).expect("Morrowind.bsa should be readable");
    let bsa = Bsa::parse(&bytes).unwrap();
    // Spot-check that a .dds entry's bytes really are a DDS file ("DDS " magic),
    // proving the size/offset resolution lands on the right data.
    let dds = bsa
        .files
        .iter()
        .find(|f| f.name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&dds.data[..4], b"DDS ");
}
