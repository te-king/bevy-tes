//! `LEVC` — a leveled creature list.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_u16, le_u32};

/// One entry in a leveled creature list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeveledCreature<'a> {
    pub creature: &'a L1Str,
    pub level: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levc<'a> {
    pub id: &'a L1Str,
    /// `0x1` = calc from all levels ≤ PC level.
    pub flags: u32,
    pub chance_none: u8,
    pub creatures: Vec<LeveledCreature<'a>>,
}

impl<'a> Levc<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Levc<'a> {
        let mut out = Levc::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.flags = finish(le_u32(sub.data)).unwrap_or(0),
                b"NNAM" => out.chance_none = sub.data.first().copied().unwrap_or(0),
                b"INDX" => {} // Count of following creatures; recoverable from len().
                b"CNAM" => out.creatures.push(LeveledCreature {
                    creature: l1(sub.data),
                    level: 0,
                }),
                b"INTV" => {
                    if let Some(last) = out.creatures.last_mut() {
                        last.level = finish(le_u16(sub.data)).unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        out
    }
}
