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

use std::path::Path;

mod directory;

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

/// An open TES3 BSA archive: an mmap of the archive file plus its validated directory
/// geometry. All accessors are zero-copy views into the mapping and, because
/// [`open`](Self::open) validated the directory up front, panic-free.
///
/// Dropping the `Bsa` releases the mmap (the OS unmaps the pages).
#[derive(Debug)]
pub struct Bsa {
    mmap: memmap2::Mmap,
    dir: directory::Directory,
}

impl Bsa {
    /// Open a BSA archive at `path`, mapping it into memory and validating the directory.
    /// No file data or names are copied; everything is served on demand as zero-copy
    /// views via [`files`](Self::files), [`bytes`](Self::bytes) and [`get`](Self::get).
    pub fn open(path: impl AsRef<Path>) -> Result<Bsa, BsaError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: We open the file read-only and never write through the mapping.
        // Concurrent modification of game data files by another process is not expected.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let dir = directory::parse(&mmap)?;
        Ok(Bsa { mmap, dir })
    }

    /// The archive's layout version — [`VERSION_TES3`] for anything that opens.
    pub fn version(&self) -> u32 {
        self.dir.version
    }

    /// Number of archived files.
    pub fn len(&self) -> usize {
        self.dir.count
    }

    pub fn is_empty(&self) -> bool {
        self.dir.count == 0
    }

    /// Directory entry `i` (entries are ordered by [`name_hash`]). Panics if `i` is out
    /// of range, like indexing.
    pub fn file(&self, i: usize) -> FileRecord<'_> {
        assert!(i < self.dir.count, "file index {i} out of range");
        self.dir.record(&self.mmap, i)
    }

    /// Iterate over all directory entries.
    pub fn files(&self) -> impl ExactSizeIterator<Item = FileRecord<'_>> {
        (0..self.dir.count).map(|i| self.dir.record(&self.mmap, i))
    }

    /// The raw bytes for `record`, as a zero-copy slice into the archive mapping.
    pub fn bytes(&self, record: FileRecord<'_>) -> &[u8] {
        let start = self.dir.data_start + record.offset as usize;
        &self.mmap[start..start + record.size as usize]
    }

    /// Look up a file by path, case-insensitively and tolerant of `/` vs `\` separators.
    /// Returns a zero-copy slice into the archive mapping on success.
    ///
    /// This computes the path's [`name_hash`] and binary-searches the archive's hash
    /// table — engine-identical semantics, including the (astronomically unlikely)
    /// possibility of a 64-bit hash collision resolving a name the archive doesn't
    /// contain.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let i = self.dir.find(&self.mmap, name_hash(name))?;
        Some(self.bytes(self.dir.record(&self.mmap, i)))
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
