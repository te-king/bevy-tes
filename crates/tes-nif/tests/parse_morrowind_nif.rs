//! Parse a real Morrowind `.nif` pulled out of `Morrowind.bsa`.
//!
//! The archive is the (gitignored, locally supplied) game data shared with the `tes3-bsa`
//! crate; the test is skipped when it isn't present.

use std::path::Path;

use tes_nif::{Nif, VERSION_TES3, block_type};
use tes3_bsa::Bsa;

/// `Morrowind.bsa` lives in the sibling `tes3-bsa` crate's `tests` dir.
const BSA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tes3-bsa/tests/Morrowind.bsa"
);

/// Pull the first `.nif` out of the archive, returning its raw bytes.
fn first_nif() -> Option<Vec<u8>> {
    if !Path::new(BSA_PATH).exists() {
        eprintln!("skipping: {BSA_PATH} not present");
        return None;
    }
    let bytes = std::fs::read(BSA_PATH).expect("read bsa");
    let bsa = Bsa::parse(&bytes).expect("parse bsa");
    let entry = bsa
        .files
        .iter()
        .find(|f| f.name.decode().to_ascii_lowercase().ends_with(".nif"))
        .expect("archive contains at least one .nif");
    Some(entry.data.clone())
}

#[test]
fn header_is_v4_with_blocks() {
    let Some(nif_bytes) = first_nif() else {
        return;
    };
    let nif = Nif::parse(&nif_bytes).expect("parse nif");
    assert_eq!(nif.header.version, VERSION_TES3);
    assert!(nif.header.num_blocks > 0, "expected at least one block");
    assert_eq!(nif.header.ident, "NetImmerse File Format, Version 4.0.0.2");
}

#[test]
fn first_block_type_is_a_ni_class() {
    let Some(nif_bytes) = first_nif() else {
        return;
    };
    // The header is the identifier line (+ newline) followed by two u32s; the first block
    // begins immediately after, with its inline type name.
    let nl = nif_bytes.iter().position(|&b| b == b'\n').unwrap();
    let after_header = &nif_bytes[nl + 1 + 8..];
    let (_, ty) = block_type(after_header).expect("read first block type");
    let name = ty.decode();
    assert!(
        name.starts_with("Ni") || name.starts_with("Root") || name.starts_with("Avoid"),
        "unexpected first block type: {name:?}"
    );
}
