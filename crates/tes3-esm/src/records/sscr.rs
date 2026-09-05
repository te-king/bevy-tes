//! `SSCR` — a start script (a feature added by Tribunal, also used by Bloodmoon).

use crate::common::l1;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Sscr<'a> {
    /// Unknown data — a series of ASCII digits.
    #[tes(tag = b"DATA", decode = l1)]
    pub data: &'a L1Str,
    /// Script name (technically optional).
    #[tes(tag = b"NAME", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
}
