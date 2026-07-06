//! `ENCH` — an enchantment.

use crate::common::{Subrecord, flags, l1, le_u32, parse_or_default, parse_struct};
use crate::shared::{Effect, effect};
use tes_core::L1String;

bitflags::bitflags! {
    /// Enchantment flags (`ENDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct EnchantFlags: u32 {
        const AUTOCALC = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnchantData {
    /// 0 = Cast Once, 1 = Cast Strikes, 2 = Cast when Used, 3 = Constant Effect.
    pub kind: u32,
    pub cost: u32,
    pub charge: u32,
    pub flags: EnchantFlags,
}

parse_struct! {
    fn enchant_data -> EnchantData {
        kind: le_u32,
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
