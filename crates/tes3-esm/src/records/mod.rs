//! Typed record definitions, one module per TES3 record type.
//!
//! Each module exposes a struct plus a `from_subrecords` constructor that builds the
//! typed value from the record's already-parsed list of [`Subrecord`](super::common::Subrecord)s.
//! Field assignment is order-tolerant where the format allows it and sequential where
//! fields are positionally coupled (e.g. `MAST`/`DATA` pairs, AI packages).
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
