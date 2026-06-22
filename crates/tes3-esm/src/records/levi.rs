//! `LEVI` — a leveled item list.

use crate::common::{Subrecord, finish, l1, le_u16, le_u32};
use tes_core::L1String;

/// One entry in a leveled list: an item ID and the PC level it becomes available at.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LeveledItem {
    pub item: L1String,
    pub level: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Levi {
    pub id: L1String,
    /// `0x1` = calc for each item in count, `0x2` = calc from all levels ≤ PC level.
    pub flags: u32,
    /// Chance that nothing is produced.
    pub chance_none: u8,
    pub items: Vec<LeveledItem>,
}

impl Levi {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Levi {
        let mut out = Levi::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.flags = finish(le_u32(sub.data)).unwrap_or(0),
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
