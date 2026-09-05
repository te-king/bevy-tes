//! `CLOT` — an item of clothing.

use crate::common::{Subrecord, enumeration, l1, le_f32, le_u16, parse_or_default};
use crate::macros::enum_field;
use crate::shared::BipedItem;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = clothing_data)]
pub struct ClothingData {
    #[tes(read = enumeration)]
    pub kind: ClothingKind,
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u16)]
    pub value: u16,
    #[tes(read = le_u16)]
    pub enchant_points: u16,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::biped_field)]
pub struct Clot<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"CTDT", decode = |bytes| parse_or_default(clothing_data, bytes))]
    pub data: ClothingData,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(skip)]
    pub biped: Vec<BipedItem<'a>>,
    #[tes(tag = b"ENAM", decode = |bytes| Some(l1(bytes)))]
    pub enchantment: Option<&'a L1Str>,
}

impl<'a> Clot<'a> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Tag;

    #[test]
    fn biped_models_update_only_the_latest_slot() {
        let fields: &[([u8; 4], &[u8])] = &[
            (*b"BNAM", b"orphan-male"),
            (*b"CNAM", b"orphan-female"),
            (*b"INDX", &[3, 99]),
            (*b"BNAM", b"old"),
            (*b"FNAM", b"clothing"),
            (*b"BNAM", b"male\0"),
            (*b"CNAM", b"female\0"),
            (*b"ZZZZ", b"ignored"),
            (*b"INDX", &[]),
            (*b"CNAM", b"second"),
            (*b"CNAM", &[]),
            (*b"ENAM", b"enchantment"),
        ];
        let out = Clot::from_subrecords(fields.iter().map(|&(tag, data)| Subrecord {
            tag: Tag(tag),
            data,
        }));
        assert_eq!(
            out.biped,
            vec![
                BipedItem {
                    index: 3,
                    male_model: Some(l1(b"male")),
                    female_model: Some(l1(b"female")),
                },
                BipedItem {
                    index: 0,
                    male_model: None,
                    female_model: Some(l1(b"")),
                },
            ]
        );
        assert_eq!(out.name, Some(l1(b"clothing")));
        assert_eq!(out.enchantment, Some(l1(b"enchantment")));
    }
}
