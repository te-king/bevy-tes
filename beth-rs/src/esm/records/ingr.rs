//! `INGR` — an alchemy ingredient.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_i32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IngredientData {
    pub weight: f32,
    pub value: u32,
    /// Up to four effect indices (`-1` if unused).
    pub effects: [i32; 4],
    /// Skill ID per effect (where applicable).
    pub skills: [i32; 4],
    /// Attribute ID per effect (where applicable).
    pub attributes: [i32; 4],
}

fn read4(input: &[u8]) -> IResult<&[u8], [i32; 4]> {
    let (input, a) = le_i32(input)?;
    let (input, b) = le_i32(input)?;
    let (input, c) = le_i32(input)?;
    let (input, d) = le_i32(input)?;
    Ok((input, [a, b, c, d]))
}

fn ingredient_data(input: &[u8]) -> IResult<&[u8], IngredientData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, effects) = read4(input)?;
    let (input, skills) = read4(input)?;
    let (input, attributes) = read4(input)?;
    Ok((
        input,
        IngredientData {
            weight,
            value,
            effects,
            skills,
            attributes,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ingr<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub data: IngredientData,
    pub script: Option<&'a L1Str>,
    pub icon: Option<&'a L1Str>,
}

impl<'a> Ingr<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Ingr<'a> {
        let mut out = Ingr::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"IRDT" => out.data = parse_or_default(ingredient_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
