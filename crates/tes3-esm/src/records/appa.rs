//! `APPA` — an alchemy apparatus.

use crate::common::{enumeration, l1, le_f32, le_u32, parse_or_default};
use crate::macros::enum_field;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

enum_field! {
    /// Apparatus type (`AADT`).
    pub enum ApparatusKind: u32 {
        MortarAndPestle = 0,
        Alembic = 1,
        Calcinator = 2,
        Retort = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = apparatus_data)]
pub struct ApparatusData {
    #[tes(read = enumeration)]
    pub kind: ApparatusKind,
    #[tes(read = le_f32)]
    pub quality: f32,
    #[tes(read = le_f32)]
    pub weight: f32,
    #[tes(read = le_u32)]
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Appa<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"MODL", decode = |bytes| Some(l1(bytes)))]
    pub model: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub name: Option<&'a L1Str>,
    #[tes(tag = b"SCRI", decode = |bytes| Some(l1(bytes)))]
    pub script: Option<&'a L1Str>,
    #[tes(tag = b"AADT", decode = |bytes| Some(parse_or_default(apparatus_data, bytes)))]
    pub data: Option<ApparatusData>,
    #[tes(tag = b"ITEX", decode = |bytes| Some(l1(bytes)))]
    pub icon: Option<&'a L1Str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Subrecord, Tag};

    #[test]
    fn apparatus_payload_preserves_order_unknown_kind_and_presence() {
        assert_eq!(Appa::from_subrecords(std::iter::empty()).data, None);
        let bytes = [
            99_u32.to_le_bytes(),
            1.5_f32.to_le_bytes(),
            2.5_f32.to_le_bytes(),
            17_u32.to_le_bytes(),
        ]
        .concat();
        let expected = ApparatusData {
            kind: ApparatusKind::Unknown(99),
            quality: 1.5,
            weight: 2.5,
            value: 17,
        };
        assert_eq!(apparatus_data(&bytes).unwrap(), (&[][..], expected));
        let out =
            Appa::from_subrecords(
                [&bytes[..], &bytes[..15]]
                    .into_iter()
                    .map(|data| Subrecord {
                        tag: Tag(*b"AADT"),
                        data,
                    }),
            );
        assert_eq!(out.data, Some(ApparatusData::default()));
    }
}
