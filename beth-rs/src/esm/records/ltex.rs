//! `LTEX` — a landscape texture.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_u32};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ltex<'a> {
    pub id: &'a L1Str,
    /// Texture index (referenced by `VTEX` indices in LAND records).
    pub index: u32,
    /// Texture file name.
    pub texture: &'a L1Str,
}

impl<'a> Ltex<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Ltex<'a> {
        let mut out = Ltex::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"INTV" => out.index = finish(le_u32(sub.data)).unwrap_or(0),
                b"DATA" => out.texture = l1(sub.data),
                _ => {}
            }
        }
        out
    }
}
