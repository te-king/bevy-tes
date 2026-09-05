//! `MGEF` — a magic effect.

use crate::common::{enumeration, finish, flags, l1, le_f32, le_u32, parse_or_default};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Magic effect flags (`MEDT`). Only these bits are stored in the file; behavior
    /// flags like Harmful are hardcoded per effect index in the engine.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MagicEffectFlags: u32 {
        const SPELLMAKING = 0x0200;
        const ENCHANTING = 0x0400;
        const NEGATIVE = 0x0800;
    }
}

enum_field! {
    /// Spell school (`MEDT`).
    pub enum MagicSchool: u32 {
        Alteration = 0,
        Conjuration = 1,
        Destruction = 2,
        Illusion = 3,
        Mysticism = 4,
        Restoration = 5,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = magic_effect_data)]
pub struct MagicEffectData {
    #[tes(read = enumeration)]
    pub school: MagicSchool,
    #[tes(read = le_f32)]
    pub base_cost: f32,
    #[tes(read = flags)]
    pub flags: MagicEffectFlags,
    #[tes(read = le_u32)]
    pub red: u32,
    #[tes(read = le_u32)]
    pub green: u32,
    #[tes(read = le_u32)]
    pub blue: u32,
    #[tes(read = le_f32)]
    pub speed_x: f32,
    #[tes(read = le_f32)]
    pub size_x: f32,
    #[tes(read = le_f32)]
    pub size_cap: f32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Mgef<'a> {
    /// Effect index (names are hardcoded in the engine).
    #[tes(tag = b"INDX", decode = |bytes| finish(le_u32(bytes)).unwrap_or(0))]
    pub index: u32,
    #[tes(tag = b"MEDT", decode = |bytes| parse_or_default(magic_effect_data, bytes))]
    pub data: MagicEffectData,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"PTEX", decode = |bytes| Some(l1(bytes)))]
    pub particle_texture: Option<&'a L1Str>,
    #[tes(tag = b"BSND", decode = |bytes| Some(l1(bytes)))]
    pub bolt_sound: Option<&'a L1Str>,
    #[tes(tag = b"CSND", decode = |bytes| Some(l1(bytes)))]
    pub casting_sound: Option<&'a L1Str>,
    #[tes(tag = b"HSND", decode = |bytes| Some(l1(bytes)))]
    pub hit_sound: Option<&'a L1Str>,
    #[tes(tag = b"ASND", decode = |bytes| Some(l1(bytes)))]
    pub area_sound: Option<&'a L1Str>,
    #[tes(tag = b"CVFX", decode = |bytes| Some(l1(bytes)))]
    pub casting_visual: Option<&'a L1Str>,
    #[tes(tag = b"BVFX", decode = |bytes| Some(l1(bytes)))]
    pub bolt_visual: Option<&'a L1Str>,
    #[tes(tag = b"HVFX", decode = |bytes| Some(l1(bytes)))]
    pub hit_visual: Option<&'a L1Str>,
    #[tes(tag = b"AVFX", decode = |bytes| Some(l1(bytes)))]
    pub area_visual: Option<&'a L1Str>,
    #[tes(tag = b"DESC", decode = |bytes| Some(l1(bytes)))]
    pub description: Option<&'a L1Str>,
}
