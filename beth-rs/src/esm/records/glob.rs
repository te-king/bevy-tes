//! `GLOB` — a global variable.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_f32};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Glob<'a> {
    pub id: &'a L1Str,
    /// Variable type character: `s` = short, `l` = long, `f` = float.
    pub kind: Option<char>,
    /// Value (all globals are stored as floats regardless of declared type).
    pub value: f32,
}

impl<'a> Glob<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Glob<'a> {
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
