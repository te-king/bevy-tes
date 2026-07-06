//! `LEVC` — a leveled creature list.

use crate::common::{Subrecord, finish, flags, l1, le_u16};
use tes_core::L1String;

bitflags::bitflags! {
    /// Leveled creature list flags (`DATA`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct LeveledCreatureFlags: u32 {
        /// Draw from all levels ≤ the PC's level, not just the highest.
        const CALC_ALL_LEVELS = 0x1;
    }
}

/// One entry in a leveled creature list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeveledCreature {
    pub creature: L1String,
    pub level: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levc {
    pub id: L1String,
    pub flags: LeveledCreatureFlags,
    pub chance_none: u8,
    pub creatures: Vec<LeveledCreature>,
}

impl Levc {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Levc {
        let mut out = Levc::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.flags = finish(flags(sub.data)).unwrap_or_default(),
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
