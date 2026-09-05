//! `BODY` — a body part.

use crate::common::{enumeration, flags, l1, le_u8, parse_or_default};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

bitflags::bitflags! {
    /// Body part flags (`BYDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct BodyPartFlags: u8 {
        const FEMALE = 0x1;
        const PLAYABLE = 0x2;
    }
}

enum_field! {
    /// Which body slot the part occupies (`BYDT`).
    pub enum BodyPart: u8 {
        Head = 0,
        Hair = 1,
        Neck = 2,
        Chest = 3,
        Groin = 4,
        Hand = 5,
        Wrist = 6,
        Forearm = 7,
        UpperArm = 8,
        Foot = 9,
        Ankle = 10,
        Knee = 11,
        UpperLeg = 12,
        Clavicle = 13,
        Tail = 14,
    }
}

enum_field! {
    /// What the part is made of (`BYDT`).
    pub enum BodyPartKind: u8 {
        Skin = 0,
        Clothing = 1,
        Armor = 2,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = body_data)]
pub struct BodyData {
    #[tes(read = enumeration)]
    pub part: BodyPart,
    #[tes(read = le_u8)]
    pub vampire: u8,
    #[tes(read = flags)]
    pub flags: BodyPartFlags,
    #[tes(read = enumeration)]
    pub part_type: BodyPartKind,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Body<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = l1)]
    pub model: &'a L1Str,
    /// Race this body part belongs to.
    #[tes(tag = b"FNAM", decode = l1)]
    pub race: &'a L1Str,
    #[tes(tag = b"BYDT", decode = |bytes| parse_or_default(body_data, bytes))]
    pub data: BodyData,
}
