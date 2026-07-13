//! `MISC` — a miscellaneous item.

use crate::common::{Subrecord, flags, l1, le_f32, le_u32, parse_or_default};
use nom::IResult;
use tes_core::L1Str;

bitflags::bitflags! {
    /// Miscellaneous item flags (`MCDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MiscFlags: u32 {
        const KEY = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MiscData {
    pub weight: f32,
    pub value: u32,
    pub flags: MiscFlags,
}

fn misc_data(input: &[u8]) -> IResult<&[u8], MiscData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, flags) = flags(input)?;
    Ok((
        input,
        MiscData {
            weight,
            value,
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Misc<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub data: MiscData,
    pub script: Option<&'a L1Str>,
    pub icon: Option<&'a L1Str>,
}

impl<'a> Misc<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Misc<'a> {
        let mut out = Misc::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"MCDT" => out.data = parse_or_default(misc_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
