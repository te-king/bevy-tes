//! `LTEX` — a landscape texture.

use crate::common::{Subrecord, finish, l1, le_u32};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ltex {
    pub id: L1String,
    /// Texture index (referenced by `VTEX` indices in LAND records).
    pub index: u32,
    /// Texture file name.
    pub texture: L1String,
}

impl Ltex {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Ltex {
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
