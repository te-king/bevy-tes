//! Integration tests for [`bevy_beth::BethPlugin`].
//!
//! The wiring test always runs. The end-to-end test loads a real `Morrowind.esm`
//! through Bevy's `AssetServer`; it is skipped when the (gitignored, locally supplied)
//! game data is not present.

use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetPlugin, AssetServer, Assets, Handle, LoadState};
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};

use bevy_beth::{BethPlugin, BsaAsset, EsmAsset};

/// Asset loading runs on Bevy's task pools, which `DefaultPlugins` would set up. A
/// headless test must initialize them itself.
fn init_task_pools() {
    IoTaskPool::get_or_init(Default::default);
    AsyncComputeTaskPool::get_or_init(Default::default);
    ComputeTaskPool::get_or_init(Default::default);
}

/// The workspace `data/` directory holding the (gitignored) game data. Bevy resolves the
/// asset root relative to `CARGO_MANIFEST_DIR` (this crate) when run under cargo.
const ASSET_ROOT: &str = "../../data";

fn app_with_assets() -> App {
    init_task_pools();
    let mut app = App::new();
    app.add_plugins((
        AssetPlugin {
            file_path: ASSET_ROOT.to_string(),
            ..Default::default()
        },
        BethPlugin::default(),
    ));
    app
}

#[test]
fn plugin_registers_asset_types() {
    let app = app_with_assets();
    // `init_asset` inserts an `Assets<T>` resource for each registered asset type.
    assert!(app.world().get_resource::<Assets<EsmAsset>>().is_some());
    assert!(app.world().get_resource::<Assets<BsaAsset>>().is_some());
}

#[test]
fn loads_morrowind_esm_through_asset_server() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<EsmAsset> = app.world().resource::<AssetServer>().load("Morrowind.esm");

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
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<EsmAsset>>();
    let esm = assets.get(&handle).expect("asset present once loaded");
    assert_eq!(esm.0.header.version, 1.2);
    assert_eq!(esm.0.records.len(), 48_296);
}
