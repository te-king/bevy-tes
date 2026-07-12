//! TES3 (Morrowind) BSA archive parsing.
//!
//! A BSA is a flat archive: a small directory (file sizes, offsets, names and lookup
//! hashes) followed by the concatenated file data. Opening an archive mmaps the file and
//! builds an in-memory directory mapping each path to its data; individual file bytes
//! are served as zero-copy slices into the mapping:
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
use self_cell::self_cell;
use std::collections::HashMap;
use std::path::Path;
use tes_core::TesPath;

/// The only BSA layout version Morrowind/Tribunal/Bloodmoon use.
pub const VERSION_TES3: u32 = 0x100;

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

/// An open TES3 BSA archive. Holds an mmap of the archive file and an in-memory directory
/// mapping each path to its data slice; file bytes are served as zero-copy slices into
/// the mapping.
///
/// Dropping the `Bsa` releases the mmap (the OS unmaps the pages).
#[derive(Debug)]
pub struct Bsa(BsaInternal);

self_cell!(
    struct BsaInternal {
        owner: memmap2::Mmap,

        #[covariant]
        dependent: BsaDirectory,
    }

    impl { Debug }
);

#[derive(Debug)]
struct BsaDirectory<'a> {
    version: u32,
    /// File name → data-section slice, both borrowed from the archive mapping. The
    /// [`TesPath`] keys keep the archive's original bytes but compare and hash in the
    /// path normal form, so lookups are case- and separator-insensitive.
    files: HashMap<&'a TesPath, &'a [u8]>,
}

impl<'a> BsaDirectory<'a> {
    /// Parse a TES3 BSA directory out of the full archive mapping. Returns the file table
    /// as name → data-slice pairs borrowed from `mmap`; `version` is read but not
    /// validated here.
    fn parse_bsa(input: &'a [u8]) -> IResult<&'a [u8], BsaDirectory<'a>> {
        let (input, version) = le_u32(input)?;
        let (input, hash_offset) = le_u32(input)?;
        let (input, count) = le_u32(input)?;

        // After the 12-byte header come three parallel tables, then a name blob,
        // then the hash table; `hash_offset` spans everything between the header and
        // the hash table.
        let (input, size_offsets) = take(count * 8)(input)?; // (u32 size, u32 offset)
        let (input, name_offsets) = take(count * 4)(input)?; // u32 into name blob
        let names_len = hash_offset
            .checked_sub(count * 12)
            .ok_or_else(|| nom_fail(input))?;
        let (input, names_buffer) = take(names_len)(input)?;
        let (data_buffer, _hashes) = take(count * 8)(input)?; // two u32 halves per file

        // Each name offset points into the name blob; each (size, offset) pair locates a
        // file's bytes within the data section. Out-of-range indices mean a corrupt
        // archive, so surface them as parse failures rather than panicking.
        let names = name_offsets.chunks_exact(4).map(|chunk| {
            let off = u32::from_le_bytes(chunk.try_into().unwrap()) as usize;
            names_buffer
                .get(off..)
                .map(TesPath::from_bytes_until_null)
                .ok_or_else(|| nom_fail(names_buffer))
        });
        let data_slices = size_offsets.chunks_exact(8).map(|chunk| {
            let size = u32::from_le_bytes(chunk[0..4].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;
            data_buffer
                .get(offset..offset + size)
                .ok_or_else(|| nom_fail(data_buffer))
        });

        let files = names
            .zip(data_slices)
            .map(|(name, data)| Ok((name?, data?)))
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok((&[], BsaDirectory { version, files }))
    }
}

impl Bsa {
    /// Open a BSA archive at `path`, mapping it into memory and building the file
    /// directory. File data is not copied; bytes are served on demand as zero-copy
    /// slices via [`get`](Self::get) and [`files`](Self::files).
    pub fn open(path: impl AsRef<Path>) -> Result<Bsa, BsaError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: We open the file read-only and never write through the mapping.
        // Concurrent modification of game data files by another process is not expected.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let internal = BsaInternal::try_new(mmap, |mmap| {
            let (_, directory) =
                BsaDirectory::parse_bsa(mmap).map_err(|e| BsaError::Parse(format!("{e:?}")))?;
            if directory.version != VERSION_TES3 {
                return Err(BsaError::Parse(format!(
                    "unsupported BSA version {:#x} (expected {:#x})",
                    directory.version, VERSION_TES3
                )));
            }
            Ok(directory)
        })?;
        Ok(Bsa(internal))
    }

    /// The archive's layout version — always [`VERSION_TES3`] for an archive that opened
    /// successfully.
    pub fn version(&self) -> u32 {
        self.0.borrow_dependent().version
    }

    /// Number of files in the archive.
    pub fn len(&self) -> usize {
        self.0.borrow_dependent().files.len()
    }

    /// Whether the archive contains no files.
    pub fn is_empty(&self) -> bool {
        self.0.borrow_dependent().files.is_empty()
    }

    /// Iterate over every `(path, bytes)` entry, in arbitrary order. Both sides are
    /// zero-copy views into the archive mapping.
    pub fn files(&self) -> impl Iterator<Item = (&TesPath, &[u8])> {
        self.0
            .borrow_dependent()
            .files
            .iter()
            .map(|(&name, &data)| (name, data))
    }

    /// Look up a file by path, case-insensitively and tolerant of `/` vs `\` separators.
    /// Returns a zero-copy slice into the archive mapping on success. A hash lookup
    /// against the directory built at open time.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.0
            .borrow_dependent()
            .files
            .get(TesPath::from_bytes(name.as_bytes()))
            .copied()
    }
}

/// Build a nom error anchored at `input` for use with the `?` operator.
fn nom_fail(input: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
    nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}
