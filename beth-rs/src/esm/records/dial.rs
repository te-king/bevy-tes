//! `DIAL` — a dialogue topic. The `INFO` records that follow it belong to it.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dial<'a> {
    pub id: &'a L1Str,
    /// Dialogue type: 0 = Topic, 1 = Voice, 2 = Greeting, 3 = Persuasion, 4 = Journal.
    /// `None` when the (rare) `DATA` field is absent.
    pub kind: Option<u8>,
}

impl<'a> Dial<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Dial<'a> {
        let mut out = Dial::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.kind = sub.data.first().copied(),
                _ => {}
            }
        }
        out
    }
}
