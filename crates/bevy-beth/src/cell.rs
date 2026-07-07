//! Spawning cells (interiors and exterior grid squares) from a loaded ESM.
//!
//! Spawn an entity with a [`CellSeed`] naming a cell; once the [`EsmAsset`] is loaded,
//! [`spawn_cells`] resolves the cell record and spawns one child entity per object
//! reference — each with the reference's placement as a Y-up [`Transform`] and, when its
//! object's model resolves in the VFS, a [`WorldAssetRoot`] pointing at the NIF's
//! `#Scene` sub-asset. Only the NIFs a spawned cell actually references get loaded.
//!
//! ```ignore
//! commands.spawn(CellSeed {
//!     esm: asset_server.load("tes://Morrowind.esm"),
//!     cell: CellId::interior("Balmora, Guild of Mages"),
//! });
//! ```
//!
//! Exterior cells also grow a terrain child tagged [`CellTerrain`] — a mesh built from
//! the cell's `LAND` record (65×65 vertex heights, normals and colors) — plus a sea-level
//! water plane when the terrain dips below height 0.
//!
//! What is *not* spawned (counted in [`CellSpawned::skipped`], logged at debug level):
//! NPCs and creatures (their NIFs are skinned, which the scene builder doesn't support
//! yet), leveled-creature/item spawn points (they need runtime list resolution),
//! references flagged disabled, and `moved_references` (correct handling needs
//! multi-plugin merging — in a single vanilla ESM they don't occur). Lights spawn a
//! [`PointLight`] (plus their model, when they have one); interiors with water get a
//! translucent stand-in plane tagged [`CellWater`]. Ambient/fog values are surfaced on
//! the seed as [`CellEnvironment`] for the app to apply — Bevy's ambient light is
//! per-camera, so the library doesn't force it.

use std::collections::HashSet;

use bevy::asset::{AssetServer, Assets};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::name::Name;
use bevy::ecs::query::Without;
use bevy::ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy::light::PointLight;
use bevy::material::AlphaMode;
use bevy::math::primitives::Plane3d;
use bevy::math::{Vec2, Vec3};
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use tes3_esm::records::cell::{Cell, CellFlags, Reference};
use tes3_esm::records::land::CELL_SIZE;
use tes3_esm::records::ligh::LightFlags;

use crate::index::{CellId, ObjectKind};
use crate::{EsmAsset, TesVfsHandle, convert};

/// Point-light lumens per game-unit² of `LightData::radius`. A documented heuristic, not
/// game data: Morrowind's fixed-function attenuation doesn't translate to physical
/// units, so this is chosen so a radius-256 torch reads correctly at game-unit scale
/// (1 m ≈ 70 units; illuminance falls with the square of the distance in units).
const LIGHT_INTENSITY_PER_UNIT_SQ: f32 = 20_000.0;

/// Asks for a cell's contents to be spawned as children of this entity, once `esm`
/// finishes loading. One-shot: the seed entity is tagged [`CellSpawned`] (or
/// [`CellSpawnFailed`]) afterwards. See the [module docs](self).
#[derive(Component, Debug, Clone)]
#[require(Transform, Visibility)]
pub struct CellSeed {
    /// The plugin to read the cell from.
    pub esm: bevy::asset::Handle<EsmAsset>,
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

/// Inserted on the seed entity instead of [`CellSpawned`] when the ESM failed to load or
/// the cell doesn't exist in it.
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
    /// Water surface height in game units — equal to the Bevy Y coordinate after the
    /// Z-up→Y-up conversion.
    pub water_height: Option<f32>,
}

