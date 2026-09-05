//! `WEAP` — a weapon.

use crate::common::{enumeration, flags, l1, le_f32, le_u8, le_u16, le_u32, parse_or_default};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

enum_field! {
    /// Weapon type (`WPDT`).
    pub enum WeaponKind: u16 {
        ShortBladeOneHand = 0,
        LongBladeOneHand = 1,
        LongBladeTwoClose = 2,
        BluntOneHand = 3,
        BluntTwoClose = 4,
        BluntTwoWide = 5,
        SpearTwoWide = 6,
        AxeOneHand = 7,
        AxeTwoHand = 8,
        MarksmanBow = 9,
        MarksmanCrossbow = 10,
        MarksmanThrown = 11,
        Arrow = 12,
        Bolt = 13,
    }
}

bitflags::bitflags! {
    /// Weapon flags (`WPDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct WeaponFlags: u32 {
        const IGNORE_NORMAL_WEAPON_RESISTANCE = 0x1;
        const SILVER = 0x2;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = weapon_data)]
pub struct WeaponData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = enumeration)]
    pub kind: WeaponKind,
    #[tes(read = le_u16)]
    pub health: u16,
    #[tes(read = le_f32)]
    pub speed: f32,
    #[tes(read = le_f32)]
    pub reach: f32,
    #[tes(read = le_u16)]
    pub enchant_points: u16,
    #[tes(read = le_u8)]
    pub chop_min: u8,
    #[tes(read = le_u8)]
    pub chop_max: u8,
    #[tes(read = le_u8)]
    pub slash_min: u8,
    #[tes(read = le_u8)]
    pub slash_max: u8,
    #[tes(read = le_u8)]
    pub thrust_min: u8,
    #[tes(read = le_u8)]
    pub thrust_max: u8,
    #[tes(read = flags)]
    pub flags: WeaponFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Weap<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"WPDT", decode = |bytes| parse_or_default(weapon_data, bytes))]
    pub data: WeaponData,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"ENAM", decode = |bytes| Some(l1(bytes)))]
    pub enchantment: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Subrecord, Tag};

    #[test]
    fn weapon_mixed_width_fields_preserve_layout_and_malformed_defaults() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2.5_f32.to_le_bytes());
        bytes.extend_from_slice(&123_u32.to_le_bytes());
        bytes.extend_from_slice(&99_u16.to_le_bytes());
        bytes.extend_from_slice(&321_u16.to_le_bytes());
        bytes.extend_from_slice(&1.5_f32.to_le_bytes());
        bytes.extend_from_slice(&3.5_f32.to_le_bytes());
        bytes.extend_from_slice(&456_u16.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        bytes.extend_from_slice(&0x8000_0003_u32.to_le_bytes());
        bytes.push(0xff);
        let (rest, data) = weapon_data(&bytes).unwrap();
        assert_eq!(rest, &[0xff]);
        assert_eq!(
            data,
            WeaponData {
                weight: 2.5,
                value: 123,
                kind: WeaponKind::Unknown(99),
                health: 321,
                speed: 1.5,
                reach: 3.5,
                enchant_points: 456,
                chop_min: 1,
                chop_max: 2,
                slash_min: 3,
                slash_max: 4,
                thrust_min: 5,
                thrust_max: 6,
                flags: WeaponFlags::from_bits_retain(0x8000_0003),
            }
        );
        for len in 0..32 {
            assert!(weapon_data(&bytes[..len]).is_err());
            let out = Weap::from_subrecords([&bytes[..], &bytes[..len]].into_iter().map(|data| {
                Subrecord {
                    tag: Tag(*b"WPDT"),
                    data,
                }
            }));
            assert_eq!(out.data, WeaponData::default());
        }
    }
}
