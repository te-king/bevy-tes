//! TES3 (Morrowind) BSA archive parsing.
//!
//! A BSA is a flat archive: a small directory (file sizes, offsets, names and lookup
//! hashes) followed by the concatenated file data. Opening an archive mmaps the file and
//! validates the directory once — nothing is copied or decoded up front. Directory
//! entries are served as [`FileRecord`] views borrowing from the mapping, and lookups go
//! through the archive's own sorted hash table (a [`name_hash`] computation plus a binary
//! search — the same scheme the game engine uses):
//!
//! ```no_run
//! let bsa = tes3_bsa::Bsa::open("data/Morrowind.bsa").unwrap();
//! if let Some(bytes) = bsa.get(r"meshes\m\probe_journeyman_01.nif") {
//!     println!("{} bytes", bytes.len());
//! }
//! ```

use std::fmt;
use std::path::Path;

use memmap2::Mmap;

mod directory;

use directory::Directory;
pub use directory::{FileRecord, VERSION_TES3, name_hash};

/// Error returned when reading or parsing a BSA archive.
#[derive(Debug, thiserror::Error)]
pub enum BsaError {
    /// I/O failure while reading the archive from disk.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The byte stream could not be parsed as a valid TES3 BSA.
    #[error("parse error: {0}")]
    Parse(String),
}

self_cell::self_cell!(
    struct BsaCell {
        owner: Mmap,

        #[covariant]
        dependent: Directory,
    }
);

/// An open TES3 BSA archive: an mmap of the archive file coupled to the [`Directory`]
/// of slices borrowing from it (a `self_cell` pairing, so the mapping and its views move
/// as one value). All accessors are zero-copy views into the mapping and, because
/// [`open`](Self::open) validated the directory up front, panic-free.
///
/// Dropping the `Bsa` releases the mmap (the OS unmaps the pages).
pub struct Bsa(BsaCell);

impl Bsa {
    /// Open a BSA archive at `path`, mapping it into memory and validating the directory.
    /// No file data or names are copied; everything is served on demand as zero-copy
    /// views via [`files`](Self::files), [`bytes`](Self::bytes) and [`get`](Self::get).
    pub fn open(path: impl AsRef<Path>) -> Result<Bsa, BsaError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: We open the file read-only and never write through the mapping.
        // Concurrent modification of game data files by another process is not expected.
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Bsa(BsaCell::try_new(mmap, |mmap| directory::parse(mmap))?))
    }

    fn dir(&self) -> &Directory<'_> {
        self.0.borrow_dependent()
    }

    /// The archive's layout version — [`VERSION_TES3`] for anything that opens.
    pub fn version(&self) -> u32 {
        self.dir().version()
    }

    /// Number of archived files.
    pub fn len(&self) -> usize {
        self.dir().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Directory entry `i` (entries are ordered by [`name_hash`]). Panics if `i` is out
    /// of range, like indexing.
    pub fn file(&self, i: usize) -> FileRecord<'_> {
        assert!(i < self.len(), "file index {i} out of range");
        self.dir().record(i)
    }

    /// Iterate over all directory entries.
    pub fn files(&self) -> impl ExactSizeIterator<Item = FileRecord<'_>> {
        let dir = self.dir();
        (0..dir.len()).map(|i| dir.record(i))
    }

    /// The raw bytes for `record`, as a zero-copy slice into the archive mapping.
    pub fn bytes(&self, record: FileRecord<'_>) -> &[u8] {
        self.dir().bytes(record)
    }

    /// Look up a file by path, case-insensitively and tolerant of `/` vs `\` separators.
    /// Returns a zero-copy slice into the archive mapping on success.
    ///
    /// This computes the path's [`name_hash`] and binary-searches the archive's hash
    /// table — engine-identical semantics, including the (astronomically unlikely)
    /// possibility of a 64-bit hash collision resolving a name the archive doesn't
    /// contain.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let dir = self.dir();
        Some(dir.bytes(dir.record(dir.find(name_hash(name))?)))
    }
}

impl fmt::Debug for Bsa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bsa")
            .field("version", &self.version())
            .field("files", &self.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directory::testutil::build_archive;

    /// End-to-end over a real (temporary) file: the only test that needs `open`'s mmap
    /// path; everything else is covered on byte slices in `directory::tests`.
    #[test]
    fn opens_and_serves_a_synthetic_archive() {
        let archive = build_archive(&[
            (r"Meshes\B\Thing.NIF", b"NIF-DATA"),
            (r"textures\wood.dds", b"DDS"),
        ]);
        let path = std::env::temp_dir().join(format!("tes3-bsa-test-{}.bsa", std::process::id()));
        std::fs::write(&path, &archive).expect("write temp archive");

        let bsa = Bsa::open(&path).expect("open");
        assert_eq!(bsa.version(), VERSION_TES3);
        assert_eq!(bsa.len(), 2);
        assert_eq!(bsa.files().len(), 2);

        // Any case, either separator.
        let nif = bsa.get("meshes/b/THING.nif").expect("hash lookup hit");
        assert_eq!(nif, b"NIF-DATA");
        assert_eq!(bsa.get("meshes/b/missing.nif"), None);

        let record = bsa
            .files()
            .find(|f| f.name == r"textures\wood.dds")
            .unwrap();
        assert_eq!(bsa.bytes(record), b"DDS");

        let _ = std::fs::remove_file(&path);
    }
}
