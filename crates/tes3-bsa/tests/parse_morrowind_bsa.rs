//! End-to-end tests parsing the `Morrowind.bsa` archive (gitignored, locally supplied
//! game data; the tests skip themselves when it isn't present).

use tes3_bsa::Bsa;

fn open_bsa() -> Option<Bsa> {
    Some(Bsa::open(tes_testdata::fixture("Morrowind.bsa")?).expect("open bsa"))
}

#[test]
fn parses_archive() {
    let Some(bsa) = open_bsa() else { return };
    assert_eq!(bsa.version(), 0x100);
    assert_eq!(bsa.len(), 11_090);

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
    let Some(bsa) = open_bsa() else { return };
    for (name, _) in bsa.files() {
        assert!(!name.is_empty(), "every entry should have a name");
    }
    let total: usize = bsa.files().map(|(_, data)| data.len()).sum();
    assert!(total > 0, "archive should contain file data");
}

#[test]
fn dds_textures_have_the_dds_magic() {
    let Some(bsa) = open_bsa() else { return };
    // Spot-check that a .dds entry's bytes really are a DDS file ("DDS " magic),
    // proving the size/offset resolution lands on the right data.
    let (_, dds) = bsa
        .files()
        .find(|(name, _)| name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&dds[..4], b"DDS ");
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

    let Some(path) = tes_testdata::fixture("Morrowind.bsa") else {
        return;
    };
    let total_data: usize = Bsa::open(&path)
        .expect("open bsa")
        .files()
        .map(|(_, data)| data.len())
        .sum();
    const ITERATIONS: u32 = 20;

    let mut total = Duration::ZERO;
    let mut best = Duration::MAX;
    let mut checksum = 0u64;
    let mut file_count = 0;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let bsa = Bsa::open(&path).expect("open should succeed");
        let mut acc = 0u64;
        for (_, data) in bsa.files() {
            acc = fold(acc, data);
        }
        let elapsed = start.elapsed();
        checksum ^= acc;
        file_count = bsa.len();
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
