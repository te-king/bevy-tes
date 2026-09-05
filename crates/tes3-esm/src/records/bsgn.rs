//! `BSGN` — a birthsign.

use crate::common::{Subrecord, l1};
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::spell_field)]
pub struct Bsgn<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    /// Spell/ability IDs granted by the birthsign.
    #[tes(skip)]
    pub spells: Vec<&'a L1Str>,
    /// Texture file name.
    #[tes(tag = b"TNAM", decode = |bytes| Some(l1(bytes)))]
    pub texture: Option<&'a L1Str>,
    #[tes(tag = b"DESC", decode = |bytes| Some(l1(bytes)))]
    pub description: Option<&'a L1Str>,
}

impl<'a> Bsgn<'a> {
    fn spell_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"NPCS" {
            self.spells.push(l1(sub.data));
        }
    }
}
