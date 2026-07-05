//! `GMST` — a game setting.

use crate::common::{Subrecord, finish, l1, le_f32, le_i32};
use tes_core::L1String;

/// A game setting's value. The type is determined by which value subrecord is present;
/// a setting may also have no value at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GmstValue {
    #[default]
    None,
    Float(f32),
    Int(i32),
    Str(L1String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gmst {
    pub id: L1String,
    pub value: GmstValue,
}

impl Gmst {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Gmst {
        let mut out = Gmst::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"FLTV" => out.value = GmstValue::Float(finish(le_f32(sub.data)).unwrap_or(0.0)),
                b"INTV" => out.value = GmstValue::Int(finish(le_i32(sub.data)).unwrap_or(0)),
                b"STRV" => out.value = GmstValue::Str(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
