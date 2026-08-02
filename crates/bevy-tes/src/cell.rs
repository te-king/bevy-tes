//! Spawning cells (interiors and exterior grid squares) from a loaded load order.
//!
//! Spawn an entity with a [`CellSeed`] naming a cell; once the [`LoadOrderAsset`] is
//! loaded, [`spawn_cells`] resolves the cell record and spawns one child entity per
//! object reference — each with the reference's placement as a Y-up [`Transform`] in
//! meters (see [`convert::METERS_PER_UNIT`](crate::convert::METERS_PER_UNIT)) and,
//! when its object's model resolves in the VFS, a [`WorldAssetRoot`] pointing at the
//! NIF's `#Scene` sub-asset. Only the NIFs a spawned cell actually references get
//! loaded.
//!
//! ```ignore
//! commands.spawn(CellSeed {
//!     load_order: asset_server.load("tes://Morrowind.esm"),
//!     cell: CellId::interior("Balmora, Guild of Mages"),
//! });
//! ```
//!
//! The work splits in two: the `cell_build` module turns the cell record into an owned,
//! ECS-free plan (all the CPU work — reference resolution, terrain mesh building,
//! texture path resolution), and this module applies the plan — spawning entities and
//! starting the asset loads it names.
//!
//! Exterior cells also grow a terrain child tagged [`CellTerrain`] — a mesh built from
//! the cell's `LAND` record (65×65 vertex heights, normals and colors) — plus a sea-level
//! water plane when the terrain dips below height 0. When
//! [`TerrainPlugin`](crate::TerrainPlugin) is added, the terrain is texture-splatted
//! from the `LAND`'s `VTEX` grid (see [`terrain`](crate::terrain)); otherwise it stays
//! vertex-tinted white.
//!
//! What is *not* spawned (counted in [`CellSpawned::skipped`], logged at debug level):
//! NPCs and creatures (their NIFs are skinned, which the scene builder doesn't support
//! yet), leveled-creature/item spawn points (they need runtime list resolution),
//! references flagged disabled, and `moved_references` (correct handling needs
//! multi-plugin merging — in a single vanilla ESM they don't occur). Lights spawn a
//! [`PointLight`](bevy::light::PointLight) (plus their model, when they have one);
//! interiors with water get a translucent stand-in plane tagged [`CellWater`].
//! Ambient/fog values are surfaced on the seed as [`CellEnvironment`] for the app to
//! apply — Bevy's ambient light is per-camera, so the library doesn't force it.

use std::collections::HashSet;

