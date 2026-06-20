//! `ACTI` — an activator.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Acti<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
}

impl<'a> Acti<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Acti<'a> {
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
