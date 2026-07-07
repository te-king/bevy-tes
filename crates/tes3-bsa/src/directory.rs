//! The on-disk directory of a TES3 BSA archive, and zero-copy views over it.
//!
//! The directory is a 12-byte little-endian header — `version`, `hash_offset`, `count` —
//! followed by four parallel tables, every one ordered by [`name_hash`]:
//!
//! | section      | size        | contents                                    |
//! |--------------|-------------|---------------------------------------------|
//! | size/offset  | `count × 8` | `(u32 size, u32 offset)` per file           |
//! | name offsets | `count × 4` | `u32` offset of each name in the name blob  |
//! | name blob    | *variable*  | NUL-terminated Windows-1252 paths           |
//! | hash table   | `count × 8` | each name's hash as two `u32` halves        |
//!
//! `hash_offset` spans from the end of the header to the start of the hash table (so the
//! name blob's length is `hash_offset − count × 12`), and the file data section begins at
//! `12 + hash_offset + count × 8`; file `offset`s are relative to it.
//!
//! [`parse`] validates the whole geometry up front — tables in bounds, name offsets
//! inside the blob, hash table sorted, data extents inside the archive — and returns a
//! [`Directory`] borrowing each section as a slice. Every accessor after that serves
//! [`FileRecord`] views, hash lookups and file bytes straight out of those slices, with
//! no copies and no failure paths.

use nom::IResult;
use nom::number::complete::le_u32;
use tes_core::L1Str;
use tes_core::paths::normalize;

use crate::BsaError;

/// The only BSA layout version Morrowind/Tribunal/Bloodmoon use.
pub const VERSION_TES3: u32 = 0x100;

/// Bytes before the directory tables: `version`, `hash_offset`, `count`.
const HEADER_LEN: usize = 12;

/// Compute the TES3 hash of a data path — the key under which archive directories sort
/// their entries and the engine looks files up.
///
/// The path is hashed in its [normal form](tes_core::paths::normalize) (lowercase, `\`
/// separators — the form directories store natively): the first half of the bytes is
/// XOR-folded into the high 32 bits, the second half XOR-plus-rotate-folded into the low
/// 32 bits. Word placement and byte order are pinned against every entry of the vanilla
/// archives by the fixture tests.
pub fn name_hash(name: &str) -> u64 {
    let name = normalize(name);
    let bytes = name.as_bytes();
    let half = bytes.len() / 2;

    let mut high: u32 = 0;
    for (i, &b) in bytes[..half].iter().enumerate() {
        high ^= (b as u32) << ((i * 8) & 0x1F);
    }
    let mut low: u32 = 0;
    for (i, &b) in bytes[half..].iter().enumerate() {
        let temp = (b as u32) << ((i * 8) & 0x1F);
        low = (low ^ temp).rotate_right(temp & 0x1F);
    }
    (high as u64) << 32 | low as u64
}

/// Directory entry for a single archived file, borrowed from the archive mapping:
/// its name, lookup hash, and location within the data section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRecord<'a> {
    /// Path within the archive, e.g. `meshes\m\probe_journeyman_01.nif` (Windows-1252,
    /// backslash-separated).
    pub name: &'a L1Str,
    /// The directory's precomputed [`name_hash`] for `name`.
    pub hash: u64,
    /// Byte offset of this file's data within the archive's data section.
    pub offset: u32,
    /// Byte length of this file's data.
    pub size: u32,
}

/// An archive's validated directory: each section of the layout above, plus the data
/// section, borrowed as a slice. Produced by [`parse`]; every accessor is infallible,
/// because the geometry was checked up front. [`Bsa`](crate::Bsa) couples a `Directory`
/// to the mmap it borrows from via `self_cell`.
pub(crate) struct Directory<'a> {
    version: u32,
    /// `(u32 size, u32 offset)` per file; offsets are relative to `data`.
    sizes: &'a [u8],
    /// `u32` offset of each name within `names`.
    name_offsets: &'a [u8],
    /// The name blob: NUL-terminated Windows-1252 paths.
    names: &'a [u8],
    /// The hash table: each name's [`name_hash`] as two `u32` halves, sorted.
    hashes: &'a [u8],
    /// The file data section.
    data: &'a [u8],
}

impl<'a> Directory<'a> {
    pub(crate) fn version(&self) -> u32 {
        self.version
    }

    /// Number of archived files.
    pub(crate) fn len(&self) -> usize {
        self.name_offsets.len() / 4
    }

    /// Decode entry `i` as a zero-copy view. Panics if `i >= self.len()`.
    pub(crate) fn record(&self, i: usize) -> FileRecord<'a> {
        let size = u32_at(self.sizes, i * 8);
        let offset = u32_at(self.sizes, i * 8 + 4);

