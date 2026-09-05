//! `BOOK` — a book or scroll.

use crate::common::{flags, l1, le_f32, le_i32, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Book flags (`BKDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct BookFlags: u32 {
        const SCROLL = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = book_data)]
pub struct BookData {
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
    #[tes(read = flags)]
    pub flags: BookFlags,
    /// Skill ID taught (for skill books), or `-1`.
    #[tes(read = le_i32)]
    pub skill: i32,
    #[tes(read = le_u32)]
    pub enchant_points: u32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Book<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"BKDT", decode = |bytes| parse_or_default(book_data, bytes))]
    pub data: BookData,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
    #[tes(tag = b"TEXT", decode = |bytes| Some(l1(bytes)))]
    pub text: Option<&'a L1Str>,
    #[tes(tag = b"ENAM", decode = |bytes| Some(l1(bytes)))]
    pub enchantment: Option<&'a L1Str>,
}
