//! TES3 (Morrowind) plugin format parsing.
//!
//! A plugin file is a flat sequence of records: a leading [`Tes3`](records::tes3::Tes3)
//! header followed by content records. Parsing is zero-copy — the parsed [`Plugin`] and
//! its records borrow strings and binary blobs directly from the input buffer, which
//! must therefore outlive the `Plugin`:
//!
//! ```no_run
//! let bytes = std::fs::read("beth-rs/assets/Morrowind.esm").unwrap();
//! let plugin = beth_rs::Plugin::parse(&bytes).unwrap();
//! ```

pub mod common;
pub mod records;
pub mod shared;

use common::{EsmError, RecordFlags, Tag, record_header, subrecords};
use nom::bytes::complete::take;

pub use records::tes3::Tes3;

// Bring every record struct into scope for the `Record` enum.
use records::{
    acti::Acti, alch::Alch, appa::Appa, armo::Armo, body::Body, book::Book, bsgn::Bsgn, cell::Cell,
    clas::Clas, clot::Clot, cont::Cont, crea::Crea, dial::Dial, door::Door, ench::Ench, fact::Fact,
    glob::Glob, gmst::Gmst, info::Info, ingr::Ingr, land::Land, levc::Levc, levi::Levi, ligh::Ligh,
    lock::Lock, ltex::Ltex, mgef::Mgef, misc::Misc, npc::Npc, pgrd::Pgrd, prob::Prob, race::Race,
    regn::Regn, repa::Repa, scpt::Scpt, skil::Skil, sndg::Sndg, soun::Soun, spel::Spel, stat::Stat,
    weap::Weap,
};

