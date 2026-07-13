//! TES3 (Morrowind) plugin format parsing.
//!
//! A plugin file is a flat sequence of records: a leading [`Tes3`](records::tes3::Tes3)
//! header followed by content records. Parsing is zero-copy: the parsed [`EsmDirectory`]
//! and its records borrow their strings ([`&L1Str`](crate::L1Str)) and binary blobs from
//! the input buffer, which must therefore outlive the directory.
//!
//! [`Esm`] bundles the buffer and the parsed [`EsmDirectory`] view in one owned value —
//! a self-referential wrapper, mirroring [`tes3_bsa::Bsa`](https://docs.rs/tes3-bsa) — so
//! a single value carries both. This is the usual entry point:
//!
//! ```no_run
//! let bytes = std::fs::read("data/Morrowind.esm").unwrap();
//! let esm = tes3_esm::Esm::parse(bytes).unwrap();
//! println!("{} records", esm.directory().records.len());
//! ```
//!
//! To parse a buffer you already own and keep borrowing yourself, call
//! [`EsmDirectory::parse`] directly.

pub mod common;
mod macros;
pub mod records;
pub mod shared;

use common::record_header;
use nom::bytes::complete::take;
use self_cell::self_cell;

pub use common::EsmError;
pub use records::Record;
pub use records::tes3::Tes3;
pub use tes_core::{L1Str, L1String};

/// An owned, parsed TES3 plugin (`.esm`/`.esp`): the raw file bytes plus the zero-copy
/// [`EsmDirectory`] view borrowing them (reach it via [`Esm::directory`]).
///
/// A self-referential wrapper mirroring [`tes3_bsa::Bsa`](https://docs.rs/tes3-bsa): the
/// buffer and the records that borrow it travel together, so callers hold one value
/// instead of juggling a buffer and a view with a lifetime. Dropping the `Esm` frees the
/// buffer.
pub struct Esm(EsmInternal);

self_cell!(
    struct EsmInternal {
        owner: Vec<u8>,

        #[covariant]
        dependent: EsmDirectory,
    }
);

impl Esm {
    /// Parse `bytes` into an owned plugin. The buffer is moved into the returned value and
    /// the parsed records borrow it; no record data is copied out.
    pub fn parse(bytes: Vec<u8>) -> Result<Esm, EsmError> {
        let internal = EsmInternal::try_new(bytes, |bytes| EsmDirectory::parse(bytes))?;
        Ok(Esm(internal))
    }

    /// Wrap an in-memory [`EsmDirectory<'static>`] (e.g. a synthetic test plugin built
    /// from `&'static` literals) without a backing buffer.
    pub fn from_static(directory: EsmDirectory<'static>) -> Esm {
        Esm(EsmInternal::new(Vec::new(), |_| directory))
    }

    /// The parsed plugin directory: header plus all records in file order.
    pub fn directory(&self) -> &EsmDirectory<'_> {
        self.0.borrow_dependent()
    }
}

// Manual: self_cell's generated Debug would print the raw file bytes.
impl std::fmt::Debug for Esm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Esm").field(self.directory()).finish()
    }
}

/// A fully parsed TES3 plugin directory: the header plus every record, borrowing their
/// strings and blobs from the input buffer, which must outlive it. For an owned value that
/// carries the buffer with it, see [`Esm`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EsmDirectory<'a> {
    /// The leading `TES3` header record.
    pub header: Tes3<'a>,
    /// All content records following the header, in file order.
    pub records: Vec<Record<'a>>,
}

impl<'a> EsmDirectory<'a> {
    /// Parse a plugin from an in-memory byte slice. Zero-copy: the returned
    /// [`EsmDirectory`] borrows `input`.
    pub fn parse(input: &'a [u8]) -> Result<EsmDirectory<'a>, EsmError> {
        let mut remaining = input;
        let mut records = Vec::new();
        let mut header: Option<Tes3> = None;

        while !remaining.is_empty() {
            let (rest, hdr) = record_header(remaining)
                .map_err(|e| EsmError::Parse(format!("record header: {e:?}")))?;
            let (rest, data) = take::<_, _, nom::error::Error<&[u8]>>(hdr.size)(rest)
                .map_err(|e| EsmError::Parse(format!("record body ({}): {e:?}", hdr.tag)))?;

            let record = Record::from_parts(hdr.tag, hdr.flags, data);
            if let Record::Tes3(h) = &record
                && header.is_none()
            {
                // Every record consumes at least a 16-byte header, which caps how much a
                // hostile num_records can over-reserve.
                records.reserve((h.num_records as usize).min(remaining.len() / 16));
                header = Some(h.clone());
            }
            records.push(record);
            remaining = rest;
        }

        Ok(EsmDirectory {
            header: header.unwrap_or_default(),
            records,
        })
    }
}
