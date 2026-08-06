//! Paging exterior cells in and out around a moving anchor (typically the camera).
//!
//! Put a [`CellStreamer`] on the anchor entity; every frame [`page_cells`] maps the
//! anchor's position to the exterior grid and diffs the wanted neighborhood against the
//! cells currently live: cells whose center falls within [`CellStreamer::radius`] are
//! spawned as ordinary [`CellSeed`]s (nearest first, at most [`CellStreamer::budget`]
//! per frame — each seed's cell builds in the background and applies whole a frame or
//! two later), and live cells whose center drifts beyond `radius + hysteresis` are
//! despawned with their entire contents. The hysteresis ring keeps boundary cells from
//! thrashing as the anchor wanders, and doubles as an unload delay: a despawned cell's
//! asset handles drop with its entities, so its meshes and textures stay cached only
//! while something else still holds them.
//!
//! Each seed is tied to its streamer by the [`StreamedBy`] relationship; the inverse
//! [`StreamedCells`] on the anchor is the live set. The ECS maintains the pair: a seed
//! despawned by anything else simply leaves [`StreamedCells`] (and is re-seeded next
//! frame while still in range), and despawning the streamer entity despawns its live
//! cells with it.
//!
//! ```ignore
//! commands.spawn((
//!     Camera3d::default(),
//!     Transform::from_xyz(-3.0 * CELL_SIZE_METERS, 30.0, 2.0 * CELL_SIZE_METERS),
//!     CellStreamer::new(load_order.0.clone()),
//! ));
//! ```
//!
//! Exterior-only: grid squares with no `CELL` record (past the map edge, or holes) are
//! simply never seeded — absence is normal, not an error. Interiors are a door-transition
//! problem for later. One streamer is expected at a time: streamers ignore each other's
//! seeds, so overlapping neighborhoods would spawn a cell once per streamer.

use std::collections::HashSet;

use bevy::asset::{Assets, Handle};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::name::Name;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::math::Vec2;
use bevy::transform::components::GlobalTransform;

use crate::LoadOrderAsset;
use crate::cell::CellSeed;
use crate::convert::CELL_SIZE_METERS;
use crate::tes_loadorder::CellId;

/// Streams exterior cells around this entity: cells within `radius` of the anchor page
/// in as [`CellSeed`]s, live cells beyond `radius + hysteresis` page out. The live set
/// is the [`StreamedCells`] relationship target on the same entity. See the
/// [module docs](self).
#[derive(Component, Debug, Clone)]
pub struct CellStreamer {
    /// The load order to stream cells from.
    pub load_order: Handle<LoadOrderAsset>,
    /// Page-in radius in meters, measured from the anchor to a cell's center.
    pub radius: f32,
    /// Extra meters beyond `radius` a live cell may drift before paging out.
    pub hysteresis: f32,
    /// Maximum cells paged in per frame. A paged-in seed starts a background build;
    /// the finished cell — a few hundred entities — still applies in a single frame,
    /// so this bounds how many such applies can land together.
    pub budget: usize,
}

impl CellStreamer {
    /// A streamer with defaults tuned for a fly camera: radius 2.5 cells (~293 m),
    /// hysteresis half a cell (~59 m), 2 cells paged in per frame.
    pub fn new(load_order: Handle<LoadOrderAsset>) -> CellStreamer {
        CellStreamer {
            load_order,
            radius: 2.5 * CELL_SIZE_METERS,
            hysteresis: 0.5 * CELL_SIZE_METERS,
            budget: 2,
        }
    }
}

