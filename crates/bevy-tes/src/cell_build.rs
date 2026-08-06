//! The pure, ECS-free half of cell spawning: [`build_cell`] turns a cell record into an
//! owned [`CellPlan`] — reference placements with resolved model paths, the terrain mesh
//! with resolved splat texture paths, the water plane, the staging environment.
//!
//! Everything here is CPU work over shared data (`&TesLoadOrder` + `&TesVfs`) with no
//! `Commands`, no `AssetServer`, and no `Assets` borrow, so it can run on a background
//! task; [`spawn_cells`](crate::cell::spawn_cells) applies the finished plan on the main
//! thread — spawning entities and starting the asset loads the plan's paths name.

use std::collections::{HashMap, HashSet};

use bevy::color::Color;
use bevy::light::PointLight;
use bevy::math::Vec3;
use bevy::mesh::Mesh;
use bevy::transform::components::Transform;
use tes3_esm::records::cell::{Cell, CellFlags, Reference};
use tes3_esm::records::land::VTEX_GRID;
use tes3_esm::records::ligh::LightFlags;

use crate::cell::CellEnvironment;
use crate::convert;
use crate::terrain::MAX_TERRAIN_LAYERS;
use crate::tes_loadorder::{CellId, ObjectKind, TesLoadOrder};
use crate::tes_vfs::TesVfs;

/// Point-light lumens per meter² of light range. A documented heuristic, not game data:
/// Morrowind's fixed-function attenuation doesn't translate to physical units, so this
/// is chosen so a radius-256 torch (range ≈ 3.7 m) reads correctly. Scaling intensity
/// with the *square* of the range keeps the illuminance at any given fraction of the
/// range constant across light sizes. The absolute values are far above physical lumens
/// (the viewers meter exposure automatically); a physical retune is future work.
const LIGHT_INTENSITY_PER_METER_SQ: f32 = 20_000.0;

/// Everything needed to spawn one cell, fully owned. Built by [`build_cell`].
pub(crate) struct CellPlan {
    /// One entry per spawnable reference, in authored order.
    pub references: Vec<ReferencePlan>,
    /// References not planned (NPCs/creatures, leveled lists, disabled, unknown ids).
    pub skipped: usize,
    /// `MVRF` entries in the cell record, skipped pending multi-plugin merging.
    pub moved_references: usize,
    pub terrain: Option<TerrainPlan>,
    pub water: Option<WaterPlan>,
    pub environment: CellEnvironment,
    /// Diagnostics to emit, deduplicated by `key` against warnings from other cells.
    pub warnings: Vec<Warning>,
}

/// One placed object: the reference entity's components plus the model to load, if any.
pub(crate) struct ReferencePlan {
    /// The reference's `FRMR` id.
    pub id: u32,
    /// The object's editor id, as authored.
    pub object: String,
    pub transform: Transform,
    /// The model's resolved VFS path (forward-slash form), for loading as
    /// `tes://{path}#Scene`. `None` for model-less objects and unresolvable models.
    pub model_path: Option<String>,
    pub light: Option<PointLight>,
}

/// An exterior cell's terrain mesh, ready to insert into `Assets<Mesh>`.
pub(crate) struct TerrainPlan {
    pub mesh: Mesh,
    pub transform: Transform,
    pub name: String,
    pub splat: Option<SplatPlan>,
}

/// The cell's `VTEX` grid resolved to texture paths: what a `TerrainSplatMaterial`
/// binds, minus the asset loads themselves.
pub(crate) struct SplatPlan {
    /// The distinct layers' resolved VFS paths, at most
    /// [`MAX_TERRAIN_LAYERS`]; `None` for unresolvable textures (bound as a white
    /// stand-in at apply time).
    pub layers: Vec<Option<String>>,
    /// Layer slot per `VTEX` texel (the `decode_textures` ordering).
    pub indices: [u32; VTEX_GRID * VTEX_GRID],
}

/// The stand-in water plane's placement (see `spawn_water` in the `cell` module docs).
pub(crate) struct WaterPlan {
    pub center: Vec3,
    pub half_size: f32,
}

