//! End-to-end tests of cell spawning (`scene` feature): a `CellSeed` entity becomes one
//! child per supported object reference.
//!
//! The synthetic test builds its plugin in memory and always runs; the game-data tests
//! skip themselves when the (gitignored) `data/` fixtures are absent.
#![cfg(feature = "scene")]

use std::f32::consts::FRAC_PI_2;

use bevy::asset::{AssetServer, Assets, Handle, LoadState};
use bevy::ecs::hierarchy::ChildOf;
use bevy::light::PointLight;
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use tes3_esm::records::cell::{Cell, CellData, CellFlags, Reference, ReferenceTransform};
use tes3_esm::records::crea::Crea;
use tes3_esm::records::land::{HEIGHT_SCALE, LAND_GRID, Land, LandFlags, VTEX_GRID};
use tes3_esm::records::ligh::{Ligh, LightData};
use tes3_esm::records::ltex::Ltex;
use tes3_esm::records::stat::Stat;
use tes3_esm::{EsmDirectory, L1Str, Record};

use bevy_beth::{
    CellId, CellReference, CellSeed, CellSpawnFailed, CellSpawned, CellTerrain, CellWater,
    LoadOrderAsset, TerrainSplatMaterial,
};

mod common;
use common::{app_with_assets, pump_until_loaded};

fn l1(s: &'static str) -> &'static L1Str {
    L1Str::from_bytes(s.as_bytes())
}

fn reference(
    id: u32,
    object: &'static str,
    transform: Option<ReferenceTransform>,
) -> Reference<'static> {
    Reference {
        id,
        object: l1(object),
        transform,
        ..Default::default()
    }
}

/// A plugin with one interior cell exercising every spawn rule: a placed static (whose
/// model doesn't exist in any VFS), a model-less light, a creature (skipped), a disabled
/// static (skipped), an unknown id (skipped) — plus water.
fn synthetic_asset() -> LoadOrderAsset {
    let esm = EsmDirectory {
        header: Default::default(),
        records: vec![
            Record::Stat(Stat {
                id: l1("test_stat"),
                model: l1(r"x\nowhere.nif"),
            }),
            Record::Ligh(Ligh {
                id: l1("test_light"),
                model: None,
                data: LightData {
                    radius: 256,
                    ..Default::default()
                },
                ..Default::default()
            }),
            Record::Crea(Crea {
                id: l1("test_creature"),
                model: l1(r"r\nowhere.nif"),
                ..Default::default()
            }),
            Record::Cell(Cell {
                name: l1("Test Cell"),
                data: CellData {
                    flags: CellFlags::INTERIOR | CellFlags::HAS_WATER,
                    ..Default::default()
                },
                water_height: Some(50.0),
                references: vec![
                    reference(
                        1,
                        "Test_Stat", // case-mismatched on purpose
                        Some(ReferenceTransform {
                            position: [100.0, 200.0, 300.0],
                            rotation: [0.0, 0.0, FRAC_PI_2],
                        }),
                    ),
                    reference(2, "test_light", None),
                    reference(3, "test_creature", None),
                    Reference {
                        disabled: Some(1),
                        ..reference(4, "test_stat", None)
                    },
                    reference(5, "no_such_object", None),
                ],
                ..Default::default()
            }),
        ],
    };
    LoadOrderAsset::from_static(esm)
}

/// Pump the app until the seed resolves (spawned or failed), up to a frame budget.
fn pump_until_settled(app: &mut bevy::app::App, seed: bevy::ecs::entity::Entity) {
    // Generous deadline: seeding can precede the ESM load, and parsing the full game
    // master on the IO pool takes seconds in a debug build. Exits as soon as the seed
    // resolves, so tests against synthetic (pre-inserted) assets return immediately.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        app.update();
        let entity = app.world().entity(seed);
        if entity.contains::<CellSpawned>() || entity.contains::<CellSpawnFailed>() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn synthetic_cell_spawns_and_skips() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_asset());

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order: handle,
            cell: CellId::interior("tEsT cElL"), // matching is case-insensitive
        })
        .id();
    pump_until_settled(&mut app, seed);

    let spawned = app
        .world()
        .entity(seed)
        .get::<CellSpawned>()
        .expect("seed resolved");
    assert_eq!(spawned.spawned, 2, "the placed stat and the light");
    assert_eq!(spawned.skipped, 3, "creature, disabled, unknown id");

    // The stat: placed per cell_reference_transform, present despite its unresolvable
    // model — but with no scene attached.
    let mut refs = app.world_mut().query::<(&CellReference, &Transform)>();
    let (_, stat_transform) = refs
        .iter(app.world())
        .find(|(r, _)| r.object == "Test_Stat")
        .expect("stat child exists without a resolvable model");
    assert!((stat_transform.translation - Vec3::new(100.0, 300.0, -200.0)).length() < 1e-4);
    assert!(
        stat_transform
            .rotation
            .dot(Quat::from_rotation_y(-FRAC_PI_2))
            .abs()
            > 1.0 - 1e-6
    );
    let mut with_scene = app.world_mut().query::<(&CellReference, &WorldAssetRoot)>();
    assert_eq!(
        with_scene.iter(app.world()).count(),
        0,
        "no model resolved, so no scene handles"
    );

    // The model-less light spawns as a point light child.
    let mut lights = app.world_mut().query::<(&CellReference, &PointLight)>();
    let (light_ref, light) = lights.iter(app.world()).next().expect("light child");
    assert_eq!(light_ref.object, "test_light");
    assert_eq!(light.range, 256.0);

    // Interior water: one stand-in plane at the water height.
    let mut water = app
        .world_mut()
        .query::<(&CellWater, &Transform, &ChildOf)>();
    let (_, water_transform, parent) = water.iter(app.world()).next().expect("water plane");
    assert_eq!(parent.parent(), seed);
    assert_eq!(water_transform.translation.y, 50.0);
}

