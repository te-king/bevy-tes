//! CLI for inspecting TES3 plugins (`.esm`/`.esp`): parses the file directly with
//! [`Plugin::parse`] and prints a summary (header, masters, records by type).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tes3_esm::Plugin;
use tes3_esm::records::tes3::HeaderFlags;

/// Parse a Morrowind (TES3) plugin and print a summary.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Path to the plugin file.
    #[arg(default_value = "data/Morrowind.esm")]
    path: PathBuf,
}

fn main() -> ExitCode {
    let Cli { path } = Cli::parse();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return fail(&path, &e),
    };
    let plugin = match Plugin::parse(&bytes) {
        Ok(p) => p,
        Err(e) => return fail(&path, &e),
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

fn fail(path: &std::path::Path, e: &dyn std::fmt::Display) -> ExitCode {
    eprintln!("{}: {e}", path.display());
    ExitCode::FAILURE
}
