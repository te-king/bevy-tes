//! `WEAP` — a weapon.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_u16, le_u32, le_u8, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponData {
    pub weight: f32,
    pub value: u32,
    /// Weapon type (0 = short blade … 13 = bolt).
    pub kind: u16,
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
    /// `0x1` = ignore normal weapon resistance, `0x2` = silver.
    pub flags: u32,
}

fn weapon_data(input: &[u8]) -> IResult<&[u8], WeaponData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, kind) = le_u16(input)?;
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
    let (input, flags) = le_u32(input)?;
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
pub struct Weap<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub data: WeaponData,
    pub icon: Option<&'a L1Str>,
    pub enchantment: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
}

impl<'a> Weap<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Weap<'a> {
        let mut out = Weap::default();
        for sub in subs {
            match &sub.tag {
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
