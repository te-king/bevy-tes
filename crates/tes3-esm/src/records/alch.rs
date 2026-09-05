//! `ALCH` — a potion or other alchemy item.

use crate::common::{Subrecord, flags, l1, le_f32, le_u32, parse_or_default};
use crate::shared::{Effect, effect};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Alchemy item flags (`ALDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct AlchemyFlags: u32 {
        const AUTOCALC = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = alchemy_data)]
pub struct AlchemyData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = flags)]
    pub flags: AlchemyFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::effect_field)]
pub struct Alch<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = |bytes| Some(l1(bytes)))]
    pub model: Option<&'a L1Str>,
    /// Inventory icon name (stored in a `TEXT` subrecord for this record).
    #[tes(tag = b"TEXT", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"ALDT", decode = |bytes| Some(parse_or_default(alchemy_data, bytes)))]
    pub data: Option<AlchemyData>,
    #[tes(skip)]
    pub effects: Vec<Effect>,
}

impl<'a> Alch<'a> {
    fn effect_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"ENAM" {
            self.effects.push(parse_or_default(effect, sub.data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Tag;

    fn sub(tag: [u8; 4], data: &[u8]) -> Subrecord<'_> {
        Subrecord {
            tag: Tag(tag),
            data,
        }
    }

    #[test]
    fn malformed_duplicate_data_is_some_default() {
        assert_eq!(Alch::from_subrecords(std::iter::empty()).data, None);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2.5_f32.to_le_bytes());
        bytes.extend_from_slice(&17_u32.to_le_bytes());
        bytes.extend_from_slice(&0x8000_0001_u32.to_le_bytes());
        bytes.push(0xff);
        let valid = Alch::from_subrecords([sub(*b"ALDT", &bytes)].into_iter());
        assert_eq!(
            valid.data,
            Some(AlchemyData {
                weight: 2.5,
                value: 17,
                flags: AlchemyFlags::from_bits_retain(0x8000_0001),
            })
        );
        for len in 0..12 {
            let out = Alch::from_subrecords(
                [sub(*b"ALDT", &bytes), sub(*b"ALDT", &bytes[..len])].into_iter(),
            );
            assert_eq!(out.data, Some(AlchemyData::default()));
        }
    }

    #[test]
    fn repeated_effects_keep_order_and_malformed_entries() {
        let mut first = [0; 24];
        first[..2].copy_from_slice(&7_u16.to_le_bytes());
        let mut second = first;
        second[..2].copy_from_slice(&9_u16.to_le_bytes());
        let out = Alch::from_subrecords(
            [
                sub(*b"ENAM", &first),
                sub(*b"FNAM", b"potion\0"),
                sub(*b"ENAM", &first[..23]),
                sub(*b"ZZZZ", &second),
                sub(*b"ENAM", &second),
            ]
            .into_iter(),
        );
        assert_eq!(
            out.effects,
            vec![
                Effect {
                    effect_index: 7,
                    ..Effect::default()
                },
                Effect::default(),
                Effect {
                    effect_index: 9,
                    ..Effect::default()
                },
            ]
        );
        assert_eq!(out.name, Some(l1(b"potion")));
    }
}
