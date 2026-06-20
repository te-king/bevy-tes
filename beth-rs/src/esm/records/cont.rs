//! `CONT` — a container.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_f32, le_u32, parse_or_default};
use crate::esm::shared::{InventoryItem, inventory_item};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cont<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub weight: f32,
    /// `0x1` = Organic, `0x2` = Respawns, `0x8` = Unknown (always set).
    pub flags: u32,
    pub items: Vec<InventoryItem<'a>>,
    pub script: Option<&'a L1Str>,
}

impl<'a> Cont<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Cont<'a> {
        let mut out = Cont::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"CNDT" => out.weight = finish(le_f32(sub.data)).unwrap_or(0.0),
                b"FLAG" => out.flags = finish(le_u32(sub.data)).unwrap_or(0),
                b"NPCO" => out.items.push(parse_or_default(inventory_item, sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
