//! `tes-testdata` — internal test-support crate locating the workspace's game data.
//!
//! All integration tests read Morrowind's data files from the single workspace `data/`
//! directory (see `data/README.md` for the expected layout). The files are copyrighted
//! game content and never enter the repository, so every test that needs one must skip
//! itself when the file is absent — that's what [`fixture`] and [`read`] encode: they
//! return `None` (after printing a skip notice) instead of panicking.
//!
//! ```no_run
//! let Some(bytes) = tes_testdata::read("Morrowind.esm") else {
//!     return; // fixture absent: the test is skipped
//! };
//! ```
//!
//! This crate is a dev-dependency only and is never published.

use std::path::PathBuf;

/// The workspace `data/` directory holding the (gitignored, locally supplied) game data.
pub fn data_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"))
}

/// Resolve `relative` (e.g. `"Morrowind.esm"` or `"meshes/i/In_De_Shack_01.nif"`) under
/// [`data_dir`]. Returns `None` — after printing a skip notice for the test log — when
/// the file isn't present, so callers can `else { return }` to skip.
pub fn fixture(relative: &str) -> Option<PathBuf> {
    let path = data_dir().join(relative);
    if !path.exists() {
        eprintln!("skipping: {} not present", path.display());
        return None;
    }
    Some(path)
}

/// Read a fixture's bytes via [`fixture`]; `None` (with a skip notice) when absent.
pub fn read(relative: &str) -> Option<Vec<u8>> {
    let path = fixture(relative)?;
    Some(std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())))
}