/// Seeds that still need spawning: not yet done, not yet failed.
type PendingSeeds<'w, 's> =
    Query<'w, 's, (Entity, &'static CellSeed), (Without<CellSpawned>, Without<CellSpawnFailed>)>;

/// Resolves pending [`CellSeed`]s and spawns their cells. Registered by `BethPlugin`
/// under the `scene` feature; polls until each seed's ESM loads, then spawns once.
#[allow(clippy::too_many_arguments)]
pub fn spawn_cells(
    mut commands: Commands,
    seeds: PendingSeeds,
    esms: Res<Assets<EsmAsset>>,
    asset_server: Res<AssetServer>,
    vfs: Res<TesVfsHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut warned: Local<HashSet<String>>,
    mut terrain_material: Local<Option<bevy::asset::Handle<StandardMaterial>>>,
) {
    for (seed_entity, seed) in &seeds {
        let Some(esm) = esms.get(&seed.esm) else {
            if let bevy::asset::LoadState::Failed(e) = asset_server.load_state(&seed.esm) {
                eprintln!("bevy-beth: ESM failed to load for {:?}: {e}", seed.cell);
                commands
                    .entity(seed_entity)
                    .insert(CellSpawnFailed(format!("ESM failed to load: {e}")));
            }
            continue; // still loading; try again next frame
        };
        let Some(cell) = esm.index.cell(&esm.plugin, &seed.cell) else {
            eprintln!("bevy-beth: cell {:?} not found in plugin", seed.cell);
            commands
                .entity(seed_entity)
                .insert(CellSpawnFailed(format!("no such cell: {:?}", seed.cell)));
            continue;
        };

        let mut spawner = CellSpawner {
            commands: &mut commands,
            esm,
            asset_server: &asset_server,
            vfs: &vfs,
            warned: &mut warned,
            seed_entity,
            spawned: 0,
            skipped: 0,
            position_sum: Vec3::ZERO,
        };
        for reference in &cell.references {
            spawner.spawn_reference(reference);
        }
        if !cell.moved_references.is_empty() {
            // MVRF relocates references defined by another plugin; meaningless without
            // multi-plugin merging (future work) and absent from single vanilla ESMs.
            eprintln!(
                "bevy-beth: skipping {} moved references in {:?}",
                cell.moved_references.len(),
                seed.cell
            );
        }
        let (spawned, skipped) = (spawner.spawned, spawner.skipped);
        let center = spawner.position_sum / spawned.max(1) as f32;

        let terrain_min_height = if cell.data.flags.contains(CellFlags::INTERIOR) {
            None
        } else {
            spawn_terrain(
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut terrain_material,
                seed_entity,
                esm,
                cell.data.grid_x,
                cell.data.grid_y,
            )
        };
        spawn_water(
            &mut commands,
            &mut meshes,
            &mut materials,
            seed_entity,
            cell,
            center,
            terrain_min_height,
        );
        commands
            .entity(seed_entity)
            .insert((environment(cell), CellSpawned { spawned, skipped }));
    }
}

/// Per-cell spawn pass: walks the reference list, spawning children under the seed.
struct CellSpawner<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    esm: &'a EsmAsset,
    asset_server: &'a AssetServer,
    vfs: &'a TesVfsHandle,
    /// Ids/models already warned about, shared across cells and frames.
    warned: &'a mut HashSet<String>,
    seed_entity: Entity,
    spawned: usize,
    skipped: usize,
    /// Sum of spawned children's (Y-up) translations, for centring the water plane.
    position_sum: Vec3,
}

impl CellSpawner<'_, '_, '_> {
    fn spawn_reference(&mut self, reference: &Reference) {
        let object_id = reference.object.decode().into_owned();
        let Some(info) = self.esm.index.object(&object_id) else {
            if self.warned.insert(object_id.clone()) {
                eprintln!("bevy-beth: cell references unknown object id {object_id:?}");
            }
            self.skipped += 1;
            return;
        };
        // Skinned models and runtime-resolved spawn points aren't supported yet; a
        // disabled reference is authored not to appear.
        let unsupported = matches!(
            info.kind,
            ObjectKind::Npc
                | ObjectKind::Creature
                | ObjectKind::BodyPart
                | ObjectKind::LeveledCreature
                | ObjectKind::LeveledItem
        );
        if unsupported || reference.disabled.is_some() {
            self.skipped += 1;
            return;
        }

        let transform = reference
            .transform
            .as_ref()
            .map(|t| convert::cell_reference_transform(t, reference.scale.unwrap_or(1.0)))
            .unwrap_or_default();
        self.position_sum += transform.translation;

        let mut child = self.commands.spawn((
            transform,
            Visibility::default(),
            Name::new(object_id.clone()),
            CellReference {
                id: reference.id,
                object: object_id.clone(),
            },
            ChildOf(self.seed_entity),
        ));

        if let Some(model) = &info.model {
            match self.vfs.0.resolve_model(model) {
                Some(path) => {
                    child.insert(WorldAssetRoot(
                        self.asset_server
                            .load::<WorldAsset>(format!("tes://{path}#Scene")),
                    ));
                }
                None => {
                    if self.warned.insert(model.clone()) {
                        eprintln!("bevy-beth: cannot resolve model {model:?} (for {object_id:?})");
                    }
                }
            }
        }

        if info.kind == ObjectKind::Light
            && let Some(light) = &info.light
            && !light
                .flags
                .intersects(LightFlags::NEGATIVE | LightFlags::OFF_BY_DEFAULT)
        {
            let radius = light.radius as f32;
            child.insert(PointLight {
                color: Color::srgb_u8(light.color.r, light.color.g, light.color.b),
                intensity: LIGHT_INTENSITY_PER_UNIT_SQ * radius * radius,
                range: radius,
                ..Default::default()
            });
        }

        self.spawned += 1;
    }
}

