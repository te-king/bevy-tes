//! `SNDG` — a sound generator.

use crate::common::{Subrecord, enumeration, finish, l1};
use crate::macros::enum_field;
use tes_core::L1Str;

enum_field! {
    /// Sound generator trigger (`DATA`).
    pub enum SoundGenKind: u32 {
        LeftFoot = 0,
        RightFoot = 1,
        SwimLeft = 2,
        SwimRight = 3,
        Moan = 4,
        Roar = 5,
        Scream = 6,
        Land = 7,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sndg<'a> {
    pub id: &'a L1Str,
    pub kind: SoundGenKind,
    pub creature: Option<&'a L1Str>,
    /// Sound ID string.
    pub sound: Option<&'a L1Str>,
}

impl<'a> Sndg<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Sndg<'a> {
        let mut out = Sndg::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"DATA" => out.kind = finish(enumeration(sub.data)).unwrap_or_default(),
                b"CNAM" => out.creature = Some(l1(sub.data)),
                b"SNAM" => out.sound = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
