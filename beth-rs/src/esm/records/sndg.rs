//! `SNDG` — a sound generator.

use crate::types::latin1::L1String;
use crate::esm::common::{Subrecord, l1, finish, le_u32};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sndg {
    pub id: L1String,
    /// Sound type (0 = Left Foot … 7 = Land).
    pub kind: u32,
    pub creature: Option<L1String>,
    /// Sound ID string.
    pub sound: Option<L1String>,
}

impl Sndg {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Sndg {
        let mut out = Sndg::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.kind = finish(le_u32(sub.data)).unwrap_or(0),
                b"CNAM" => out.creature = Some(l1(sub.data)),
                b"SNAM" => out.sound = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
