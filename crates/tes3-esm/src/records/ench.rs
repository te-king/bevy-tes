//! `ENCH` — an enchantment.

use crate::common::{Subrecord, enumeration, flags, l1, le_u32, parse_or_default};
use crate::macros::enum_field;
use crate::shared::{Effect, effect};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = enchant_data)]
pub struct EnchantData {
    #[tes(read = enumeration)]
    pub kind: EnchantKind,
    #[tes(read = le_u32)]
    pub cost: u32,
    #[tes(read = le_u32)]
    pub charge: u32,
    #[tes(read = flags)]
    pub flags: EnchantFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::effect_field)]
pub struct Ench<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"ENDT", decode = |bytes| parse_or_default(enchant_data, bytes))]
    pub data: EnchantData,
    #[tes(skip)]
    pub effects: Vec<Effect>,
}

impl<'a> Ench<'a> {
    fn effect_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"ENAM" {
            self.effects.push(parse_or_default(effect, sub.data));
        }
    }
}
