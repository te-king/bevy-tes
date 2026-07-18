//! End-to-end tests of exterior cell streaming (`scene` feature): a `CellStreamer` on
//! an anchor entity pages `CellSeed`s in and out as the anchor moves.
//!
//! The synthetic tests build their plugin in memory and always run; the game-data test
//! skips itself when the (gitignored) `data/` fixtures are absent.
#![cfg(feature = "scene")]

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bevy::app::App;
use bevy::asset::{AssetServer, Assets, Handle};
use bevy::math::Vec2;
use bevy::transform::components::GlobalTransform;
use tes3_esm::records::cell::{Cell, CellData, CellFlags, Reference};
use tes3_esm::records::stat::Stat;
use tes3_esm::{EsmDirectory, L1Str, Record};

use bevy_tes::{
    CELL_SIZE_METERS, CellId, CellReference, CellSeed, CellSpawnFailed, CellSpawned, CellStreamer,
    LoadOrderAsset,
};

mod common;
use common::{app_with_assets, pump_until_loaded};

fn l1(s: &'static str) -> &'static L1Str {
    L1Str::from_bytes(s.as_bytes())
}

/// A plugin authoring one exterior cell per grid coordinate, each placing a single
/// static (whose model resolves nowhere — placement is all these tests need).
fn synthetic_grid_asset(grids: &[(i32, i32)]) -> LoadOrderAsset {
    let mut records = vec![Record::Stat(Stat {
        id: l1("test_stat"),
        model: l1(r"x\nowhere.nif"),
    })];
    for &(grid_x, grid_y) in grids {
        records.push(Record::Cell(Cell {
            data: CellData {
                flags: CellFlags::empty(),
                grid_x,
                grid_y,
            },
            references: vec![Reference {
                id: 1,
                object: l1("test_stat"),
                ..Default::default()
            }],
            ..Default::default()
        }));
    }
    LoadOrderAsset::from_static(EsmDirectory {
        header: Default::default(),
        records,
    })
}

/// The 3×3 block (0,0)..=(2,2) with the (2,2) corner left unauthored.
fn block_minus_corner() -> Vec<(i32, i32)> {
    let mut grids: Vec<(i32, i32)> = (0..3).flat_map(|x| (0..3).map(move |y| (x, y))).collect();
    grids.retain(|&g| g != (2, 2));
    grids
}

/// An anchor `GlobalTransform` hovering over the center of exterior cell `(gx, gy)`.
fn over_cell(gx: i32, gy: i32) -> GlobalTransform {
    GlobalTransform::from_xyz(
        (gx as f32 + 0.5) * CELL_SIZE_METERS,
        30.0,
        -(gy as f32 + 0.5) * CELL_SIZE_METERS,
    )
}

/// The grid coordinates of every live seed.
fn live_grids(app: &mut App) -> BTreeSet<(i32, i32)> {
    let mut seeds = app.world_mut().query::<&CellSeed>();
    seeds
        .iter(app.world())
        .map(|seed| match seed.cell {
            CellId::Exterior { x, y } => (x, y),
            ref interior => panic!("streamer seeded an interior: {interior:?}"),
        })
        .collect()
}

/// Pump the app until `done`, up to a deadline (falls through to the caller's asserts).
fn pump_until(app: &mut App, mut done: impl FnMut(&mut App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        app.update();
        if done(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Whether every live seed has resolved (spawned or failed).
fn all_resolved(app: &mut App) -> bool {
    let mut seeds = app
        .world_mut()
        .query::<(&CellSeed, Option<&CellSpawned>, Option<&CellSpawnFailed>)>();
    seeds
        .iter(app.world())
        .all(|(_, spawned, failed)| spawned.is_some() || failed.is_some())
}

#[test]
fn pages_in_authored_cells_and_skips_absent_grids() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_grid_asset(&block_minus_corner()));

    // Radius 2 cells from the center of (1,1): every authored center is within reach
    // (the far corners sit at √2 cells), as are plenty of unauthored grids around and
    // beyond the missing (2,2) corner — those must simply never seed.
    let mut streamer = CellStreamer::new(handle);
    streamer.radius = 2.0 * CELL_SIZE_METERS;
    streamer.budget = 16;
    app.world_mut().spawn((over_cell(1, 1), streamer));

    let expected: BTreeSet<(i32, i32)> = block_minus_corner().into_iter().collect();
    pump_until(&mut app, |app| {
        live_grids(app) == expected && all_resolved(app)
    });

    assert_eq!(live_grids(&mut app), expected);
    let mut failed = app.world_mut().query::<&CellSpawnFailed>();
    assert_eq!(
        failed.iter(app.world()).count(),
        0,
        "absent grids are probed before seeding, so nothing fails"
    );
    // One placed static per authored cell.
    let mut references = app.world_mut().query::<&CellReference>();
    assert_eq!(references.iter(app.world()).count(), expected.len());
}

#[test]
fn budget_throttles_page_in_nearest_first() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_grid_asset(&block_minus_corner()));

    let mut streamer = CellStreamer::new(handle);
    streamer.radius = 2.0 * CELL_SIZE_METERS;
    streamer.budget = 1;
    app.world_mut().spawn((over_cell(1, 1), streamer));

    // One cell per frame, and the first is the one under the anchor.
    app.update();
    assert_eq!(live_grids(&mut app), BTreeSet::from([(1, 1)]));
    app.update();
    assert_eq!(live_grids(&mut app).len(), 2);
    app.update();
    assert_eq!(live_grids(&mut app).len(), 3);
}

