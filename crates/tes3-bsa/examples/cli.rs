//! CLI for inspecting and extracting from TES3 BSA archives.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tes3_bsa::Bsa;

/// Inspect Morrowind (TES3) BSA archives.
#[derive(Parser, Debug)]
#[command(version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
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
        Command::List { archive, limit } => run_list(&archive, limit),
        Command::Extract { archive, name, out } => run_extract(&archive, &name, out.as_deref()),
        Command::ExtractAll { archive, out } => run_extract_all(&archive, &out),
    }
}

fn run_list(path: &Path, limit: Option<usize>) -> ExitCode {
    let bsa = match Bsa::open(path) {
        Ok(b) => b,
        Err(e) => return fail(path, &e),
    };

    let total: u64 = bsa.files().map(|f| f.size as u64).sum();
    println!(
        "Archive: {}  (version {:#x}, {} files, {} bytes of data)",
        path.display(),
        bsa.version(),
        bsa.len(),
        total
    );

    let shown = limit.unwrap_or(bsa.len()).min(bsa.len());
    for f in bsa.files().take(shown) {
        println!("{:>10}  {}", f.size, f.name);
    }
    if shown < bsa.len() {
        println!("... {} more (use --limit to show more)", bsa.len() - shown);
    }
    ExitCode::SUCCESS
}

fn run_extract(archive: &Path, name: &str, out: Option<&Path>) -> ExitCode {
    let bsa = match Bsa::open(archive) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };

    let Some(data) = bsa.get(name) else {
        eprintln!("{}: no such file in archive: {name}", archive.display());
        return ExitCode::FAILURE;
    };

    let result = match out {
        Some(path) => std::fs::write(path, data),
        None => std::io::stdout().write_all(data),
    };
    if let Err(e) = result {
        eprintln!("failed to write output: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = out {
        eprintln!("wrote {} bytes to {}", data.len(), path.display());
    }
    ExitCode::SUCCESS
}

fn run_extract_all(archive: &Path, out_dir: &Path) -> ExitCode {
    let bsa = match Bsa::open(archive) {
        Ok(b) => b,
        Err(e) => return fail(archive, &e),
    };

    let mut files = 0u64;
    let mut total = 0u64;
    for f in bsa.files() {
        let name = f.name.decode();
        let Some(rel) = safe_relative_path(&name) else {
            eprintln!("skipping unsafe entry name: {name}");
            continue;
        };
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return fail(parent, &e);
        }
        let data = bsa.bytes(f);
        if let Err(e) = std::fs::write(&dest, data) {
            return fail(&dest, &e);
        }
        files += 1;
        total += data.len() as u64;
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
