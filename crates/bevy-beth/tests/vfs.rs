//! Tests for [`bevy_beth::TesVfs`] — the layered game-data view behind `tes://`.
//!
//! The synthetic tests build a throwaway directory tree and always run; the game-data
//! tests skip themselves when the (gitignored) `data/` fixtures are absent.

use std::fs;
use std::path::PathBuf;

use bevy_beth::TesVfs;

/// A fresh temp directory tree with a couple of loose files, mimicking `Data Files`
/// layout quirks (mixed case, nested dirs).
struct SyntheticRoot(PathBuf);

impl SyntheticRoot {
    fn new(tag: &str) -> SyntheticRoot {
        let root = std::env::temp_dir().join(format!("bevy-beth-vfs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Textures")).unwrap();
        fs::create_dir_all(root.join("meshes/x")).unwrap();
        fs::write(root.join("Textures/TX_Wood.dds"), b"dds bytes").unwrap();
        fs::write(root.join("meshes/x/Thing.NIF"), b"nif bytes").unwrap();
        SyntheticRoot(root)
    }
}

impl Drop for SyntheticRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn loose_lookups_ignore_case_and_separator() {
    let root = SyntheticRoot::new("case");
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    for path in [
        r"Textures\TX_Wood.dds",
        r"TEXTURES\tx_wood.DDS",
        "textures/tx_wood.dds",
        r"MESHES/x\thing.nif",
    ] {
        assert!(vfs.contains(path), "should resolve {path:?}");
    }
    assert_eq!(vfs.read("textures/tx_wood.dds").unwrap(), b"dds bytes");
    assert!(!vfs.contains(r"textures\missing.dds"));
    assert!(vfs.read(r"textures\missing.dds").is_none());
}

#[test]
fn resolve_texture_swaps_extensions() {
    let root = SyntheticRoot::new("swap");
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    // Exact name.
    assert_eq!(
        vfs.resolve_texture("TX_Wood.dds").as_deref(),
        Some("textures/tx_wood.dds")
    );
    // The NIF names a .tga; only the .dds exists (a very common Morrowind situation).
    assert_eq!(
        vfs.resolve_texture("tx_wood.tga").as_deref(),
        Some("textures/tx_wood.dds")
    );
    // An embedded textures\ prefix is honoured.
    assert_eq!(
        vfs.resolve_texture(r"textures\tx_wood.dds").as_deref(),
        Some("textures/tx_wood.dds")
    );
    assert_eq!(vfs.resolve_texture("tx_nowhere.tga"), None);
}

#[test]
fn resolve_model_prepends_meshes() {
    let root = SyntheticRoot::new("model");
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    // MODL values are relative to meshes\ without the prefix.
    assert_eq!(
        vfs.resolve_model(r"x\Thing.NIF").as_deref(),
        Some("meshes/x/thing.nif")
    );
    // An embedded meshes\ prefix (odd mods) still resolves.
    assert_eq!(
        vfs.resolve_model(r"meshes\x\thing.nif").as_deref(),
        Some("meshes/x/thing.nif")
    );
    assert_eq!(vfs.resolve_model(r"x\nowhere.nif"), None);
}

#[test]
fn reads_out_of_archives() {
    let Some(bsa) = tes_testdata::fixture("Morrowind.bsa") else {
        return;
    };
    let root = SyntheticRoot::new("archive");
    let vfs = TesVfs::new(&root.0, [&bsa]).unwrap();

    // A file that exists only inside the archive (reference length from the tes3-bsa
    // suite's independent directory scan).
    let probe = vfs
        .read(r"meshes\m\probe_journeyman_01.nif")
        .expect("archive-backed read");
    assert_eq!(probe.len(), 6276);
    assert!(vfs.contains("MESHES/M/PROBE_JOURNEYMAN_01.NIF"));
}

#[test]
fn loose_files_override_archives() {
    let Some(bsa) = tes_testdata::fixture("Morrowind.bsa") else {
        return;
    };
    let root = SyntheticRoot::new("override");
    // Shadow an archive path with a loose file.
    fs::create_dir_all(root.0.join("meshes/m")).unwrap();
    fs::write(root.0.join("meshes/m/probe_journeyman_01.nif"), b"LOOSE").unwrap();

    let vfs = TesVfs::new(&root.0, [&bsa]).unwrap();
    assert_eq!(
        vfs.read(r"meshes\m\probe_journeyman_01.nif").unwrap(),
        b"LOOSE",
        "the loose file must win over the archive copy"
    );
}

#[test]
fn open_discovers_archives_in_the_data_dir() {
    if tes_testdata::fixture("Morrowind.bsa").is_none() {
        return;
    }
    let vfs = TesVfs::open(tes_testdata::data_dir()).unwrap();
    // Served from Morrowind.bsa (or a loose override — either way it must resolve).
    assert!(vfs.contains(r"meshes\m\probe_journeyman_01.nif"));
    // Bloodmoon.bsa is a later archive; its unique content must be visible too.
    assert!(vfs.contains(r"textures\c_nordic02_upperarm.dds"));
}
