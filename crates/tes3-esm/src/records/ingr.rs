//! `INGR` — an alchemy ingredient.

use crate::common::{array, l1, le_f32, le_i32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, PartialEq, Default, TesPayload)]
#[tes(parser = ingredient_data)]
pub struct IngredientData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    /// Up to four effect indices (`-1` if unused).
    #[tes(read = array(le_i32))]
    pub effects: [i32; 4],
    /// Skill ID per effect (where applicable).
    #[tes(read = array(le_i32))]
    pub skills: [i32; 4],
    /// Attribute ID per effect (where applicable).
    #[tes(read = array(le_i32))]
    pub attributes: [i32; 4],
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Ingr<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"IRDT", decode = |bytes| parse_or_default(ingredient_data, bytes))]
    pub data: IngredientData,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Subrecord, Tag};

    #[test]
    fn ingredient_arrays_remain_sequential_and_truncation_defaults_whole_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.5_f32.to_le_bytes());
        bytes.extend_from_slice(&19_u32.to_le_bytes());
        for value in -1_i32..11 {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.push(0xff);
        let (rest, data) = ingredient_data(&bytes).unwrap();
        assert_eq!(rest, &[0xff]);
        assert_eq!(
            data,
            IngredientData {
                weight: 1.5,
                value: 19,
                effects: [-1, 0, 1, 2],
                skills: [3, 4, 5, 6],
                attributes: [7, 8, 9, 10],
            }
        );
        for len in 0..56 {
            assert!(ingredient_data(&bytes[..len]).is_err());
            let out = Ingr::from_subrecords([&bytes[..], &bytes[..len]].into_iter().map(|data| {
                Subrecord {
                    tag: Tag(*b"IRDT"),
                    data,
                }
            }));
            assert_eq!(out.data, IngredientData::default());
        }
    }
}
