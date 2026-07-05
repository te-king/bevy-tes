//! TES3 (Morrowind) plugin format parsing.
//!
//! A plugin file is a flat sequence of records: a leading [`Tes3`](records::tes3::Tes3)
//! header followed by content records. Parsing copies into owned structures: the parsed
//! [`Plugin`] and its records own their strings ([`L1String`](crate::L1String)) and
//! binary blobs, so the `Plugin` is `'static` and the input buffer is only borrowed for
//! the duration of the parse call:
//!
//! ```no_run
//! let bytes = std::fs::read("data/Morrowind.esm").unwrap();
//! let plugin = tes3_esm::Plugin::parse(&bytes).unwrap();
//! ```

pub mod common;
pub mod records;
pub mod shared;

use common::{RecordFlags, Subrecords, Tag, record_header};
use nom::bytes::complete::take;

pub use common::EsmError;
pub use records::tes3::Tes3;
pub use tes_core::{L1Str, L1String};

// Bring every record struct into scope for the `Record` enum.
use records::{
    acti::Acti, alch::Alch, appa::Appa, armo::Armo, body::Body, book::Book, bsgn::Bsgn, cell::Cell,
    clas::Clas, clot::Clot, cont::Cont, crea::Crea, dial::Dial, door::Door, ench::Ench, fact::Fact,
    glob::Glob, gmst::Gmst, info::Info, ingr::Ingr, land::Land, levc::Levc, levi::Levi, ligh::Ligh,
    lock::Lock, ltex::Ltex, mgef::Mgef, misc::Misc, npc::Npc, pgrd::Pgrd, prob::Prob, race::Race,
    regn::Regn, repa::Repa, scpt::Scpt, skil::Skil, sndg::Sndg, soun::Soun, spel::Spel, sscr::Sscr,
    stat::Stat, weap::Weap,
};

/// Generate the [`Record`] enum and its tag dispatch from one `Variant(Type) = b"TAG"`
/// table, so each record type is listed exactly once instead of three times (variant,
/// tag accessor, parser dispatch).
macro_rules! records {
    ($( $variant:ident($ty:ty) = $tag:literal, )*) => {
        /// A single parsed record. One variant per known TES3 record type, plus
        /// [`Record::Unknown`] as a safety net for tags not modeled by this crate.
        /// Records own their data and are `'static`.
        #[derive(Debug, Clone, PartialEq)]
        pub enum Record {
            $( $variant($ty), )*
            /// A record whose 4-byte tag is not recognized; its raw payload is preserved.
            Unknown {
                tag: Tag,
                flags: RecordFlags,
                data: Vec<u8>,
            },
        }

        impl Record {
            /// The 4-byte tag of this record.
            pub fn tag(&self) -> Tag {
                match self {
                    $( Record::$variant(_) => Tag(*$tag), )*
                    Record::Unknown { tag, .. } => *tag,
                }
            }

            /// Build a typed record from its tag, header flags and data block.
            fn from_parts(tag: Tag, flags: RecordFlags, data: &[u8]) -> Record {
                // Subrecords are parsed lazily from `data`; a malformed/truncated
                // subrecord just ends iteration (the record keeps whatever fields parsed
                // before it). Only one match arm runs, so moving `subs` into it is fine.
                let subs = Subrecords::new(data);
                match &tag.0 {
                    $( $tag => Record::$variant(<$ty>::from_subrecords(subs)), )*
                    _ => Record::Unknown {
                        tag,
                        flags,
                        data: data.to_vec(),
                    },
                }
            }
        }
    };
}

records! {
    Tes3(Tes3) = b"TES3",
    Gmst(Gmst) = b"GMST",
    Glob(Glob) = b"GLOB",
    Clas(Clas) = b"CLAS",
    Fact(Fact) = b"FACT",
    Race(Race) = b"RACE",
    Soun(Soun) = b"SOUN",
    Skil(Skil) = b"SKIL",
    Mgef(Mgef) = b"MGEF",
    Scpt(Scpt) = b"SCPT",
    Regn(Regn) = b"REGN",
    Bsgn(Bsgn) = b"BSGN",
    Ltex(Ltex) = b"LTEX",
    Stat(Stat) = b"STAT",
    Door(Door) = b"DOOR",
    Misc(Misc) = b"MISC",
    Weap(Weap) = b"WEAP",
    Cont(Cont) = b"CONT",
    Spel(Spel) = b"SPEL",
    Crea(Crea) = b"CREA",
    Body(Body) = b"BODY",
    Ligh(Ligh) = b"LIGH",
    Ench(Ench) = b"ENCH",
    Npc(Npc) = b"NPC_",
    Armo(Armo) = b"ARMO",
    Clot(Clot) = b"CLOT",
    Repa(Repa) = b"REPA",
    Acti(Acti) = b"ACTI",
    Appa(Appa) = b"APPA",
    Lock(Lock) = b"LOCK",
    Prob(Prob) = b"PROB",
    Ingr(Ingr) = b"INGR",
    Book(Book) = b"BOOK",
    Alch(Alch) = b"ALCH",
    Levi(Levi) = b"LEVI",
    Levc(Levc) = b"LEVC",
    Cell(Cell) = b"CELL",
    Land(Land) = b"LAND",
    Pgrd(Pgrd) = b"PGRD",
    Sndg(Sndg) = b"SNDG",
    Dial(Dial) = b"DIAL",
    Info(Info) = b"INFO",
    Sscr(Sscr) = b"SSCR",
}

/// A fully parsed TES3 plugin (`.esm`/`.esp`). Owns all of its data, so it is `'static`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plugin {
    /// The leading `TES3` header record.
    pub header: Tes3,
    /// All content records following the header, in file order.
    pub records: Vec<Record>,
}

impl Plugin {
    /// Parse a plugin from an in-memory byte slice. The returned [`Plugin`] owns its data
    /// (copied out of `input`), so it does not borrow `input` after this returns.
    pub fn parse(input: &[u8]) -> Result<Plugin, EsmError> {
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
                header = Some(h.clone());
            }
            records.push(record);
            remaining = rest;
        }

        Ok(Plugin {
            header: header.unwrap_or_default(),
            records,
        })
    }
}
