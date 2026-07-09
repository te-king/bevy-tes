//! The TES3 game-data path convention.
//!
//! The engine treats data paths — BSA directory entries, ESM model/icon references, NIF
//! texture names, loose files on disk — as case-insensitive with interchangeable `/` and
//! `\` separators. Every index or comparison therefore goes through one **normal form**:
//! lowercase, backslash-separated (the form BSA directories store natively).
//!
//! [`normalize`] produces that form as an owned `String`; [`TesPath`] is a zero-copy
//! view type that compares and hashes *as if* normalized, without allocating.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use crate::L1Str;

/// Normalize a game-data path for lookup or comparison: ASCII-lowercase, `/` → `\`.
///
/// This is the shared normal form used by `tes3_bsa`'s archive index and `bevy_beth`'s
/// VFS — indexes built with it and keys looked up through it always agree.
pub fn normalize(path: &str) -> String {
    path.to_ascii_lowercase().replace('/', "\\")
}

/// A game-data path (`meshes\m\probe.nif`) whose [`PartialEq`]/[`Eq`] and [`Hash`] use
/// the shared path normal form — ASCII-lowercase, `/` → `\` — so `MESHES\Probe.NIF` and
/// `meshes/probe.nif` are equal and hash alike. Derefs to the underlying [`L1Str`] for
/// the original (un-normalized) bytes.
///
/// Like [`Path`](std::path::Path) over [`OsStr`](std::ffi::OsStr) (and [`L1Str`] over
/// `[u8]`), this is an unsized view type: it is only ever handled by reference, and
/// `&L1Str` → `&TesPath` is a free pointer cast. Normalization is applied lazily on
/// comparison rather than stored, keeping the view zero-copy over the source data.
#[derive(Debug)]
#[repr(transparent)]
pub struct TesPath(L1Str);

impl TesPath {
    /// View a Windows-1252 path string as a `TesPath` without copying or decoding.
    /// Normalization is deferred to the [`PartialEq`] and [`Hash`] impls.
    pub fn new(path: &L1Str) -> &TesPath {
        // SAFETY: `TesPath` is `repr(transparent)` over `L1Str`, so `&L1Str` and
        // `&TesPath` have identical layout (including the length metadata), and every
        // Windows-1252 string is a valid path view.
        unsafe { &*(path as *const L1Str as *const TesPath) }
    }

    /// View raw Windows-1252 path bytes as a `TesPath` without copying or decoding.
    pub fn from_bytes(bytes: &[u8]) -> &TesPath {
        TesPath::new(L1Str::from_bytes(bytes))
    }

    /// View raw Windows-1252 path bytes as a `TesPath` ending at the first NUL byte (or
    /// the whole slice if there is none), without copying or decoding. For reading
    /// NUL-terminated names straight out of file directories (e.g. a BSA name blob).
    pub fn from_bytes_until_null(bytes: &[u8]) -> &TesPath {
        TesPath::new(L1Str::from_bytes_until_null(bytes))
    }

    /// The path bytes in normal form: ASCII-lowercase with `/` rewritten to `\`. Both
    /// equality and hashing go through this so they always agree.
    fn iter_normalized(&self) -> impl Iterator<Item = u8> + '_ {
        self.0
            .as_bytes()
            .iter()
            .map(|b| b.to_ascii_lowercase())
            .map(|b| match b {
                b'/' => b'\\',
                _ => b,
            })
    }
}

impl Deref for TesPath {
    type Target = L1Str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq for TesPath {
    fn eq(&self, other: &Self) -> bool {
        self.iter_normalized().eq(other.iter_normalized())
    }
}

impl Eq for TesPath {}

impl Hash for TesPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter_normalized().for_each(|b| state.write_u8(b));
        state.write_usize(self.0.len());
    }
}

/// Displays the original (un-normalized) path text.
impl fmt::Display for TesPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    #[test]
    fn normalizes_case_and_separators() {
        assert_eq!(normalize("Textures/TX_Wood.DDS"), r"textures\tx_wood.dds");
        assert_eq!(normalize(r"meshes\i\Shack.NIF"), r"meshes\i\shack.nif");
        assert_eq!(normalize(""), "");
    }

    fn hash_of(p: &TesPath) -> u64 {
        let mut hasher = DefaultHasher::new();
        p.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equal_ignoring_case_and_separators() {
        let a = TesPath::from_bytes(br"MESHES\I\Shack.NIF");
        let b = TesPath::from_bytes(b"meshes/i/shack.nif");
        assert_eq!(a, b);
        assert_eq!(hash_of(a), hash_of(b));
    }

    #[test]
    fn distinct_paths_differ() {
        assert_ne!(
            TesPath::from_bytes(br"meshes\a.nif"),
            TesPath::from_bytes(br"meshes\b.nif")
        );
    }

    #[test]
    fn deref_exposes_original_bytes() {
        let p = TesPath::from_bytes(br"MESHES\Probe.NIF");
        // Deref reaches the un-normalized name.
        assert_eq!(p.as_bytes(), br"MESHES\Probe.NIF");
    }
}
