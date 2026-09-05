//! `LTEX` — a landscape texture.

use crate::common::{finish, l1, le_u32};
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Ltex<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    /// Texture index (referenced by `VTEX` indices in LAND records).
    #[tes(tag = b"INTV", decode = |bytes| finish(le_u32(bytes)).unwrap_or(0))]
    pub index: u32,
    /// Texture file name.
    #[tes(tag = b"DATA", decode = l1)]
    pub texture: &'a L1Str,
}
