//! `CLAS` — a character class.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClassData {
    /// Two primary attribute IDs.
    pub primary_attributes: [u32; 2],
    /// 0 = Combat, 1 = Magic, 2 = Stealth.
    pub specialization: u32,
    /// Five (minor, major) skill pairs.
    pub skills: [[u32; 2]; 5],
    /// `0x1` = Playable.
    pub flags: u32,
    /// Services available for auto-calc / bartering (bitfield).
    pub autocalc_flags: u32,
}

fn class_data(input: &[u8]) -> IResult<&[u8], ClassData> {
    let (input, p0) = le_u32(input)?;
    let (input, p1) = le_u32(input)?;
    let (input, specialization) = le_u32(input)?;
    let mut input = input;
    let mut skills = [[0u32; 2]; 5];
    for pair in skills.iter_mut() {
        let (rest, minor) = le_u32(input)?;
        let (rest, major) = le_u32(rest)?;
        *pair = [minor, major];
        input = rest;
    }
    let (input, flags) = le_u32(input)?;
    let (input, autocalc_flags) = le_u32(input)?;
    Ok((
        input,
        ClassData {
            primary_attributes: [p0, p1],
            specialization,
            skills,
            flags,
            autocalc_flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clas<'a> {
    pub id: &'a L1Str,
    pub name: &'a L1Str,
    pub data: ClassData,
    pub description: Option<&'a L1Str>,
}

impl<'a> Clas<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Clas<'a> {
        let mut out = Clas::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = l1(sub.data),
                b"CLDT" => out.data = parse_or_default(class_data, sub.data),
                b"DESC" => out.description = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
