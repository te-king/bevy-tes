//! `SKIL` — a character skill.

use crate::common::{Subrecord, enumeration, finish, l1, le_f32, le_u32, parse_or_default};
use crate::shared::Specialization;
use nom::IResult;
use tes_core::L1String;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillData {
    pub attribute: u32,
    pub specialization: Specialization,
    /// Four use values that grant skill experience.
    pub use_values: [f32; 4],
}

fn skill_data(input: &[u8]) -> IResult<&[u8], SkillData> {
    let (input, attribute) = le_u32(input)?;
    let (input, specialization) = enumeration(input)?;
    let mut input = input;
    let mut use_values = [0f32; 4];
    for slot in use_values.iter_mut() {
        let (rest, v) = le_f32(input)?;
        *slot = v;
        input = rest;
    }
    Ok((
        input,
        SkillData {
            attribute,
            specialization,
            use_values,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skil {
    /// Skill index (the skill's identity; names are hardcoded in the engine).
    pub index: u32,
    pub data: SkillData,
    pub description: Option<L1String>,
}

impl Skil {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Skil {
        let mut out = Skil::default();
        for sub in subs {
            match &sub.tag.0 {
                b"INDX" => out.index = finish(le_u32(sub.data)).unwrap_or(0),
                b"SKDT" => out.data = parse_or_default(skill_data, sub.data),
                b"DESC" => out.description = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
