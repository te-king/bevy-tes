//! `BODY` — a body part.

use crate::common::{Subrecord, enumeration, flags, l1, le_u8, parse_or_default};
use crate::macros::{enum_field, parse_struct};
use tes_core::L1String;

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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyData {
    pub part: BodyPart,
    pub vampire: u8,
    pub flags: BodyPartFlags,
    pub part_type: BodyPartKind,
}

parse_struct! {
    fn body_data -> BodyData {
        part: enumeration,
        vampire: le_u8,
        flags: flags,
        part_type: enumeration,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Body {
    pub id: L1String,
    pub model: L1String,
    /// Race this body part belongs to.
    pub race: L1String,
    pub data: BodyData,
}

impl Body {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Body {
        let mut out = Body::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"MODL" => out.model = l1(sub.data),
                b"FNAM" => out.race = l1(sub.data),
                b"BYDT" => out.data = parse_or_default(body_data, sub.data),
                _ => {}
            }
        }
        out
    }
}
