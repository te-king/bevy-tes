//! `CLAS` — a character class.

use crate::common::{Subrecord, flags, l1, le_u32, parse_or_default};
use crate::shared::ServiceFlags;
use nom::IResult;
use tes_core::L1String;

bitflags::bitflags! {
    /// Class flags (`CLDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ClassFlags: u32 {
        const PLAYABLE = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClassData {
    /// Two primary attribute IDs.
    pub primary_attributes: [u32; 2],
    /// 0 = Combat, 1 = Magic, 2 = Stealth.
    pub specialization: u32,
    /// Five (minor, major) skill pairs.
    pub skills: [[u32; 2]; 5],
    pub flags: ClassFlags,
    /// Services available for auto-calc / bartering.
    pub autocalc_flags: ServiceFlags,
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
    let (input, class_flags) = flags(input)?;
    let (input, autocalc_flags) = flags(input)?;
    Ok((
        input,
        ClassData {
            primary_attributes: [p0, p1],
            specialization,
            skills,
            flags: class_flags,
            autocalc_flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clas {
    pub id: L1String,
    pub name: L1String,
    pub data: ClassData,
    pub description: Option<L1String>,
}

impl Clas {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Clas {
        let mut out = Clas::default();
        for sub in subs {
            match &sub.tag.0 {
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
