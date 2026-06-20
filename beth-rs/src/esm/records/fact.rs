//! `FACT` — a faction.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_i32, le_u32, parse_or_default};
use nom::IResult;

/// Per-rank requirements within a faction (part of the `FADT` struct).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RankData {
    pub attribute_mods: [u32; 2],
    pub primary_skill_mod: u32,
    pub favored_skill_mod: u32,
    pub reaction_mod: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FactionData {
    pub attributes: [u32; 2],
    pub ranks: [RankData; 10],
    /// Seven favored skill IDs (`-1` to ignore).
    pub skills: [i32; 7],
    /// `0x1` = Hidden from player.
    pub flags: u32,
}

fn rank_data(input: &[u8]) -> IResult<&[u8], RankData> {
    let (input, a0) = le_u32(input)?;
    let (input, a1) = le_u32(input)?;
    let (input, primary_skill_mod) = le_u32(input)?;
    let (input, favored_skill_mod) = le_u32(input)?;
    let (input, reaction_mod) = le_u32(input)?;
    Ok((
        input,
        RankData {
            attribute_mods: [a0, a1],
            primary_skill_mod,
            favored_skill_mod,
            reaction_mod,
        },
    ))
}

fn faction_data(input: &[u8]) -> IResult<&[u8], FactionData> {
    let (input, a0) = le_u32(input)?;
    let (input, a1) = le_u32(input)?;
    let mut input = input;
    let mut ranks = [RankData::default(); 10];
    for rank in ranks.iter_mut() {
        let (rest, r) = rank_data(input)?;
        *rank = r;
        input = rest;
    }
    let mut skills = [0i32; 7];
    for skill in skills.iter_mut() {
        let (rest, s) = le_i32(input)?;
        *skill = s;
        input = rest;
    }
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        FactionData {
            attributes: [a0, a1],
            ranks,
            skills,
            flags,
        },
    ))
}

/// A reaction adjustment toward another faction (an `ANAM`/`INTV` pair).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reaction<'a> {
    pub faction: &'a L1Str,
    pub adjustment: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fact<'a> {
    pub id: &'a L1Str,
    pub name: &'a L1Str,
    /// Rank names (conventionally 10 entries).
    pub rank_names: Vec<&'a L1Str>,
    pub data: FactionData,
    pub reactions: Vec<Reaction<'a>>,
}

impl<'a> Fact<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Fact<'a> {
        let mut out = Fact::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = l1(sub.data),
                b"RNAM" => out.rank_names.push(l1(sub.data)),
                b"FADT" => out.data = parse_or_default(faction_data, sub.data),
                b"ANAM" => out.reactions.push(Reaction {
                    faction: l1(sub.data),
                    adjustment: 0,
                }),
                b"INTV" => {
                    if let Some(last) = out.reactions.last_mut() {
                        last.adjustment = finish(le_i32(sub.data)).unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        out
    }
}
