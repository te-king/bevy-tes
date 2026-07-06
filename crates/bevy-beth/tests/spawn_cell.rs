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
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAssetRoot;
use tes3_esm::records::cell::{Cell, CellData, CellFlags, Reference, ReferenceTransform};
use tes3_esm::records::crea::Crea;
use tes3_esm::records::ligh::{Ligh, LightData};
use tes3_esm::records::stat::Stat;
use tes3_esm::{L1String, Plugin, Record};

use bevy_beth::{
    CellId, CellReference, CellSeed, CellSpawnFailed, CellSpawned, CellWater, EsmAsset, EsmIndex,
};

mod common;
use common::{app_with_assets, pump_until_loaded};

fn l1(s: &str) -> L1String {
    L1String::from_bytes(s.as_bytes().to_vec())
}

fn reference(id: u32, object: &str, transform: Option<ReferenceTransform>) -> Reference {
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
fn synthetic_asset() -> EsmAsset {
    let plugin = Plugin {
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
    let index = EsmIndex::build(&plugin);
    EsmAsset { plugin, index }
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
        .resource_mut::<Assets<EsmAsset>>()
        .add(synthetic_asset());

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            esm: handle,
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

#[test]
fn unknown_cell_marks_failure() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<EsmAsset>>()
        .add(synthetic_asset());

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            esm: handle,
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
    let esm: Handle<EsmAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            esm: esm.clone(),
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
        let esms = app.world().resource::<Assets<EsmAsset>>();
        let asset = esms.get(&esm).expect("ESM loaded");
        let cell = asset
            .index
            .cell(
                &asset.plugin,
                &CellId::interior("balmora, caius cosades' house"),
            )
            .expect("cell exists");
        let reference = cell
            .references
            .iter()
            .find(|r| {
                r.transform.is_some()
                    && asset
                        .index
                        .object(&r.object.decode())
                        .is_some_and(|o| o.kind == bevy_beth::ObjectKind::Static)
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
    let esm: Handle<EsmAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");
    let state = pump_until_loaded(&mut app, &esm);
    assert!(matches!(state, LoadState::Loaded), "{state:?}");

    // Any well-populated exterior square will do; find one instead of pinning a grid.
    let grid = {
        let esms = app.world().resource::<Assets<EsmAsset>>();
        let asset = esms.get(&esm).expect("ESM loaded");
        asset
            .plugin
            .records
            .iter()
            .find_map(|r| match r {
                Record::Cell(c)
                    if !c.data.flags.contains(CellFlags::INTERIOR) && c.references.len() > 20 =>
                {
                    Some((c.data.grid_x, c.data.grid_y))
                }
                _ => None,
            })
            .expect("a populated exterior cell")
    };

    let seed = app
        .world_mut()
        .spawn(CellSeed {
            esm,
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
    let mut water = app.world_mut().query::<&CellWater>();
    assert_eq!(
        water.iter(app.world()).count(),
        0,
        "exterior water is deferred until terrain exists"
    );
}