#[test]
fn moving_pages_out_beyond_hysteresis() {
    let mut app = app_with_assets();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<LoadOrderAsset>>()
        .add(synthetic_grid_asset(&block_minus_corner()));

    // Radius 1.2 cells: from the center of (1,1) that's the cell itself plus its four
    // orthogonal neighbors (diagonals sit at √2 ≈ 1.41). Hysteresis 0.5 pages out
    // beyond 1.7 cells.
    let mut streamer = CellStreamer::new(handle);
    streamer.radius = 1.2 * CELL_SIZE_METERS;
    streamer.hysteresis = 0.5 * CELL_SIZE_METERS;
    streamer.budget = 16;
    let anchor = app.world_mut().spawn((over_cell(1, 1), streamer)).id();

    let start: BTreeSet<(i32, i32)> = BTreeSet::from([(1, 1), (0, 1), (2, 1), (1, 0), (1, 2)]);
    pump_until(&mut app, |app| {
        live_grids(app) == start && all_resolved(app)
    });
    assert_eq!(live_grids(&mut app), start);

    // One cell east, over (2,1). From there: (0,1) is 2.0 cells out — beyond the
    // hysteresis ring, paged out (entity and children gone). (1,0)/(1,2) are √2 out —
    // beyond the radius but inside the ring, so they stay. (2,0) comes into range;
    // (3,1) and (2,2) are in range but unauthored.
    app.world_mut().entity_mut(anchor).insert(over_cell(2, 1));
    let moved: BTreeSet<(i32, i32)> = BTreeSet::from([(1, 1), (2, 1), (1, 0), (1, 2), (2, 0)]);
    pump_until(&mut app, |app| {
        live_grids(app) == moved && all_resolved(app)
    });

    assert_eq!(live_grids(&mut app), moved);
    // The paged-out cell's subtree went with it: one placed static per live cell.
    let mut references = app.world_mut().query::<&CellReference>();
    assert_eq!(references.iter(app.world()).count(), moved.len());
}

/// The authored exterior cells whose centers lie within `radius` meters of the center
/// of cell `(ax, ay)` — what a streamer parked there must converge on.
fn authored_in_radius(
    load_order: &LoadOrderAsset,
    (ax, ay): (i32, i32),
    radius: f32,
) -> BTreeSet<(i32, i32)> {
    let anchor = Vec2::new(ax as f32 + 0.5, ay as f32 + 0.5) * CELL_SIZE_METERS;
    let span = (radius / CELL_SIZE_METERS).ceil() as i32 + 1;
    let mut set = BTreeSet::new();
    for gx in (ax - span)..=(ax + span) {
        for gy in (ay - span)..=(ay + span) {
            let center = Vec2::new(gx as f32 + 0.5, gy as f32 + 0.5) * CELL_SIZE_METERS;
            if anchor.distance(center) <= radius
                && load_order.cell(&CellId::exterior(gx, gy)).is_some()
            {
                set.insert((gx, gy));
            }
        }
    }
    set
}

#[test]
fn streams_the_game_master_around_balmora() {
    if tes_testdata::fixture("Morrowind.esm").is_none() {
        return;
    }

    let mut app = app_with_assets();
    let handle: Handle<LoadOrderAsset> = app
        .world()
        .resource::<AssetServer>()
        .load("tes://Morrowind.esm");
    pump_until_loaded(&mut app, &handle);

    let radius = 1.5 * CELL_SIZE_METERS;
    let load_orders = app.world().resource::<Assets<LoadOrderAsset>>();
    let load_order = load_orders.get(&handle).expect("master loaded");
    let start = authored_in_radius(load_order, (-3, -2), radius);
    let moved = authored_in_radius(load_order, (2, -2), radius);
    assert!(
        start.contains(&(-3, -2)) && start.len() >= 4,
        "Balmora's neighborhood is authored: {start:?}"
    );
    assert!(
        start.is_disjoint(&moved),
        "the teleport must fully leave the hysteresis ring"
    );

    let mut streamer = CellStreamer::new(handle);
    streamer.radius = radius;
    streamer.budget = 2;
    let anchor = app.world_mut().spawn((over_cell(-3, -2), streamer)).id();
    pump_until(&mut app, |app| {
        live_grids(app) == start && all_resolved(app)
    });
    assert_eq!(live_grids(&mut app), start);

    // Five cells east — far beyond radius + hysteresis, so the Balmora set unloads
    // entirely and the destination neighborhood loads in its place.
    app.world_mut().entity_mut(anchor).insert(over_cell(2, -2));
    pump_until(&mut app, |app| {
        live_grids(app) == moved && all_resolved(app)
    });
    assert_eq!(live_grids(&mut app), moved);
}
