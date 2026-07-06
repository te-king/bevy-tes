//! `BOOK` — a book or scroll.

use crate::common::{Subrecord, flags, l1, le_f32, le_i32, le_u32, parse_or_default, parse_struct};
use tes_core::L1String;

bitflags::bitflags! {
    /// Book flags (`BKDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct BookFlags: u32 {
        const SCROLL = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BookData {
    pub weight: f32,
    pub value: u32,
    pub flags: BookFlags,
    /// Skill ID taught (for skill books), or `-1`.
    pub skill: i32,
    pub enchant_points: u32,
}

parse_struct! {
    fn book_data -> BookData {
        weight: le_f32,
        value: le_u32,
        flags: flags,
        skill: le_i32,
        enchant_points: le_u32,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Book {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub data: BookData,
    pub script: Option<L1String>,
    pub icon: Option<L1String>,
    pub text: Option<L1String>,
    pub enchantment: Option<L1String>,
}

impl Book {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Book {
        let mut out = Book::default();
        for sub in subs {
            match &sub.tag.0 {
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