use bevy::asset::{AssetServer, Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::name::Name;
use bevy::ecs::query::Without;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy::image::{
    Image, ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::material::AlphaMode;
use bevy::math::Vec2;
use bevy::math::primitives::Plane3d;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::render::renderer::RenderDevice;
use bevy::transform::components::Transform;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::cell_build::{CellPlan, SplatPlan, build_cell};
use crate::terrain::{self, TerrainSplatMaterial};
use crate::tes_loadorder::CellId;
use crate::{LoadOrderAsset, TesVfsHandle};

/// Asks for a cell's contents to be spawned as children of this entity, once
/// `load_order` finishes loading. One-shot: the seed entity is tagged [`CellSpawned`]
/// (or [`CellSpawnFailed`]) afterwards. See the [module docs](self).
#[derive(Component, Debug, Clone)]
#[require(Transform, Visibility)]
pub struct CellSeed {
    /// The load order to read the cell from.
    pub load_order: bevy::asset::Handle<LoadOrderAsset>,
    /// Which cell to spawn.
    pub cell: CellId,
}

/// Inserted on the seed entity once its children have been spawned.
#[derive(Component, Debug)]
pub struct CellSpawned {
    /// Reference children spawned (including model-less stand-ins).
    pub spawned: usize,
    /// References skipped (NPCs/creatures, leveled lists, disabled, unknown ids).
    pub skipped: usize,
}

/// Inserted on the seed entity instead of [`CellSpawned`] when the load order failed to
/// load or the cell doesn't exist in it.
#[derive(Component, Debug)]
pub struct CellSpawnFailed(pub String);

/// On every spawned reference child: which cell reference it came from.
#[derive(Component, Debug, Clone)]
pub struct CellReference {
    /// The reference's `FRMR` id.
    pub id: u32,
    /// The object's editor id, as authored.
    pub object: String,
}

/// Marker on the stand-in water plane spawned for cells with water (interior water at
/// its authored height, exterior sea level at 0); despawn or replace it for real water
/// rendering.
#[derive(Component, Debug)]
pub struct CellWater;

/// Marker on the terrain mesh child spawned for an exterior cell with `LAND` data.
#[derive(Component, Debug)]
pub struct CellTerrain;

/// The cell's staging values, converted to Bevy colors and inserted on the seed entity.
/// The library doesn't apply them — ambient light is per-camera in Bevy — so the app
/// decides (e.g. set the camera's `AmbientLight` from `ambient` for interiors).
#[derive(Component, Debug, Clone, Default)]
pub struct CellEnvironment {
    /// Whether this is an interior cell.
    pub interior: bool,
    /// Interior ambient colour (`AMBI`).
    pub ambient: Option<Color>,
    /// Interior directional "sunlight" colour (`AMBI`).
    pub sunlight: Option<Color>,
    /// Interior fog colour and density (`AMBI`).
    pub fog: Option<(Color, f32)>,
    /// Water surface height in meters — the Bevy Y coordinate of the water plane.
    pub water_height: Option<f32>,
}

/// Seeds that still need spawning: not yet done, not yet failed.
type PendingSeeds<'w, 's> =
    Query<'w, 's, (Entity, &'static CellSeed), (Without<CellSpawned>, Without<CellSpawnFailed>)>;

/// Resolves pending [`CellSeed`]s and spawns their cells. Registered by `TesPlugin`
/// under the `scene` feature; polls until each seed's load order loads, then builds the
/// cell's plan (`cell_build`) and applies it once.
///
/// Terrain is texture-splatted when `Assets<TerrainSplatMaterial>` exists (i.e.
/// [`TerrainPlugin`](crate::TerrainPlugin) — or a test harness — registered it) and the
/// render device, if any, supports binding arrays; otherwise terrain keeps the plain
/// vertex-tinted white material.
#[allow(clippy::too_many_arguments)]
pub fn spawn_cells(
    mut commands: Commands,
    seeds: PendingSeeds,
    load_orders: Res<Assets<LoadOrderAsset>>,
    asset_server: Res<AssetServer>,
    vfs: Res<TesVfsHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut splat_materials: Option<ResMut<Assets<TerrainSplatMaterial>>>,
    render_device: Option<Res<RenderDevice>>,
    mut warned: Local<HashSet<String>>,
    mut terrain_material: Local<Option<Handle<StandardMaterial>>>,
    mut missing_layer: Local<Option<Handle<Image>>>,
) {
    // Headless apps have no render device — proceed (nothing renders, tests assert on
    // the material); a device without binding arrays falls back to the white material.
    let splat_supported = render_device
        .as_deref()
        .is_none_or(terrain::splat_supported);
    for (seed_entity, seed) in &seeds {
        let Some(load_order) = load_orders.get(&seed.load_order) else {
            if let bevy::asset::LoadState::Failed(e) = asset_server.load_state(&seed.load_order) {
                eprintln!(
                    "bevy-tes: load order failed to load for {:?}: {e}",
                    seed.cell
                );
                commands
                    .entity(seed_entity)
                    .insert(CellSpawnFailed(format!("load order failed to load: {e}")));
            }
            continue; // still loading; try again next frame
        };
        let plan = match build_cell(load_order.load_order(), &vfs.0, &seed.cell) {
            Ok(plan) => plan,
            Err(reason) => {
                eprintln!("bevy-tes: {reason} (for {:?})", seed.cell);
                commands.entity(seed_entity).insert(CellSpawnFailed(reason));
                continue;
            }
        };
        apply_plan(
            &mut commands,
            seed_entity,
            &seed.cell,
            plan,
            &asset_server,
            &mut meshes,
            &mut materials,
            &mut images,
            splat_materials.as_deref_mut().filter(|_| splat_supported),
            &mut warned,
            &mut terrain_material,
            &mut missing_layer,
        );
    }
}

/// Apply a built [`CellPlan`] under the seed entity: spawn the planned children and
/// start the asset loads the plan's paths name. The main-thread half of cell spawning —
/// everything here needs `Commands`, the `AssetServer` or an `Assets` collection.
#[allow(clippy::too_many_arguments)]
fn apply_plan(
    commands: &mut Commands,
    seed_entity: Entity,
    cell: &CellId,
    plan: CellPlan,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    splat_materials: Option<&mut Assets<TerrainSplatMaterial>>,
    warned: &mut HashSet<String>,
    terrain_material: &mut Option<Handle<StandardMaterial>>,
    missing_layer: &mut Option<Handle<Image>>,
) {
    // The plan's warnings deduplicate across cells and frames by key, so a model or
    // texture missing from many cells warns once per app run.
    for warning in &plan.warnings {
        if warned.insert(warning.key.clone()) {
            eprintln!("bevy-tes: {}", warning.message);
        }
    }
    if plan.moved_references > 0 {
        // MVRF relocates references defined by another plugin; meaningless without
        // multi-plugin merging (future work) and absent from single vanilla ESMs.
        eprintln!(
            "bevy-tes: skipping {} moved references in {cell:?}",
            plan.moved_references
        );
    }

    let (spawned, skipped) = (plan.references.len(), plan.skipped);
    for reference in plan.references {
        let mut child = commands.spawn((
            reference.transform,
            Visibility::default(),
            Name::new(reference.object.clone()),
            CellReference {
                id: reference.id,
                object: reference.object,
            },
            ChildOf(seed_entity),
        ));
        if let Some(path) = reference.model_path {
            child.insert(WorldAssetRoot(
                asset_server.load::<WorldAsset>(format!("tes://{path}#Scene")),
            ));
        }
        if let Some(light) = reference.light {
            child.insert(light);
        }
    }

    if let Some(terrain_plan) = plan.terrain {
        let mut terrain = commands.spawn((
            Mesh3d(meshes.add(terrain_plan.mesh)),
            terrain_plan.transform,
            Visibility::default(),
            Name::new(terrain_plan.name),
            CellTerrain,
            ChildOf(seed_entity),
        ));
        let splat = splat_materials.and_then(|splats| {
            let splat = terrain_plan.splat?;
            Some(splats.add(splat_material(splat, asset_server, images, missing_layer)))
        });
        match splat {
            Some(material) => {
                terrain.insert(MeshMaterial3d(material));
            }
            None => {
                // All cells share one matte white material; the LAND vertex colors carry
                // the tint.
                let material = terrain_material
                    .get_or_insert_with(|| {
                        materials.add(StandardMaterial {
                            base_color: Color::WHITE,
                            perceptual_roughness: 1.0,
                            ..Default::default()
                        })
                    })
                    .clone();
                terrain.insert(MeshMaterial3d(material));
            }
        }
    }

    if let Some(water) = plan.water {
        commands.spawn((
            Mesh3d(meshes.add(Mesh::from(Plane3d::new(
                bevy::math::Vec3::Y,
                Vec2::splat(water.half_size),
            )))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.1, 0.3, 0.5, 0.6),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..Default::default()
            })),
            Transform::from_translation(water.center),
            Visibility::default(),
            Name::new("Water"),
            CellWater,
            ChildOf(seed_entity),
        ));
    }

    commands
        .entity(seed_entity)
        .insert((plan.environment, CellSpawned { spawned, skipped }));
}

/// Turn a [`SplatPlan`] into a [`TerrainSplatMaterial`] by starting the layer texture
/// loads; unresolvable layers bind the shared white stand-in. Load settings mirror the
/// NIF loader's texture loads (sRGB, repeat) so a texture shared between terrain and
/// models isn't requested with conflicting settings.
fn splat_material(
    splat: SplatPlan,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    missing_layer: &mut Option<Handle<Image>>,
) -> TerrainSplatMaterial {
    let layers = splat
        .layers
        .into_iter()
        .map(|layer| match layer {
            Some(path) => asset_server
                .load_builder()
                .with_settings(|s: &mut ImageLoaderSettings| {
                    s.is_srgb = true;
                    let mut sampler = ImageSamplerDescriptor::default();
                    sampler.set_address_mode(ImageAddressMode::Repeat);
                    s.sampler = ImageSampler::Descriptor(sampler);
                })
                .load(format!("tes://{path}")),
            // The shared 1×1 white stand-in ([`Image::default`] is all-white).
            None => missing_layer
                .get_or_insert_with(|| images.add(Image::default()))
                .clone(),
        })
        .collect();
    TerrainSplatMaterial {
        layers,
        indices: splat.indices,
    }
}
