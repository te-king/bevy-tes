//! `CREA` — a creature.

use crate::common::{
    Subrecord, array, enumeration, finish, flags, l1, le_f32, le_u32, parse_or_default,
};
use crate::macros::enum_field;
use crate::shared::{AiData, AiPackage, InventoryItem, TravelDestination, actor_field, ai_data};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

enum_field! {
    /// Creature type (`NPDT`).
    pub enum CreatureKind: u32 {
        Creature = 0,
        Daedra = 1,
        Undead = 2,
        Humanoid = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = creature_data)]
pub struct CreatureData {
    #[tes(read = enumeration)]
    pub kind: CreatureKind,
    #[tes(read = le_u32)]
    pub level: u32,
    /// Eight attributes, ordered by attribute ID.
    #[tes(read = array(le_u32))]
    pub attributes: [u32; 8],
    #[tes(read = le_u32)]
    pub health: u32,
    #[tes(read = le_u32)]
    pub spell_points: u32,
    #[tes(read = le_u32)]
    pub fatigue: u32,
    #[tes(read = le_u32)]
    pub soul: u32,
    #[tes(read = le_u32)]
    pub combat: u32,
    #[tes(read = le_u32)]
    pub magic: u32,
    #[tes(read = le_u32)]
    pub stealth: u32,
    /// Three (min, max) melee attack ranges.
    #[tes(read = array(array(le_u32)))]
    pub attacks: [[u32; 2]; 3],
    #[tes(read = le_u32)]
    pub gold: u32,
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

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::actor_field)]
pub struct Crea<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"CNAM", decode = |bytes| Some(l1(bytes)))]
    pub sound_gen: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"NPDT", decode = |bytes| parse_or_default(creature_data, bytes))]
    pub data: CreatureData,
    #[tes(tag = b"FLAG", decode = |bytes| finish(flags(bytes)).unwrap_or_default())]
    pub flags: CreatureFlags,
    #[tes(tag = b"XSCL", decode = |bytes| finish(le_f32(bytes)))]
    pub scale: Option<f32>,
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

impl<'a> Crea<'a> {
    fn actor_field(&mut self, sub: Subrecord<'a>) {
        actor_field(
            sub,
            &mut self.inventory,
            &mut self.spells,
            &mut self.destinations,
            &mut self.ai_packages,
        );
    }
}
