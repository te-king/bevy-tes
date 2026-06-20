//! `NPC_` — a non-player character.

use crate::types::latin1::L1String;
use crate::esm::common::{
    Subrecord, l1, finish, fixed_l1str, le_u16, le_u32, parse_or_default,
};
use crate::esm::shared::{
    AiData, AiPackage, InventoryItem, TravelDestination, ai_activate, ai_data, ai_escort,
    ai_follow, ai_travel, ai_wander, inventory_item, travel_destination,
};
use nom::IResult;
use nom::number::complete::le_u8;

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
    let mut input = input;
    let mut attributes = [0u8; 8];
    for a in attributes.iter_mut() {
        let (rest, v) = le_u8(input)?;
        *a = v;
        input = rest;
    }
    let mut skills = [0u8; 27];
    for s in skills.iter_mut() {
        let (rest, v) = le_u8(input)?;
        *s = v;
        input = rest;
    }
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Npc {
    pub id: L1String,
    pub model: Option<L1String>,
    pub name: Option<L1String>,
    pub race: L1String,
    pub class: L1String,
    pub faction: Option<L1String>,
    pub head_model: L1String,
    pub hair_model: Option<L1String>,
    pub script: Option<L1String>,
    pub stats: NpcStats,
    /// `0x1` = Female, `0x2` = Essential, `0x10` = Autocalc, etc.
    pub flags: u32,
    pub inventory: Vec<InventoryItem>,
    pub spells: Vec<L1String>,
    pub ai_data: Option<AiData>,
    pub destinations: Vec<TravelDestination>,
    pub ai_packages: Vec<AiPackage>,
}

impl Npc {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Npc {
        let mut out = Npc::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = Some(l1(sub.data)),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"RNAM" => out.race = l1(sub.data),
                b"CNAM" => out.class = l1(sub.data),
                b"ANAM" => out.faction = Some(l1(sub.data)),
                b"BNAM" => out.head_model = l1(sub.data),
                b"KNAM" => out.hair_model = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"NPDT" => {
                    // Distinguish the 12-byte autocalc form from the 52-byte full form.
                    let parsed = if sub.data.len() <= 12 {
                        finish(npc_autocalc(sub.data))
                    } else {
                        finish(npc_full(sub.data))
                    };
                    if let Some(stats) = parsed {
                        out.stats = stats;
                    }
                }
                b"FLAG" => out.flags = finish(le_u32(sub.data)).unwrap_or(0),
                b"NPCO" => out.inventory.push(parse_or_default(inventory_item, sub.data)),
                b"NPCS" => out.spells.push(finish(fixed_l1str(32)(sub.data)).unwrap_or_default()),
                b"AIDT" => out.ai_data = Some(parse_or_default(ai_data, sub.data)),
                b"DODT" => {
                    if let Some(dest) = finish(travel_destination(sub.data)) {
                        out.destinations.push(dest);
                    }
                }
                b"DNAM" => {
                    if let Some(last) = out.destinations.last_mut() {
                        last.cell = Some(l1(sub.data));
                    }
                }
                b"AI_A" => push_pkg(&mut out.ai_packages, finish(ai_activate(sub.data))),
                b"AI_E" => push_pkg(&mut out.ai_packages, finish(ai_escort(sub.data))),
                b"AI_F" => push_pkg(&mut out.ai_packages, finish(ai_follow(sub.data))),
                b"AI_T" => push_pkg(&mut out.ai_packages, finish(ai_travel(sub.data))),
                b"AI_W" => push_pkg(&mut out.ai_packages, finish(ai_wander(sub.data))),
                b"CNDT" => attach_cell(&mut out.ai_packages, l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}

fn push_pkg(packages: &mut Vec<AiPackage>, pkg: Option<AiPackage>) {
    if let Some(pkg) = pkg {
        packages.push(pkg);
    }
}

fn attach_cell(packages: &mut [AiPackage], cell: L1String) {
    if let Some(AiPackage::Escort { cell: c, .. } | AiPackage::Follow { cell: c, .. }) =
        packages.last_mut()
    {
        *c = Some(cell);
    }
}
