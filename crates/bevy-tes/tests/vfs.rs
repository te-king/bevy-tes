//! Tests for [`bevy_tes::TesVfs`] — the layered game-data view behind `tes://`.
//!
//! The synthetic tests build a throwaway directory tree and always run; the game-data
//! tests skip themselves when the (gitignored) `data/` fixtures are absent.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::{AssetReader, Reader};
use bevy::tasks::block_on;
use bevy_tes::{TesVfs, TesVfsReader, tes3_bsa::Bsa};
use tes_core::TesPath;

/// A fresh temp directory tree with a couple of loose files, mimicking `Data Files`
/// layout quirks (mixed case, nested dirs).
struct SyntheticRoot(PathBuf);

impl SyntheticRoot {
    fn new(tag: &str) -> SyntheticRoot {
        let root = std::env::temp_dir().join(format!("bevy-tes-vfs-{tag}-{}", std::process::id()));
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

/// A single-file BSA whose directory name is stored as authored Windows-1252 bytes.
fn write_archive(path: &Path, name: &[u8], payload: &[u8]) {
    let mut bytes = Vec::new();
    for value in [
        0x100u32,
        12 + name.len() as u32 + 1,
        1,
        payload.len() as u32,
        0,
        0,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(name);
    bytes.push(0);
    bytes.extend([0; 8]);
    bytes.extend(payload);
    fs::write(path, bytes).unwrap();
}

fn read_asset(vfs: Arc<TesVfs>, root: &Path, path: &str) -> Vec<u8> {
    let reader = TesVfsReader::new(vfs, root);
    block_on(async {
        let mut source = reader.read(Path::new(path)).await.unwrap();
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes).await.unwrap();
        bytes
    })
}

#[test]
fn unicode_paths_share_archive_and_loose_keys() {
    let root = SyntheticRoot::new("encoding");
    let archive = root.0.join("test.bsa");
    let raw = b"Textures\\Caf\xe9\x99.dds";
    let path = "textures/caf\u{e9}\u{2122}.dds";
    write_archive(&archive, raw, b"archived");

    let bsa = Bsa::open(&archive).unwrap();
    assert_eq!(bsa.get(path), Some(b"archived".as_slice()));
    assert_eq!(bsa.get_path(TesPath::from_bytes(raw)), bsa.get(path));
    assert!(bsa.get("textures/\u{1f600}.dds").is_none());

    let vfs = Arc::new(TesVfs::new(&root.0, [&archive]).unwrap());
    assert!(vfs.contains(path));
    assert_eq!(vfs.read(path).unwrap(), b"archived");
    assert_eq!(
        vfs.resolve_texture("Caf\u{e9}\u{2122}.tga").as_deref(),
        Some(path)
    );
    assert_eq!(read_asset(vfs, &root.0, path), b"archived");

    fs::write(root.0.join("Textures/Caf\u{e9}\u{2122}.dds"), b"loose").unwrap();
    fs::write(root.0.join("meshes/x/Caf\u{e9}.NIF"), b"model").unwrap();
    let vfs = Arc::new(TesVfs::new(&root.0, [&archive]).unwrap());
    assert_eq!(vfs.read(path).unwrap(), b"loose");
    assert_eq!(read_asset(vfs.clone(), &root.0, path), b"loose");
    assert_eq!(
        vfs.resolve_model("x\\Caf\u{e9}.nif").as_deref(),
        Some("meshes/x/caf\u{e9}.nif")
    );
}

#[test]
fn unrepresentable_loose_paths_do_not_prevent_loading() {
    let root = SyntheticRoot::new("unrepresentable");
    let path = "Textures/\u{1f600}.dds";
    fs::write(root.0.join(path), b"not game data").unwrap();
    let vfs = Arc::new(TesVfs::new(&root.0, Vec::<PathBuf>::new()).unwrap());
    assert!(vfs.contains("textures/tx_wood.dds"));
    assert!(!vfs.contains(path));
    assert!(vfs.read(path).is_none());
    let reader = TesVfsReader::new(vfs, &root.0);
    assert!(block_on(reader.read(Path::new(path))).is_err());
}

#[cfg(unix)]
#[test]
fn non_unicode_asset_paths_are_rejected() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(OsString::from_vec(b"Textures/raw\xff.dds".to_vec()));
    let reader = TesVfsReader::new(Arc::new(TesVfs::empty()), std::env::temp_dir());
    assert!(block_on(reader.read(&path)).is_err());
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
