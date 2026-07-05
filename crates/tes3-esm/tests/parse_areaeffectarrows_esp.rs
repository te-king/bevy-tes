//! End-to-end test parsing a plugin (`.esp`) rather than a master (`.esm`).
//!
//! In TES3, ESP and ESM share one binary format; the only difference is bit `0x1`
//! ("treat as master") in the `HEDR` flags. This test proves `Plugin::parse` reads a
//! plugin unchanged, that the master flag is *clear*, and that a plugin's master
//! dependency (`MAST`/`DATA`) is decoded.

use std::collections::BTreeMap;

use tes3_esm::{Plugin, Record};

/// The file is gitignored, locally supplied game data; `None` means skip the test.
fn load_bytes() -> Option<Vec<u8>> {
    tes_testdata::read("AreaEffectArrows.esp")
}

#[test]
fn header_is_v12_plugin() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = Plugin::parse(&bytes).unwrap();
    assert_eq!(plugin.header.version, 1.2);
    // The defining ESP-vs-ESM property: the master flag is clear (inverse of an ESM).
    assert_eq!(
        plugin.header.flags & 0x1,
        0,
        "ESP should not be flagged master"
    );
    assert_eq!(plugin.header.num_records as usize, plugin.records.len() - 1);
}

#[test]
fn depends_on_morrowind_master() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = Plugin::parse(&bytes).unwrap();
    // A plugin's HEDR carries MAST/DATA pairs for the masters it extends.
    assert_eq!(plugin.header.masters.len(), 1);
    let master = &plugin.header.masters[0];
    assert_eq!(master.name, "Morrowind.esm");
    assert_eq!(master.size, 79_837_557);
}

#[test]
fn total_record_count_matches_reference() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = Plugin::parse(&bytes).unwrap();
    assert_eq!(plugin.records.len(), 85);
}

#[test]
fn per_type_counts_match_reference() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &plugin.records {
        let tag = String::from_utf8_lossy(&record.tag()).into_owned();
        *counts.entry(tag).or_default() += 1;
    }

    let expected = [
        ("WEAP", 44),
        ("ENCH", 12),
        ("GMST", 12),
        ("CONT", 6),
        ("CELL", 4),
        ("ACTI", 1),
        ("DOOR", 1),
        ("MISC", 1),
        ("NPC_", 1),
        ("PGRD", 1),
        ("STAT", 1),
        ("TES3", 1),
    ];
    for (tag, count) in expected {
        assert_eq!(
            counts.get(tag).copied(),
            Some(count),
            "count mismatch for {tag}"
        );
    }
    assert_eq!(
        counts.len(),
        12,
        "plugin should contain 12 distinct record types"
    );
}

#[test]
fn no_record_is_unknown() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = Plugin::parse(&bytes).unwrap();
    let unknown: Vec<_> = plugin
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Unknown { tag, .. } => Some(String::from_utf8_lossy(tag).into_owned()),
            _ => None,
        })
        .collect();
    assert!(unknown.is_empty(), "unmodelled record types: {unknown:?}");
}
