//! `CREA` — a creature.

use crate::common::{
    Subrecord, enum_field, enumeration, finish, fixed_l1str, flags, l1, le_f32, le_u32,
    parse_or_default,
};
use crate::shared::{
    AiData, AiPackage, InventoryItem, TravelDestination, ai_activate, ai_data, ai_escort,
    ai_follow, ai_travel, ai_wander, inventory_item, travel_destination,
};
use nom::IResult;
use tes_core::L1String;

enum_field! {
    /// Creature type (`NPDT`).
    pub enum CreatureKind: u32 {
        Creature = 0,
        Daedra = 1,
        Undead = 2,
        Humanoid = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CreatureData {
    pub kind: CreatureKind,
    pub level: u32,
    /// Eight attributes, ordered by attribute ID.
    pub attributes: [u32; 8],
    pub health: u32,
    pub spell_points: u32,
    pub fatigue: u32,
    pub soul: u32,
    pub combat: u32,
    pub magic: u32,
    pub stealth: u32,
    /// Three (min, max) melee attack ranges.
    pub attacks: [[u32; 2]; 3],
    pub gold: u32,
}

fn creature_data(input: &[u8]) -> IResult<&[u8], CreatureData> {
    let (input, kind) = enumeration(input)?;
    let (input, level) = le_u32(input)?;
    let mut input = input;
    let mut attributes = [0u32; 8];
    for a in attributes.iter_mut() {
        let (rest, v) = le_u32(input)?;
        *a = v;
        input = rest;
    }
    let (input, health) = le_u32(input)?;
    let (input, spell_points) = le_u32(input)?;
    let (input, fatigue) = le_u32(input)?;
    let (input, soul) = le_u32(input)?;
    let (input, combat) = le_u32(input)?;
    let (input, magic) = le_u32(input)?;
    let (input, stealth) = le_u32(input)?;
    let mut input = input;
    let mut attacks = [[0u32; 2]; 3];
    for atk in attacks.iter_mut() {
        let (rest, min) = le_u32(input)?;
        let (rest, max) = le_u32(rest)?;
        *atk = [min, max];
        input = rest;
    }
    let (input, gold) = le_u32(input)?;
    Ok((
        input,
        CreatureData {
            kind,
            level,
            attributes,
            health,
            spell_points,
            fatigue,
            soul,
            combat,
            magic,
            stealth,
            attacks,
            gold,
        },
    ))
}

bitflags::bitflags! {
    /// Creature flags (`FLAG`). Bits above `0x80` encode the blood type (skeleton,
    /// metal sparks, …) and are retained unnamed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct CreatureFlags: u32 {
        const BIPED = 0x01;
        const RESPAWN = 0x02;
        const WEAPON_AND_SHIELD = 0x04;
        /// Set on every vanilla creature; meaning unknown.
        const BASE = 0x08;
        const SWIMS = 0x10;
        const FLIES = 0x20;
        const WALKS = 0x40;
        const ESSENTIAL = 0x80;
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Crea {
    pub id: L1String,
    pub model: L1String,
    pub sound_gen: Option<L1String>,
    pub name: Option<L1String>,
    pub script: Option<L1String>,
    pub data: CreatureData,
    pub flags: CreatureFlags,
    pub scale: Option<f32>,
    pub inventory: Vec<InventoryItem>,
    pub spells: Vec<L1String>,
    pub ai_data: Option<AiData>,
    pub destinations: Vec<TravelDestination>,
    pub ai_packages: Vec<AiPackage>,
}

impl Crea {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Crea {
        let mut out = Crea::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"CNAM" => out.sound_gen = Some(l1(sub.data)),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"NPDT" => out.data = parse_or_default(creature_data, sub.data),
                b"FLAG" => out.flags = finish(flags(sub.data)).unwrap_or_default(),
                b"XSCL" => out.scale = finish(le_f32(sub.data)),
                b"NPCO" => out
                    .inventory
                    .push(parse_or_default(inventory_item, sub.data)),
                b"NPCS" => out
                    .spells
                    .push(finish(fixed_l1str(32)(sub.data)).unwrap_or_default()),
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

/// Push a parsed AI package if it decoded successfully.
fn push_pkg(packages: &mut Vec<AiPackage>, pkg: Option<AiPackage>) {
    if let Some(pkg) = pkg {
        packages.push(pkg);
    }
}

/// Attach a trailing `CNDT` cell name to the most recent Escort/Follow package.
fn attach_cell(packages: &mut [AiPackage], cell: L1String) {
    if let Some(AiPackage::Escort { cell: c, .. } | AiPackage::Follow { cell: c, .. }) =
        packages.last_mut()
    {
        *c = Some(cell);
    }
}
