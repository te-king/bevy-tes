//! `DOOR` — a door.

use crate::common::{Subrecord, l1};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Door {
    pub id: L1String,
    pub model: L1String,
    pub name: Option<L1String>,
    pub script: Option<L1String>,
    /// Sound played when opening.
    pub open_sound: Option<L1String>,
    /// Sound played when closing.
    pub close_sound: Option<L1String>,
}

impl Door {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Door {
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
