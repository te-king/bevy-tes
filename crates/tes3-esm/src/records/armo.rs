//! `ARMO` — armor.

use crate::common::{
    Subrecord, enum_field, enumeration, l1, le_f32, le_u32, parse_or_default, parse_struct,
};
use crate::shared::BipedItem;
use tes_core::L1String;

enum_field! {
    /// Armor slot (`AODT`).
    pub enum ArmorKind: u32 {
        Helmet = 0,
        Cuirass = 1,
        LeftPauldron = 2,
        RightPauldron = 3,
        Greaves = 4,
        Boots = 5,
        LeftGauntlet = 6,
        RightGauntlet = 7,
        Shield = 8,
        LeftBracer = 9,
        RightBracer = 10,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArmorData {
    pub kind: ArmorKind,
    pub weight: f32,
    pub value: u32,
    pub health: u32,
    pub enchant_points: u32,
    pub armor_rating: u32,
}

parse_struct! {
    fn armor_data -> ArmorData {
        kind: enumeration,
        weight: le_f32,
        value: le_u32,
        health: le_u32,
        enchant_points: le_u32,
        armor_rating: le_u32,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Armo {
    pub id: L1String,
    pub model: L1String,
    pub name: L1String,
    pub script: Option<L1String>,
    pub data: ArmorData,
    pub icon: Option<L1String>,
    /// Biped slots (`INDX` with optional `BNAM`/`CNAM` model overrides).
    pub biped: Vec<BipedItem>,
    pub enchantment: Option<L1String>,
}

impl Armo {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Armo {
        let mut out = Armo::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = l1(sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"AODT" => out.data = parse_or_default(armor_data, sub.data),
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
