//! `ACTI` — an activator.

use crate::common::{Subrecord, l1};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Acti {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub script: Option<L1String>,
}

impl Acti {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Acti {
        let mut out = Acti::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
