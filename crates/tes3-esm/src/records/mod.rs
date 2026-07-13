//! Typed record definitions, one module per TES3 record type.
//!
//! Each module exposes a struct plus a `from_subrecords` constructor that builds the
//! typed value from the record's already-parsed list of [`Subrecord`](super::common::Subrecord)s.
//! Field assignment is order-tolerant where the format allows it and sequential where
//! fields are positionally coupled (e.g. `MAST`/`DATA` pairs, AI packages).
//!
//! The [`Record`] enum (re-exported at the crate root) and its tag dispatch are generated
//! here from the `records!` table below, so each record type is listed exactly once.
//!
//! The `from_subrecords` loops are deliberately hand-written rather than macro-generated:
//! roughly a quarter of the records are stateful scans (CELL's reference phases, TES3's
//! `MAST`/`DATA` pairing, LEVC/LEVI's `last_mut` coupling, NPC_/CREA's length-dispatched
//! `NPDT`), so a dispatch macro would split the crate into two idioms for little gain.

pub mod acti;
pub mod alch;
pub mod appa;
pub mod armo;
pub mod body;
pub mod book;
pub mod bsgn;
pub mod cell;
pub mod clas;
pub mod clot;
pub mod cont;
pub mod crea;
pub mod dial;
pub mod door;
pub mod ench;
pub mod fact;
pub mod glob;
pub mod gmst;
pub mod info;
pub mod ingr;
pub mod land;
pub mod levc;
pub mod levi;
pub mod ligh;
pub mod lock;
pub mod ltex;
pub mod mgef;
pub mod misc;
pub mod npc;
pub mod pgrd;
pub mod prob;
pub mod race;
pub mod regn;
pub mod repa;
pub mod scpt;
pub mod skil;
pub mod sndg;
pub mod soun;
pub mod spel;
pub mod sscr;
pub mod stat;
pub mod tes3;
pub mod weap;

use crate::common::{RecordFlags, Subrecords, Tag};
use crate::macros::records;

// Bring every record struct into scope for the `Record` enum.
use self::{
    acti::Acti, alch::Alch, appa::Appa, armo::Armo, body::Body, book::Book, bsgn::Bsgn, cell::Cell,
    clas::Clas, clot::Clot, cont::Cont, crea::Crea, dial::Dial, door::Door, ench::Ench, fact::Fact,
    glob::Glob, gmst::Gmst, info::Info, ingr::Ingr, land::Land, levc::Levc, levi::Levi, ligh::Ligh,
    lock::Lock, ltex::Ltex, mgef::Mgef, misc::Misc, npc::Npc, pgrd::Pgrd, prob::Prob, race::Race,
    regn::Regn, repa::Repa, scpt::Scpt, skil::Skil, sndg::Sndg, soun::Soun, spel::Spel, sscr::Sscr,
    stat::Stat, tes3::Tes3, weap::Weap,
};

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
