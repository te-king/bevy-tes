//! `tes-nif` — parser for NetImmerse/Gamebryo `.nif` model files (TES3 / Morrowind).
//!
//! # Scope (scaffold)
//!
//! This crate currently parses the NIF **header** and exposes the block-stream framing
//! primitive ([`block_type`]); full typed block bodies (`NiNode`, `NiTriShapeData`, …)
//! are the next step. The reason it stops at the header is a property of the format
//! version Morrowind uses:
//!
//! Morrowind ships NIF version **4.0.0.2** (`0x0400_0002`). Unlike the later 20.x NIFs
//! (Oblivion+), a 4.0.0.2 file has **no block-type table and no block-size table** in its
//! header. Instead each block is preceded inline by its type name (a length-prefixed
//! string), and block sizes are *implicit* — the only way to find where one block ends and
//! the next begins is to fully decode the current block's body. So a generic
//! `Unknown { bytes }` block that lets the whole file round-trip is not possible here;
//! traversal requires a body parser per block type, which is deferred.
//!
//! ```no_run
//! let bytes = std::fs::read("model.nif").unwrap();
//! let nif = tes_nif::Nif::parse(&bytes).unwrap();
//! assert_eq!(nif.header.version, tes_nif::VERSION_TES3);
//! ```

use nom::IResult;
use nom::bytes::complete::take;
use nom::number::complete::le_u32;
use std::fmt;
use tes_core::L1String;

/// The NIF version Morrowind/Tribunal/Bloodmoon use: `4.0.0.2`.
pub const VERSION_TES3: u32 = 0x0400_0002;

/// Error returned when reading or parsing a NIF file.
#[derive(Debug)]
pub enum NifError {
    /// I/O failure while reading the file from disk.
    Io(std::io::Error),
    /// The byte stream could not be parsed as a supported NIF.
    Parse(String),
}

impl fmt::Display for NifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NifError::Io(e) => write!(f, "I/O error: {e}"),
            NifError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for NifError {}

impl From<std::io::Error> for NifError {
    fn from(e: std::io::Error) -> Self {
        NifError::Io(e)
    }
}

/// The NIF header (version 4.0.0.2 layout): a newline-terminated identifier string, the
/// numeric version, and the number of blocks that follow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NifHeader {
    /// The version identifier line, e.g. `NetImmerse File Format, Version 4.0.0.2`
    /// (without the trailing newline).
    pub ident: L1String,
    /// Numeric version, e.g. [`VERSION_TES3`].
    pub version: u32,
    /// Number of blocks following the header.
    pub num_blocks: u32,
}

/// A parsed NIF file. For now this carries only the [`NifHeader`]; the typed block graph
/// is future work (see the crate docs for why traversal needs per-type body parsers).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Nif {
    pub header: NifHeader,
}

impl Nif {
    /// Parse a NIF from an in-memory byte slice. Validates the version is [`VERSION_TES3`].
    pub fn parse(input: &[u8]) -> Result<Nif, NifError> {
        let (_, header) =
            nif_header(input).map_err(|e| NifError::Parse(format!("header: {e:?}")))?;
        if header.version != VERSION_TES3 {
            return Err(NifError::Parse(format!(
                "unsupported NIF version {:#010x} (expected {:#010x})",
                header.version, VERSION_TES3
            )));
        }
        Ok(Nif { header })
    }
}

/// Parse the version-4.0.0.2 header: identifier line (terminated by `\n`), then the
/// version and block-count `u32`s.
fn nif_header(input: &[u8]) -> IResult<&[u8], NifHeader> {
    let nl = input
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| nom_fail(input))?;
    let ident = L1String::from_bytes(input[..nl].to_vec());
    let input = &input[nl + 1..];

    let (input, version) = le_u32(input)?;
    let (input, num_blocks) = le_u32(input)?;
    Ok((
        input,
        NifHeader {
            ident,
            version,
            num_blocks,
        },
    ))
}

/// Read a block's inline type name: a little-endian `u32` length prefix followed by that
/// many bytes (e.g. `NiNode`). This is the framing every 4.0.0.2 block begins with; it is
/// the entry point future per-type body parsers will dispatch on.
pub fn block_type(input: &[u8]) -> IResult<&[u8], L1String> {
    let (input, len) = le_u32(input)?;
    let (input, bytes) = take(len as usize)(input)?;
    Ok((input, L1String::from_bytes(bytes.to_vec())))
}

/// Build a nom error anchored at `input` for use with the `?` operator.
fn nom_fail(input: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
    nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_header() {
        let mut bytes = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
        bytes.extend_from_slice(&VERSION_TES3.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes()); // num_blocks
        // First block type name, length-prefixed.
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(b"NiNode");

        let nif = Nif::parse(&bytes).unwrap();
        assert_eq!(nif.header.version, VERSION_TES3);
        assert_eq!(nif.header.num_blocks, 3);
        assert_eq!(nif.header.ident, "NetImmerse File Format, Version 4.0.0.2");

        // The bytes after the header begin the first block's inline type name.
        let after_header = &bytes[40 + 8..];
        let (_, ty) = block_type(after_header).unwrap();
        assert_eq!(ty, "NiNode");
    }

    #[test]
    fn rejects_wrong_version() {
        let mut bytes = b"NetImmerse File Format, Version 10.0.1.0\n".to_vec();
        bytes.extend_from_slice(&0x0A01_0000u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(Nif::parse(&bytes).is_err());
    }
}
