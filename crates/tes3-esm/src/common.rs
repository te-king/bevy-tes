//! Core parsing primitives shared by every record type.
//!
//! The TES3 format is a flat sequence of *records*; each record has a fixed 16-byte
//! header followed by a block of *subrecords* (also called fields). Both records and
//! subrecords are framed identically: a 4-byte ASCII tag, a little-endian `u32` size,
//! then that many bytes of payload. All multi-byte integers are little-endian.

use nom::IResult;
use nom::bytes::complete::take;
use std::fmt;

// Re-export the little-endian leaf parsers so record modules have a single import site.
pub use nom::number::complete::{le_f32, le_i8, le_i16, le_i32, le_u8, le_u16, le_u32, le_u64};

// Format-agnostic helpers and types now live in `tes_core`; re-export them here so the
// record modules keep a single `crate::common` import site.
pub use tes_core::bytes::{finish, fixed_l1str, l1, parse_or_default};
pub use tes_core::math::{Color, color};

/// A 4-byte record or subrecord tag, e.g. `b"TES3"` or `b"NAME"`.
pub type Tag = [u8; 4];

/// Error type returned by the public parse entry points.
#[derive(Debug)]
pub enum EsmError {
    /// I/O failure while reading a file from disk.
    Io(std::io::Error),
    /// The byte stream could not be parsed as a valid TES3 plugin.
    Parse(String),
}

impl fmt::Display for EsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EsmError::Io(e) => write!(f, "I/O error: {e}"),
            EsmError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for EsmError {}

impl From<std::io::Error> for EsmError {
    fn from(e: std::io::Error) -> Self {
        EsmError::Io(e)
    }
}

/// Bitflags found in a record header's flags field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecordFlags(pub u32);

impl RecordFlags {
    pub const DELETED: u32 = 0x0000_0020;
    pub const PERSISTENT: u32 = 0x0000_0400;
    pub const INITIALLY_DISABLED: u32 = 0x0000_0800;
    pub const BLOCKED: u32 = 0x0000_2000;

    pub fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub fn is_deleted(self) -> bool {
        self.contains(Self::DELETED)
    }
    pub fn is_persistent(self) -> bool {
        self.contains(Self::PERSISTENT)
    }
    pub fn is_initially_disabled(self) -> bool {
        self.contains(Self::INITIALLY_DISABLED)
    }
    pub fn is_blocked(self) -> bool {
        self.contains(Self::BLOCKED)
    }
}

/// The 16-byte record header that precedes every record's data block.
#[derive(Debug, Clone, Copy)]
pub struct RecordHeader {
    pub tag: Tag,
    pub size: u32,
    pub unused: u32,
    pub flags: RecordFlags,
}

/// A single subrecord (field): its tag and a borrowed slice of its payload.
#[derive(Debug, Clone, Copy)]
pub struct Subrecord<'a> {
    pub tag: Tag,
    pub data: &'a [u8],
}

/// Read a 4-byte tag.
pub fn tag(input: &[u8]) -> IResult<&[u8], Tag> {
    let (input, bytes) = take(4usize)(input)?;
    Ok((input, [bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Parse a record header (tag, size, unused, flags).
pub fn record_header(input: &[u8]) -> IResult<&[u8], RecordHeader> {
    let (input, tag) = tag(input)?;
    let (input, size) = le_u32(input)?;
    let (input, unused) = le_u32(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        RecordHeader {
            tag,
            size,
            unused,
            flags: RecordFlags(flags),
        },
    ))
}

/// Parse a single subrecord: tag, size, then `size` bytes of payload.
pub fn subrecord(input: &[u8]) -> IResult<&[u8], Subrecord<'_>> {
    let (input, tag) = tag(input)?;
    let (input, size) = le_u32(input)?;
    let (input, data) = take(size as usize)(input)?;
    Ok((input, Subrecord { tag, data }))
}

/// A non-allocating iterator over the subrecords in a record's data block.
///
/// Subrecords are parsed on demand straight from the borrowed buffer. A malformed or
/// truncated subrecord simply ends iteration (the record keeps whatever fields parsed
/// before it) rather than erroring — valid files always consume the block exactly.
#[derive(Debug, Clone)]
pub struct Subrecords<'a> {
    input: &'a [u8],
}

impl<'a> Subrecords<'a> {
    pub fn new(input: &'a [u8]) -> Subrecords<'a> {
        Subrecords { input }
    }
}

impl<'a> Iterator for Subrecords<'a> {
    type Item = Subrecord<'a>;

    fn next(&mut self) -> Option<Subrecord<'a>> {
        if self.input.is_empty() {
            return None;
        }
        match subrecord(self.input) {
            Ok((rest, sub)) => {
                self.input = rest;
                Some(sub)
            }
            Err(_) => {
                self.input = &[];
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_record_header() {
        // "TES3", size = 0x34, unused = 0, flags = 0.
        let bytes = b"TES3\x34\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let (_, hdr) = record_header(bytes).unwrap();
        assert_eq!(&hdr.tag, b"TES3");
        assert_eq!(hdr.size, 0x134);
        assert_eq!(hdr.flags, RecordFlags(0));
    }

    #[test]
    fn parses_subrecords() {
        // NAME "abc\0" then FNAM "hi\0".
        let bytes = b"NAME\x04\x00\x00\x00abc\x00FNAM\x03\x00\x00\x00hi\x00";
        let subs: Vec<_> = Subrecords::new(bytes).collect();
        assert_eq!(subs.len(), 2);
        assert_eq!(&subs[0].tag, b"NAME");
        assert_eq!(l1(subs[0].data), "abc");
        assert_eq!(&subs[1].tag, b"FNAM");
        assert_eq!(l1(subs[1].data), "hi");
    }

    #[test]
    fn l1_stops_at_nul() {
        // Trailing NUL padding is dropped; the raw bytes are otherwise untouched.
        assert_eq!(l1(b"a\x00bc").as_bytes(), b"a");
        assert_eq!(l1(b"abc").as_bytes(), b"abc");
    }
}
