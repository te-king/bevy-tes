//! `SKIL` — a character skill.

use crate::common::{array, enumeration, finish, l1, le_f32, le_u32, parse_or_default};
use crate::shared::Specialization;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = skill_data)]
pub struct SkillData {
    #[tes(read = le_u32)]
    pub attribute: u32,
    #[tes(read = enumeration)]
    pub specialization: Specialization,
    /// Four use values that grant skill experience.
    #[tes(read = array(le_f32))]
    pub use_values: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Skil<'a> {
    /// Skill index (the skill's identity; names are hardcoded in the engine).
    #[tes(tag = b"INDX", decode = |bytes| finish(le_u32(bytes)).unwrap_or(0))]
    pub index: u32,
    #[tes(tag = b"SKDT", decode = |bytes| parse_or_default(skill_data, bytes))]
    pub data: SkillData,
    #[tes(tag = b"DESC", decode = |bytes| Some(l1(bytes)))]
    pub description: Option<&'a L1Str>,
}
