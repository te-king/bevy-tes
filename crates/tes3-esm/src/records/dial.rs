//! `DIAL` — a dialogue topic. The `INFO` records that follow it belong to it.

use crate::common::{Subrecord, l1};
use crate::macros::enum_field;
use tes_core::L1Str;

enum_field! {
    /// Dialogue type (`DATA`). Shared with the INFO records that follow the topic.
    pub enum DialogueKind: u8 {
        Topic = 0,
        Voice = 1,
        Greeting = 2,
        Persuasion = 3,
        Journal = 4,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dial<'a> {
    pub id: &'a L1Str,
    /// `None` when the (rare) `DATA` field is absent.
    pub kind: Option<DialogueKind>,
}

impl<'a> Dial<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Dial<'a> {
        let mut out = Dial::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.kind = sub.data.first().map(|&b| DialogueKind::from(b)),
                _ => {}
            }
        }
        out
    }
}