/// The cell's `AMBI`/water staging values as a [`CellEnvironment`].
fn environment(cell: &Cell) -> CellEnvironment {
    let srgb = |c: tes_core::math::Color| Color::srgb_u8(c.r, c.g, c.b);
    CellEnvironment {
        interior: cell.data.flags.contains(CellFlags::INTERIOR),
        ambient: cell.ambient.map(|a| srgb(a.ambient)),
        sunlight: cell.ambient.map(|a| srgb(a.sunlight)),
        fog: cell.ambient.map(|a| (srgb(a.fog), a.fog_density)),
        water_height: cell.water_height,
    }
}

/// Spawn the terrain mesh child for an exterior cell, when its `LAND` record exists and
/// has heights. Returns the minimum decoded terrain height in game units (it drives the
/// sea-level water decision). Cells without `LAND` — map edges, sparse plugins — skip
/// silently: that's authored absence, not an error, and (like water) terrain doesn't
/// count toward the seed's reference tallies.
#[allow(clippy::too_many_arguments)]
fn spawn_terrain(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    terrain_material: &mut Option<bevy::asset::Handle<StandardMaterial>>,
    seed_entity: Entity,
    esm: &EsmAsset,
    grid_x: i32,
    grid_y: i32,
) -> Option<f32> {
    let land = esm.index.land(&esm.plugin, grid_x, grid_y)?;
    let mesh = convert::land_mesh(land)?;
    let min = land
        .decode_heights()?
        .into_iter()
        .fold(f32::INFINITY, f32::min);
    // All cells share one matte white material; the LAND vertex colors carry the tint.
    let material = terrain_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                ..Default::default()
            })
        })
        .clone();
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        convert::land_transform(grid_x, grid_y),
        Visibility::default(),
        Name::new(format!("Terrain {grid_x},{grid_y}")),
        CellTerrain,
        ChildOf(seed_entity),
    ));
    Some(min)
}

/// Spawn the stand-in water plane for a cell:
///
/// - **Interior** with water: a large translucent sheet at the authored water height,
///   centred on the spawned references (interior coordinates aren't origin-centred).
/// - **Exterior**: sea level is the implicit global height 0 — one cell-sized plane,
///   spawned only when the cell has terrain that dips below it (inland cells skip the
///   hidden plane; neighbouring cells' planes tile seamlessly).
fn spawn_water(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    seed_entity: Entity,
    cell: &Cell,
    center: Vec3,
    terrain_min_height: Option<f32>,
) {
    let interior = cell.data.flags.contains(CellFlags::INTERIOR);
    let (center, half_size) = if interior {
        let has_water =
            cell.data.flags.contains(CellFlags::HAS_WATER) || cell.water_height.is_some();
        if !has_water {
            return;
        }
        let height = cell.water_height.unwrap_or(0.0);
        (Vec3::new(center.x, height, center.z), 8192.0)
    } else {
        if !terrain_min_height.is_some_and(|min| min < 0.0) {
            return;
        }
        let half = CELL_SIZE / 2.0;
        let corner = convert::land_transform(cell.data.grid_x, cell.data.grid_y).translation;
        (corner + Vec3::new(half, 0.0, -half), half)
    };
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(half_size))))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.1, 0.3, 0.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..Default::default()
        })),
        Transform::from_translation(center),
        Visibility::default(),
        Name::new("Water"),
        CellWater,
        ChildOf(seed_entity),
    ));
}
