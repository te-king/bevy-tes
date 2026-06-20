//! `GMST` — a game setting.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_f32, le_i32};

/// A game setting's value. The type is determined by which value subrecord is present;
/// a setting may also have no value at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GmstValue<'a> {
    #[default]
    None,
    Float(f32),
    Int(i32),
    Str(&'a L1Str),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gmst<'a> {
    pub id: &'a L1Str,
    pub value: GmstValue<'a>,
}

impl<'a> Gmst<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Gmst<'a> {
        let mut out = Gmst::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FLTV" => {
                    out.value = GmstValue::Float(finish(le_f32(sub.data)).unwrap_or(0.0))
                }
                b"INTV" => out.value = GmstValue::Int(finish(le_i32(sub.data)).unwrap_or(0)),
                b"STRV" => out.value = GmstValue::Str(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
