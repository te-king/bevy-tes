//! `RACE` — a character race.

use crate::common::{Subrecord, array, flags, l1, le_f32, le_i32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Race flags (`RADT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RaceFlags: u32 {
        const PLAYABLE = 0x1;
        const BEAST = 0x2;
    }
}

/// A skill bonus granted by the race (skill ID + bonus amount).
#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = skill_bonus)]
pub struct SkillBonus {
    /// Skill ID, or `-1` for an empty slot.
    #[tes(read = le_i32)]
    pub skill: i32,
    #[tes(read = le_i32)]
    pub bonus: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = race_data)]
pub struct RaceData {
    #[tes(read = array(skill_bonus))]
    pub skill_bonuses: [SkillBonus; 7],
    /// Attribute base values, indexed `[attribute][gender]`.
    #[tes(read = array(array(le_u32)))]
    pub attributes: [[u32; 2]; 8],
    /// Height per gender.
    #[tes(read = array(le_f32))]
    pub height: [f32; 2],
    /// Weight per gender.
    #[tes(read = array(le_f32))]
    pub weight: [f32; 2],
    #[tes(read = flags)]
    pub flags: RaceFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::power_field)]
pub struct Race<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"RADT", decode = |bytes| parse_or_default(race_data, bytes))]
    pub data: RaceData,
    /// Special power / ability spell IDs.
    #[tes(skip)]
    pub powers: Vec<&'a L1Str>,
    #[tes(tag = b"DESC", decode = |bytes| Some(l1(bytes)))]
    pub description: Option<&'a L1Str>,
}

impl<'a> Race<'a> {
    fn power_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"NPCS" {
            self.powers.push(l1(sub.data));
        }
    }
}
