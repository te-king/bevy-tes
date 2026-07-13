//! `WEAP` — a weapon.

use crate::common::{
    Subrecord, enumeration, flags, l1, le_f32, le_u8, le_u16, le_u32, parse_or_default,
};
use crate::macros::enum_field;
use nom::IResult;
use tes_core::L1String;

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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponData {
    pub weight: f32,
    pub value: u32,
    pub kind: WeaponKind,
    pub health: u16,
    pub speed: f32,
    pub reach: f32,
    pub enchant_points: u16,
    pub chop_min: u8,
    pub chop_max: u8,
    pub slash_min: u8,
    pub slash_max: u8,
    pub thrust_min: u8,
    pub thrust_max: u8,
    pub flags: WeaponFlags,
}

fn weapon_data(input: &[u8]) -> IResult<&[u8], WeaponData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, kind) = enumeration(input)?;
    let (input, health) = le_u16(input)?;
    let (input, speed) = le_f32(input)?;
    let (input, reach) = le_f32(input)?;
    let (input, enchant_points) = le_u16(input)?;
    let (input, chop_min) = le_u8(input)?;
    let (input, chop_max) = le_u8(input)?;
    let (input, slash_min) = le_u8(input)?;
    let (input, slash_max) = le_u8(input)?;
    let (input, thrust_min) = le_u8(input)?;
    let (input, thrust_max) = le_u8(input)?;
    let (input, flags) = flags(input)?;
    Ok((
        input,
        WeaponData {
            weight,
            value,
            kind,
            health,
            speed,
            reach,
            enchant_points,
            chop_min,
            chop_max,
            slash_min,
            slash_max,
            thrust_min,
            thrust_max,
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weap {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: WeaponData,
    pub icon: Option<L1String>,
    pub enchantment: Option<L1String>,
    pub script: Option<L1String>,
}

impl Weap {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Weap {
        let mut out = Weap::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"WPDT" => out.data = parse_or_default(weapon_data, sub.data),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"ENAM" => out.enchantment = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
