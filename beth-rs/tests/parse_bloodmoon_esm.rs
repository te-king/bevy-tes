//! End-to-end test parsing the Bloodmoon expansion master.
//!
//! Bloodmoon is a v1.3 file and exercises things Morrowind.esm does not — most notably
//! the `SSCR` (start script) record, which only appears in the Tribunal/Bloodmoon era.
//! This guards against accidentally tailoring the parser to the base game.

use std::collections::BTreeMap;

use beth_rs::{Plugin, Record};

const ESM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Bloodmoon.esm");

fn load_bytes() -> Vec<u8> {
    std::fs::read(ESM_PATH).expect("Bloodmoon.esm should be readable")
}

#[test]
fn header_is_v13_master() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    // Bloodmoon bumps the format version to 1.3 (Morrowind.esm is 1.2).
    assert_eq!(plugin.header.version, 1.3);
    assert!(plugin.header.flags & 0x1 != 0, "ESM should be flagged master");
    assert_eq!(plugin.header.num_records as usize, plugin.records.len() - 1);
}

#[test]
fn total_record_count_matches_reference() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    assert_eq!(plugin.records.len(), 10_776);
}

#[test]
fn per_type_counts_match_reference() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &plugin.records {
        let tag = String::from_utf8_lossy(&record.tag()).into_owned();
        *counts.entry(tag).or_default() += 1;
    }

    let expected = [
        ("INFO", 6_504),
        ("DIAL", 860),
        ("STAT", 517),
        ("CELL", 276),
        ("NPC_", 215),
        ("LAND", 150),
        ("CREA", 75),
        ("REGN", 6), // v1.3 REGN carries 10-byte weather (snow/blizzard)
        ("SSCR", 1), // only present in Tribunal/Bloodmoon
        ("TES3", 1),
        ("CLAS", 1),
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
        38,
        "Bloodmoon should contain 38 distinct record types"
    );
}

#[test]
fn no_record_is_unknown() {
    let bytes = load_bytes();
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

#[test]
fn sscr_is_decoded() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let sscr = plugin
        .records
        .iter()
        .find_map(|r| match r {
            Record::Sscr(s) => Some(s),
            _ => None,
        })
        .expect("Bloodmoon contains a start script");
    // The DATA field is a non-empty run of ASCII digits.
    assert!(!sscr.data.is_empty());
    assert!(sscr.data.as_bytes().iter().all(|b| b.is_ascii_digit()));
}

#[test]
fn v13_region_weather_has_snow_and_blizzard() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    // At least one region should set the v1.3-only snow/blizzard weather chances,
    // confirming the variable-length WEAT field isn't hard-capped at the v1.2 size.
    let any_snow = plugin.records.iter().any(|r| match r {
        Record::Regn(reg) => reg.weather.snow != 0 || reg.weather.blizzard != 0,
        _ => false,
    });
    assert!(any_snow, "expected v1.3 snow/blizzard weather data");
}
