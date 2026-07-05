//! TES3 (Morrowind) BSA archive parsing.
//!
//! A BSA is a flat archive: a small directory (file sizes, offsets, names and lookup
//! hashes) followed by the concatenated file data. Opening an archive mmaps the file and
//! builds an in-memory directory of [`FileRecord`]s; individual file bytes are served as
//! zero-copy slices into the mapping:
//!
//! ```no_run
//! let bsa = tes3_bsa::Bsa::open("data/Morrowind.bsa").unwrap();
//! if let Some(bytes) = bsa.get(r"meshes\m\probe_journeyman_01.nif") {
//!     println!("{} bytes", bytes.len());
//! }
//! ```

use nom::IResult;
use nom::bytes::complete::take;
use nom::number::complete::le_u32;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use tes_core::L1String;

/// The only BSA layout version Morrowind/Tribunal/Bloodmoon use.
pub const VERSION_TES3: u32 = 0x100;

/// Error returned when reading or parsing a BSA archive.
#[derive(Debug)]
pub enum BsaError {
    /// I/O failure while reading the archive from disk.
    Io(std::io::Error),
    /// The byte stream could not be parsed as a valid TES3 BSA.
    Parse(String),
}

impl fmt::Display for BsaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BsaError::Io(e) => write!(f, "I/O error: {e}"),
            BsaError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for BsaError {}

impl From<std::io::Error> for BsaError {
    fn from(e: std::io::Error) -> Self {
        BsaError::Io(e)
    }
}

/// Directory entry for a single archived file: its name, lookup hash, and location within
/// the data section of the archive.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRecord {
    /// Path within the archive, e.g. `meshes\m\probe_journeyman_01.nif` (Windows-1252,
    /// backslash-separated).
    pub name: L1String,
    /// The directory's precomputed lookup hash for `name`.
    pub hash: u64,
    /// Byte offset of this file's data within the archive's data section.
    pub offset: u32,
    /// Byte length of this file's data.
    pub size: u32,
}

/// An open TES3 BSA archive. Holds an mmap of the archive file and an in-memory directory
/// of [`FileRecord`]s; file bytes are served as zero-copy slices into the mapping.
///
/// Dropping the `Bsa` releases the mmap (the OS unmaps the pages).
#[derive(Debug)]
pub struct Bsa {
    pub version: u32,
    pub files: Vec<FileRecord>,
    mmap: memmap2::Mmap,
    /// Absolute byte offset within `mmap` at which the file data section begins.
    data_start: usize,
    /// Normalized name → index into `files`, so [`get`](Self::get) is a hash lookup.
    index: HashMap<String, usize>,
}

/// Read a little-endian `u32` from a fixed position in an exact-length block.
fn u32_at(block: &[u8], byte: usize) -> u32 {
    u32::from_le_bytes(block[byte..byte + 4].try_into().expect("4 bytes"))
}

impl Bsa {
    /// Open a BSA archive at `path`, mapping it into memory and building the file directory.
    /// File data is not copied; bytes are served on demand as zero-copy slices via
    /// [`bytes`](Self::bytes) and [`get`](Self::get).
    pub fn open(path: impl AsRef<Path>) -> Result<Bsa, BsaError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: We open the file read-only and never write through the mapping.
        // Concurrent modification of game data files by another process is not expected.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::parse_directory(mmap)
    }

    fn parse_directory(mmap: memmap2::Mmap) -> Result<Bsa, BsaError> {
        let parse = |input| -> IResult<&[u8], (u32, usize, Vec<FileRecord>)> {
            let (input, version) = le_u32(input)?;
            let (input, hash_offset) = le_u32(input)?;
            let (input, count) = le_u32(input)?;
            let count = count as usize;
            let hash_offset = hash_offset as usize;

            // After the 12-byte header come three parallel tables, then a name blob,
            // then the hash table; `hash_offset` spans everything between the header and
            // the hash table.
            let (input, size_offsets) = take(count * 8)(input)?; // (u32 size, u32 offset)
            let (input, name_offsets) = take(count * 4)(input)?; // u32 into name blob
            let names_len = hash_offset
                .checked_sub(count * 12)
                .ok_or_else(|| nom_fail(input))?;
            let (input, names) = take(names_len)(input)?;
            let (_, hashes) = take(count * 8)(input)?; // two u32 halves per file

            // data_start = 12 (header) + hash_offset (dir tables) + count * 8 (hash table)
            let data_start = 12 + hash_offset + count * 8;

            let mut files = Vec::with_capacity(count);
            for i in 0..count {
                let size = u32_at(size_offsets, i * 8);
                let offset = u32_at(size_offsets, i * 8 + 4);
                let name_off = u32_at(name_offsets, i * 4) as usize;

                let name_bytes = names.get(name_off..).ok_or_else(|| nom_fail(hashes))?;
                let end = name_bytes
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(name_bytes.len());
                let name = L1String::from_bytes(name_bytes[..end].to_vec());

                let hash = u64::from_le_bytes(hashes[i * 8..i * 8 + 8].try_into().unwrap());

                files.push(FileRecord {
                    name,
                    hash,
                    offset,
                    size,
                });
            }

            Ok((&[], (version, data_start, files)))
        };

        let input: &[u8] = &mmap;
        let (_, (version, data_start, files)) =
            parse(input).map_err(|e| BsaError::Parse(format!("{e:?}")))?;

        if version != VERSION_TES3 {
            return Err(BsaError::Parse(format!(
                "unsupported BSA version {:#x} (expected {:#x})",
                version, VERSION_TES3
            )));
        }

        let index = files
            .iter()
            .enumerate()
            .map(|(i, f)| (normalize(&f.name.decode()), i))
            .collect();

        Ok(Bsa {
            version,
            files,
            mmap,
            data_start,
            index,
        })
    }

    /// Return the raw bytes for `record` as a zero-copy slice into the archive mapping.
    pub fn bytes(&self, record: &FileRecord) -> &[u8] {
        let start = self.data_start + record.offset as usize;
        &self.mmap[start..start + record.size as usize]
    }

    /// Look up a file by path, case-insensitively and tolerant of `/` vs `\` separators.
    /// Returns a zero-copy slice into the archive mapping on success. A hash lookup against
    /// an index built at open time.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let record = &self.files[*self.index.get(&normalize(name))?];
        Some(self.bytes(record))
    }
}

/// Normalize a path for comparison: lowercase, forward slashes to backslashes.
fn normalize(path: &str) -> String {
    path.chars()
        .map(|c| {
            if c == '/' {
                '\\'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Build a nom error anchored at `input` for use with the `?` operator.
fn nom_fail(input: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
    nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}
