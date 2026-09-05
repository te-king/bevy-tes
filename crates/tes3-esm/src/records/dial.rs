//! `DIAL` — a dialogue topic. The `INFO` records that follow it belong to it.

use crate::common::l1;
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

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

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Dial<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    /// `None` when the (rare) `DATA` field is absent.
    #[tes(tag = b"DATA", decode = |bytes: &[u8]| bytes.first().map(|&b| DialogueKind::from(b)))]
    pub kind: Option<DialogueKind>,
}