/// Re-swizzle a logical row-major 16×16 grid (south-west origin) into `VTEX` storage
/// order — 4×4 blocks of 4×4 texels, the inverse of `Land::decode_textures` — so the
/// spawn path exercises the de-swizzle end-to-end.
fn vtex_bytes(logical: &[u16; VTEX_GRID * VTEX_GRID]) -> Vec<u8> {
    let mut stored = [0u16; VTEX_GRID * VTEX_GRID];
    for (stored_pos, slot) in stored.iter_mut().enumerate() {
        let (block, texel) = (stored_pos / 16, stored_pos % 16);
        let (bx, by) = (block % 4, block / 4);
        let (tx, ty) = (texel % 4, texel / 4);
        *slot = logical[(by * 4 + ty) * VTEX_GRID + (bx * 4 + tx)];
    }
    stored.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A plugin with one exterior cell at grid (1, 2): a placed static plus a LAND record
/// whose terrain sits uniformly below sea level (offset −10 → all heights −80).
///
/// With `vtex`, the LAND also carries a texture grid — value 1 (LTEX 0, `tx_a.dds`)
/// everywhere except texel (5, 9), which is value 2 (LTEX 1, `tx_b.dds`).
fn synthetic_exterior_asset(vtex: bool) -> LoadOrderAsset {
    // The synthetic EsmDirectory is 'static, so the computed VTEX blob is leaked (a few hundred
    // bytes, once per test).
    let texture_data = vtex.then(|| {
        let mut logical = [1u16; VTEX_GRID * VTEX_GRID];
        logical[9 * VTEX_GRID + 5] = 2;
        &*Box::leak(vtex_bytes(&logical).into_boxed_slice())
    });
    static ZERO_HEIGHTS: [u8; LAND_GRID * LAND_GRID] = [0; LAND_GRID * LAND_GRID];
    let esm = EsmDirectory {
        header: Default::default(),
        records: vec![
            Record::Stat(Stat {
                id: l1("test_stat"),
                model: l1(r"x\nowhere.nif"),
            }),
            Record::Ltex(Ltex {
                id: l1("tex_a"),
                index: 0,
                texture: l1("tx_a.dds"),
            }),
            Record::Ltex(Ltex {
                id: l1("tex_b"),
                index: 1,
                texture: l1("tx_b.dds"),
            }),
            Record::Cell(Cell {
                data: CellData {
                    flags: CellFlags::empty(),
                    grid_x: 1,
                    grid_y: 2,
                },
                references: vec![reference(1, "test_stat", None)],
                ..Default::default()
            }),
            Record::Land(Land {
                grid_x: 1,
                grid_y: 2,
                data_types: LandFlags::HAS_HEIGHTS | LandFlags::HAS_TEXTURES,
                height_offset: Some(-10.0),
                heights: Some(&ZERO_HEIGHTS),
                texture_data,
                ..Default::default()
            }),
        ],
    };
    LoadOrderAsset::from_static(esm)
}

#[test]
fn synthetic_exterior_spawns_terrain_and_sea() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_exterior_asset(true));

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order: handle,
            cell: CellId::exterior(1, 2),
        })
        .id();
    pump_until_settled(&mut app, seed);

    // Terrain and water are extras, not references: only the stat is counted.
    let spawned = app
        .world()
        .entity(seed)
        .get::<CellSpawned>()
        .expect("seed resolved");
    assert_eq!((spawned.spawned, spawned.skipped), (1, 0));

    // The terrain child sits at the cell's south-west corner with the full vertex grid.
    let mut terrain = app
        .world_mut()
        .query::<(&CellTerrain, &Transform, &Mesh3d, &ChildOf)>();
    let (_, transform, mesh, parent) = terrain.iter(app.world()).next().expect("terrain child");
    assert_eq!(parent.parent(), seed);
    assert_eq!(transform.translation, Vec3::new(8192.0, 0.0, -16384.0));
    let mesh = mesh.0.clone();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let positions = meshes
        .get(&mesh)
        .expect("terrain mesh stored")
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    assert_eq!(positions.len(), LAND_GRID * LAND_GRID);
    assert_eq!(positions[0][1], -10.0 * HEIGHT_SCALE);

    // Sunken terrain gets a sea-level plane at the cell's centre.
    let mut water = app
        .world_mut()
        .query::<(&CellWater, &Transform, &ChildOf)>();
    let (_, water_transform, parent) = water.iter(app.world()).next().expect("sea-level water");
    assert_eq!(parent.parent(), seed);
    assert_eq!(
        water_transform.translation,
        Vec3::new(8192.0 + 4096.0, 0.0, -(16384.0 + 4096.0))
    );

    // The VTEX grid became a splat material: two distinct textures in first-appearance
    // order (both unresolvable here, so white stand-ins — the layer count still holds),
    // with the odd texel remapped through the de-swizzle.
    let mut terrain = app
        .world_mut()
        .query::<(&CellTerrain, &MeshMaterial3d<TerrainSplatMaterial>)>();
    let (_, material) = terrain
        .iter(app.world())
        .next()
        .expect("terrain carries the splat material");
    let material = material.0.clone();
    let splats = app.world().resource::<Assets<TerrainSplatMaterial>>();
    let splat = splats.get(&material).expect("splat material stored");
    assert_eq!(splat.layers.len(), 2);
    assert_eq!(splat.indices[0], 0, "the dominant texture takes slot 0");
    assert_eq!(
        splat.indices[9 * VTEX_GRID + 5],
        1,
        "texel (5,9) is layer 1"
    );
    assert_eq!(splat.indices.iter().filter(|&&s| s == 1).count(), 1);
}

