//! End-to-end tests parsing the `Bloodmoon.bsa` archive (gitignored, locally supplied
//! game data; the tests skip themselves when it isn't present).

use tes3_bsa::Bsa;

fn open_bsa() -> Option<Bsa> {
    Some(Bsa::open(tes_testdata::fixture("Bloodmoon.bsa")?).expect("open bsa"))
}

#[test]
fn parses_archive() {
    let Some(bsa) = open_bsa() else { return };
    assert_eq!(bsa.version(), 0x100);
    assert_eq!(bsa.len(), 1_545);

    let tex = bsa
        .get(r"textures\c_nordic02_upperarm.dds")
        .expect("known texture should be present");
    assert_eq!(tex.len(), 43_856);
}

#[test]
fn dds_textures_have_the_dds_magic() {
    let Some(bsa) = open_bsa() else { return };
    let (_, dds) = bsa
        .files()
        .find(|(name, _)| name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&dds[..4], b"DDS ");
}

/// Fold bytes into a checksum fast enough to stay memory-bandwidth bound, so the read
/// can't be optimized away.
fn fold(mut acc: u64, data: &[u8]) -> u64 {
    let (chunks, remainder) = data.as_chunks::<8>();
    for c in chunks {
        acc ^= u64::from_le_bytes(*c);
    }
    for &b in remainder {
        acc = acc.rotate_left(8) ^ b as u64;
    }
    acc
}

/// Time opening the archive and reading every file's bytes.
/// `cargo test -p tes3-bsa --release parse_timing -- --show-output`
#[test]
fn parse_timing() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    let Some(path) = tes_testdata::fixture("Bloodmoon.bsa") else {
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

    assert_eq!(file_count, 1_545);
    assert!(
        best < Duration::from_secs(30),
        "read unexpectedly slow: {best:.2?}"
    );
}