/// A diagnostic with a stable deduplication key: the same key warns once per app run,
/// not once per cell that trips it.
pub(crate) struct Warning {
    pub key: String,
    pub message: String,
}

/// Build the [`CellPlan`] for `cell_id`. `Err` when the cell doesn't exist in the load
/// order (the caller maps it to `CellSpawnFailed`).
pub(crate) fn build_cell(
    load_order: &TesLoadOrder,
    vfs: &TesVfs,
    cell_id: &CellId,
) -> Result<CellPlan, String> {
    let Some(cell) = load_order.cell(cell_id) else {
        return Err(format!("no such cell: {cell_id:?}"));
    };
    let mut builder = PlanBuilder {
        load_order,
        vfs,
        references: Vec::new(),
        skipped: 0,
        position_sum: Vec3::ZERO,
        warned: HashSet::new(),
        warnings: Vec::new(),
    };
    for reference in load_order.references(cell_id) {
        builder.plan_reference(reference);
    }

    let terrain = if cell.data.flags.contains(CellFlags::INTERIOR) {
        None
    } else {
        builder.plan_terrain(cell.data.grid_x, cell.data.grid_y)
    };
    let center = builder.position_sum / builder.references.len().max(1) as f32;
    let water = plan_water(cell, center, &terrain);

    Ok(CellPlan {
        references: builder.references,
        skipped: builder.skipped,
        moved_references: cell.moved_references.len(),
        terrain: terrain.map(|(plan, _)| plan),
        water,
        environment: environment(cell),
        warnings: builder.warnings,
    })
}

/// Accumulates the plan for one cell; the per-build counterpart of what used to be
/// interleaved with entity spawning.
struct PlanBuilder<'a> {
    load_order: &'a TesLoadOrder,
    vfs: &'a TesVfs,
    references: Vec<ReferencePlan>,
    skipped: usize,
    /// Sum of planned references' (Y-up) translations, for centring the water plane.
    position_sum: Vec3,
    /// Keys already warned about within this build (cross-cell dedup happens at apply).
    warned: HashSet<String>,
    warnings: Vec<Warning>,
}

