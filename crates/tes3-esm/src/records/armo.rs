//! `ARMO` — armor.

use crate::common::{Subrecord, enumeration, l1, le_f32, le_u32, parse_or_default};
use crate::macros::enum_field;
use crate::shared::BipedItem;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = armor_data)]
pub struct ArmorData {
    #[tes(read = enumeration)]
    pub kind: ArmorKind,
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = le_u32)]
    pub health: u32,
    #[tes(read = le_u32)]
    pub enchant_points: u32,
    #[tes(read = le_u32)]
    pub armor_rating: u32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::biped_field)]
pub struct Armo<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = l1)]
    pub name: &'a L1Str,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"AODT", decode = |bytes| parse_or_default(armor_data, bytes))]
    pub data: ArmorData,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    /// Biped slots (`INDX` with optional `BNAM`/`CNAM` model overrides).
    #[tes(skip)]
    pub biped: Vec<BipedItem<'a>>,
    #[tes(tag = b"ENAM", decode = |bytes| Some(l1(bytes)))]
    pub enchantment: Option<&'a L1Str>,
}

impl<'a> Armo<'a> {
    fn biped_field(&mut self, sub: Subrecord<'a>) {
        match &sub.tag.0 {
            b"INDX" => self.biped.push(BipedItem {
                index: sub.data.first().copied().unwrap_or(0),
                male_model: None,
                female_model: None,
            }),
            b"BNAM" => {
                if let Some(last) = self.biped.last_mut() {
                    last.male_model = Some(l1(sub.data));
                }
            }
            b"CNAM" => {
                if let Some(last) = self.biped.last_mut() {
                    last.female_model = Some(l1(sub.data));
                }
            }
            _ => {}
        }
    }
}