        let name_off = u32_at(self.name_offsets, i * 4) as usize;
        let name = &self.names[name_off..];
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());

        FileRecord {
            name: L1Str::from_bytes(&name[..end]),
            hash: hash_at(self.hashes, i),
            offset,
            size,
        }
    }

    /// Binary-search the hash table for `hash`, returning the entry index on a hit.
    pub(crate) fn find(&self, hash: u64) -> Option<usize> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if hash_at(self.hashes, mid) < hash {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (lo < self.len() && hash_at(self.hashes, lo) == hash).then_some(lo)
    }

    /// The raw bytes for `record`, as a zero-copy slice of the data section.
    pub(crate) fn bytes(&self, record: FileRecord<'_>) -> &'a [u8] {
        &self.data[record.offset as usize..][..record.size as usize]
    }
}

/// Compact by hand: a derived impl would dump the borrowed tables byte by byte.
impl std::fmt::Debug for Directory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Directory")
            .field("version", &self.version)
            .field("files", &self.len())
            .finish_non_exhaustive()
    }
}

/// Read a little-endian `u32` from a fixed position in an exact-length block.
fn u32_at(block: &[u8], byte: usize) -> u32 {
    u32::from_le_bytes(block[byte..byte + 4].try_into().expect("4 bytes"))
}

/// Read hash-table entry `i`: the first stored word is the high half of the sort key.
fn hash_at(hashes: &[u8], i: usize) -> u64 {
    (u32_at(hashes, i * 8) as u64) << 32 | u32_at(hashes, i * 8 + 4) as u64
}

fn header(input: &[u8]) -> IResult<&[u8], (u32, u32, u32)> {
    let (input, version) = le_u32(input)?;
    let (input, hash_offset) = le_u32(input)?;
    let (input, count) = le_u32(input)?;
    Ok((input, (version, hash_offset, count)))
}

/// Parse and validate an archive's directory into borrowed section slices. After this
/// succeeds, every [`Directory`] accessor is panic-free: all table bounds, name offsets,
/// and data extents have been checked, and the hash table is verified sorted (the
/// binary-search precondition).
pub(crate) fn parse(archive: &[u8]) -> Result<Directory<'_>, BsaError> {
    let (_, (version, hash_offset, count)) =
        header(archive).map_err(|_| BsaError::Parse("truncated BSA header".into()))?;
    if version != VERSION_TES3 {
        return Err(BsaError::Parse(format!(
            "unsupported BSA version {version:#x} (expected {VERSION_TES3:#x})"
        )));
    }
    let (hash_offset, count) = (hash_offset as usize, count as usize);

    let names_len = hash_offset.checked_sub(count * 12).ok_or_else(|| {
        BsaError::Parse(format!(
            "hash offset {hash_offset} leaves no room for {count} directory entries"
        ))
    })?;
    let data_start = HEADER_LEN + hash_offset + count * 8;
    if archive.len() < data_start {
        return Err(BsaError::Parse(format!(
            "truncated BSA directory: {} bytes, directory alone needs {data_start}",
            archive.len()
        )));
    }

    let dir = Directory {
        version,
        sizes: &archive[HEADER_LEN..HEADER_LEN + count * 8],
        name_offsets: &archive[HEADER_LEN + count * 8..HEADER_LEN + count * 12],
        names: &archive[HEADER_LEN + count * 12..HEADER_LEN + count * 12 + names_len],
        hashes: &archive[HEADER_LEN + hash_offset..data_start],
        data: &archive[data_start..],
    };
    let mut prev_hash = 0;
    for i in 0..count {
        let name_off = u32_at(dir.name_offsets, i * 4) as usize;
        if name_off > dir.names.len() {
            return Err(BsaError::Parse(format!(
                "BSA entry {i}: name offset {name_off} outside the name blob"
            )));
        }
        let record = dir.record(i);
        if record.offset as u64 + record.size as u64 > dir.data.len() as u64 {
            return Err(BsaError::Parse(format!(
                "BSA entry {i}: data extends past the end of the archive"
            )));
        }
        if record.hash < prev_hash {
            return Err(BsaError::Parse(format!(
                "BSA hash table is not sorted at entry {i}"
            )));
        }
        prev_hash = record.hash;
    }
    Ok(dir)
}

/// Test-only builder for syntactically valid archives, shared with the crate-root tests.
#[cfg(test)]
pub(crate) mod testutil {
    use super::{VERSION_TES3, name_hash};

