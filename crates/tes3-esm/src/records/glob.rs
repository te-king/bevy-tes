//! `GLOB` — a global variable.

use crate::common::{finish, l1, le_f32};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

enum_field! {
    /// Declared variable type (`FNAM`, stored as an ASCII type character).
    pub enum GlobalKind: u8 {
        Short = 0x73, // 's'
        Long = 0x6c,  // 'l'
        Float = 0x66, // 'f'
    }
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Glob<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    /// `None` when the `FNAM` field is absent.
    #[tes(tag = b"FNAM", decode = |bytes: &[u8]| bytes.first().map(|&b| GlobalKind::from(b)))]
    pub kind: Option<GlobalKind>,
    /// Value (all globals are stored as floats regardless of declared type).
    #[tes(tag = b"FLTV", decode = |bytes| finish(le_f32(bytes)).unwrap_or(0.0))]
    pub value: f32,
}
