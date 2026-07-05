//! Integration tests for [`bevy_beth::BethPlugin`].
//!
//! The wiring test always runs. The end-to-end tests load a real `Morrowind.esm`
//! through Bevy's `AssetServer` — via the default source and via the `tes://` source —
//! and skip when the (gitignored, locally supplied) game data is not present.

use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetPlugin, AssetServer, Assets, Handle, LoadState};
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};

use bevy_beth::{BethPlugin, EsmAsset, NifAsset};

/// Asset loading runs on Bevy's task pools, which `DefaultPlugins` would set up. A
/// headless test must initialize them itself.
fn init_task_pools() {
    IoTaskPool::get_or_init(Default::default);
    AsyncComputeTaskPool::get_or_init(Default::default);
    ComputeTaskPool::get_or_init(Default::default);
}

/// The workspace `data/` directory holding the (gitignored) game data. Bevy resolves the
/// asset root relative to `CARGO_MANIFEST_DIR` (this crate) when run under cargo, and
/// `BethPlugin`'s VFS resolves against the process working directory — the same place
/// under cargo.
const DATA_ROOT: &str = "../../data";

fn app_with_assets() -> App {
    init_task_pools();
    let mut app = App::new();
    app.add_plugins((
        // BethPlugin must precede AssetPlugin: asset sources register before the server.
        BethPlugin::new(DATA_ROOT),
        AssetPlugin {
            file_path: DATA_ROOT.to_string(),
            ..Default::default()
        },
    ));
    // With the scene feature the NIF loader emits Mesh/material/scene sub-assets, whose
    // types a full app's render plugins would register. Stand in for them here.
    #[cfg(feature = "scene")]
    {
        use bevy::asset::AssetApp;
        use bevy::image::{CompressedImageFormats, Image, ImageLoader};
        app.init_asset::<Image>()
            .init_asset::<bevy::mesh::Mesh>()
            .init_asset::<bevy::pbr::StandardMaterial>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .register_asset_loader(ImageLoader::new(CompressedImageFormats::BC));
    }
    // Headless apps must finish plugin setup themselves; BethPlugin registers its
    // loaders in `Plugin::finish`.
    app.finish();
    app
}

/// Pump the app until `handle` finishes loading (or a generous timeout expires).
fn pump_until_loaded<A: bevy::asset::Asset>(app: &mut App, handle: &Handle<A>) -> LoadState {
    let mut state = LoadState::NotLoaded;
    for _ in 0..2000 {
        app.update();
        state = app.world().resource::<AssetServer>().load_state(handle);
        if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    state
}

#[test]
fn plugin_registers_asset_types() {
    let app = app_with_assets();
    // `init_asset` inserts an `Assets<T>` resource for each registered asset type.
    assert!(app.world().get_resource::<Assets<EsmAsset>>().is_some());
    assert!(app.world().get_resource::<Assets<NifAsset>>().is_some());
}

#[test]
fn loads_morrowind_esm_through_asset_server() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<EsmAsset> = app.world().resource::<AssetServer>().load("Morrowind.esm");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<EsmAsset>>();
    let esm = assets.get(&handle).expect("asset present once loaded");
    assert_eq!(esm.0.header.version, 1.2);
    assert_eq!(esm.0.records.len(), 48_296);
}

#[test]
fn loads_morrowind_esm_through_the_tes_source() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<EsmAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<EsmAsset>>();
    let esm = assets.get(&handle).expect("asset present once loaded");
    assert_eq!(esm.0.records.len(), 48_296);
}

#[test]
fn loads_an_archived_nif_through_the_tes_source() {
    if tes_testdata::fixture("Morrowind.bsa").is_none() {
        return;
    }

    let mut app = app_with_assets();
    // This mesh exists only inside Morrowind.bsa — the load proves the VFS layering, not
    // just loose-file passthrough.
    let handle: Handle<NifAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://meshes/m/probe_journeyman_01.nif");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<NifAsset>>();
    let nif = assets.get(&handle).expect("asset present once loaded");
    assert!(!nif.nif.instances().is_empty(), "probe has drawable shapes");
}
