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
    let dds = bsa
        .files()
        .find(|f| f.name.decode().to_ascii_lowercase().ends_with(".dds"))
        .expect("archive contains textures");
    assert_eq!(&bsa.bytes(dds)[..4], b"DDS ");
}

/// Recompute every entry's name hash and resolve every name through `get` — same pin as
/// the Morrowind.bsa test, against the expansion's directory.
#[test]
fn computed_hashes_match_and_every_name_resolves() {
    let Some(bsa) = open_bsa() else { return };
    for f in bsa.files() {
        let name = f.name.decode();
        assert_eq!(
            tes3_bsa::name_hash(&name),
            f.hash,
            "computed hash should match the directory's for {name}"
        );
        let data = bsa
            .get(&name)
            .unwrap_or_else(|| panic!("{name} should resolve through the hash table"));
        assert!(
            std::ptr::eq(data, bsa.bytes(f)),
            "{name} should resolve to its own data"
        );
    }
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
        .map(|f| f.size as usize)
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
        for f in bsa.files() {
            acc = fold(acc, bsa.bytes(f));
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
