//! End-to-end tests parsing the bundled `Bloodmoon.bsa` archive.

use beth_rs::Bsa;

const BLOODMOON_BSA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/Bloodmoon.bsa");

fn load() -> Vec<u8> {
    std::fs::read(BLOODMOON_BSA).expect("Bloodmoon.bsa should be readable")
}

#[test]
fn parses_archive() {
    let bytes = load();
    let bsa = Bsa::parse(&bytes).unwrap();
    assert_eq!(bsa.version, 0x100);
    assert_eq!(bsa.files.len(), 1_545);

    let tex = bsa
        .get(r"textures\c_nordic02_upperarm.dds")
        .expect("known texture should be present");
    assert_eq!(tex.data.len(), 43_856);
}

#[test]
fn dds_textures_have_the_dds_magic() {
    let bytes = load();
    let bsa = Bsa::parse(&bytes).unwrap();
    let dds = bsa
        .files
        .iter()
        .find(|f| f.name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&dds.data[..4], b"DDS ");
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
/// `cargo test -p beth-rs --release parse_timing -- --show-output`
#[test]
fn parse_timing() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    let bytes = load();
    let total_data: usize = Bsa::parse(&bytes).unwrap().files.iter().map(|f| f.data.len()).sum();
    const ITERATIONS: u32 = 20;

    let mut total = Duration::ZERO;
    let mut best = Duration::MAX;
    let mut checksum = 0u64;
    let mut file_count = 0;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let bsa = Bsa::parse(&bytes).expect("parse should succeed");
        let mut acc = 0u64;
        for f in &bsa.files {
            acc = fold(acc, &f.data);
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

    assert_eq!(file_count, 1_545);
    assert!(
        best < Duration::from_secs(30),
        "read unexpectedly slow: {best:.2?}"
    );
}