/// Relationship on a streamed [`CellSeed`]: the [`CellStreamer`] entity that paged it
/// in. Inserted by [`page_cells`]; the inverse is [`StreamedCells`] on the streamer.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = StreamedCells)]
pub struct StreamedBy(#[entities] pub Entity);

impl StreamedBy {
    /// The streamer entity that paged this seed in.
    pub fn streamer(&self) -> Entity {
        self.0
    }
}

/// The live seeds of a [`CellStreamer`], maintained by the ECS from the [`StreamedBy`]
/// relationship on each seed. `linked_spawn`: despawning the streamer despawns its live
/// cells with it. Derefs to `[Entity]`, like [`Children`](bevy::ecs::hierarchy::Children).
#[derive(Component, Default, Debug)]
#[relationship_target(relationship = StreamedBy, linked_spawn)]
pub struct StreamedCells(Vec<Entity>);

impl std::ops::Deref for StreamedCells {
    type Target = [Entity];

    fn deref(&self) -> &[Entity] {
        &self.0
    }
}

/// The center of exterior cell `(gx, gy)` in the horizontal plane, as game-frame
/// meters `(x, y)` — the Bevy mapping is `x = x`, `y = -z` (see
/// [`convert::land_transform`](crate::convert::land_transform)).
fn cell_center(gx: i32, gy: i32) -> Vec2 {
    Vec2::new(
        (gx as f32 + 0.5) * CELL_SIZE_METERS,
        (gy as f32 + 0.5) * CELL_SIZE_METERS,
    )
}

/// Diffs each [`CellStreamer`]'s neighborhood against its live cells, spawning and
/// despawning [`CellSeed`]s. Registered by `TesPlugin` (chained before
/// [`spawn_cells`](crate::cell::spawn_cells), so a paged-in seed starts its background
/// build the same frame). Waits until the streamer's load order is loaded.
pub fn page_cells(
    mut commands: Commands,
    streamers: Query<(
        Entity,
        &GlobalTransform,
        &CellStreamer,
        Option<&StreamedCells>,
    )>,
    seeds: Query<&CellSeed>,
    load_orders: Res<Assets<LoadOrderAsset>>,
) {
    for (streamer_entity, anchor, streamer, live) in &streamers {
        let Some(load_order) = load_orders.get(&streamer.load_order) else {
            continue; // still loading; try again next frame
        };
        let translation = anchor.translation();
        let anchor_xy = Vec2::new(translation.x, -translation.z);

        // Page out first: live cells beyond the hysteresis ring. Seeds something else
        // despawned have already left `StreamedCells`, so there is nothing to prune.
        let drop_beyond = streamer.radius + streamer.hysteresis;
        let mut live_grids = HashSet::new();
        for &entity in live.map(|cells| &cells[..]).unwrap_or_default() {
            let Ok(seed) = seeds.get(entity) else {
                continue;
            };
            let CellId::Exterior { x: gx, y: gy } = seed.cell else {
                continue; // page_cells only seeds exteriors
            };
            if anchor_xy.distance(cell_center(gx, gy)) <= drop_beyond {
                live_grids.insert((gx, gy));
            } else {
                commands.entity(entity).despawn();
            }
        }

        // Page in: authored cells within the radius, nearest first, through the budget.
        // Absent grids (map edge) probe as None and are simply skipped — re-probing
        // them next frame is a hash lookup, not worth caching.
        let radius = streamer.radius;
        let min_gx = ((anchor_xy.x - radius) / CELL_SIZE_METERS).floor() as i32;
        let max_gx = ((anchor_xy.x + radius) / CELL_SIZE_METERS).floor() as i32;
        let min_gy = ((anchor_xy.y - radius) / CELL_SIZE_METERS).floor() as i32;
        let max_gy = ((anchor_xy.y + radius) / CELL_SIZE_METERS).floor() as i32;
        let mut wanted = Vec::new();
        for gx in min_gx..=max_gx {
            for gy in min_gy..=max_gy {
                if live_grids.contains(&(gx, gy)) {
                    continue;
                }
                let distance_sq = anchor_xy.distance_squared(cell_center(gx, gy));
                if distance_sq <= radius * radius
                    && load_order.cell(&CellId::exterior(gx, gy)).is_some()
                {
                    wanted.push(((gx, gy), distance_sq));
                }
            }
        }
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &((gx, gy), _) in wanted.iter().take(streamer.budget) {
            commands.spawn((
                CellSeed {
                    load_order: streamer.load_order.clone(),
                    cell: CellId::exterior(gx, gy),
                },
                StreamedBy(streamer_entity),
                Name::new(format!("Cell {gx},{gy}")),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_center_matches_land_transform_extents() {
        // Cell (gx, gy) spans [gx·CSM, (gx+1)·CSM] × [gy·CSM, (gy+1)·CSM] in game-frame
        // meters (land_transform places its corner at the grid product).
        assert_eq!(cell_center(0, 0), Vec2::splat(0.5 * CELL_SIZE_METERS));
        assert_eq!(
            cell_center(-3, 2),
            Vec2::new(-2.5 * CELL_SIZE_METERS, 2.5 * CELL_SIZE_METERS)
        );
    }
}
