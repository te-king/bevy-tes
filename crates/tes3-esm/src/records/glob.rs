//! `GLOB` — a global variable.

use crate::common::{Subrecord, enum_field, finish, l1, le_f32};
use tes_core::L1String;

enum_field! {
    /// Declared variable type (`FNAM`, stored as an ASCII type character).
    pub enum GlobalKind: u8 {
        Short = 0x73, // 's'
        Long = 0x6c,  // 'l'
        Float = 0x66, // 'f'
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Glob {
    pub id: L1String,
    /// `None` when the `FNAM` field is absent.
    pub kind: Option<GlobalKind>,
    /// Value (all globals are stored as floats regardless of declared type).
    pub value: f32,
}

impl Glob {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Glob {
        let mut out = Glob::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.kind = sub.data.first().map(|&b| GlobalKind::from(b)),
                b"FLTV" => out.value = finish(le_f32(sub.data)).unwrap_or(0.0),
                _ => {}
            }
        }
        out
    }
}
