//! `MISC` — a miscellaneous item.

use crate::common::{flags, l1, le_f32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Miscellaneous item flags (`MCDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MiscFlags: u32 {
        const KEY = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = misc_data)]
pub struct MiscData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = flags)]
    pub flags: MiscFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Misc<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"MCDT", decode = |bytes| parse_or_default(misc_data, bytes))]
    pub data: MiscData,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
}
