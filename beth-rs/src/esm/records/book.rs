//! `BOOK` — a book or scroll.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_f32, le_i32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BookData {
    pub weight: f32,
    pub value: u32,
    /// `0x1` = scroll.
    pub flags: u32,
    /// Skill ID taught (for skill books), or `-1`.
    pub skill: i32,
    pub enchant_points: u32,
}

fn book_data(input: &[u8]) -> IResult<&[u8], BookData> {
    let (input, weight) = le_f32(input)?;
    let (input, value) = le_u32(input)?;
    let (input, flags) = le_u32(input)?;
    let (input, skill) = le_i32(input)?;
    let (input, enchant_points) = le_u32(input)?;
    Ok((
        input,
        BookData {
            weight,
            value,
            flags,
            skill,
            enchant_points,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Book<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub data: BookData,
    pub script: Option<&'a L1Str>,
    pub icon: Option<&'a L1Str>,
    pub text: Option<&'a L1Str>,
    pub enchantment: Option<&'a L1Str>,
}

impl<'a> Book<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Book<'a> {
        let mut out = Book::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"BKDT" => out.data = parse_or_default(book_data, sub.data),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"TEXT" => out.text = Some(l1(sub.data)),
                b"ENAM" => out.enchantment = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
