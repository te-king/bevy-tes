//! `LOCK` — a lockpick.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LockData {
    pub weight: f32,
    pub value: u32,
    pub quality: f32,
    pub uses: u32,
}

fn lock_data(input: &[u8]) -> IResult<&[u8], LockData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, quality) = le_f32(input)?;
    let (input, uses) = le_u32(input)?;
    Ok((
        input,
        LockData {
            weight,
            value,
            quality,
            uses,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lock<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub data: LockData,
    pub script: Option<&'a L1Str>,
    pub icon: Option<&'a L1Str>,
}

impl<'a> Lock<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Lock<'a> {
        let mut out = Lock::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"LKDT" => out.data = parse_or_default(lock_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
