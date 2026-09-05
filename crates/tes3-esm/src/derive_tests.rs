use crate::common::{Subrecord, Subrecords, Tag, finish, fixed_l1str, le_u8, le_u16, le_u32};
use crate::records::acti::Acti;
use crate::records::armo::{Armo, ArmorData, ArmorKind};
use crate::records::body::{Body, BodyData, BodyPart, BodyPartKind};
use crate::{EsmDirectory, L1Str, Record};
use tes3_esm_derive::{TesPayload, TesRecord};

fn subrecords(fields: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (tag, data) in fields {
        bytes.extend_from_slice(*tag);
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
    }
    bytes
}

#[test]
fn activator_preserves_defaults_optional_fields_and_borrowing() {
    assert_eq!(Acti::from_subrecords(std::iter::empty()), Acti::default());
    let bytes = subrecords(&[
        (b"NAME", b"old"),
        (b"ZZZZ", b"ignored"),
        (b"SCRI", b""),
        (b"MODL", b"x\\thing.nif\0padding"),
        (b"NAME", b"caf\xe9\0"),
    ]);
    let record = Acti::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(record.id.as_bytes(), b"caf\xe9");
    assert_eq!(record.model, "x\\thing.nif");
    assert_eq!(record.name, None);
    assert_eq!(record.script, Some(L1Str::from_bytes(b"")));
    let id_payload = Subrecords::new(&bytes).last().unwrap().data;
    assert_eq!(record.id.as_bytes().as_ptr(), id_payload.as_ptr());
}

#[test]
fn body_payload_preserves_order_flags_unknown_enums_and_trailing_bytes() {
    let bytes = subrecords(&[
        (b"BYDT", &[2, 1, 0x83, 2, 99]),
        (b"FNAM", b"race"),
        (b"MODL", b"model"),
        (b"NAME", b"part"),
    ]);
    let record = Body::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(record.data.part, BodyPart::Neck);
    assert_eq!(record.data.vampire, 1);
    assert_eq!(record.data.flags.bits(), 0x83);
    assert_eq!(record.data.part_type, BodyPartKind::Armor);
    assert_eq!(record.race, "race");
    assert_eq!(record.model, "model");
    assert_eq!(record.id, "part");

    let bytes = subrecords(&[(b"BYDT", &[250, 0, 0, 251])]);
    let record = Body::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(record.data.part, BodyPart::Unknown(250));
    assert_eq!(record.data.part_type, BodyPartKind::Unknown(251));
}

#[test]
fn malformed_duplicate_payloads_reset_to_default() {
    for len in 0..4 {
        let bytes = subrecords(&[(b"BYDT", &[2, 1, 3, 2]), (b"BYDT", &[9; 4][..len])]);
        let record = Body::from_subrecords(Subrecords::new(&bytes));
        assert_eq!(record.data, BodyData::default());
    }
    let valid: Vec<u8> = [1u32, 2.5f32.to_bits(), 3, 4, 5, 6]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    for len in 0..24 {
        let bytes = subrecords(&[(b"AODT", &valid), (b"AODT", &valid[..len])]);
        let record = Armo::from_subrecords(Subrecords::new(&bytes));
        assert_eq!(record.data, ArmorData::default());
    }
}

