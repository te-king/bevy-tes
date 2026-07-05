//! Integration tests for [`bevy_beth::BethPlugin`].
//!
//! The wiring test always runs. The end-to-end tests load a real `Morrowind.esm`
//! through Bevy's `AssetServer` — via the default source and via the `tes://` source —
//! and skip when the (gitignored, locally supplied) game data is not present.

use bevy::asset::{AssetServer, Assets, Handle, LoadState};

use bevy_beth::{EsmAsset, NifAsset};

mod common;
use common::{app_with_assets, pump_until_loaded};

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
