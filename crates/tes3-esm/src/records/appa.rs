//! `APPA` — an alchemy apparatus.

use crate::common::{Subrecord, enumeration, l1, le_f32, le_u32, parse_or_default};
use crate::macros::enum_field;
use nom::IResult;
use tes_core::L1String;

enum_field! {
    /// Apparatus type (`AADT`).
    pub enum ApparatusKind: u32 {
        MortarAndPestle = 0,
        Alembic = 1,
        Calcinator = 2,
        Retort = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ApparatusData {
    pub kind: ApparatusKind,
    pub quality: f32,
    pub weight: f32,
    pub value: u32,
}

fn apparatus_data(input: &[u8]) -> IResult<&[u8], ApparatusData> {
    let (input, kind) = enumeration(input)?;
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
pub struct Appa {
    pub id: L1String,
    pub model: Option<L1String>,
    pub name: Option<L1String>,
    pub script: Option<L1String>,
    pub data: Option<ApparatusData>,
    pub icon: Option<L1String>,
}

impl Appa {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Appa {
        let mut out = Appa::default();
        for sub in subs {
            match &sub.tag.0 {
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
