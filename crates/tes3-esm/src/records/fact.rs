//! `FACT` — a faction.

use crate::common::{Subrecord, array, finish, flags, l1, le_i32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Faction flags (`FADT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FactionFlags: u32 {
        const HIDDEN_FROM_PLAYER = 0x1;
    }
}

/// Per-rank requirements within a faction (part of the `FADT` struct).
#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = rank_data)]
pub struct RankData {
    #[tes(read = array(le_u32))]
    pub attribute_mods: [u32; 2],
    #[tes(read = le_u32)]
    pub primary_skill_mod: u32,
    #[tes(read = le_u32)]
    pub favored_skill_mod: u32,
    #[tes(read = le_u32)]
    pub reaction_mod: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = faction_data)]
pub struct FactionData {
    #[tes(read = array(le_u32))]
    pub attributes: [u32; 2],
    #[tes(read = array(rank_data))]
    pub ranks: [RankData; 10],
    /// Seven favored skill IDs (`-1` to ignore).
    #[tes(read = array(le_i32))]
    pub skills: [i32; 7],
    #[tes(read = flags)]
    pub flags: FactionFlags,
}

/// A reaction adjustment toward another faction (an `ANAM`/`INTV` pair).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reaction<'a> {
    pub faction: &'a L1Str,
    pub adjustment: i32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::list_field)]
pub struct Fact<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = l1)]
    pub name: &'a L1Str,
    /// Rank names (conventionally 10 entries).
    #[tes(skip)]
    pub rank_names: Vec<&'a L1Str>,
    #[tes(tag = b"FADT", decode = |bytes| parse_or_default(faction_data, bytes))]
    pub data: FactionData,
    #[tes(skip)]
    pub reactions: Vec<Reaction<'a>>,
}

impl<'a> Fact<'a> {
    fn list_field(&mut self, sub: Subrecord<'a>) {
        match &sub.tag.0 {
            b"RNAM" => self.rank_names.push(l1(sub.data)),
            b"ANAM" => self.reactions.push(Reaction {
                faction: l1(sub.data),
                adjustment: 0,
            }),
            b"INTV" => {
                if let Some(last) = self.reactions.last_mut() {
                    last.adjustment = finish(le_i32(sub.data)).unwrap_or(0);
                }
            }
            _ => {}
        }
    }
}