#[test]
fn armor_handler_keeps_grouping_and_duplicate_semantics() {
    let data: Vec<u8> = [1u32, 2.5f32.to_bits(), 3, 4, 5, 6]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .chain([99])
        .collect();
    let bytes = subrecords(&[
        (b"BNAM", b"orphan"),
        (b"CNAM", b"orphan"),
        (b"INDX", &[7]),
        (b"BNAM", b"old"),
        (b"NAME", b"armor"),
        (b"ZZZZ", b"ignored"),
        (b"BNAM", b"male"),
        (b"CNAM", b"female"),
        (b"INDX", &[]),
        (b"CNAM", b"second"),
        (b"ENAM", b"enchantment"),
        (b"AODT", &data),
    ]);
    let record = Armo::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(record.biped.len(), 2);
    assert_eq!(record.biped[0].index, 7);
    assert_eq!(record.biped[0].male_model.unwrap(), "male");
    assert_eq!(record.biped[0].female_model.unwrap(), "female");
    assert_eq!(record.biped[1].index, 0);
    assert_eq!(record.biped[1].male_model, None);
    assert_eq!(record.biped[1].female_model.unwrap(), "second");
    assert_eq!(record.id, "armor");
    assert_eq!(record.enchantment.unwrap(), "enchantment");
    assert_eq!(
        record.data,
        ArmorData {
            kind: ArmorKind::Cuirass,
            weight: 2.5,
            value: 3,
            health: 4,
            enchant_points: 5,
            armor_rating: 6,
        }
    );
}

#[test]
fn record_dispatch_and_truncated_subrecords_keep_existing_behavior() {
    let mut bytes = subrecords(&[(b"NAME", b"kept")]);
    bytes.extend(b"FNAM\x08\0\0\0short");
    let acti = Acti::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(acti.id, "kept");
    assert_eq!(acti.name, None);

    let mut plugin = Vec::new();
    for tag in [b"BODY", b"ACTI", b"ARMO"] {
        plugin.extend(tag);
        plugin.extend((bytes.len() as u32).to_le_bytes());
        plugin.extend([0; 8]);
        plugin.extend(&bytes);
    }
    let parsed = EsmDirectory::parse(&plugin).unwrap();
    assert!(matches!(&parsed.records[..],
        [Record::Body(body), Record::Acti(acti), Record::Armo(armo)]
        if body.id == "kept" && acti.id == "kept" && armo.id == "kept"
    ));
}

#[derive(Debug, PartialEq, TesPayload)]
#[tes(parser = borrowed_payload)]
struct BorrowedPayload<'data> {
    #[tes(read = le_u16)]
    count: u16,
    #[tes(read = fixed_l1str(3))]
    text: &'data L1Str,
}

#[derive(Debug, PartialEq, TesPayload)]
#[tes(parser = owned_payload)]
struct OwnedPayload {
    #[tes(read = le_u8)]
    input: u8,
    #[tes(read = le_u16)]
    value: u16,
}

#[test]
fn payload_parsers_borrow_return_remainder_and_propagate_errors() {
    let bytes = b"\x02\x01ab\0tail";
    let (rest, parsed) = borrowed_payload(bytes).unwrap();
    assert_eq!(parsed.count, 258);
    assert_eq!(parsed.text, "ab");
    assert_eq!(parsed.text.as_bytes().as_ptr(), bytes[2..].as_ptr());
    assert_eq!(rest, b"tail");
    for len in 0..5 {
        assert!(borrowed_payload(&bytes[..len]).is_err());
    }
    let (rest, parsed) = owned_payload(&[7, 2, 1, 99]).unwrap();
    assert_eq!(
        parsed,
        OwnedPayload {
            input: 7,
            value: 258
        }
    );
    assert_eq!(rest, &[99]);
}

#[derive(Default, TesRecord)]
#[tes(unmapped = Self::other)]
struct WithHandler {
    #[tes(tag = b"DATA", decode = |bytes| finish(le_u32(bytes)))]
    value: Option<u32>,
    #[tes(skip)]
    other_tags: Vec<Tag>,
}

impl WithHandler {
    fn other(&mut self, sub: Subrecord<'_>) {
        self.other_tags.push(sub.tag);
    }
}

#[test]
fn lifetime_free_records_forward_only_unmapped_tags_in_order() {
    let bytes = subrecords(&[
        (b"AAAA", b""),
        (b"DATA", &[1, 0, 0, 0]),
        (b"BBBB", b""),
        (b"DATA", &[]),
    ]);
    let parsed = WithHandler::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(parsed.value, None);
    assert_eq!(parsed.other_tags, [Tag(*b"AAAA"), Tag(*b"BBBB")]);
}
