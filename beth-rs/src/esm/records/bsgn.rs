//! `BSGN` — a birthsign.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bsgn<'a> {
    pub id: &'a L1Str,
    pub name: Option<&'a L1Str>,
    /// Spell/ability IDs granted by the birthsign.
    pub spells: Vec<&'a L1Str>,
    /// Texture file name.
    pub texture: Option<&'a L1Str>,
    pub description: Option<&'a L1Str>,
}

impl<'a> Bsgn<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Bsgn<'a> {
        let mut out = Bsgn::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"NPCS" => out.spells.push(l1(sub.data)),
                b"TNAM" => out.texture = Some(l1(sub.data)),
                b"DESC" => out.description = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
