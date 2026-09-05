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

#[test]
fn fixed_arrays_preserve_nested_order_and_short_read_errors() {
    use crate::common::array;
    use crate::records::{crea::Crea, fact::Fact};

    let data: Vec<u8> = (0..60u32).flat_map(u32::to_le_bytes).collect();
    let bytes = subrecords(&[(b"NPDT", &data[..96])]);
    let creature = Crea::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(creature.data.attributes, [2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(creature.data.attacks, [[17, 18], [19, 20], [21, 22]]);
    assert_eq!(creature.data.gold, 23);

    let bytes = subrecords(&[(b"FADT", &data)]);
    let faction = Fact::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(faction.data.attributes, [0, 1]);
    assert_eq!(faction.data.ranks[9].attribute_mods, [47, 48]);
    assert_eq!(faction.data.ranks[9].reaction_mod, 51);
    assert_eq!(faction.data.skills, [52, 53, 54, 55, 56, 57, 58]);
    assert_eq!(faction.data.flags.bits(), 59);

    let mut parse = array::<_, 2>(array::<_, 2>(le_u16));
    let (rest, values) = parse(&data[..10]).unwrap();
    assert_eq!(values, [[0, 0], [1, 0]]);
    assert_eq!(rest, &data[8..10]);
    for length in 0..8 {
        assert!(parse(&data[..length]).is_err());
    }
    let (rest, values) = array::<_, 0>(le_u8)(&data).unwrap();
    assert_eq!(rest, data);
    assert_eq!(values, []);
}

#[test]
fn npc_variants_retain_previous_stats_after_malformed_duplicates() {
    use crate::records::npc::{Npc, NpcStats};

    let full: Vec<u8> = (0..52).collect();
    let bytes = subrecords(&[(b"NPDT", &full), (b"NPDT", &[0; 13]), (b"NPDT", &[])]);
    let record = Npc::from_subrecords(Subrecords::new(&bytes));
    let NpcStats::Full {
        level,
        attributes,
        skills,
        health,
        gold,
        ..
    } = record.stats
    else {
        panic!("malformed duplicates must not replace the full stats");
    };
    assert_eq!(level, 256);
    assert_eq!(attributes, [2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(skills, std::array::from_fn(|i| i as u8 + 10));
    assert_eq!(health, 0x2726);
    assert_eq!(gold, 0x33323130);

    let bytes = subrecords(&[(b"NPDT", &full), (b"NPDT", &full[..12]), (b"NPDT", &[])]);
    let record = Npc::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(
        record.stats,
        NpcStats::AutoCalc {
            level: 256,
            disposition: 2,
            reputation: 3,
            rank: 4,
            gold: 0x0b0a0908,
        }
    );
}

#[test]
fn actor_handlers_preserve_order_and_malformed_group_behavior() {
    use crate::records::{crea::Crea, npc::Npc};
    use crate::shared::AiPackage;

    let mut escort = [0; 48];
    escort[14..20].copy_from_slice(b"target");
    let bytes = subrecords(&[
        (b"DNAM", b"orphan"),
        (b"CNDT", b"orphan"),
        (b"NPCO", &[]),
        (b"NPCS", b"short"),
        (b"DODT", &[0; 24]),
        (b"DNAM", b"old"),
        (b"DODT", &[]),
        (b"DNAM", b"destination"),
        (b"AI_E", &escort),
        (b"AI_F", &[]),
        (b"NAME", b"actor"),
        (b"CNDT", b"escort cell"),
        (b"AI_T", &[0; 12]),
        (b"CNDT", b"must not modify escort"),
    ]);
    let npc = Npc::from_subrecords(Subrecords::new(&bytes));
    let crea = Crea::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(npc.inventory, crea.inventory);
    assert_eq!(npc.spells, crea.spells);
    assert_eq!(npc.destinations, crea.destinations);
    assert_eq!(npc.ai_packages, crea.ai_packages);
    assert_eq!(npc.inventory.len(), 1);
    assert_eq!(npc.inventory[0].count, 0);
    assert_eq!(npc.spells, [L1Str::from_bytes(b"")]);
    assert_eq!(npc.destinations.len(), 1);
    assert_eq!(npc.destinations[0].cell.unwrap(), "destination");
    assert!(matches!(&npc.ai_packages[..],
        [AiPackage::Escort { id, cell: Some(cell), .. }, AiPackage::Travel { .. }]
        if *id == "target" && *cell == "escort cell"
    ));
}

#[test]
fn padded_payloads_and_partial_path_points_keep_their_boundaries() {
    use crate::records::{info::Info, pgrd::Pgrd};

    let mut info = [0; 12];
    info[4..8].copy_from_slice(&42u32.to_le_bytes());
    info[8..11].copy_from_slice(&[1, 2, 3]);
    let bytes = subrecords(&[(b"DATA", &info)]);
    let parsed = Info::from_subrecords(Subrecords::new(&bytes));
    let data = parsed.data.unwrap();
    assert_eq!(
        (data.disposition, data.rank, data.gender, data.pc_rank),
        (42, 1, 2, 3)
    );
    let bytes = subrecords(&[(b"DATA", &info), (b"DATA", &info[..11])]);
    assert_eq!(
        Info::from_subrecords(Subrecords::new(&bytes)).data,
        Some(Default::default())
    );

    let mut point = [0; 16];
    point[..4].copy_from_slice(&7i32.to_le_bytes());
    point[13] = 2;
    let mut points = point.to_vec();
    points.extend(&point[..15]);
    let bytes = subrecords(&[(b"PGRP", &points)]);
    let parsed = Pgrd::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(parsed.points.len(), 1);
    assert_eq!(
        (parsed.points[0].x, parsed.points[0].connection_count),
        (7, 2)
    );
}

#[test]
fn grouped_filters_and_leveled_entries_reset_only_the_current_value() {
    use crate::records::{info::Info, levc::Levc, levi::Levi};

    let bytes = subrecords(&[
        (b"INTV", &[9, 0, 0, 0]),
        (b"SCVR", b"first"),
        (b"INTV", &[3, 0, 0, 0]),
        (b"SCVR", b"second"),
        (b"INTV", &[4, 0, 0, 0]),
        (b"ZZZZ", b""),
        (b"INTV", &[]),
    ]);
    let info = Info::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(info.filters.len(), 2);
    assert_eq!(info.filters[0].int_value, Some(3));
    assert_eq!(info.filters[1].int_value, None);

    let bytes = subrecords(&[
        (b"INTV", &[9, 0]),
        (b"INAM", b"item"),
        (b"CNAM", b"creature"),
        (b"INTV", &[7, 0]),
        (b"NAME", b"list"),
        (b"INTV", &[]),
    ]);
    let items = Levi::from_subrecords(Subrecords::new(&bytes));
    let creatures = Levc::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].level, 0);
    assert_eq!(creatures.creatures.len(), 1);
    assert_eq!(creatures.creatures[0].level, 0);
}

#[test]
fn land_retains_grouped_fields_but_script_lists_replace() {
    use crate::records::{land::Land, scpt::Scpt};

    let coords: Vec<u8> = [7i32, -2].into_iter().flat_map(i32::to_le_bytes).collect();
    let bytes = subrecords(&[
        (b"INTV", &coords),
        (b"INTV", &[0]),
        (b"VHGT", &[0, 0, 0x80, 0x3f, 2, 3]),
        (b"VHGT", &[0]),
        (b"VTEX", &[4, 5]),
    ]);
    let land = Land::from_subrecords(Subrecords::new(&bytes));
    assert_eq!((land.grid_x, land.grid_y), (7, -2));
    assert_eq!(land.height_offset, Some(1.0));
    assert_eq!(land.heights, Some([2, 3].as_slice()));
    assert_eq!(land.texture_data, Some([4, 5].as_slice()));

    let bytes = subrecords(&[
        (b"SCVR", b"one\0\0two\0"),
        (b"SCDT", b"bytecode"),
        (b"SCVR", b"last\0"),
    ]);
    let script = Scpt::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(script.variables, [L1Str::from_bytes(b"last")]);
    assert_eq!(script.data, b"bytecode");
    assert_eq!(
        script.variables[0].as_bytes().as_ptr(),
        Subrecords::new(&bytes).last().unwrap().data.as_ptr()
    );
}

#[test]
fn cell_payload_derives_preserve_header_and_reference_phases() {
    use crate::records::cell::{Cell, CellFlags};

    let header: Vec<u8> = [CellFlags::INTERIOR.bits(), 7, 8]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let transform: Vec<u8> = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let bytes = subrecords(&[
        (b"NAME", b"cell"),
        (b"DATA", &header),
        (b"FRMR", &[1, 0, 0, 0]),
        (b"NAME", b"object"),
        (b"DATA", &transform),
    ]);
    let cell = Cell::from_subrecords(Subrecords::new(&bytes));
    assert_eq!(cell.name, "cell");
    assert_eq!((cell.data.grid_x, cell.data.grid_y), (7, 8));
    assert_eq!(cell.references.len(), 1);
    assert_eq!(cell.references[0].object, "object");
    let transform = cell.references[0].transform.unwrap();
    assert_eq!(transform.position, [1.0, 2.0, 3.0]);
    assert_eq!(transform.rotation, [4.0, 5.0, 6.0]);
}
