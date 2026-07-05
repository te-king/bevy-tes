//! `SPEL` — a spell.

use crate::common::{Subrecord, l1, le_u32, parse_or_default, parse_struct};
use crate::shared::{Effect, effect};
use tes_core::L1String;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpellData {
    /// 0 = Spell, 1 = Ability, 2 = Blight, 3 = Disease, 4 = Curse, 5 = Power.
    pub kind: u32,
    pub cost: u32,
    /// `0x1` = AutoCalc, `0x2` = PC Start, `0x4` = Always Succeeds.
    pub flags: u32,
}

parse_struct! {
    fn spell_data -> SpellData {
        kind: le_u32,
        cost: le_u32,
        flags: le_u32,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Spel {
    pub id: L1String,
    pub name: Option<L1String>,
    pub data: SpellData,
    pub effects: Vec<Effect>,
}

impl Spel {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Spel {
        let mut out = Spel::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"SPDT" => out.data = parse_or_default(spell_data, sub.data),
                b"ENAM" => out.effects.push(parse_or_default(effect, sub.data)),
                _ => {}
            }
        }
        out
    }
}
