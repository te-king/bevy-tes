//! Load a TES3 plugin (`.esm`/`.esp`) through Bevy's `AssetServer` via [`TesPlugin`]
//! and print a summary of what was parsed.
//!
//! Run with the local game master (default, see `data/README.md`):
//!
//! ```text
//! cargo run --example load_esm
//! ```
//!
//! …or point it at another file:
//!
//! ```text
//! cargo run --example load_esm -- path/to/Plugin.esp
//! ```
//!
//! This is a headless example: it adds only `AssetPlugin` + `TesPlugin` and pumps the
//! app manually until the asset finishes loading, so it needs none of Bevy's rendering
//! or windowing features.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetPlugin, AssetServer, Assets, Handle, LoadState};
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};
use clap::Parser;

use bevy_tes::{LoadOrderAsset, TesPlugin};
use tes3_esm::records::tes3::HeaderFlags;

/// Load a TES3 plugin (`.esm`/`.esp`) through Bevy's `AssetServer` and print a summary.
#[derive(Parser, Debug)]
struct Args {
    /// Path to the plugin file to load.
    #[arg(default_value = "data/Morrowind.esm")]
    path: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let path = &args.path;

    if !path.exists() {
        eprintln!("file not found: {}", path.display());
        return ExitCode::FAILURE;
    }
    // Resolve to an absolute path first: Bevy resolves a *relative* asset root against
    // `CARGO_MANIFEST_DIR`, not the process working directory, so a relative root would
    // break depending on where `cargo run` is invoked. An absolute root sidesteps that.
    let abs = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot resolve {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };

    // The AssetServer loads paths relative to an asset root, so split into a root
    // directory (the asset source) and the file name to load from it.
    let Some(file_name) = abs.file_name().and_then(|n| n.to_str()) else {
        eprintln!("invalid path: {}", path.display());
        return ExitCode::FAILURE;
    };
    let root = abs
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string());

    // DefaultPlugins would set these up; a headless app must initialize them itself.
    IoTaskPool::get_or_init(Default::default);
    AsyncComputeTaskPool::get_or_init(Default::default);
    ComputeTaskPool::get_or_init(Default::default);

    let mut app = App::new();
    // TesPlugin first: it must register its asset source before AssetPlugin builds the
    // AssetServer (it asserts this).
    app.add_plugins((
        TesPlugin::default(),
        AssetPlugin {
            file_path: root,
            ..Default::default()
        },
    ));
    // Headless apps must finish() themselves for loader registration to run.
    app.finish();

    let handle: Handle<LoadOrderAsset> = app
        .world()
        .resource::<AssetServer>()
        .load(file_name.to_string());

    // Pump the schedule until the background load completes (or we give up).
    let mut state = LoadState::NotLoaded;
    for _ in 0..2000 {
        app.update();
        state = app.world().resource::<AssetServer>().load_state(&handle);
        if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    if let LoadState::Failed(err) = &state {
        eprintln!("failed to load {}: {err}", path.display());
        return ExitCode::FAILURE;
    }
    if !matches!(state, LoadState::Loaded) {
        eprintln!(
            "timed out loading {} (last state: {state:?})",
            path.display()
        );
        return ExitCode::FAILURE;
    }

    let assets = app.world().resource::<Assets<LoadOrderAsset>>();
    let plugin = assets.get(&handle).expect("asset present once loaded");
    print_summary(&path.display().to_string(), plugin);
    ExitCode::SUCCESS
}

fn print_summary(path: &str, asset: &LoadOrderAsset) {
    let directory = asset.load_order().esms()[0].directory();
    let h = &directory.header;
    println!("Loaded {path} via Bevy AssetServer");
    println!("  version:          {}", h.version);
    println!(
        "  master flag:      {}",
        h.flags.contains(HeaderFlags::MASTER)
    );
    println!("  company:          {}", h.company);
    println!("  declared records: {}", h.num_records);
    if !h.masters.is_empty() {
        println!("  masters:");
        for m in &h.masters {
            println!("    - {} ({} bytes)", m.name, m.size);
        }
    }

    println!("  parsed records:   {}", directory.records.len());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &directory.records {
        *counts.entry(record.tag().to_string()).or_default() += 1;
    }
    let mut by_count: Vec<_> = counts.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("  top record types:");
    for (tag, count) in by_count.into_iter().take(10) {
        println!("    {tag}  {count}");
    }
}
