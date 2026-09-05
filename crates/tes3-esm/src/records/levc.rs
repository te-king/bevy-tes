//! `LEVC` — a leveled creature list.

use crate::common::{Subrecord, finish, flags, l1, le_u16};
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

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
pub struct LeveledCreature<'a> {
    pub creature: &'a L1Str,
    pub level: u16,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::entry_field)]
pub struct Levc<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"DATA", decode = |bytes| finish(flags(bytes)).unwrap_or_default())]
    pub flags: LeveledCreatureFlags,
    #[tes(tag = b"NNAM", decode = |bytes: &[u8]| bytes.first().copied().unwrap_or(0))]
    pub chance_none: u8,
    #[tes(skip)]
    pub creatures: Vec<LeveledCreature<'a>>,
}

impl<'a> Levc<'a> {
    fn entry_field(&mut self, sub: Subrecord<'a>) {
        match &sub.tag.0 {
            b"CNAM" => self.creatures.push(LeveledCreature {
                creature: l1(sub.data),
                level: 0,
            }),
            b"INTV" => {
                if let Some(last) = self.creatures.last_mut() {
                    last.level = finish(le_u16(sub.data)).unwrap_or(0);
                }
            }
            _ => {}
        }
    }
}
