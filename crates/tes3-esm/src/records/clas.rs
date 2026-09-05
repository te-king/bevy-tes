//! `CLAS` — a character class.

use crate::common::{array, enumeration, flags, l1, le_u32, parse_or_default};
use crate::shared::{ServiceFlags, Specialization};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Class flags (`CLDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ClassFlags: u32 {
        const PLAYABLE = 0x1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = class_data)]
pub struct ClassData {
    /// Two primary attribute IDs.
    #[tes(read = array(le_u32))]
    pub primary_attributes: [u32; 2],
    #[tes(read = enumeration)]
    pub specialization: Specialization,
    /// Five (minor, major) skill pairs.
    #[tes(read = array(array(le_u32)))]
    pub skills: [[u32; 2]; 5],
    #[tes(read = flags)]
    pub flags: ClassFlags,
    /// Services available for auto-calc / bartering.
    #[tes(read = flags)]
    pub autocalc_flags: ServiceFlags,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Clas<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = l1)]
    pub name: &'a L1Str,
    #[tes(tag = b"CLDT", decode = |bytes| parse_or_default(class_data, bytes))]
    pub data: ClassData,
    #[tes(tag = b"DESC", decode = |bytes| Some(l1(bytes)))]
    pub description: Option<&'a L1Str>,
}
