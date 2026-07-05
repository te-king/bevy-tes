//! `DIAL` — a dialogue topic. The `INFO` records that follow it belong to it.

use crate::common::{Subrecord, l1};
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dial {
    pub id: L1String,
    /// Dialogue type: 0 = Topic, 1 = Voice, 2 = Greeting, 3 = Persuasion, 4 = Journal.
    /// `None` when the (rare) `DATA` field is absent.
    pub kind: Option<u8>,
}

impl Dial {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Dial {
        let mut out = Dial::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.kind = sub.data.first().copied(),
                _ => {}
            }
        }
        out
    }
}