impl PlanBuilder<'_> {
    fn warn_once(&mut self, key: String, message: String) {
        if self.warned.insert(key.clone()) {
            self.warnings.push(Warning { key, message });
        }
    }

    fn plan_reference(&mut self, reference: &Reference) {
        let object_id = reference.object.decode().into_owned();
        let Some(info) = self.load_order.object(&object_id) else {
            self.warn_once(
                object_id.clone(),
                format!("cell references unknown object id {object_id:?}"),
            );
            self.skipped += 1;
            return;
        };
        // Skinned models and runtime-resolved spawn points aren't supported yet; a
        // disabled reference is authored not to appear.
        let unsupported = matches!(
            info.kind(),
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

        let model_path = info.model().and_then(|model| {
            let decoded = model.decode();
            let resolved = self.vfs.resolve_model(&decoded);
            if resolved.is_none() {
                self.warn_once(
                    decoded.into_owned(),
                    format!("cannot resolve model {model} (for {object_id:?})"),
                );
            }
            resolved
        });

        let light = info.light().filter(|light| {
            !light
                .flags
                .intersects(LightFlags::NEGATIVE | LightFlags::OFF_BY_DEFAULT)
        });
        let light = light.map(|light| {
            let range = light.radius as f32 * convert::METERS_PER_UNIT;
            PointLight {
                color: Color::srgb_u8(light.color.r, light.color.g, light.color.b),
                intensity: LIGHT_INTENSITY_PER_METER_SQ * range * range,
                range,
                ..Default::default()
            }
        });

        self.references.push(ReferencePlan {
            id: reference.id,
            object: object_id,
            transform,
            model_path,
            light,
        });
    }

    /// The terrain plan and its minimum height in meters (which drives the sea-level
    /// water decision), when the cell's `LAND` record exists and has heights. Cells
    /// without `LAND` — map edges, sparse plugins — return `None` silently: that's
    /// authored absence, not an error.
    fn plan_terrain(&mut self, grid_x: i32, grid_y: i32) -> Option<(TerrainPlan, f32)> {
        let land = self.load_order.land(grid_x, grid_y)?;
        let mesh = convert::land_mesh(land)?;
        // The mesh's Y coordinates are the decoded heights in meters — fold the minimum
        // out of them rather than decoding the height grid a second time.
        let min = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)?
            .as_float3()?
            .iter()
            .fold(f32::INFINITY, |min, p| min.min(p[1]));

        let splat = land.decode_textures().map(|grid| {
            let mut slots: HashMap<u16, u32> = HashMap::new();
            let mut layers: Vec<Option<String>> = Vec::new();
            let mut indices = [0u32; VTEX_GRID * VTEX_GRID];
            for (texel, &value) in grid.iter().enumerate() {
                let slot = match slots.get(&value) {
                    Some(&slot) => slot,
                    None => {
                        let slot = if layers.len() < MAX_TERRAIN_LAYERS {
                            layers.push(self.layer_texture(value));
                            (layers.len() - 1) as u32
                        } else {
                            let message = format!(
                                "cell {grid_x},{grid_y} uses more than {MAX_TERRAIN_LAYERS} land textures"
                            );
                            self.warn_once(message.clone(), message);
                            0
                        };
                        slots.insert(value, slot);
                        slot
                    }
                };
                indices[texel] = slot;
            }
            SplatPlan { layers, indices }
        });

        Some((
            TerrainPlan {
                mesh,
                transform: convert::land_transform(grid_x, grid_y),
                name: format!("Terrain {grid_x},{grid_y}"),
                splat,
            },
            min,
        ))
    }

    /// Resolve one `VTEX` value to a texture path through `LTEX` and the VFS. `None`
    /// (with a warning) when it doesn't resolve, so one bad reference can't hold up the
    /// whole cell.
    fn layer_texture(&mut self, value: u16) -> Option<String> {
        let name = if value == 0 {
            // No explicit texture: the engine's hardcoded default.
            std::borrow::Cow::Borrowed("_land_default.tga")
        } else {
            match self.load_order.ltex(value as u32 - 1) {
                Some(ltex) => ltex.texture.decode(),
                None => {
                    let message = format!("no LTEX record with index {}", value - 1);
                    self.warn_once(message.clone(), message);
                    return None;
                }
            }
        };
        let resolved = self.vfs.resolve_texture(&name);
        if resolved.is_none() {
            let message = format!("land texture {name:?} not found in the VFS");
            self.warn_once(message.clone(), message);
        }
        resolved
    }
}

