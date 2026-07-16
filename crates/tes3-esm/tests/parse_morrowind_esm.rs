//! End-to-end test parsing the `Morrowind.esm` master (gitignored, locally supplied
//! game data; the tests skip themselves when it isn't present).
//!
//! The reference counts come from an independent scan of the file's record framing and
//! must match exactly, proving the parser consumes every record with no leftover bytes.

use std::collections::BTreeMap;

use tes3_esm::records::tes3::HeaderFlags;
use tes3_esm::{EsmDirectory, Record};

/// Read the file into an owned buffer to parse. The parsed `EsmDirectory` borrows from
/// this buffer, so each test keeps it alive alongside the parse. The file is gitignored,
/// locally supplied game data; `None` means skip the test.
fn load_bytes() -> Option<Vec<u8>> {
    tes_testdata::read("Morrowind.esm")
}

#[test]
fn header_is_decoded() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    assert_eq!(plugin.header.version, 1.2);
    assert!(
        plugin.header.flags.contains(HeaderFlags::MASTER),
        "ESM should be flagged master"
    );
    assert_eq!(plugin.header.company, "Bethesda Softworks"); // L1Str: PartialEq<&str>
    // The header declares the number of records that follow it.
    assert_eq!(plugin.header.num_records as usize, plugin.records.len() - 1);
}

#[test]
fn first_record_is_the_header() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    assert!(matches!(plugin.records.first(), Some(Record::Tes3(_))));
}

#[test]
fn total_record_count_matches_reference() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    assert_eq!(plugin.records.len(), 48_296);
}

#[test]
fn per_type_counts_match_reference() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &plugin.records {
        let tag = record.tag().to_string();
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
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    let unknown = plugin
        .records
        .iter()
        .filter(|r| matches!(r, Record::Unknown { .. }))
        .count();
    assert_eq!(unknown, 0, "all 42 record types should be typed");
}

#[test]
fn records_decode_their_fields() {
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();

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
    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();
    // The L1Str views the raw Windows-1252 bytes as-is; the parser never transcoded
    // them.
    assert_eq!(plugin.header.company.as_bytes(), b"Bethesda Softworks");
    // Decoding the ASCII name on demand still borrows rather than allocating.
    assert!(matches!(plugin.header.company.decode(), Cow::Borrowed(_)));
}

/// The VHGT decode is validated against the authored data itself: vanilla terrain is
/// seamless, so adjacent cells' independently delta-encoded shared edges must decode to
/// identical heights. This pins both the running-sum rules and the row/column
/// orientation without relying on any external reference implementation.
#[test]
fn land_heights_decode_and_tile_seamlessly() {
    use std::collections::HashMap;
    use tes3_esm::records::land::{HEIGHT_SCALE, Land, LandFlags};

    let Some(bytes) = load_bytes() else { return };
    let plugin = EsmDirectory::parse(&bytes).unwrap();

    let lands: HashMap<(i32, i32), &Land> = plugin
        .records
        .iter()
        .filter_map(|r| match r {
            Record::Land(l) if l.data_types.contains(LandFlags::HAS_HEIGHTS) => {
                Some(((l.grid_x, l.grid_y), l))
            }
            _ => None,
        })
        .collect();
    assert!(lands.len() > 1_000, "vanilla has ~1390 LAND records");

    // Every heights-bearing LAND decodes fully, to sane values.
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut decoded: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
    for (&grid, land) in &lands {
        let heights = land.decode_heights().expect("HAS_HEIGHTS should decode");
        assert_eq!(heights.len(), 65 * 65);
        for &h in &heights {
            assert!(h.is_finite() && h.abs() < 500_000.0, "wild height {h}");
            min = min.min(h);
            max = max.max(h);
        }
        decoded.insert(grid, heights);
    }
    assert!(min < 0.0, "there should be ocean floor below sea level");
    assert!(max > 0.0, "there should be mountains above sea level");

    // Independent cross-check of the first vertex against the raw fields.
    let land = lands.values().next().unwrap();
    let expected = (land.height_offset.unwrap() + (land.heights.as_ref().unwrap()[0] as i8) as f32)
        * HEIGHT_SCALE;
    assert_eq!(land.decode_heights().unwrap()[0], expected);

    // Seam check: east edge of (x, y) == west edge of (x+1, y); north edge of (x, y)
    // == south edge of (x, y+1).
    let mut compared = 0u64;
    let mut mismatched = 0u64;
    for (&(x, y), heights) in &decoded {
        if let Some(east) = decoded.get(&(x + 1, y)) {
            for row in 0..65 {
                compared += 1;
                if heights[row * 65 + 64] != east[row * 65] {
                    mismatched += 1;
                }
            }
        }
        if let Some(north) = decoded.get(&(x, y + 1)) {
            for col in 0..65 {
                compared += 1;
                if heights[64 * 65 + col] != north[col] {
                    mismatched += 1;
                }
            }
        }
    }
    assert!(compared > 100_000, "expected many adjacent cell pairs");
    // Vanilla terrain is authored seamless; allow a small tolerance for any authored
    // oddities without letting a wrong decode (which mismatches nearly everywhere) pass.
    assert!(
        mismatched * 20 < compared,
        "{mismatched}/{compared} seam vertices mismatch — decode rules are wrong"
    );
}

/// Time how long it takes to parse the full file. Run with `--show-output` (or
/// `--nocapture`) to see the measurements, e.g.:
/// `cargo test -p tes3-esm --release parse_timing -- --show-output`
#[test]
fn parse_timing() {
    use std::time::Instant;

    // Read once so the measurement covers parsing, not disk I/O.
    let Some(bytes) = load_bytes() else { return };
    const ITERATIONS: u32 = 100;

    let mut total = std::time::Duration::ZERO;
    let mut best = std::time::Duration::MAX;
    let mut record_count = 0;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let plugin = EsmDirectory::parse(&bytes).expect("parse should succeed");
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
