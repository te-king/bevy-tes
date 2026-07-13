//! Parse a plugin (.esm/.esp) and print a summary of its header and record counts.
//!
//! ```text
//! cargo run -p tes3-esm --example inspect -- data/Morrowind.esm
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use tes3_esm::Esm;
use tes3_esm::records::tes3::HeaderFlags;

/// Inspect a Morrowind (TES3) plugin file.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Path to the plugin file.
    #[arg(default_value = "data/Morrowind.esm")]
    path: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    run_esm(&cli.path)
}

fn run_esm(path: &Path) -> ExitCode {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return fail(path, &e),
    };
    // SAFETY: We open the file read-only and never write through the mapping.
    // Concurrent modification of game data files by another process is not expected.
    let mmap = match unsafe { memmap2::Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => return fail(path, &e),
    };
    let plugin = match Esm::parse(&mmap) {
        Ok(p) => p,
        Err(e) => return fail(path, &e),
    };

    let h = &plugin.header;
    println!("File:        {}", path.display());
    println!("Version:     {}", h.version);
    println!("Master flag: {}", h.flags.contains(HeaderFlags::MASTER));
    println!("Company:     {}", h.company);
    println!("Description: {}", h.description.decode().trim());
    println!("Declared records: {}", h.num_records);
    if !h.masters.is_empty() {
        println!("Masters:");
        for m in &h.masters {
            println!("  - {} ({} bytes)", m.name, m.size);
        }
    }

    println!("\nParsed records: {}", plugin.records.len());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &plugin.records {
        *counts.entry(record.tag().to_string()).or_default() += 1;
    }
    println!("\nRecords by type:");
    let mut by_count: Vec<_> = counts.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, count) in by_count {
        println!("  {tag}  {count}");
    }
    ExitCode::SUCCESS
}

fn fail(path: &Path, e: &dyn std::fmt::Display) -> ExitCode {
    eprintln!("{}: {e}", path.display());
    ExitCode::FAILURE
}
