//! `CONT` — a container.

use crate::common::{Subrecord, finish, flags, l1, le_f32, parse_or_default};
use crate::shared::{InventoryItem, inventory_item};
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

bitflags::bitflags! {
    /// Container flags (`FLAG`). Bit `0x8` is undocumented but set on every vanilla
    /// container; it is retained unnamed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ContainerFlags: u32 {
        const ORGANIC = 0x1;
        const RESPAWNS = 0x2;
    }
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::inventory_field)]
pub struct Cont<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"CNDT", decode = |bytes| finish(le_f32(bytes)).unwrap_or(0.0))]
    pub weight: f32,
    #[tes(tag = b"FLAG", decode = |bytes| finish(flags(bytes)).unwrap_or_default())]
    pub flags: ContainerFlags,
    #[tes(skip)]
    pub items: Vec<InventoryItem<'a>>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
}

impl<'a> Cont<'a> {
    fn inventory_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"NPCO" {
            self.items.push(parse_or_default(inventory_item, sub.data));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Tag;

    #[test]
    fn repeated_items_and_malformed_scalar_duplicates_keep_recovery_policy() {
        let mut item = [0; 36];
        item[..4].copy_from_slice(&(-2_i32).to_le_bytes());
        item[4..8].copy_from_slice(b"item");
        let weight = 3.5_f32.to_le_bytes();
        let flags = 0x8000_0009_u32.to_le_bytes();
        let fields: &[([u8; 4], &[u8])] = &[
            (*b"CNDT", &weight),
            (*b"FLAG", &flags),
            (*b"NPCO", &item),
            (*b"NPCO", &item[..35]),
            (*b"ZZZZ", &item),
            (*b"NPCO", &item),
        ];
        let out = Cont::from_subrecords(fields.iter().map(|&(tag, data)| Subrecord {
            tag: Tag(tag),
            data,
        }));
        assert_eq!(out.weight, 3.5);
        assert_eq!(out.flags.bits(), 0x8000_0009);
        assert_eq!(
            out.items,
            vec![
                InventoryItem {
                    count: -2,
                    object: l1(b"item")
                },
                InventoryItem::default(),
                InventoryItem {
                    count: -2,
                    object: l1(b"item")
                },
            ]
        );
        assert!(std::ptr::eq(out.items[0].object, l1(&item[4..])));
        let malformed: &[([u8; 4], &[u8])] = &[(*b"CNDT", &weight[..3]), (*b"FLAG", &flags[..3])];
        let out =
            Cont::from_subrecords(
                fields
                    .iter()
                    .chain(malformed)
                    .map(|&(tag, data)| Subrecord {
                        tag: Tag(tag),
                        data,
                    }),
            );
        assert_eq!(out.weight, 0.0);
        assert_eq!(out.flags, ContainerFlags::default());
        assert_eq!(out.items.len(), 3);
    }
}
