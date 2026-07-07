//! `REPA` — a repair item.

use crate::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use crate::macros::parse_struct;
use tes_core::L1String;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RepairData {
    pub weight: f32,
    pub value: u32,
    pub uses: u32,
    pub quality: f32,
}

parse_struct! {
    fn repair_data -> RepairData {
        weight: le_f32,
        value: le_u32,
        uses: le_u32,
        quality: le_f32,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Repa {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: RepairData,
    pub icon: Option<L1String>,
    pub script: Option<L1String>,
}

impl Repa {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Repa {
        let mut out = Repa::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"RIDT" => out.data = parse_or_default(repair_data, sub.data),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
