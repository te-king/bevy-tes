//! `APPA` — an alchemy apparatus.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ApparatusData {
    /// 0 = Mortar and Pestle, 1 = Alembic, 2 = Calcinator, 3 = Retort.
    pub kind: u32,
    pub quality: f32,
    pub weight: f32,
    pub value: u32,
}

fn apparatus_data(input: &[u8]) -> IResult<&[u8], ApparatusData> {
    let (input, kind) = le_u32(input)?;
    let (input, quality) = le_f32(input)?;
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    Ok((
        input,
        ApparatusData {
            kind,
            quality,
            weight,
            value,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Appa<'a> {
    pub id: &'a L1Str,
    pub model: Option<&'a L1Str>,
    pub name: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
    pub data: Option<ApparatusData>,
    pub icon: Option<&'a L1Str>,
}

impl<'a> Appa<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Appa<'a> {
        let mut out = Appa::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = Some(l1(sub.data)),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"AADT" => out.data = Some(parse_or_default(apparatus_data, sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
