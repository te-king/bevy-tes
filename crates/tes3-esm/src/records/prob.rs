//! `PROB` — a probe.

use crate::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use nom::IResult;
use tes_core::L1String;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProbeData {
    pub weight: f32,
    pub value: u32,
    pub quality: f32,
    pub uses: u32,
}

fn probe_data(input: &[u8]) -> IResult<&[u8], ProbeData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, quality) = le_f32(input)?;
    let (input, uses) = le_u32(input)?;
    Ok((
        input,
        ProbeData {
            weight,
            value,
            quality,
            uses,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prob {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: ProbeData,
    pub icon: Option<L1String>,
    pub script: Option<L1String>,
}

impl Prob {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Prob {
        let mut out = Prob::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"PBDT" => out.data = parse_or_default(probe_data, sub.data),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
