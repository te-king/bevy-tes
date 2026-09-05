//! `SNDG` — a sound generator.

use crate::common::{enumeration, finish, l1};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

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

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Sndg<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"DATA", decode = |bytes| finish(enumeration(bytes)).unwrap_or_default())]
    pub kind: SoundGenKind,
    #[tes(tag = b"CNAM", decode = |bytes| Some(l1(bytes)))]
    pub creature: Option<&'a L1Str>,
    /// Sound ID string.
    #[tes(tag = b"SNAM", decode = |bytes| Some(l1(bytes)))]
    pub sound: Option<&'a L1Str>,
}
