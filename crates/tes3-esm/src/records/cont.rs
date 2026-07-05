//! `CONT` — a container.

use crate::common::{Subrecord, finish, l1, le_f32, le_u32, parse_or_default};
use crate::shared::{InventoryItem, inventory_item};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cont {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub weight: f32,
    /// `0x1` = Organic, `0x2` = Respawns, `0x8` = Unknown (always set).
    pub flags: u32,
    pub items: Vec<InventoryItem>,
    pub script: Option<L1String>,
}

impl Cont {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Cont {
        let mut out = Cont::default();
        for sub in subs {
            match &sub.tag.0 {
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
