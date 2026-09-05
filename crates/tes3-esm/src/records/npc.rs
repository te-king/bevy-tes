//! `NPC_` — a non-player character.

use crate::common::{Subrecord, array, finish, flags, l1, le_u16, le_u32, parse_or_default};
use crate::shared::{AiData, AiPackage, InventoryItem, TravelDestination, actor_field, ai_data};
use nom::IResult;
use nom::number::complete::le_u8;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

/// NPC stats. The compact form is used when the auto-calc flag is set (12-byte `NPDT`);
/// otherwise the full stat block is stored (52-byte `NPDT`).
#[derive(Debug, Clone, PartialEq)]
pub enum NpcStats {
    AutoCalc {
        level: u16,
        disposition: u8,
        reputation: u8,
        rank: u8,
        gold: u32,
    },
    Full {
        level: u16,
        attributes: [u8; 8],
        skills: [u8; 27],
        health: u16,
        spell_points: u16,
        fatigue: u16,
        disposition: u8,
        reputation: u8,
        rank: u8,
        gold: u32,
    },
}

impl Default for NpcStats {
    fn default() -> Self {
        NpcStats::AutoCalc {
            level: 0,
            disposition: 0,
            reputation: 0,
            rank: 0,
            gold: 0,
        }
    }
}

fn npc_autocalc(input: &[u8]) -> IResult<&[u8], NpcStats> {
    let (input, level) = le_u16(input)?;
    let (input, disposition) = le_u8(input)?;
    let (input, reputation) = le_u8(input)?;
    let (input, rank) = le_u8(input)?;
    let (input, _pad) = nom::bytes::complete::take(3usize)(input)?;
    let (input, gold) = le_u32(input)?;
    Ok((
        input,
        NpcStats::AutoCalc {
            level,
            disposition,
            reputation,
            rank,
            gold,
        },
    ))
}

fn npc_full(input: &[u8]) -> IResult<&[u8], NpcStats> {
    let (input, level) = le_u16(input)?;
    let (input, attributes) = array(le_u8)(input)?;
    let (input, skills) = array(le_u8)(input)?;
    let (input, _pad) = le_u8(input)?;
    let (input, health) = le_u16(input)?;
    let (input, spell_points) = le_u16(input)?;
    let (input, fatigue) = le_u16(input)?;
    let (input, disposition) = le_u8(input)?;
    let (input, reputation) = le_u8(input)?;
    let (input, rank) = le_u8(input)?;
    let (input, _pad) = le_u8(input)?;
    let (input, gold) = le_u32(input)?;
    Ok((
        input,
        NpcStats::Full {
            level,
            attributes,
            skills,
            health,
            spell_points,
            fatigue,
            disposition,
            reputation,
            rank,
            gold,
        },
    ))
}

bitflags::bitflags! {
    /// NPC flags (`FLAG`). Bits above `0x10` encode the blood type and are retained
    /// unnamed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct NpcFlags: u32 {
        const FEMALE = 0x01;
        const ESSENTIAL = 0x02;
        const RESPAWN = 0x04;
        /// Set on every vanilla NPC; meaning unknown.
        const BASE = 0x08;
        const AUTOCALC = 0x10;
    }
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::actor_field)]
pub struct Npc<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = |bytes| Some(l1(bytes)))]
    pub model: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"RNAM", decode = l1)]
    pub race: &'a L1Str,
    #[tes(tag = b"CNAM", decode = l1)]
    pub class: &'a L1Str,
    #[tes(tag = b"ANAM", decode = |bytes| Some(l1(bytes)))]
    pub faction: Option<&'a L1Str>,
    #[tes(tag = b"BNAM", decode = l1)]
    pub head_model: &'a L1Str,
    #[tes(tag = b"KNAM", decode = |bytes| Some(l1(bytes)))]
    pub hair_model: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(skip)]
    pub stats: NpcStats,
    #[tes(tag = b"FLAG", decode = |bytes| finish(flags(bytes)).unwrap_or_default())]
    pub flags: NpcFlags,
    #[tes(skip)]
    pub inventory: Vec<InventoryItem<'a>>,
    #[tes(skip)]
    pub spells: Vec<&'a L1Str>,
    #[tes(tag = b"AIDT", decode = |bytes| Some(parse_or_default(ai_data, bytes)))]
    pub ai_data: Option<AiData>,
    #[tes(skip)]
    pub destinations: Vec<TravelDestination<'a>>,
    #[tes(skip)]
    pub ai_packages: Vec<AiPackage<'a>>,
}

impl<'a> Npc<'a> {
    fn actor_field(&mut self, sub: Subrecord<'a>) {
        if sub.tag == b"NPDT" {
            // Distinguish the 12-byte autocalc form from the 52-byte full form.
            let parsed = if sub.data.len() <= 12 {
                finish(npc_autocalc(sub.data))
            } else {
                finish(npc_full(sub.data))
            };
            if let Some(stats) = parsed {
                self.stats = stats;
            }
        } else {
            actor_field(
                sub,
                &mut self.inventory,
                &mut self.spells,
                &mut self.destinations,
                &mut self.ai_packages,
            );
        }
    }
}
