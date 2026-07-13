//! `STAT` — a static object.

use crate::common::{Subrecord, l1};
use tes_core::L1Str;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stat<'a> {
    pub id: &'a L1Str,
    /// NIF model file name.
    pub model: &'a L1Str,
}

impl<'a> Stat<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Stat<'a> {
        let mut out = Stat::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                _ => {}
            }
        }
        out
    }
}
