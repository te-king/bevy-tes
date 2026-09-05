//! `ACTI` — an activator.

use crate::common::l1;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Acti<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
}
