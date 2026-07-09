//! Scan every `.nif` in the workspace game data — all `*.bsa` archives plus loose
//! `meshes/` files — and report parse coverage: how many models parse, and the failure
//! tally grouped by cause (unsupported block type, or a decode error suggesting a wrong
//! block layout).
//!
//! ```text
//! cargo run -p tes-nif --example scan
//! ```
//!
//! This is the ground-truth harness for extending block coverage: NIF 4.0.0.2 has no
//! block-size table, so a mis-sized parser desyncs the stream and fails loudly here.

use std::collections::BTreeMap;

use tes_nif::{Nif, NifError};
use tes3_bsa::Bsa;

#[derive(Default)]
struct Tally {
    total: usize,
    ok: usize,
    with_geometry: usize,
    /// failure cause → (count, first failing file)
    failures: BTreeMap<String, (usize, String)>,
}

impl Tally {
    fn record(&mut self, name: &str, bytes: &[u8]) {
        self.total += 1;
        match Nif::parse(bytes) {
            Ok(nif) => {
                self.ok += 1;
                if !nif.instances().is_empty() {
                    self.with_geometry += 1;
                }
            }
            Err(e) => {
                let cause = failure_cause(&e);
                let entry = self
                    .failures
                    .entry(cause)
                    .or_insert_with(|| (0, name.to_string()));
                entry.0 += 1;
            }
        }
    }
}

/// Group failures by the unsupported block type when there is one, else by the raw
/// parse-error message (those indicate a wrong layout in a *supported* block).
fn failure_cause(e: &NifError) -> String {
    let msg = e.to_string();
    match msg.split("unsupported block type \"").nth(1) {
        Some(rest) => rest.split('"').next().unwrap_or(rest).to_string(),
        None => msg,
    }
}

fn main() {
    let data = tes_testdata::data_dir();
    let mut tally = Tally::default();

    // Every archive in the data dir.
    let mut archives: Vec<_> = std::fs::read_dir(&data)
        .expect("read data dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bsa")))
        .collect();
    archives.sort();
    for path in &archives {
        let bsa = Bsa::open(path).expect("open bsa");
        let archive = path.file_name().unwrap().to_string_lossy();
        for (name, data) in bsa.files() {
            let name = name.decode();
            if name.to_ascii_lowercase().ends_with(".nif") {
                tally.record(&format!("{archive}:{name}"), data);
            }
        }
    }

    // Loose meshes.
    let mut stack = vec![data.join("meshes")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("nif"))
            {
                let bytes = std::fs::read(&path).expect("read loose nif");
                tally.record(&path.display().to_string(), &bytes);
            }
        }
    }

    println!(
        "scanned {} files: {} parsed ({} with geometry), {} failed",
        tally.total,
        tally.ok,
        tally.with_geometry,
        tally.total - tally.ok
    );
    let mut failures: Vec<_> = tally.failures.into_iter().collect();
    failures.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for (cause, (count, first)) in failures {
        println!("{count:5}  {cause}");
        println!("       e.g. {first}");
    }
}
