//! `LIGH` — a light.

use crate::common::{Color, color, flags, l1, le_f32, le_i32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = light_data)]
pub struct LightData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = le_i32)]
    pub time: i32,
    #[tes(read = le_u32)]
    pub radius: u32,
    #[tes(read = color)]
    pub color: Color,
    #[tes(read = flags)]
    pub flags: LightFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Ligh<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = |bytes| Some(l1(bytes)))]
    pub model: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"LHDT", decode = |bytes| parse_or_default(light_data, bytes))]
    pub data: LightData,
    #[tes(tag = b"SNAM", decode = |bytes| Some(l1(bytes)))]
    pub sound: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
}
