//! `SOUN` — a sound effect.

use crate::common::{l1, le_u8, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = sound_data)]
pub struct SoundData {
    /// Volume, 0 = 0.00 … 255 = 1.00.
    #[tes(read = le_u8)]
    pub volume: u8,
    #[tes(read = le_u8)]
    pub min_range: u8,
    #[tes(read = le_u8)]
    pub max_range: u8,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Soun<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = l1)]
    pub filename: &'a L1Str,
    #[tes(tag = b"DATA", decode = |bytes| parse_or_default(sound_data, bytes))]
    pub data: SoundData,
}
