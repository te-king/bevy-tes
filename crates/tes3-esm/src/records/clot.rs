//! `CLOT` — an item of clothing.

use crate::common::{
    Subrecord, enum_field, enumeration, l1, le_f32, le_u16, parse_or_default, parse_struct,
};
use crate::shared::BipedItem;
use tes_core::L1String;

enum_field! {
    /// Clothing slot (`CTDT`).
    pub enum ClothingKind: u32 {
        Pants = 0,
        Shoes = 1,
        Shirt = 2,
        Belt = 3,
        Robe = 4,
        RightGlove = 5,
        LeftGlove = 6,
        Skirt = 7,
        Ring = 8,
        Amulet = 9,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClothingData {
    pub kind: ClothingKind,
    pub weight: f32,
    pub value: u16,
    pub enchant_points: u16,
}

parse_struct! {
    fn clothing_data -> ClothingData {
        kind: enumeration,
        weight: le_f32,
        value: le_u16,
        enchant_points: le_u16,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clot {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: ClothingData,
    pub script: Option<L1String>,
    pub icon: Option<L1String>,
    pub biped: Vec<BipedItem>,
    pub enchantment: Option<L1String>,
}

impl Clot {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Clot {
        let mut out = Clot::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"CTDT" => out.data = parse_or_default(clothing_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"INDX" => out.biped.push(BipedItem {
                    index: sub.data.first().copied().unwrap_or(0),
                    male_model: None,
                    female_model: None,
                }),
                b"BNAM" => {
                    if let Some(last) = out.biped.last_mut() {
                        last.male_model = Some(l1(sub.data));
                    }
                }
                b"CNAM" => {
                    if let Some(last) = out.biped.last_mut() {
                        last.female_model = Some(l1(sub.data));
                    }
                }
                b"ENAM" => out.enchantment = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
