//! `LEVI` — a leveled item list.

use crate::common::{Subrecord, finish, flags, l1, le_u16};
use tes_core::L1Str;

bitflags::bitflags! {
    /// Leveled item list flags (`DATA`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct LeveledItemFlags: u32 {
        /// Roll separately for each item produced by a count.
        const CALC_EACH_ITEM = 0x1;
        /// Draw from all levels ≤ the PC's level, not just the highest.
        const CALC_ALL_LEVELS = 0x2;
    }
}

/// One entry in a leveled list: an item ID and the PC level it becomes available at.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeveledItem<'a> {
    pub item: &'a L1Str,
    pub level: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levi<'a> {
    pub id: &'a L1Str,
    pub flags: LeveledItemFlags,
    /// Chance that nothing is produced.
    pub chance_none: u8,
    pub items: Vec<LeveledItem<'a>>,
}

impl<'a> Levi<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Levi<'a> {
        let mut out = Levi::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.flags = finish(flags(sub.data)).unwrap_or_default(),
                b"NNAM" => out.chance_none = sub.data.first().copied().unwrap_or(0),
                b"INDX" => {} // Count of following items; recoverable from `items.len()`.
                b"INAM" => out.items.push(LeveledItem {
                    item: l1(sub.data),
                    level: 0,
                }),
                b"INTV" => {
                    if let Some(last) = out.items.last_mut() {
                        last.level = finish(le_u16(sub.data)).unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        out
    }
}
