//! `RACE` — a character race.

use crate::common::{Subrecord, l1, le_f32, le_i32, le_u32, parse_or_default};
use nom::IResult;
use tes_core::L1String;

/// A skill bonus granted by the race (skill ID + bonus amount).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillBonus {
    /// Skill ID, or `-1` for an empty slot.
    pub skill: i32,
    pub bonus: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RaceData {
    pub skill_bonuses: [SkillBonus; 7],
    /// Attribute base values, indexed `[attribute][gender]`.
    pub attributes: [[u32; 2]; 8],
    /// Height per gender.
    pub height: [f32; 2],
    /// Weight per gender.
    pub weight: [f32; 2],
    /// `0x1` = Playable, `0x2` = Beast Race.
    pub flags: u32,
}

fn race_data(input: &[u8]) -> IResult<&[u8], RaceData> {
    let mut input = input;
    let mut skill_bonuses = [SkillBonus::default(); 7];
    for slot in skill_bonuses.iter_mut() {
        let (rest, skill) = le_i32(input)?;
        let (rest, bonus) = le_i32(rest)?;
        *slot = SkillBonus { skill, bonus };
        input = rest;
    }
    let mut attributes = [[0u32; 2]; 8];
    for attr in attributes.iter_mut() {
        let (rest, m) = le_u32(input)?;
        let (rest, f) = le_u32(rest)?;
        *attr = [m, f];
        input = rest;
    }
    let (input, hm) = le_f32(input)?;
    let (input, hf) = le_f32(input)?;
    let (input, wm) = le_f32(input)?;
    let (input, wf) = le_f32(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        RaceData {
            skill_bonuses,
            attributes,
            height: [hm, hf],
            weight: [wm, wf],
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Race {
    pub id: L1String,
    pub name: Option<L1String>,
    pub data: RaceData,
    /// Special power / ability spell IDs.
    pub powers: Vec<L1String>,
    pub description: Option<L1String>,
}

impl Race {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Race {
        let mut out = Race::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"RADT" => out.data = parse_or_default(race_data, sub.data),
                b"NPCS" => out.powers.push(l1(sub.data)),
                b"DESC" => out.description = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
