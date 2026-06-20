//! `LOCK` — a lockpick.

use crate::types::latin1::L1String;
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
pub struct Lock {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: LockData,
    pub script: Option<L1String>,
    pub icon: Option<L1String>,
}

impl Lock {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Lock {
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
