//! End-to-end tests parsing the bundled `Morrowind.bsa` archive.

use tes3_bsa::Bsa;

const MORROWIND_BSA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Morrowind.bsa");

#[test]
fn parses_archive() {
    let bsa = Bsa::open(MORROWIND_BSA).unwrap();
    assert_eq!(bsa.version, 0x100);
    assert_eq!(bsa.files.len(), 11_090);

    // Reference entry from an independent directory scan.
    let probe = bsa
        .get(r"meshes\m\probe_journeyman_01.nif")
        .expect("known mesh should be present");
    assert_eq!(probe.len(), 6276);

    // Lookup is case-insensitive and separator-tolerant.
    assert!(bsa.get("MESHES/M/PROBE_JOURNEYMAN_01.NIF").is_some());
}

#[test]
fn every_entry_has_a_name_and_data() {
    let bsa = Bsa::open(MORROWIND_BSA).unwrap();
    for f in &bsa.files {
        assert!(!f.name.is_empty(), "every entry should have a name");
    }
    let total: usize = bsa.files.iter().map(|f| bsa.bytes(f).len()).sum();
    assert!(total > 0, "archive should contain file data");
}

#[test]
fn dds_textures_have_the_dds_magic() {
    let bsa = Bsa::open(MORROWIND_BSA).unwrap();
    // Spot-check that a .dds entry's bytes really are a DDS file ("DDS " magic),
    // proving the size/offset resolution lands on the right data.
    let dds = bsa
        .files
        .iter()
        .find(|f| f.name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&bsa.bytes(dds)[..4], b"DDS ");
}

/// Fold bytes into a checksum fast enough to stay memory-bandwidth bound, so the read
/// can't be optimized away.
fn fold(mut acc: u64, data: &[u8]) -> u64 {
    let mut chunks = data.chunks_exact(8);
    for c in &mut chunks {
        acc ^= u64::from_le_bytes(c.try_into().unwrap());
    }
    for &b in chunks.remainder() {
        acc = acc.rotate_left(8) ^ b as u64;
    }
    acc
}

/// Time opening the archive and reading every file's bytes — the BSA analogue of the
/// ESM parse-timing test. Run with `--show-output` to see the measurement, e.g.:
/// `cargo test -p tes3-bsa --release parse_timing -- --show-output`
#[test]
fn parse_timing() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    let total_data: usize = Bsa::open(MORROWIND_BSA)
        .unwrap()
        .files
        .iter()
        .map(|f| f.size as usize)
        .sum();
    const ITERATIONS: u32 = 20;

    let mut total = Duration::ZERO;
    let mut best = Duration::MAX;
    let mut checksum = 0u64;
    let mut file_count = 0;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let bsa = Bsa::open(MORROWIND_BSA).expect("open should succeed");
        let mut acc = 0u64;
        for f in &bsa.files {
            acc = fold(acc, bsa.bytes(f));
        }
        let elapsed = start.elapsed();
        checksum ^= acc;
        file_count = bsa.files.len();
        total += elapsed;
        best = best.min(elapsed);
    }
    black_box(checksum);

    let mib = total_data as f64 / (1024.0 * 1024.0);
    let avg = total / ITERATIONS;
    let throughput = mib / best.as_secs_f64();
    println!(
        "read {file_count} files / {mib:.1} MiB over {ITERATIONS} runs: \
         best {best:.2?}, avg {avg:.2?} ({throughput:.0} MiB/s)"
    );

    assert_eq!(file_count, 11_090);
    assert!(
        best < Duration::from_secs(30),
        "read unexpectedly slow: {best:.2?}"
    );
}
