//! `MISC` — a miscellaneous item.

use crate::common::{Subrecord, flags, l1, le_f32, le_u32, parse_or_default};
use crate::macros::parse_struct;
use tes_core::L1String;

bitflags::bitflags! {
    /// Miscellaneous item flags (`MCDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MiscFlags: u32 {
        const KEY = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MiscData {
    pub weight: f32,
    pub value: u32,
    pub flags: MiscFlags,
}

parse_struct! {
    fn misc_data -> MiscData {
        weight: le_f32,
        value: le_u32,
        flags: flags,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Misc {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: MiscData,
    pub script: Option<L1String>,
    pub icon: Option<L1String>,
}

impl Misc {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Misc {
        let mut out = Misc::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"MCDT" => out.data = parse_or_default(misc_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
