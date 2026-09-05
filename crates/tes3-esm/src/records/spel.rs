//! `SPEL` — a spell.

use crate::common::{Subrecord, enumeration, flags, l1, le_u32, parse_or_default};
use crate::macros::enum_field;
use crate::shared::{Effect, effect};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = spell_data)]
pub struct SpellData {
    #[tes(read = enumeration)]
    pub kind: SpellKind,
    #[tes(read = le_u32)]
    pub cost: u32,
    #[tes(read = flags)]
    pub flags: SpellFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::effect_field)]
pub struct Spel<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"SPDT", decode = |bytes| parse_or_default(spell_data, bytes))]
    pub data: SpellData,
    #[tes(skip)]
    pub effects: Vec<Effect>,
}

impl<'a> Spel<'a> {
    fn effect_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"ENAM" {
            self.effects.push(parse_or_default(effect, sub.data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::ench::Ench;

    #[test]
    fn repeated_effects_include_defaults_for_malformed_payloads() {
        let mut first = [0u8; 24];
        first[..2].copy_from_slice(&17u16.to_le_bytes());
        let mut last = [0u8; 24];
        last[..2].copy_from_slice(&29u16.to_le_bytes());
        let subs = [
            Subrecord {
                tag: (*b"ENAM").into(),
                data: &first,
            },
            Subrecord {
                tag: (*b"????").into(),
                data: &first,
            },
            Subrecord {
                tag: (*b"ENAM").into(),
                data: &[1, 2],
            },
            Subrecord {
                tag: (*b"ENAM").into(),
                data: &last,
            },
        ];
        let spell = Spel::from_subrecords(subs.iter().copied());
        let enchantment = Ench::from_subrecords(subs.into_iter());
        assert_eq!(spell.effects, enchantment.effects);
        assert_eq!(spell.effects.len(), 3);
        assert_eq!(spell.effects[0].effect_index, 17);
        assert_eq!(spell.effects[1], Effect::default());
        assert_eq!(spell.effects[2].effect_index, 29);
    }
}
