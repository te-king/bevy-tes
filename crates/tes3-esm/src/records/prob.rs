//! `PROB` — a probe.

use crate::common::{l1, le_f32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = probe_data)]
pub struct ProbeData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = le_f32)]
    pub quality: f32,
    #[tes(read = le_u32)]
    pub uses: u32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Prob<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"PBDT", decode = |bytes| parse_or_default(probe_data, bytes))]
    pub data: ProbeData,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
}
