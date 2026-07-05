//! End-to-end test of the scene-emitting NIF load path (`scene` feature), fully
//! headless: no renderer, no window — just the asset machinery.
//!
//! Skips when the (gitignored, locally supplied) game data is absent.
#![cfg(feature = "scene")]

use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetApp, AssetPlugin, AssetServer, Assets, Handle, LoadState};
use bevy::image::{CompressedImageFormats, Image, ImageAddressMode, ImageLoader, ImageSampler};
use bevy::mesh::Mesh;
use bevy::pbr::StandardMaterial;
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};
use bevy::world_serialization::WorldAsset;

use bevy_beth::{BethPlugin, NifAsset};

const DATA_ROOT: &str = "../../data";

/// A headless app with just enough asset machinery for scene-emitting NIF loads: the
/// plugins register the `tes://` source and the NIF loader; the manual registrations
/// stand in for the render plugins that would normally own `Image`/`Mesh`/material
/// assets.
fn headless_scene_app() -> App {
    IoTaskPool::get_or_init(Default::default);
    AsyncComputeTaskPool::get_or_init(Default::default);
    ComputeTaskPool::get_or_init(Default::default);

    let mut app = App::new();
    app.add_plugins((
        BethPlugin::new(DATA_ROOT),
        AssetPlugin {
            file_path: DATA_ROOT.to_string(),
            ..Default::default()
        },
    ));
    app.init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<WorldAsset>()
        .register_asset_loader(ImageLoader::new(CompressedImageFormats::BC));
    app.finish();
    app
}

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
fn nif_load_emits_scene_meshes_and_textured_materials() {
    if tes_testdata::fixture("meshes/i/In_De_Shack_01.nif").is_none() {
        return;
    }

    let mut app = headless_scene_app();
    let handle: Handle<NifAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://meshes/i/in_de_shack_01.nif");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    // The primary asset carries the labeled sub-asset handles.
    let (scene, mesh_handles, material_handles) = {
        let assets = app.world().resource::<Assets<NifAsset>>();
        let nif = assets.get(&handle).expect("asset present once loaded");
        assert!(nif.meshes.len() > 1, "the shack has several parts");
        assert_eq!(
            nif.meshes.len(),
            nif.nif.instances().len(),
            "one mesh per drawable shape"
        );
        assert!(!nif.materials.is_empty());
        (nif.scene.clone(), nif.meshes.clone(), nif.materials.clone())
    };

    // The labeled sub-assets resolve to real assets.
    let world = app.world();
    assert!(
        world.resource::<Assets<WorldAsset>>().get(&scene).is_some(),
        "scene world present"
    );
    let meshes = world.resource::<Assets<Mesh>>();
    for mesh in &mesh_handles {
        assert!(meshes.get(mesh).is_some(), "labeled mesh present");
    }

    // At least one material is textured, and its texture loads with Repeat addressing.
    let texture = {
        let materials = world.resource::<Assets<StandardMaterial>>();
        material_handles
            .iter()
            .filter_map(|m| materials.get(m))
            .find_map(|m| m.base_color_texture.clone())
            .expect("the shack has textured materials")
    };
    let state = pump_until_loaded(&mut app, &texture);
    assert!(
        matches!(state, LoadState::Loaded),
        "texture load state: {state:?}"
    );
    let images = app.world().resource::<Assets<Image>>();
    let image = images.get(&texture).expect("texture image present");
    let ImageSampler::Descriptor(desc) = &image.sampler else {
        panic!(
            "expected a custom sampler descriptor, got {:?}",
            image.sampler
        );
    };
    assert_eq!(desc.address_mode_u, ImageAddressMode::Repeat);
    assert_eq!(desc.address_mode_v, ImageAddressMode::Repeat);
}

#[test]
fn scene_labels_are_addressable_directly() {
    if tes_testdata::fixture("meshes/i/In_De_Shack_01.nif").is_none() {
        return;
    }

    let mut app = headless_scene_app();
    // Loading the labeled sub-asset directly — as WorldAssetRoot spawning would.
    let scene: Handle<WorldAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://meshes/i/in_de_shack_01.nif#Scene");

    let state = pump_until_loaded(&mut app, &scene);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );
    let worlds = app.world().resource::<Assets<WorldAsset>>();
    assert!(worlds.get(&scene).is_some());
}