#[test]
fn exterior_without_vtex_keeps_the_plain_material() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_exterior_asset(false));

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order: handle,
            cell: CellId::exterior(1, 2),
        })
        .id();
    pump_until_settled(&mut app, seed);

    // No VTEX grid → the shared vertex-tinted StandardMaterial, not a splat.
    let mut plain = app
        .world_mut()
        .query::<(&CellTerrain, &MeshMaterial3d<StandardMaterial>)>();
    assert_eq!(plain.iter(app.world()).count(), 1);
    let mut splat = app
        .world_mut()
        .query::<(&CellTerrain, &MeshMaterial3d<TerrainSplatMaterial>)>();
    assert_eq!(splat.iter(app.world()).count(), 0);
}

#[test]
fn unknown_cell_marks_failure() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_asset());

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order: handle,
            cell: CellId::interior("nowhere"),
        })
        .id();
    pump_until_settled(&mut app, seed);

    assert!(app.world().entity(seed).contains::<CellSpawnFailed>());
    let mut refs = app.world_mut().query::<&CellReference>();
    assert_eq!(refs.iter(app.world()).count(), 0, "nothing spawned");
}

#[test]
fn interior_cell_spawns_references() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }
    let mut app = app_with_assets();
    let load_order: Handle<LoadOrderAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order: load_order.clone(),
            cell: CellId::interior("Balmora, Caius Cosades' House"),
        })
        .id();
    pump_until_settled(&mut app, seed);

    let spawned_count = {
        let spawned = app
            .world()
            .entity(seed)
            .get::<CellSpawned>()
            .expect("seed resolved against the real ESM");
        assert!(spawned.spawned > 10, "a furnished interior: {spawned:?}");
        spawned.spawned
    };

    // Every reference child carries its provenance; count matches the report.
    let mut refs = app.world_mut().query::<(&CellReference, &ChildOf)>();
    let children = refs
        .iter(app.world())
        .filter(|(_, p)| p.parent() == seed)
        .count();
    assert_eq!(children, spawned_count);

    // A cross-check against the raw record: the first placed reference with a transform
    // that got spawned must sit exactly where cell_reference_transform puts it.
    let (expected, object) = {
        let load_orders = app.world().resource::<Assets<LoadOrderAsset>>();
        let asset = load_orders.get(&load_order).expect("load order loaded");
        let cell = asset
            .cell(&CellId::interior("balmora, caius cosades' house"))
            .expect("cell exists");
        let reference = cell
            .references
            .iter()
            .find(|r| {
                r.transform.is_some()
                    && asset
                        .object(&r.object.decode())
                        .is_some_and(|o| o.kind() == bevy_beth::ObjectKind::Static)
            })
            .expect("a placed static");
        (
            bevy_beth::convert::cell_reference_transform(
                reference.transform.as_ref().unwrap(),
                reference.scale.unwrap_or(1.0),
            ),
            reference.object.decode().into_owned(),
        )
    };
    let mut refs = app.world_mut().query::<(&CellReference, &Transform)>();
    let (_, transform) = refs
        .iter(app.world())
        .find(|(r, _)| r.object == object)
        .expect("the static spawned");
    assert!((transform.translation - expected.translation).length() < 1e-4);

    // At least one child carries a NIF scene, and that scene finishes loading.
    let scene = {
        let mut scenes = app.world_mut().query::<(&CellReference, &WorldAssetRoot)>();
        let (_, root) = scenes
            .iter(app.world())
            .next()
            .expect("resolved models produce scene handles");
        root.0.clone()
    };
    let state = pump_until_loaded(&mut app, &scene);
    assert!(
        matches!(state, LoadState::Loaded),
        "scene load state: {state:?}"
    );
}

