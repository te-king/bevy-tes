//! `STAT` — a static object.

use crate::common::l1;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Stat<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    /// NIF model file name.
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
}
