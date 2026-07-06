//! End-to-end test of the scene-emitting NIF load path (`scene` feature), fully
//! headless: no renderer, no window — just the asset machinery.
//!
//! Skips when the (gitignored, locally supplied) game data is absent.
#![cfg(feature = "scene")]

use bevy::asset::{AssetServer, Assets, Handle, LoadState};
use bevy::image::{Image, ImageAddressMode, ImageSampler};
use bevy::mesh::Mesh;
use bevy::pbr::StandardMaterial;
use bevy::world_serialization::WorldAsset;

use bevy_beth::NifAsset;

mod common;
use common::{app_with_assets, pump_until_loaded};

#[test]
fn nif_load_emits_scene_meshes_and_textured_materials() {
    if tes_testdata::fixture("meshes/i/In_De_Shack_01.nif").is_none() {
        return;
    }

    let mut app = app_with_assets();
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
fn foliage_gets_alpha_masked_materials() {
    // Bloodmoon's holly: leaf cards whose texture alpha marks the leaf shape. Its
    // NiAlphaProperty (alpha test, GREATER) must come through as an alpha-masked
    // material — this is what keeps the cards' backgrounds from rendering opaque.
    if tes_testdata::fixture("Bloodmoon.bsa").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<NifAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://meshes/o/flora_bm_holly_06.nif");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let material_handles = {
        let assets = app.world().resource::<Assets<NifAsset>>();
        assets
            .get(&handle)
            .expect("asset present once loaded")
            .materials
            .clone()
    };
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    let masked = material_handles
        .iter()
        .filter_map(|m| materials.get(m))
        .filter(|m| matches!(m.alpha_mode, bevy::material::AlphaMode::Mask(_)))
        .count();
    assert!(
        masked > 0,
        "the holly's leaf materials should be alpha-masked"
    );
}

#[test]
fn glow_maps_become_emissive_textures() {
    // Bloodmoon's ice troll carries a glow map (slot 4, tx_ice_troll03.dds) over its
    // body. It must come through as the material's emissive texture with a white
    // emissive factor (Bevy multiplies the two; the map alone drives the glow), and the
    // texture itself must resolve through the VFS and load.
    if tes_testdata::fixture("Bloodmoon.bsa").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<NifAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://meshes/r/ice troll.nif");

    let state = pump_until_loaded(&mut app, &handle);
    assert!(
        matches!(state, LoadState::Loaded),
        "unexpected load state: {state:?}"
    );

    let material_handles = {
        let assets = app.world().resource::<Assets<NifAsset>>();
        assets
            .get(&handle)
            .expect("asset present once loaded")
            .materials
            .clone()
    };
    let glow_texture = {
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let glowing: Vec<_> = material_handles
            .iter()
            .filter_map(|m| materials.get(m))
            .filter(|m| m.emissive_texture.is_some())
            .collect();
        assert!(!glowing.is_empty(), "the troll has a glow-mapped material");
        for material in &glowing {
            assert_eq!(
                material.emissive,
                bevy::color::LinearRgba::WHITE,
                "a glow map needs a white emissive factor to show"
            );
        }
        glowing[0].emissive_texture.clone().unwrap()
    };
    let state = pump_until_loaded(&mut app, &glow_texture);
    assert!(
        matches!(state, LoadState::Loaded),
        "glow texture load state: {state:?}"
    );
}

#[test]
fn scene_labels_are_addressable_directly() {
    if tes_testdata::fixture("meshes/i/In_De_Shack_01.nif").is_none() {
        return;
    }

    let mut app = app_with_assets();
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
