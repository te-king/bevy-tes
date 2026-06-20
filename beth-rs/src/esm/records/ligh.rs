//! `LIGH` — a light.

use crate::types::latin1::L1Str;
use crate::esm::common::{Color, Subrecord, color, l1, le_f32, le_i32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightData {
    pub weight: f32,
    pub value: u32,
    pub time: i32,
    pub radius: u32,
    pub color: Color,
    /// See record docs (Dynamic, Carry, Fire, Flicker, …).
    pub flags: u32,
}

fn light_data(input: &[u8]) -> IResult<&[u8], LightData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, time) = le_i32(input)?;
    let (input, radius) = le_u32(input)?;
    let (input, color) = color(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        LightData {
            weight,
            value,
            time,
            radius,
            color,
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ligh<'a> {
    pub id: &'a L1Str,
    pub model: Option<&'a L1Str>,
    pub name: Option<&'a L1Str>,
    pub icon: Option<&'a L1Str>,
    pub data: LightData,
    pub sound: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
}

impl<'a> Ligh<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Ligh<'a> {
        let mut out = Ligh::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = Some(l1(sub.data)),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"LHDT" => out.data = parse_or_default(light_data, sub.data),
                b"SNAM" => out.sound = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
