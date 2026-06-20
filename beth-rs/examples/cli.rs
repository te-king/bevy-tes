//! CLI for inspecting TES3 plugins and BSA archives.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use beth_rs::{Bsa, Plugin};
use clap::{Parser, Subcommand};

/// Inspect Morrowind (TES3) data files.
#[derive(Parser, Debug)]
#[command(version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Parse a plugin (.esm/.esp) and print a summary.
    Esm {
        /// Path to the plugin file.
        #[arg(default_value = "beth-rs/tests/Morrowind.esm")]
        path: PathBuf,
    },
    /// Inspect or extract from a BSA archive.
    Bsa {
        #[command(subcommand)]
        command: BsaCommand,
    },
}

#[derive(Subcommand, Debug)]
enum BsaCommand {
    /// List the contents of a BSA archive.
    List {
        /// Path to the .bsa archive.
        archive: PathBuf,
        /// Show at most this many entries.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Extract a single file from a BSA archive (to stdout, or to --out).
    Extract {
        /// Path to the .bsa archive.
        archive: PathBuf,
        /// File path inside the archive (case-insensitive, '/' or '\').
        name: String,
        /// Write to this file instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Extract all files from a BSA archive into a directory.
    ExtractAll {
        /// Path to the .bsa archive.
        archive: PathBuf,
        /// Output directory (created if missing). Defaults to the current directory.
        #[arg(short, long, default_value = ".")]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Esm { path } => run_esm(&path),
        Command::Bsa { command } => match command {
            BsaCommand::List {
                archive: path,
                limit,
            } => run_bsa(&path, limit),
            BsaCommand::Extract { archive, name, out } => {
                run_extract(&archive, &name, out.as_deref())
            }
            BsaCommand::ExtractAll { archive, out } => run_extract_all(&archive, &out),
        },
    }
}

fn run_esm(path: &Path) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return fail(path, &e),
    };
    let plugin = match Plugin::parse(&bytes) {
        Ok(p) => p,
        Err(e) => return fail(path, &e),
    };

    let h = &plugin.header;
    println!("File:        {}", path.display());
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
        *counts
            .entry(String::from_utf8_lossy(&record.tag()).into_owned())
            .or_default() += 1;
    }
    println!("\nRecords by type:");
    let mut by_count: Vec<_> = counts.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (tag, count) in by_count {
        println!("  {tag}  {count}");
    }
    ExitCode::SUCCESS
}

fn run_bsa(path: &Path, limit: Option<usize>) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return fail(path, &e),
    };
    let bsa = match Bsa::parse(&bytes) {
        Ok(b) => b,
        Err(e) => return fail(path, &e),
    };

    let total: u64 = bsa.files.iter().map(|f| f.data.len() as u64).sum();
    println!(
        "Archive: {}  (version {:#x}, {} files, {} bytes of data)",
        path.display(),
        bsa.version,
        bsa.files.len(),
        total
    );

    let shown = limit.unwrap_or(bsa.files.len()).min(bsa.files.len());
    for f in &bsa.files[..shown] {
        println!("{:>10}  {}", f.data.len(), f.name);
    }
    if shown < bsa.files.len() {
        println!(
            "... {} more (use --limit to show more)",
            bsa.files.len() - shown
        );
    }
    ExitCode::SUCCESS
}

fn run_extract(archive: &Path, name: &str, out: Option<&Path>) -> ExitCode {
    let bytes = match std::fs::read(archive) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };
    let bsa = match Bsa::parse(&bytes) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };

    let Some(file) = bsa.get(name) else {
        eprintln!("{}: no such file in archive: {name}", archive.display());
        return ExitCode::FAILURE;
    };

    let result = match out {
        Some(path) => std::fs::write(path, file.data),
        None => std::io::stdout().write_all(file.data),
    };
    if let Err(e) = result {
        eprintln!("failed to write output: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = out {
        eprintln!("wrote {} bytes to {}", file.data.len(), path.display());
    }
    ExitCode::SUCCESS
}

fn run_extract_all(archive: &Path, out_dir: &Path) -> ExitCode {
    let bytes = match std::fs::read(archive) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };
    let bsa = match Bsa::parse(&bytes) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };

    let mut files = 0u64;
    let mut total = 0u64;
    for f in &bsa.files {
        let name = f.name.decode();
        let Some(rel) = safe_relative_path(&name) else {
            eprintln!("skipping unsafe entry name: {name}");
            continue;
        };
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return fail(parent, &e);
            }
        }
        if let Err(e) = std::fs::write(&dest, f.data) {
            return fail(&dest, &e);
        }
        files += 1;
        total += f.data.len() as u64;
    }

    eprintln!(
        "extracted {files} files ({total} bytes) to {}",
        out_dir.display()
    );
    ExitCode::SUCCESS
}

/// Turn a BSA-internal name into a safe relative path under the output directory.
///
/// BSA names use Windows `\` separators. Returns `None` for anything that could
/// escape the output directory (absolute paths, `..`, or empty after sanitizing).
fn safe_relative_path(name: &str) -> Option<PathBuf> {
    use std::path::Component;

    let mut path = PathBuf::new();
    for segment in name.replace('\\', "/").split('/') {
        match Path::new(segment).components().next() {
            // Drop empty segments (leading/trailing/double separators) and `.`.
            None | Some(Component::CurDir) => {}
            Some(Component::Normal(part)) => path.push(part),
            // Anything else (`..`, root, prefix) could escape the target.
            _ => return None,
        }
    }
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

fn fail(path: &Path, e: &dyn std::fmt::Display) -> ExitCode {
    eprintln!("{}: {e}", path.display());
    ExitCode::FAILURE
}
