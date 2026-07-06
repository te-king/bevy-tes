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

/// A 4-byte record or subrecord tag, e.g. `TES3` or `NAME`.
///
/// Wraps the raw bytes (`.0`, so subrecord loops can `match &sub.tag.0` against byte
/// literals) and displays as the ASCII it holds — `TES3`, not `[84, 69, 83, 51]` — with
/// non-printable bytes escaped.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag(pub [u8; 4]);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &b in &self.0 {
            if (0x20..0x7f).contains(&b) {
                write!(f, "{}", b as char)?;
            } else {
                write!(f, "\\x{b:02x}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({self})")
    }
}

impl From<[u8; 4]> for Tag {
    fn from(bytes: [u8; 4]) -> Tag {
        Tag(bytes)
    }
}

impl PartialEq<[u8; 4]> for Tag {
    fn eq(&self, other: &[u8; 4]) -> bool {
        self.0 == *other
    }
}

impl PartialEq<&[u8; 4]> for Tag {
    fn eq(&self, other: &&[u8; 4]) -> bool {
        self.0 == **other
    }
}

/// Error type returned by the public parse entry points.
#[derive(Debug, thiserror::Error)]
pub enum EsmError {
    /// I/O failure while reading a file from disk.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The byte stream could not be parsed as a valid TES3 plugin.
    #[error("parse error: {0}")]
    Parse(String),
}

/// Generate a nom parser for a fixed-layout subrecord struct from a single
/// `field: parser` table, so parse order and struct construction cannot drift apart
/// (the struct definition stays hand-written — it carries the field docs):
///
/// ```ignore
/// parse_struct! {
///     fn misc_data -> MiscData {
///         weight: le_f32,
///         value: le_u32,
///         flags: le_u32,
///     }
/// }
/// ```
///
/// Parsers are any `Fn(&[u8]) -> IResult<&[u8], T>` expression (`le_u32`,
/// `fixed_l1str(32)`, a local helper, …). The `fn` takes an optional visibility
/// (`pub fn` for the parsers `shared` exports). Layouts with padding to skip, computed
/// fields or loops don't fit and stay hand-written.
macro_rules! parse_struct {
    ($(#[$meta:meta])* $vis:vis fn $name:ident -> $ty:ident {
        $( $field:ident : $parser:expr ),+ $(,)?
    }) => {
        $(#[$meta])*
        $vis fn $name(input: &[u8]) -> nom::IResult<&[u8], $ty> {
            $( let (input, $field) = ($parser)(input)?; )+
            Ok((input, $ty { $( $field ),+ }))
        }
    };
}
pub(crate) use parse_struct;

bitflags::bitflags! {
    /// Bitflags found in a record header's flags field.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RecordFlags: u32 {
        const DELETED = 0x0000_0020;
        const PERSISTENT = 0x0000_0400;
        const INITIALLY_DISABLED = 0x0000_0800;
        const BLOCKED = 0x0000_2000;
    }
}

/// The integer widths a flags subrecord field can be stored as, tying each width to its
/// little-endian parser so [`flags`] can be generic over the bitflags type.
pub trait FlagBits: bitflags::Bits {
    fn parse_le(input: &[u8]) -> IResult<&[u8], Self>;
}

impl FlagBits for u8 {
    fn parse_le(input: &[u8]) -> IResult<&[u8], u8> {
        le_u8(input)
    }
}

impl FlagBits for u16 {
    fn parse_le(input: &[u8]) -> IResult<&[u8], u16> {
        le_u16(input)
    }
}

impl FlagBits for u32 {
    fn parse_le(input: &[u8]) -> IResult<&[u8], u32> {
        le_u32(input)
    }
}

/// Parse a little-endian integer into a typed [`bitflags`] value; the flags type (and so
/// the integer width) is inferred from the destination field. Unknown bits are retained
/// verbatim (`from_bits_retain`), so undocumented data survives parsing untouched.
pub fn flags<F>(input: &[u8]) -> IResult<&[u8], F>
where
    F: bitflags::Flags,
    F::Bits: FlagBits,
{
    let (input, bits) = F::Bits::parse_le(input)?;
    Ok((input, F::from_bits_retain(bits)))
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
    Ok((input, Tag([bytes[0], bytes[1], bytes[2], bytes[3]])))
}

/// Parse a record header (tag, size, unused, flags).
pub fn record_header(input: &[u8]) -> IResult<&[u8], RecordHeader> {
    let (input, tag) = tag(input)?;
    let (input, size) = le_u32(input)?;
    let (input, unused) = le_u32(input)?;
    let (input, flags) = flags(input)?;
    Ok((
        input,
        RecordHeader {
            tag,
            size,
            unused,
            flags,
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
        assert_eq!(hdr.tag, *b"TES3");
        assert_eq!(hdr.tag.to_string(), "TES3");
        assert_eq!(hdr.size, 0x134);
        assert_eq!(hdr.flags, RecordFlags::empty());
    }

    #[test]
    fn parses_subrecords() {
        // NAME "abc\0" then FNAM "hi\0".
        let bytes = b"NAME\x04\x00\x00\x00abc\x00FNAM\x03\x00\x00\x00hi\x00";
        let subs: Vec<_> = Subrecords::new(bytes).collect();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].tag, *b"NAME");
        assert_eq!(l1(subs[0].data), "abc");
        assert_eq!(subs[1].tag, *b"FNAM");
        assert_eq!(l1(subs[1].data), "hi");
    }

    #[test]
    fn l1_stops_at_nul() {
        // Trailing NUL padding is dropped; the raw bytes are otherwise untouched.
        assert_eq!(l1(b"a\x00bc").as_bytes(), b"a");
        assert_eq!(l1(b"abc").as_bytes(), b"abc");
    }
}
