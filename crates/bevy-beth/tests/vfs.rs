//! Tests for [`bevy_beth::TesVfs`] — the layered game-data view behind `tes://`.
//!
//! The synthetic tests build throwaway directory trees and always run; the game-data
//! tests skip themselves when the (gitignored) `data/` fixtures are absent.
//!
//! Loose files are probed live on the filesystem (normal form first, then the path as
//! given), so what a mixed-case query resolves depends on the platform: a lowercase
//! on-disk tree resolves any casing everywhere, while a mixed-case tree resolves
//! arbitrary casings only where the filesystem itself is case-insensitive. Archive
//! lookups go through the normalized index and are case-insensitive everywhere.

use std::fs;
use std::path::PathBuf;

use bevy_beth::TesVfs;

/// A fresh temp directory tree of loose files. Dropped = deleted.
struct SyntheticRoot(PathBuf);

impl SyntheticRoot {
    fn new(tag: &str, files: &[(&str, &[u8])]) -> SyntheticRoot {
        let root = std::env::temp_dir().join(format!("bevy-beth-vfs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for (path, bytes) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        SyntheticRoot(root)
    }

    /// A lowercase on-disk tree — the shipping convention that resolves fully on every
    /// platform.
    fn lowercase(tag: &str) -> SyntheticRoot {
        SyntheticRoot::new(
            tag,
            &[
                ("textures/tx_wood.dds", b"dds bytes"),
                ("meshes/x/thing.nif", b"nif bytes"),
            ],
        )
    }
}

impl Drop for SyntheticRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn loose_lookups_on_a_lowercase_tree_ignore_case_and_separator() {
    let root = SyntheticRoot::lowercase("case");
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    for path in [
        r"Textures\TX_Wood.dds",
        r"TEXTURES\tx_wood.DDS",
        "textures/tx_wood.dds",
        r"MESHES/x\thing.nif",
    ] {
        assert!(vfs.contains(path), "should resolve {path:?}");
    }
    assert_eq!(&*vfs.read("textures/tx_wood.dds").unwrap(), b"dds bytes");
    assert!(!vfs.contains(r"textures\missing.dds"));
    assert!(vfs.read(r"textures\missing.dds").is_none());
}

#[test]
fn mixed_case_trees_resolve_by_exact_case_everywhere() {
    let root = SyntheticRoot::new("mixed", &[("Textures/TX_Wood.dds", b"dds bytes")]);
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    // The exact on-disk case resolves on every platform, with either separator.
    assert!(vfs.contains(r"Textures\TX_Wood.dds"));
    assert!(vfs.contains("Textures/TX_Wood.dds"));
    assert!(!vfs.contains(r"Textures\Missing.dds"));

    // Arbitrary casings against a mixed-case tree only resolve where the filesystem is
    // itself case-insensitive — the platforms the game targets.
    #[cfg(any(target_os = "macos", windows))]
    {
        assert!(vfs.contains(r"TEXTURES\tx_wood.DDS"));
        assert_eq!(&*vfs.read("textures/tx_wood.dds").unwrap(), b"dds bytes");
    }
}

#[test]
fn unsafe_paths_and_the_empty_vfs_miss() {
    let root = SyntheticRoot::lowercase("unsafe");
    let vfs = TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap();

    // Escaping the root is refused outright, not probed.
    assert!(!vfs.contains(r"..\..\etc\passwd"));
    assert!(vfs.read("../textures/tx_wood.dds").is_none());

    assert!(!TesVfs::empty().contains("textures/tx_wood.dds"));
}

#[test]
fn resolve_texture_swaps_extensions() {
    let root = SyntheticRoot::lowercase("swap");
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
    let root = SyntheticRoot::lowercase("model");
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
    let root = SyntheticRoot::new("archive", &[]);
    let vfs = TesVfs::new(&root.0, [&bsa]).unwrap();

    // A file that exists only inside the archive (reference length from the tes3-bsa
    // suite's independent directory scan).
    let probe = vfs
        .read(r"meshes\m\probe_journeyman_01.nif")
        .expect("archive-backed read");
    assert_eq!(probe.len(), 6276);
    // Archive lookups are case-insensitive on every platform (normalized index).
    assert!(vfs.contains("MESHES/M/PROBE_JOURNEYMAN_01.NIF"));
}

#[test]
fn loose_files_override_archives() {
    let Some(bsa) = tes_testdata::fixture("Morrowind.bsa") else {
        return;
    };
    // Shadow an archive path with a loose file.
    let root = SyntheticRoot::new(
        "override",
        &[("meshes/m/probe_journeyman_01.nif", b"LOOSE")],
    );

    let vfs = TesVfs::new(&root.0, [&bsa]).unwrap();
    assert_eq!(
        &*vfs.read(r"meshes\m\probe_journeyman_01.nif").unwrap(),
        b"LOOSE",
        "the loose file must win over the archive copy"
    );
}

#[test]
fn later_archives_override_earlier_ones() {
    let (Some(morrowind), Some(bloodmoon)) = (
        tes_testdata::fixture("Morrowind.bsa"),
        tes_testdata::fixture("Bloodmoon.bsa"),
    ) else {
        return;
    };
    let root = SyntheticRoot::new("order", &[]);
    // A mesh Bloodmoon re-ships at a different size (reference lengths from an
    // independent directory scan of both archives). Whichever archive is listed later
    // must win in the merged index.
    let path = r"meshes\x\ex_dae_wall_256_04.nif";

    let forward = TesVfs::new(&root.0, [&morrowind, &bloodmoon]).unwrap();
    assert_eq!(forward.read(path).expect("present in both").len(), 34747);

    let reverse = TesVfs::new(&root.0, [&bloodmoon, &morrowind]).unwrap();
    assert_eq!(reverse.read(path).expect("present in both").len(), 32298);
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
