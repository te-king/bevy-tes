//! `GLOB` — a global variable.

use crate::types::latin1::L1String;
use crate::esm::common::{Subrecord, l1, finish, le_f32};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Glob {
    pub id: L1String,
    /// Variable type character: `s` = short, `l` = long, `f` = float.
    pub kind: Option<char>,
    /// Value (all globals are stored as floats regardless of declared type).
    pub value: f32,
}

impl Glob {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Glob {
        let mut out = Glob::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.kind = sub.data.first().map(|&b| b as char),
                b"FLTV" => out.value = finish(le_f32(sub.data)).unwrap_or(0.0),
                _ => {}
            }
        }
        out
    }
}
