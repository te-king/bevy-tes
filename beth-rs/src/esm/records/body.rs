//! `BODY` — a body part.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_u8, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyData {
    /// Body part (0 = Head … 14 = Tail).
    pub part: u8,
    pub vampire: u8,
    /// `0x1` = Female, `0x2` = Playable.
    pub flags: u8,
    /// 0 = Skin, 1 = Clothing, 2 = Armor.
    pub part_type: u8,
}

fn body_data(input: &[u8]) -> IResult<&[u8], BodyData> {
    let (input, part) = le_u8(input)?;
    let (input, vampire) = le_u8(input)?;
    let (input, flags) = le_u8(input)?;
    let (input, part_type) = le_u8(input)?;
    Ok((
        input,
        BodyData {
            part,
            vampire,
            flags,
            part_type,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Body<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    /// Race this body part belongs to.
    pub race: &'a L1Str,
    pub data: BodyData,
}

impl<'a> Body<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Body<'a> {
        let mut out = Body::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.race = l1(sub.data),
                b"BYDT" => out.data = parse_or_default(body_data, sub.data),
                _ => {}
            }
        }
        out
    }
}
