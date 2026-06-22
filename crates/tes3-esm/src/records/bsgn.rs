//! `BSGN` — a birthsign.

use crate::common::{Subrecord, l1};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bsgn {
    pub id: L1String,
    pub name: Option<L1String>,
    /// Spell/ability IDs granted by the birthsign.
    pub spells: Vec<L1String>,
    /// Texture file name.
    pub texture: Option<L1String>,
    pub description: Option<L1String>,
}

impl Bsgn {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Bsgn {
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