/// A single parsed record. One variant per known TES3 record type, plus [`Record::Unknown`]
/// as a safety net for tags not modeled by this crate. Records borrow from the input
/// buffer (lifetime `'a`).
#[derive(Debug, Clone, PartialEq)]
pub enum Record<'a> {
    Tes3(Tes3<'a>),
    Gmst(Gmst<'a>),
    Glob(Glob<'a>),
    Clas(Clas<'a>),
    Fact(Fact<'a>),
    Race(Race<'a>),
    Soun(Soun<'a>),
    Skil(Skil<'a>),
    Mgef(Mgef<'a>),
    Scpt(Scpt<'a>),
    Regn(Regn<'a>),
    Bsgn(Bsgn<'a>),
    Ltex(Ltex<'a>),
    Stat(Stat<'a>),
    Door(Door<'a>),
    Misc(Misc<'a>),
    Weap(Weap<'a>),
    Cont(Cont<'a>),
    Spel(Spel<'a>),
    Crea(Crea<'a>),
    Body(Body<'a>),
    Ligh(Ligh<'a>),
    Ench(Ench<'a>),
    Npc(Npc<'a>),
    Armo(Armo<'a>),
    Clot(Clot<'a>),
    Repa(Repa<'a>),
    Acti(Acti<'a>),
    Appa(Appa<'a>),
    Lock(Lock<'a>),
    Prob(Prob<'a>),
    Ingr(Ingr<'a>),
    Book(Book<'a>),
    Alch(Alch<'a>),
    Levi(Levi<'a>),
    Levc(Levc<'a>),
    Cell(Cell<'a>),
    Land(Land<'a>),
    Pgrd(Pgrd<'a>),
    Sndg(Sndg<'a>),
    Dial(Dial<'a>),
    Info(Info<'a>),
    /// A record whose 4-byte tag is not recognized; its raw payload is preserved.
    Unknown {
        tag: Tag,
        flags: RecordFlags,
        data: &'a [u8],
    },
}

impl<'a> Record<'a> {
    /// The 4-byte tag of this record.
    pub fn tag(&self) -> Tag {
        match self {
            Record::Tes3(_) => *b"TES3",
            Record::Gmst(_) => *b"GMST",
            Record::Glob(_) => *b"GLOB",
            Record::Clas(_) => *b"CLAS",
            Record::Fact(_) => *b"FACT",
            Record::Race(_) => *b"RACE",
            Record::Soun(_) => *b"SOUN",
            Record::Skil(_) => *b"SKIL",
            Record::Mgef(_) => *b"MGEF",
            Record::Scpt(_) => *b"SCPT",
            Record::Regn(_) => *b"REGN",
            Record::Bsgn(_) => *b"BSGN",
            Record::Ltex(_) => *b"LTEX",
            Record::Stat(_) => *b"STAT",
            Record::Door(_) => *b"DOOR",
            Record::Misc(_) => *b"MISC",
            Record::Weap(_) => *b"WEAP",
            Record::Cont(_) => *b"CONT",
            Record::Spel(_) => *b"SPEL",
            Record::Crea(_) => *b"CREA",
            Record::Body(_) => *b"BODY",
            Record::Ligh(_) => *b"LIGH",
            Record::Ench(_) => *b"ENCH",
            Record::Npc(_) => *b"NPC_",
            Record::Armo(_) => *b"ARMO",
            Record::Clot(_) => *b"CLOT",
            Record::Repa(_) => *b"REPA",
            Record::Acti(_) => *b"ACTI",
            Record::Appa(_) => *b"APPA",
            Record::Lock(_) => *b"LOCK",
            Record::Prob(_) => *b"PROB",
            Record::Ingr(_) => *b"INGR",
            Record::Book(_) => *b"BOOK",
            Record::Alch(_) => *b"ALCH",
            Record::Levi(_) => *b"LEVI",
            Record::Levc(_) => *b"LEVC",
            Record::Cell(_) => *b"CELL",
            Record::Land(_) => *b"LAND",
            Record::Pgrd(_) => *b"PGRD",
            Record::Sndg(_) => *b"SNDG",
            Record::Dial(_) => *b"DIAL",
            Record::Info(_) => *b"INFO",
            Record::Unknown { tag, .. } => *tag,
        }
    }

    /// Build a typed record from its tag, header flags and data block.
    fn from_parts(tag: Tag, flags: RecordFlags, data: &'a [u8]) -> Record<'a> {
        // If the subrecord framing is somehow broken, keep the raw bytes.
        let subs = match subrecords(data) {
            Ok((_, subs)) => subs,
            Err(_) => return Record::Unknown { tag, flags, data },
        };
        match &tag {
            b"TES3" => Record::Tes3(Tes3::from_subrecords(&subs)),
            b"GMST" => Record::Gmst(Gmst::from_subrecords(&subs)),
            b"GLOB" => Record::Glob(Glob::from_subrecords(&subs)),
            b"CLAS" => Record::Clas(Clas::from_subrecords(&subs)),
            b"FACT" => Record::Fact(Fact::from_subrecords(&subs)),
            b"RACE" => Record::Race(Race::from_subrecords(&subs)),
            b"SOUN" => Record::Soun(Soun::from_subrecords(&subs)),
            b"SKIL" => Record::Skil(Skil::from_subrecords(&subs)),
            b"MGEF" => Record::Mgef(Mgef::from_subrecords(&subs)),
            b"SCPT" => Record::Scpt(Scpt::from_subrecords(&subs)),
            b"REGN" => Record::Regn(Regn::from_subrecords(&subs)),
            b"BSGN" => Record::Bsgn(Bsgn::from_subrecords(&subs)),
            b"LTEX" => Record::Ltex(Ltex::from_subrecords(&subs)),
            b"STAT" => Record::Stat(Stat::from_subrecords(&subs)),
            b"DOOR" => Record::Door(Door::from_subrecords(&subs)),
            b"MISC" => Record::Misc(Misc::from_subrecords(&subs)),
            b"WEAP" => Record::Weap(Weap::from_subrecords(&subs)),
            b"CONT" => Record::Cont(Cont::from_subrecords(&subs)),
            b"SPEL" => Record::Spel(Spel::from_subrecords(&subs)),
            b"CREA" => Record::Crea(Crea::from_subrecords(&subs)),
            b"BODY" => Record::Body(Body::from_subrecords(&subs)),
            b"LIGH" => Record::Ligh(Ligh::from_subrecords(&subs)),
            b"ENCH" => Record::Ench(Ench::from_subrecords(&subs)),
            b"NPC_" => Record::Npc(Npc::from_subrecords(&subs)),
            b"ARMO" => Record::Armo(Armo::from_subrecords(&subs)),
            b"CLOT" => Record::Clot(Clot::from_subrecords(&subs)),
            b"REPA" => Record::Repa(Repa::from_subrecords(&subs)),
            b"ACTI" => Record::Acti(Acti::from_subrecords(&subs)),
            b"APPA" => Record::Appa(Appa::from_subrecords(&subs)),
            b"LOCK" => Record::Lock(Lock::from_subrecords(&subs)),
            b"PROB" => Record::Prob(Prob::from_subrecords(&subs)),
            b"INGR" => Record::Ingr(Ingr::from_subrecords(&subs)),
            b"BOOK" => Record::Book(Book::from_subrecords(&subs)),
            b"ALCH" => Record::Alch(Alch::from_subrecords(&subs)),
            b"LEVI" => Record::Levi(Levi::from_subrecords(&subs)),
            b"LEVC" => Record::Levc(Levc::from_subrecords(&subs)),
            b"CELL" => Record::Cell(Cell::from_subrecords(&subs)),
            b"LAND" => Record::Land(Land::from_subrecords(&subs)),
            b"PGRD" => Record::Pgrd(Pgrd::from_subrecords(&subs)),
            b"SNDG" => Record::Sndg(Sndg::from_subrecords(&subs)),
            b"DIAL" => Record::Dial(Dial::from_subrecords(&subs)),
            b"INFO" => Record::Info(Info::from_subrecords(&subs)),
            _ => Record::Unknown { tag, flags, data },
        }
    }
}

/// A fully parsed TES3 plugin (`.esm`/`.esp`), borrowing from the source buffer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plugin<'a> {
    /// The leading `TES3` header record.
    pub header: Tes3<'a>,
    /// All content records following the header, in file order.
    pub records: Vec<Record<'a>>,
}

impl<'a> Plugin<'a> {
    /// Parse a plugin from an in-memory byte slice. The returned [`Plugin`] borrows from
    /// `input`, so read the file into a buffer that outlives it (see the module example).
    pub fn parse(input: &'a [u8]) -> Result<Plugin<'a>, EsmError> {
        let mut remaining = input;
        let mut records = Vec::new();
        let mut header: Option<Tes3<'a>> = None;

        while !remaining.is_empty() {
            let (rest, hdr) = record_header(remaining)
                .map_err(|e| EsmError::Parse(format!("record header: {e:?}")))?;
            let (rest, data) = take::<_, _, nom::error::Error<&[u8]>>(hdr.size)(rest)
                .map_err(|e| EsmError::Parse(format!("record body ({:?}): {e:?}", hdr.tag)))?;

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
