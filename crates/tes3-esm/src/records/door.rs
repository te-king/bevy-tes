//! `DOOR` — a door.

use crate::common::l1;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Door<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    /// Sound played when opening.
    #[tes(tag = b"SNAM", decode = |bytes| Some(l1(bytes)))]
    pub open_sound: Option<&'a L1Str>,
    /// Sound played when closing.
    #[tes(tag = b"ANAM", decode = |bytes| Some(l1(bytes)))]
    pub close_sound: Option<&'a L1Str>,
}
