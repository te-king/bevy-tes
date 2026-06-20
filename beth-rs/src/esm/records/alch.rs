//! `ALCH` — a potion or other alchemy item.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use crate::esm::shared::{Effect, effect};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AlchemyData {
    pub weight: f32,
    pub value: u32,
    /// `0x1` = autocalc.
    pub flags: u32,
}

fn alchemy_data(input: &[u8]) -> IResult<&[u8], AlchemyData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        AlchemyData {
            weight,
            value,
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Alch<'a> {
    pub id: &'a L1Str,
    pub model: Option<&'a L1Str>,
    /// Inventory icon name (stored in a `TEXT` subrecord for this record).
    pub icon: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
    pub name: Option<&'a L1Str>,
    pub data: Option<AlchemyData>,
    pub effects: Vec<Effect>,
}

impl<'a> Alch<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Alch<'a> {
        let mut out = Alch::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = Some(l1(sub.data)),
                b"TEXT" => out.icon = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"ALDT" => out.data = Some(parse_or_default(alchemy_data, sub.data)),
                b"ENAM" => out.effects.push(parse_or_default(effect, sub.data)),
                _ => {}
            }
        }
        out
    }
}
