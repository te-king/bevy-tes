//! Integration tests for [`bevy_beth::BethPlugin`].
//!
//! The wiring test always runs. The end-to-end tests load a real `Morrowind.esm`
//! through Bevy's `AssetServer` — via the default source and via the `tes://` source —
//! and skip when the (gitignored, locally supplied) game data is not present.

use bevy::asset::{AssetServer, Assets, Handle, LoadState};

use bevy_beth::{BethPlugin, CellId, LoadOrderAsset, LoadOrderHandle, NifAsset};

mod common;
use common::{DATA_ROOT, app_with, app_with_assets, pump_until_loaded};

#[test]
fn plugin_registers_asset_types() {
    let app = app_with_assets();
    // `init_asset` inserts an `Assets<T>` resource for each registered asset type.
    assert!(
        app.world()
            .get_resource::<Assets<LoadOrderAsset>>()
            .is_some()
    );
    assert!(app.world().get_resource::<Assets<NifAsset>>().is_some());
    // No plugin list, no load-order handle.
    assert!(app.world().get_resource::<LoadOrderHandle>().is_none());
}

#[test]
fn loads_morrowind_esm_through_asset_server() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<LoadOrderAsset> =
        app.world().resource::<AssetServer>().load("Morrowind.esm");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<LoadOrderAsset>>();
    let asset = assets.get(&handle).expect("asset present once loaded");
    let directory = asset.load_order().esms()[0].directory();
    assert_eq!(directory.header.version, 1.2);
    assert_eq!(directory.records.len(), 48_296);
}

#[test]
fn loads_morrowind_esm_through_the_tes_source() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<LoadOrderAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<LoadOrderAsset>>();
    let asset = assets.get(&handle).expect("asset present once loaded");
    let directory = asset.load_order().esms()[0].directory();
    assert_eq!(directory.records.len(), 48_296);
}

#[test]
fn builds_load_order_from_plugin_list() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with(BethPlugin::new(DATA_ROOT).with_plugins(["Morrowind.esm"]));
    let handle = app.world().resource::<LoadOrderHandle>().0.clone();

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<LoadOrderAsset>>();
    let asset = assets.get(&handle).expect("asset present once loaded");
    assert_eq!(asset.load_order().esms().len(), 1);
    assert!(
        asset
            .cell(&CellId::interior("Balmora, Guild of Mages"))
            .is_some()
    );
}

#[test]
fn merges_a_multi_plugin_load_order() {
    let masters = ["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"];
    if masters.iter().any(|m| tes_testdata::fixture(m).is_none()) {
        return;
    }

    let mut app = app_with(BethPlugin::new(DATA_ROOT).with_plugins(masters));
    let handle = app.world().resource::<LoadOrderHandle>().0.clone();

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let assets = app.world().resource::<Assets<LoadOrderAsset>>();
    let asset = assets.get(&handle).expect("asset present once loaded");
    assert_eq!(asset.load_order().esms().len(), 3);
    // One lookup per master: a cell only the expansion defines proves its records
    // merged into the shared tables.
    assert!(
        asset
            .cell(&CellId::interior("Balmora, Guild of Mages"))
            .is_some(),
        "Morrowind cell resolves"
    );
    assert!(
        asset
            .cell(&CellId::interior("Mournhold, Royal Palace Throne Room"))
            .is_some(),
        "Tribunal cell resolves"
    );
    assert!(
        asset
            .cell(&CellId::interior("Skaal Village, The Greathall"))
            .is_some(),
        "Bloodmoon cell resolves"
    );
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
