//! `LIGH` — a light.

use crate::common::{
    Color, Subrecord, color, l1, le_f32, le_i32, le_u32, parse_or_default, parse_struct,
};
use tes_core::L1String;

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

parse_struct! {
    fn light_data -> LightData {
        weight: le_f32,
        value: le_u32,
        time: le_i32,
        radius: le_u32,
        color: color,
        flags: le_u32,
    }
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
