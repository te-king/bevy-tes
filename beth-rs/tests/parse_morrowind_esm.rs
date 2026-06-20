//! End-to-end test parsing the bundled `Morrowind.esm` test file.
//!
//! The reference counts come from an independent scan of the file's record framing and
//! must match exactly, proving the parser consumes every record with no leftover bytes.

use std::collections::BTreeMap;

use beth_rs::{Plugin, Record};

const ESM_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Morrowind.esm");

/// Read the file into an owned buffer to parse. The parsed `Plugin` owns its data, so it
/// no longer depends on this buffer once parsing returns.
fn load_bytes() -> Vec<u8> {
    std::fs::read(ESM_PATH).expect("Morrowind.esm should be readable")
}

#[test]
fn header_is_decoded() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    assert_eq!(plugin.header.version, 1.2);
    assert!(
        plugin.header.flags & 0x1 != 0,
        "ESM should be flagged master"
    );
    assert_eq!(plugin.header.company, "Bethesda Softworks"); // L1String: PartialEq<&str>
    // The header declares the number of records that follow it.
    assert_eq!(plugin.header.num_records as usize, plugin.records.len() - 1);
}

#[test]
fn first_record_is_the_header() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    assert!(matches!(plugin.records.first(), Some(Record::Tes3(_))));
}

#[test]
fn total_record_count_matches_reference() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    assert_eq!(plugin.records.len(), 48_296);
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

    // A representative spread across the most and least common record types.
    let expected = [
        ("INFO", 23_693),
        ("STAT", 2_788),
        ("NPC_", 2_675),
        ("CELL", 2_538),
        ("DIAL", 2_358),
        ("GMST", 1_449),
        ("LAND", 1_390),
        ("SCPT", 632),
        ("MGEF", 137),
        ("SKIL", 27),
        ("RACE", 10),
        ("REGN", 9),
        ("LOCK", 6),
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
        42,
        "should see exactly 42 distinct record types"
    );
}

#[test]
fn no_record_is_unknown() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let unknown = plugin
        .records
        .iter()
        .filter(|r| matches!(r, Record::Unknown { .. }))
        .count();
    assert_eq!(unknown, 0, "all 42 record types should be typed");
}

#[test]
fn records_decode_their_fields() {
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();

    // A GMST with a known string value.
    let month = plugin.records.iter().find_map(|r| match r {
        Record::Gmst(g) if g.id == "sMonthMorningstar" => Some(g),
        _ => None,
    });
    assert!(month.is_some(), "GMST sMonthMorningstar should exist");

    // NPCs should have non-empty ids, races and classes.
    let npc = plugin.records.iter().find_map(|r| match r {
        Record::Npc(n) => Some(n),
        _ => None,
    });
    let npc = npc.expect("there should be NPCs");
    assert!(!npc.id.is_empty());
    assert!(!npc.race.is_empty());
    assert!(!npc.class.is_empty());

    // Statics always carry an id and a model path.
    let stat = plugin.records.iter().find_map(|r| match r {
        Record::Stat(s) => Some(s),
        _ => None,
    });
    let stat = stat.expect("there should be statics");
    assert!(!stat.id.is_empty());
    assert!(!stat.model.is_empty());
}

#[test]
fn strings_are_stored_undecoded_and_decode_lazily() {
    use std::borrow::Cow;
    let bytes = load_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    // The L1String stores the raw Windows-1252 bytes as-is; the parser never transcoded
    // them.
    assert_eq!(plugin.header.company.as_bytes(), b"Bethesda Softworks");
    // Decoding the ASCII name on demand still borrows rather than allocating.
    assert!(matches!(plugin.header.company.decode(), Cow::Borrowed(_)));
}

/// Time how long it takes to parse the full file. Run with `--show-output` (or
/// `--nocapture`) to see the measurements, e.g.:
/// `cargo test -p beth-rs --release parse_timing -- --show-output`
#[test]
fn parse_timing() {
    use std::time::Instant;

    // Read once so the measurement covers parsing, not disk I/O.
    let bytes = load_bytes();
    const ITERATIONS: u32 = 100;

    let mut total = std::time::Duration::ZERO;
    let mut best = std::time::Duration::MAX;
    let mut record_count = 0;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let plugin = Plugin::parse(&bytes).expect("parse should succeed");
        let elapsed = start.elapsed();
        record_count = plugin.records.len();
        total += elapsed;
        best = best.min(elapsed);
    }

    let mib = bytes.len() as f64 / (1024.0 * 1024.0);
    let avg = total / ITERATIONS;
    let throughput = mib / best.as_secs_f64();
    println!(
        "parsed {record_count} records from {mib:.1} MiB over {ITERATIONS} runs: \
         best {best:.2?}, avg {avg:.2?} ({throughput:.0} MiB/s)"
    );

    // Sanity guard (loose, to avoid CI flakiness): a full parse should be well under a
    // second on any reasonable machine.
    assert_eq!(record_count, 48_296);
    assert!(
        best < std::time::Duration::from_secs(5),
        "parse unexpectedly slow: {best:.2?}"
    );
}
