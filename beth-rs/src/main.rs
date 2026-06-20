//! Small CLI that loads a TES3 plugin and prints a summary.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use beth_rs::Plugin;
use clap::Parser;

/// Parse a TES3 (Morrowind) `.esm`/`.esp` file and print a summary.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to the plugin file to parse.
    #[arg(default_value = "beth-rs/tests/Morrowind.esm")]
    path: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let path = args.path.display();

    // Parsing is zero-copy, so the file bytes must outlive the parsed `Plugin`.
    let bytes = match std::fs::read(&args.path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("failed to read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let plugin = match Plugin::parse(&bytes) {
        Ok(plugin) => plugin,
        Err(e) => {
            eprintln!("failed to parse {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let h = &plugin.header;
    println!("File:        {path}");
    println!("Version:     {}", h.version);
    println!("Master flag: {}", h.flags & 0x1 != 0);
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
        let tag = String::from_utf8_lossy(&record.tag()).into_owned();
        *counts.entry(tag).or_default() += 1;
    }

    println!("\nRecords by type:");
    let mut by_count: Vec<_> = counts.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, count) in by_count {
        println!("  {tag}  {count}");
    }

    ExitCode::SUCCESS
}
