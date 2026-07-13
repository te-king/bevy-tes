//! `LIGH` — a light.

use crate::common::{Color, Subrecord, color, flags, l1, le_f32, le_i32, le_u32, parse_or_default};
use nom::IResult;
use tes_core::L1String;

bitflags::bitflags! {
    /// Light behavior flags (`LHDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct LightFlags: u32 {
        const DYNAMIC = 0x0001;
        const CAN_CARRY = 0x0002;
        /// Darkens instead of illuminating.
        const NEGATIVE = 0x0004;
        const FLICKER = 0x0008;
        const FIRE = 0x0010;
        const OFF_BY_DEFAULT = 0x0020;
        const FLICKER_SLOW = 0x0040;
        const PULSE = 0x0080;
        const PULSE_SLOW = 0x0100;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightData {
    pub weight: f32,
    pub value: u32,
    pub time: i32,
    pub radius: u32,
    pub color: Color,
    pub flags: LightFlags,
}

fn light_data(input: &[u8]) -> IResult<&[u8], LightData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, time) = le_i32(input)?;
    let (input, radius) = le_u32(input)?;
    let (input, color) = color(input)?;
    let (input, flags) = flags(input)?;
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
pub struct Ligh {
    pub id: L1String,
    pub model: Option<L1String>,
    pub name: Option<L1String>,
    pub icon: Option<L1String>,
    pub data: LightData,
    pub sound: Option<L1String>,
    pub script: Option<L1String>,
}

impl Ligh {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Ligh {
        let mut out = Ligh::default();
        for sub in subs {
            match &sub.tag.0 {
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
