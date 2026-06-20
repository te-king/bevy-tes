//! `REPA` — a repair item.

use crate::types::latin1::L1String;
use crate::esm::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RepairData {
    pub weight: f32,
    pub value: u32,
    pub uses: u32,
    pub quality: f32,
}

fn repair_data(input: &[u8]) -> IResult<&[u8], RepairData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, uses) = le_u32(input)?;
    let (input, quality) = le_f32(input)?;
    Ok((
        input,
        RepairData {
            weight,
            value,
            uses,
            quality,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Repa {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: RepairData,
    pub icon: Option<L1String>,
    pub script: Option<L1String>,
}

impl Repa {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Repa {
        let mut out = Repa::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"RIDT" => out.data = parse_or_default(repair_data, sub.data),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