/// Plan the stand-in water plane for a cell:
///
/// - **Interior** with water: a large translucent sheet at the authored water height,
///   centred on the planned references (interior coordinates aren't origin-centred).
/// - **Exterior**: sea level is the implicit global height 0 — one cell-sized plane,
///   planned only when the cell has terrain that dips below it (inland cells skip the
///   hidden plane; neighbouring cells' planes tile seamlessly).
fn plan_water(
    cell: &Cell,
    center: Vec3,
    terrain: &Option<(TerrainPlan, f32)>,
) -> Option<WaterPlan> {
    if cell.data.flags.contains(CellFlags::INTERIOR) {
        let has_water =
            cell.data.flags.contains(CellFlags::HAS_WATER) || cell.water_height.is_some();
        if !has_water {
            return None;
        }
        let height = cell.water_height.unwrap_or(0.0) * convert::METERS_PER_UNIT;
        Some(WaterPlan {
            center: Vec3::new(center.x, height, center.z),
            half_size: convert::CELL_SIZE_METERS,
        })
    } else {
        if !terrain.as_ref().is_some_and(|(_, min)| *min < 0.0) {
            return None;
        }
        let half = convert::CELL_SIZE_METERS / 2.0;
        let corner = convert::land_transform(cell.data.grid_x, cell.data.grid_y).translation;
        Some(WaterPlan {
            center: corner + Vec3::new(half, 0.0, -half),
            half_size: half,
        })
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
        water_height: cell.water_height.map(|h| h * convert::METERS_PER_UNIT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tes_core::L1Str;
    use tes3_esm::records::cell::{CellData, ReferenceTransform};
    use tes3_esm::records::crea::Crea;
    use tes3_esm::records::ligh::{Ligh, LightData};
    use tes3_esm::records::stat::Stat;
    use tes3_esm::{Esm, EsmDirectory, Record};

    fn l1(s: &'static str) -> &'static L1Str {
        L1Str::from_bytes(s.as_bytes())
    }

    fn reference(id: u32, object: &'static str) -> Reference<'static> {
        Reference {
            id,
            object: l1(object),
            ..Default::default()
        }
    }

    /// A plugin with one interior cell (with water) exercising every planning path:
    /// a static, a light, a creature (skipped), and an unknown id (skipped + warned).
    fn synthetic_order() -> TesLoadOrder {
        let records = vec![
            Record::Stat(Stat {
                id: l1("t_stat"),
                model: l1(r"x\thing.nif"),
            }),
            Record::Ligh(Ligh {
                id: l1("t_light"),
                model: None,
                data: LightData {
                    radius: 256,
                    ..Default::default()
                },
                ..Default::default()
            }),
            Record::Crea(Crea {
                id: l1("t_rat"),
                model: l1(r"r\rat.nif"),
                ..Default::default()
            }),
            Record::Cell(Cell {
                name: l1("Test Cell"),
                data: CellData {
                    flags: CellFlags::INTERIOR | CellFlags::HAS_WATER,
                    ..Default::default()
                },
                water_height: Some(64.0),
                references: vec![
                    Reference {
                        transform: Some(ReferenceTransform {
                            position: [128.0, 0.0, 0.0],
                            rotation: [0.0; 3],
                        }),
                        ..reference(1, "t_stat")
                    },
                    reference(2, "t_light"),
                    reference(3, "t_rat"),
                    reference(4, "t_missing"),
                ],
                ..Default::default()
            }),
        ];
        TesLoadOrder::from_esms(vec![Esm::from_static(EsmDirectory {
            header: Default::default(),
            records,
        })])
    }

    #[test]
    fn plans_references_with_skips_and_warnings() {
        let order = synthetic_order();
        let plan = build_cell(&order, &TesVfs::empty(), &CellId::interior("test cell")).unwrap();

        // The creature and the unknown id are skipped; the static and light are planned.
        assert_eq!(plan.references.len(), 2);
        assert_eq!(plan.skipped, 2);

        let stat = &plan.references[0];
        assert_eq!((stat.id, stat.object.as_str()), (1, "t_stat"));
        // The empty VFS resolves nothing: no model path, but a warning keyed on the model.
        assert_eq!(stat.model_path, None);
        assert!(stat.light.is_none());
        assert!(
            plan.warnings.iter().any(|w| w.key == r"x\thing.nif"),
            "unresolvable model warns keyed on the model path"
        );
        assert!(
            plan.warnings.iter().any(|w| w.key == "t_missing"),
            "unknown object id warns keyed on the id"
        );

        let light = &plan.references[1];
        let point = light.light.as_ref().expect("light reference plans a light");
        let range = 256.0 * convert::METERS_PER_UNIT;
        assert!((point.range - range).abs() < 1e-6);
    }

    #[test]
    fn plans_interior_water_centred_on_references() {
        let order = synthetic_order();
        let plan = build_cell(&order, &TesVfs::empty(), &CellId::interior("Test Cell")).unwrap();

        assert!(plan.environment.interior);
        let water = plan.water.expect("interior with water plans a plane");
        // Water height is authored game units in meters; the center averages the two
        // planned references' translations ((128, 0, 0) and the origin, in game units).
        assert_eq!(water.half_size, convert::CELL_SIZE_METERS);
        assert!((water.center.y - 64.0 * convert::METERS_PER_UNIT).abs() < 1e-6);
        assert!((water.center.x - 64.0 * convert::METERS_PER_UNIT).abs() < 1e-6);
        // No LAND for an interior: no terrain plan.
        assert!(plan.terrain.is_none());
    }

    #[test]
    fn missing_cell_is_an_error() {
        let order = synthetic_order();
        let Err(err) = build_cell(&order, &TesVfs::empty(), &CellId::exterior(9, 9)) else {
            panic!("an unauthored grid must not build a plan");
        };
        assert!(err.contains("no such cell"), "{err}");
    }
}
