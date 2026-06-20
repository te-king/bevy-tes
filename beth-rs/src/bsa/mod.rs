//! TES3 (Morrowind) BSA archive parsing.
//!
//! A BSA is a flat archive: a small directory (file sizes, offsets, names and lookup
//! hashes) followed by the concatenated file data. Parsing is zero-copy — each
//! [`FileEntry`] borrows its bytes directly from the archive buffer, which must outlive
//! the [`Bsa`]:
//!
//! ```no_run
//! let bytes = std::fs::read("beth-rs/tests/Morrowind.bsa").unwrap();
//! let bsa = beth_rs::Bsa::parse(&bytes).unwrap();
//! if let Some(file) = bsa.get(r"meshes\m\probe_journeyman_01.nif") {
//!     println!("{} bytes", file.data.len());
//! }
//! ```

use crate::types::latin1::L1Str;
use nom::IResult;
use nom::bytes::complete::take;
use nom::number::complete::le_u32;
use std::fmt;

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

/// A single archived file: its name, its bytes (borrowed from the archive) and the
/// 64-bit lookup hash stored in the directory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileEntry<'a> {
    /// Path within the archive, e.g. `meshes\m\probe_journeyman_01.nif` (Windows-1252,
    /// backslash-separated).
    pub name: &'a L1Str,
    /// The file's raw contents.
    pub data: &'a [u8],
    /// The directory's precomputed lookup hash for `name`.
    pub hash: u64,
}

/// A parsed TES3 BSA archive, borrowing from the source buffer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bsa<'a> {
    pub version: u32,
    pub files: Vec<FileEntry<'a>>,
}

/// Read a little-endian `u32` from a fixed position in an exact-length block.
fn u32_at(block: &[u8], byte: usize) -> u32 {
    u32::from_le_bytes(block[byte..byte + 4].try_into().expect("4 bytes"))
}

impl<'a> Bsa<'a> {
    /// Parse an archive from an in-memory byte slice. The returned [`Bsa`] borrows from
    /// `input`, so keep the buffer alive for as long as the archive is used.
    pub fn parse(input: &'a [u8]) -> Result<Bsa<'a>, BsaError> {
        let parse = |input| -> IResult<&'a [u8], Bsa<'a>> {
            let (input, version) = le_u32(input)?;
            let (input, hash_offset) = le_u32(input)?;
            let (input, count) = le_u32(input)?;
            let count = count as usize;

            // After the 12-byte header come three parallel tables, then a name blob,
            // then the hash table; `hash_offset` spans everything between the header and
            // the hash table.
            let (input, size_offsets) = take(count * 8)(input)?; // (u32 size, u32 offset)
            let (input, name_offsets) = take(count * 4)(input)?; // u32 into name blob
            let names_len = (hash_offset as usize)
                .checked_sub(count * 12)
                .ok_or_else(|| nom_fail(input))?;
            let (input, names) = take(names_len)(input)?;
            let (data, hashes) = take(count * 8)(input)?; // two u32 halves per file

            let mut files = Vec::with_capacity(count);
            for i in 0..count {
                let size = u32_at(size_offsets, i * 8) as usize;
                let offset = u32_at(size_offsets, i * 8 + 4) as usize;
                let name_off = u32_at(name_offsets, i * 4) as usize;

                let name_bytes = names.get(name_off..).ok_or_else(|| nom_fail(data))?;
                let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
                let name = L1Str::from_bytes(&name_bytes[..end]);

                let file_data = data
                    .get(offset..offset + size)
                    .ok_or_else(|| nom_fail(data))?;
                let hash = u64::from_le_bytes(hashes[i * 8..i * 8 + 8].try_into().unwrap());

                files.push(FileEntry { name, data: file_data, hash });
            }

            Ok((data, Bsa { version, files }))
        };

        let (_, bsa) = parse(input).map_err(|e| BsaError::Parse(format!("{e:?}")))?;
        if bsa.version != VERSION_TES3 {
            return Err(BsaError::Parse(format!(
                "unsupported BSA version {:#x} (expected {:#x})",
                bsa.version, VERSION_TES3
            )));
        }
        Ok(bsa)
    }

    /// Look up a file by path, case-insensitively and tolerant of `/` vs `\` separators.
    /// Linear scan; build your own index if you need many lookups.
    pub fn get(&self, name: &str) -> Option<&FileEntry<'a>> {
        let key = normalize(name);
        self.files
            .iter()
            .find(|f| normalize(&f.name.decode()) == key)
    }
}

/// Normalize a path for comparison: lowercase, forward slashes to backslashes.
fn normalize(path: &str) -> String {
    path.chars()
        .map(|c| if c == '/' { '\\' } else { c.to_ascii_lowercase() })
        .collect()
}

/// Build a nom error anchored at `input` for use with the `?` operator.
fn nom_fail(input: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
    nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}
