//! `STAT` — a static object.

use crate::common::{Subrecord, l1};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stat {
    pub id: L1String,
    /// NIF model file name.
    pub model: L1String,
}

impl Stat {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Stat {
        let mut out = Stat::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                _ => {}
            }
        }
        out
    }
}
