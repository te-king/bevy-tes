//! `SPEL` — a spell.

use crate::common::{Subrecord, enumeration, flags, l1, le_u32, parse_or_default};
use crate::macros::enum_field;
use crate::shared::{Effect, effect};
use nom::IResult;
use tes_core::L1String;

bitflags::bitflags! {
    /// Spell flags (`SPDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct SpellFlags: u32 {
        const AUTOCALC = 0x1;
        const PC_START = 0x2;
        const ALWAYS_SUCCEEDS = 0x4;
    }
}

enum_field! {
    /// Spell type (`SPDT`).
    pub enum SpellKind: u32 {
        Spell = 0,
        Ability = 1,
        Blight = 2,
        Disease = 3,
        Curse = 4,
        Power = 5,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpellData {
    pub kind: SpellKind,
    pub cost: u32,
    pub flags: SpellFlags,
}

fn spell_data(input: &[u8]) -> IResult<&[u8], SpellData> {
    let (input, kind) = enumeration(input)?;
    let (input, cost) = le_u32(input)?;
    let (input, flags) = flags(input)?;
    Ok((input, SpellData { kind, cost, flags }))
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