#[test]
fn exterior_cell_spawns_references() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }
    let mut app = app_with_assets();
    let load_order: Handle<LoadOrderAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");
    let state = pump_until_loaded(&mut app, &load_order);
    assert!(matches!(state, LoadState::Loaded), "{state:?}");

    // Any well-populated exterior square with terrain will do; find one instead of
    // pinning a grid. Capture the raw VHGT fields for the independent cross-checks.
    let (grid, expected_first_height, min_height, distinct_textures) = {
        let load_orders = app.world().resource::<Assets<LoadOrderAsset>>();
        let asset = load_orders.get(&load_order).expect("load order loaded");
        asset.load_order().esms()[0]
            .directory()
            .records
            .iter()
            .find_map(|r| match r {
                Record::Cell(c)
                    if !c.data.flags.contains(CellFlags::INTERIOR) && c.references.len() > 20 =>
                {
                    let land = asset.land(c.data.grid_x, c.data.grid_y)?;
                    let heights = land.decode_heights()?;
                    // Recomputed from the raw fields, independently of decode_heights.
                    let first = (land.height_offset.unwrap()
                        + (land.heights.unwrap()[0] as i8) as f32)
                        * HEIGHT_SCALE;
                    let min = heights.into_iter().fold(f32::INFINITY, f32::min);
                    let distinct = land
                        .decode_textures()
                        .map(|grid| grid.iter().collect::<std::collections::HashSet<_>>().len());
                    Some(((c.data.grid_x, c.data.grid_y), first, min, distinct))
                }
                _ => None,
            })
            .expect("a populated exterior cell with terrain")
    };

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            load_order,
            cell: CellId::exterior(grid.0, grid.1),
        })
        .id();
    pump_until_settled(&mut app, seed);

    let spawned = app
        .world()
        .entity(seed)
        .get::<CellSpawned>()
        .expect("exterior seed resolved");
    assert!(spawned.spawned > 0, "{spawned:?}");

    // Exactly one terrain child, whose first vertex height matches the raw VHGT fields.
    let mut terrain = app.world_mut().query::<(&CellTerrain, &Mesh3d, &ChildOf)>();
    let handles: Vec<_> = terrain
        .iter(app.world())
        .filter(|(_, _, p)| p.parent() == seed)
        .map(|(_, mesh, _)| mesh.0.clone())
        .collect();
    assert_eq!(handles.len(), 1, "one terrain child per exterior cell");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let positions = meshes
        .get(&handles[0])
        .expect("terrain mesh stored")
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    assert_eq!(positions.len(), LAND_GRID * LAND_GRID);
    assert_eq!(positions[0][1], expected_first_height);

    // A vanilla LAND with a VTEX grid gets the splat material, one layer per distinct
    // texture value.
    if let Some(distinct) = distinct_textures {
        let mut splat_terrain = app
            .world_mut()
            .query::<(&CellTerrain, &MeshMaterial3d<TerrainSplatMaterial>)>();
        let (_, material) = splat_terrain
            .iter(app.world())
            .next()
            .expect("vanilla terrain is texture-splatted");
        let material = material.0.clone();
        let splats = app.world().resource::<Assets<TerrainSplatMaterial>>();
        let splat = splats.get(&material).expect("splat material stored");
        assert_eq!(splat.layers.len(), distinct);
    }

    // Sea-level water appears exactly when the terrain dips below height 0.
    let mut water = app.world_mut().query::<(&CellWater, &Transform)>();
    let planes: Vec<_> = water.iter(app.world()).collect();
    if min_height < 0.0 {
        assert_eq!(planes.len(), 1, "coastal terrain gets a sea plane");
        assert_eq!(planes[0].1.translation.y, 0.0);
    } else {
        assert!(planes.is_empty(), "inland terrain spawns no water");
    }
}
