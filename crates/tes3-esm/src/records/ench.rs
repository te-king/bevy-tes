//! `ENCH` — an enchantment.

use crate::common::{
    Subrecord, enum_field, enumeration, flags, l1, le_u32, parse_or_default, parse_struct,
};
use crate::shared::{Effect, effect};
use tes_core::L1String;

bitflags::bitflags! {
    /// Enchantment flags (`ENDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct EnchantFlags: u32 {
        const AUTOCALC = 0x1;
    }
}

enum_field! {
    /// Enchantment trigger (`ENDT`).
    pub enum EnchantKind: u32 {
        CastOnce = 0,
        CastOnStrike = 1,
        CastWhenUsed = 2,
        ConstantEffect = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnchantData {
    pub kind: EnchantKind,
    pub cost: u32,
    pub charge: u32,
    pub flags: EnchantFlags,
}

parse_struct! {
    fn enchant_data -> EnchantData {
        kind: enumeration,
        cost: le_u32,
        charge: le_u32,
        flags: flags,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ench {
    pub id: L1String,
    pub data: EnchantData,
    pub effects: Vec<Effect>,
}

impl Ench {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Ench {
        let mut out = Ench::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"ENDT" => out.data = parse_or_default(enchant_data, sub.data),
                b"ENAM" => out.effects.push(parse_or_default(effect, sub.data)),
                _ => {}
            }
        }
        out
    }
}