    /// Assemble an archive from `(name, data)` pairs, hash-sorted like the real tool.
    pub(crate) fn build_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut entries: Vec<_> = files.iter().map(|&(n, d)| (n, d, name_hash(n))).collect();
        entries.sort_by_key(|&(_, _, hash)| hash);

        let mut names = Vec::new();
        let mut name_offsets = Vec::new();
        for (name, _, _) in &entries {
            name_offsets.push(names.len() as u32);
            names.extend_from_slice(name.as_bytes());
            names.push(0);
        }

        let count = entries.len();
        let hash_offset = count * 12 + names.len();
        let mut out = Vec::new();
        out.extend_from_slice(&VERSION_TES3.to_le_bytes());
        out.extend_from_slice(&(hash_offset as u32).to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        let mut data = Vec::new();
        for (_, bytes, _) in &entries {
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        for offset in &name_offsets {
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out.extend_from_slice(&names);
        for (_, _, hash) in &entries {
            out.extend_from_slice(&((hash >> 32) as u32).to_le_bytes());
            out.extend_from_slice(&(*hash as u32).to_le_bytes());
        }
        out.extend_from_slice(&data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::build_archive;
    use super::*;

    const FILES: &[(&str, &[u8])] = &[
        (r"Meshes\B\Thing.NIF", b"NIF-DATA"),
        (r"textures\wood.dds", b"DDS"),
    ];

    #[test]
    fn parses_and_serves_records() {
        let archive = build_archive(FILES);
        let dir = parse(&archive).expect("valid archive");
        assert_eq!(dir.version(), VERSION_TES3);
        assert_eq!(dir.len(), 2);
        assert_eq!(
            dir.data.len(),
            FILES.iter().map(|(_, d)| d.len()).sum::<usize>()
        );

        for &(name, data, ..) in FILES {
            let i = dir
                .find(name_hash(name))
                .unwrap_or_else(|| panic!("{name} should be found"));
            let record = dir.record(i);
            assert_eq!(record.name, name);
            assert_eq!(record.hash, name_hash(name));
            assert_eq!(dir.bytes(record), data);
        }
        assert_eq!(dir.find(name_hash("nowhere.nif")), None);
    }

    #[test]
    fn hash_normalizes_case_and_separators() {
        assert_eq!(
            name_hash("Meshes/B/Thing.NIF"),
            name_hash(r"meshes\b\thing.nif")
        );
        assert_ne!(
            name_hash(r"meshes\b\thing.nif"),
            name_hash(r"meshes\b\thing.nif2")
        );
    }

    #[test]
    fn rejects_truncated_header() {
        let err = parse(&[0; 8]).unwrap_err();
        assert!(err.to_string().contains("header"), "{err}");
    }

    #[test]
    fn rejects_wrong_version() {
        let mut archive = build_archive(FILES);
        archive[..4].copy_from_slice(&0x101u32.to_le_bytes());
        let err = parse(&archive).unwrap_err();
        assert!(err.to_string().contains("version"), "{err}");
    }

    #[test]
    fn rejects_hash_offset_smaller_than_the_tables() {
        let mut archive = build_archive(FILES);
        archive[4..8].copy_from_slice(&23u32.to_le_bytes()); // < count × 12
        let err = parse(&archive).unwrap_err();
        assert!(err.to_string().contains("no room"), "{err}");
    }

    #[test]
    fn rejects_truncated_directory() {
        let archive = build_archive(FILES);
        let err = parse(&archive[..HEADER_LEN + 4]).unwrap_err();
        assert!(err.to_string().contains("truncated"), "{err}");
    }

    #[test]
    fn rejects_name_offset_outside_the_blob() {
        let mut archive = build_archive(FILES);
        let name_offsets = HEADER_LEN + 2 * 8;
        archive[name_offsets..name_offsets + 4].copy_from_slice(&999u32.to_le_bytes());
        let err = parse(&archive).unwrap_err();
        assert!(err.to_string().contains("name offset"), "{err}");
    }

    #[test]
    fn rejects_unsorted_hash_table() {
        let mut archive = build_archive(FILES);
        let hashes = archive.len() - FILES.iter().map(|(_, d)| d.len()).sum::<usize>() - 2 * 8;
        archive[hashes..hashes + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = parse(&archive).unwrap_err();
        assert!(err.to_string().contains("not sorted"), "{err}");
    }

    #[test]
    fn rejects_data_extending_past_the_archive() {
        let mut archive = build_archive(FILES);
        archive[HEADER_LEN..HEADER_LEN + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = parse(&archive).unwrap_err();
        assert!(err.to_string().contains("past the end"), "{err}");
    }
}
