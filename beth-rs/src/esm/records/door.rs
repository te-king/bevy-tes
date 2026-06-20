//! `DOOR` — a door.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Door<'a> {
    pub id: &'a L1Str,
    pub model: &'a L1Str,
    pub name: Option<&'a L1Str>,
    pub script: Option<&'a L1Str>,
    /// Sound played when opening.
    pub open_sound: Option<&'a L1Str>,
    /// Sound played when closing.
    pub close_sound: Option<&'a L1Str>,
}

impl<'a> Door<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Door<'a> {
        let mut out = Door::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.name = Some(l1(sub.data)),
                b"SCRI" => out.script = Some(l1(sub.data)),
                b"SNAM" => out.open_sound = Some(l1(sub.data)),
                b"ANAM" => out.close_sound = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
