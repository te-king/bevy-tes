//! A load order: several plugins parsed together, with lookup tables over their merged
//! records.
//!
//! A plugin stores records as a flat, file-ordered list; placing a cell needs the reverse
//! mappings (a cell reference names its object by editor id; a cell is named by interior
//! name or exterior grid). [`TesLoadOrder`] owns the parsed [`Esm`]s and, beside them,
//! a table whose entries **borrow** those records directly — so a lookup hands back a
//! `&Cell`/`&Land`/`&Ltex` with no index indirection and no `esm` argument to thread
//! through.
//!
//! It is a self-referential value (mirroring [`Esm`] and [`tes3_bsa::Bsa`]): the plugin
//! buffers and the references into them travel together in one owned value. Records are
//! merged in load order — later plugins win on collision, matching the game's
//! last-definition-wins rule — though exterior-reference accumulation across plugins
//! (the `MVRF`/`MOVE` machinery) is still future work; today a whole `CELL` replaces the
//! earlier one for its grid.
//!
//! Editor ids and cell names are case-insensitive (via [`TesId`]), matching the game.

use std::collections::HashMap;

use self_cell::self_cell;
use tes_core::{L1Str, TesId};
use tes3_esm::records::cell::{Cell, CellFlags};
use tes3_esm::records::land::Land;
use tes3_esm::records::ligh::LightData;
use tes3_esm::records::ltex::Ltex;
use tes3_esm::{Esm, Record};

/// Identifies a cell within a load order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellId {
    /// An interior cell by name, e.g. `Balmora, Guild of Mages`. Matched
    /// case-insensitively (Morrowind ids are case-insensitive text).
    Interior(String),
    /// An exterior cell by grid coordinates.
    Exterior { x: i32, y: i32 },
}

impl CellId {
    /// An interior cell id, by name.
    pub fn interior(name: impl Into<String>) -> CellId {
        CellId::Interior(name.into())
    }

    /// An exterior cell id, by grid coordinates.
    pub fn exterior(x: i32, y: i32) -> CellId {
        CellId::Exterior { x, y }
    }
}

/// The category of a placeable object record — the record type its editor id was
/// found on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Static,
    Activator,
    Container,
    Door,
    Light,
    Misc,
    Weapon,
    Armor,
    Clothing,
    Book,
    Ingredient,
    Potion,
    Apparatus,
    Lockpick,
    Probe,
    Repair,
    Creature,
    Npc,
    BodyPart,
    LeveledCreature,
    LeveledItem,
}

/// What a cell reference's editor id resolves to: the record kind, its model path, and
/// (for lights) the light parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    pub kind: ObjectKind,
    /// Model path as authored, normalized (lowercase, `\` separators), relative to
    /// `meshes\` — e.g. `f\act_bm_firelake00.nif`. `None` for model-less records.
    pub model: Option<String>,
    /// Present only for [`ObjectKind::Light`].
    pub light: Option<LightData>,
}

/// A parsed load order: the owned plugin buffers plus a lookup table borrowing their
/// records. Built once (e.g. by the ESM loader); see the [module docs](self).
pub struct TesLoadOrder {
    inner: TesLoadOrderInternal,
}

self_cell!(
    struct TesLoadOrderInternal {
        owner: Box<[Esm]>,

        #[covariant]
        dependent: TesLoadOrderTable,
    }
);

/// Lookups over the merged records, each borrowing directly from the owning [`Esm`]s.
/// Later plugins overwrite earlier ones on key collision (last-definition-wins).
#[derive(Default)]
struct TesLoadOrderTable<'a> {
    /// Editor id → object info. Keys borrow the record id bytes and fold case, so lookups
    /// need no allocation.
    objects: HashMap<&'a TesId, ObjectInfo>,
    /// Interior cell name → its `CELL` record. Case-folded key.
    interiors: HashMap<&'a TesId, &'a Cell<'a>>,
    /// Exterior grid → its `CELL` record.
    exteriors: HashMap<(i32, i32), &'a Cell<'a>>,
    /// Exterior grid → the cell's `LAND` record.
    lands: HashMap<(i32, i32), &'a Land<'a>>,
    /// `LTEX` texture index (`INTV`) → its `LTEX` record. What a LAND `VTEX` value − 1
    /// refers to.
    ltexs: HashMap<u32, &'a Ltex<'a>>,
}

impl TesLoadOrder {
    /// Build a load order from already-parsed plugins, in load order (earliest first).
    /// The plugins are moved in and their records borrowed; nothing is copied out.
    pub fn from_esms(esms: Vec<Esm>) -> TesLoadOrder {
        let owner = esms.into_boxed_slice();
        let inner = TesLoadOrderInternal::new(owner, |esms| build_table(esms));
        TesLoadOrder { inner }
    }

    /// The parsed plugins in load order.
    pub fn esms(&self) -> &[Esm] {
        self.inner.borrow_owner()
    }

    /// Look up a placeable object by editor id (any case).
    pub fn object(&self, id: &str) -> Option<&ObjectInfo> {
        self.table().objects.get(TesId::from_bytes(id.as_bytes()))
    }

    /// Look up a cell record by id (interior names match case-insensitively).
    pub fn cell(&self, id: &CellId) -> Option<&Cell<'_>> {
        let table = self.table();
        match id {
            CellId::Interior(name) => table
                .interiors
                .get(TesId::from_bytes(name.as_bytes()))
                .copied(),
            CellId::Exterior { x, y } => table.exteriors.get(&(*x, *y)).copied(),
        }
    }

    /// Look up an exterior cell's `LAND` record by grid coordinates.
    pub fn land(&self, x: i32, y: i32) -> Option<&Land<'_>> {
        self.table().lands.get(&(x, y)).copied()
    }

    /// Look up a landscape texture by its `LTEX` index (what a LAND `VTEX` value − 1
    /// refers to).
    pub fn ltex(&self, index: u32) -> Option<&Ltex<'_>> {
        self.table().ltexs.get(&index).copied()
    }

    fn table(&self) -> &TesLoadOrderTable<'_> {
        self.inner.borrow_dependent()
    }
}

// Manual: self_cell's generated Debug would print the raw plugin bytes.
impl std::fmt::Debug for TesLoadOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TesLoadOrder")
            .field("plugins", &self.esms().len())
            .finish()
    }
}

/// Build the lookup table in one pass over every plugin's records, earliest first, so
/// later plugins overwrite earlier ones on id/grid collision.
fn build_table(esms: &[Esm]) -> TesLoadOrderTable<'_> {
    let mut table = TesLoadOrderTable::default();
    for esm in esms {
        for record in &esm.directory().records {
            match record {
                Record::Cell(cell) => {
                    if cell.data.flags.contains(CellFlags::INTERIOR) {
                        table.interiors.insert(TesId::new(cell.name), cell);
                    } else {
                        table
                            .exteriors
                            .insert((cell.data.grid_x, cell.data.grid_y), cell);
                    }
                }
                Record::Stat(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Static, Some(r.model))
                }
                Record::Acti(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Activator, Some(r.model))
                }
                Record::Cont(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Container, Some(r.model))
                }
                Record::Door(r) => object_entry(&mut table, r.id, ObjectKind::Door, Some(r.model)),
                Record::Misc(r) => object_entry(&mut table, r.id, ObjectKind::Misc, Some(r.model)),
                Record::Weap(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Weapon, Some(r.model))
                }
                Record::Armo(r) => object_entry(&mut table, r.id, ObjectKind::Armor, Some(r.model)),
                Record::Clot(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Clothing, Some(r.model))
                }
                Record::Book(r) => object_entry(&mut table, r.id, ObjectKind::Book, Some(r.model)),
                Record::Ingr(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Ingredient, Some(r.model))
                }
                Record::Lock(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Lockpick, Some(r.model))
                }
                Record::Prob(r) => object_entry(&mut table, r.id, ObjectKind::Probe, Some(r.model)),
                Record::Repa(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Repair, Some(r.model))
                }
                Record::Crea(r) => {
                    object_entry(&mut table, r.id, ObjectKind::Creature, Some(r.model))
                }
                Record::Body(r) => {
                    object_entry(&mut table, r.id, ObjectKind::BodyPart, Some(r.model))
                }
                Record::Alch(r) => object_entry(&mut table, r.id, ObjectKind::Potion, r.model),
                Record::Appa(r) => object_entry(&mut table, r.id, ObjectKind::Apparatus, r.model),
                Record::Npc(r) => object_entry(&mut table, r.id, ObjectKind::Npc, r.model),
                Record::Ligh(r) => {
                    table.objects.insert(
                        TesId::new(r.id),
                        ObjectInfo {
                            kind: ObjectKind::Light,
                            model: model_path(r.model),
                            light: Some(r.data),
                        },
                    );
                }
                // Indexed by kind only, so a skipped leveled-list reference logs as
                // "leveled list" rather than "unknown id".
                Record::Levi(r) => object_entry(&mut table, r.id, ObjectKind::LeveledItem, None),
                Record::Levc(r) => {
                    object_entry(&mut table, r.id, ObjectKind::LeveledCreature, None)
                }
                Record::Land(land) => {
                    table.lands.insert((land.grid_x, land.grid_y), land);
                }
                Record::Ltex(ltex) => {
                    table.ltexs.insert(ltex.index, ltex);
                }
                _ => {}
            }
        }
    }
    table
}

fn object_entry<'a>(
    table: &mut TesLoadOrderTable<'a>,
    id: &'a L1Str,
    kind: ObjectKind,
    model: Option<&'a L1Str>,
) {
    table.objects.insert(
        TesId::new(id),
        ObjectInfo {
            kind,
            model: model_path(model),
            light: None,
        },
    );
}

/// Normalize a `MODL` value for VFS lookup; empty strings (some records carry an empty
/// subrecord) become `None`.
fn model_path(model: Option<&L1Str>) -> Option<String> {
    let model = model?;
    if model.is_empty() {
        return None;
    }
    Some(tes_core::tes_path::normalize(&model.decode()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tes3_esm::L1Str;
    use tes3_esm::records::cell::{Cell, CellData};
    use tes3_esm::records::crea::Crea;
    use tes3_esm::records::ligh::Ligh;
    use tes3_esm::records::stat::Stat;
    use tes3_esm::{EsmDirectory, Record};

    fn l1(s: &'static str) -> &'static L1Str {
        L1Str::from_bytes(s.as_bytes())
    }

    fn synthetic_plugin() -> EsmDirectory<'static> {
        EsmDirectory {
            header: Default::default(),
            records: vec![
                Record::Stat(Stat {
                    id: l1("T_Stat"),
                    model: l1(r"F\Furn_Thing.NIF"),
                }),
                Record::Ligh(Ligh {
                    id: l1("light_fire"),
                    model: None,
                    data: LightData {
                        radius: 256,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                Record::Crea(Crea {
                    id: l1("rat_cave"),
                    model: l1(r"r\Rat.NIF"),
                    ..Default::default()
                }),
                Record::Cell(Cell {
                    name: l1("Test Cell"),
                    data: CellData {
                        flags: CellFlags::INTERIOR,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                Record::Cell(Cell {
                    data: CellData {
                        flags: CellFlags::empty(),
                        grid_x: -3,
                        grid_y: 12,
                    },
                    ..Default::default()
                }),
                Record::Land(Land {
                    grid_x: -3,
                    grid_y: 12,
                    ..Default::default()
                }),
                Record::Ltex(Ltex {
                    id: l1("Sand"),
                    index: 2,
                    texture: l1("Tx_Sand_01.tga"),
                }),
            ],
        }
    }

    fn synthetic_order() -> TesLoadOrder {
        TesLoadOrder::from_esms(vec![Esm::from_static(synthetic_plugin())])
    }

    #[test]
    fn objects_resolve_case_insensitively_with_normalized_models() {
        let order = synthetic_order();

        let stat = order.object("T_STAT").expect("stat by upper-cased id");
        assert_eq!(stat.kind, ObjectKind::Static);
        assert_eq!(stat.model.as_deref(), Some(r"f\furn_thing.nif"));
        assert_eq!(stat.light, None);

        let light = order.object("Light_Fire").expect("light by mixed-case id");
        assert_eq!(light.kind, ObjectKind::Light);
        assert_eq!(light.model, None, "empty MODL must not become a path");
        assert_eq!(light.light.unwrap().radius, 256);

        assert_eq!(order.object("Rat_Cave").unwrap().kind, ObjectKind::Creature);
        assert_eq!(order.object("nowhere"), None);
    }

    #[test]
    fn cells_resolve_by_name_and_grid() {
        let order = synthetic_order();

        let interior = order
            .cell(&CellId::interior("tEsT cElL"))
            .expect("interior by case-mismatched name");
        assert_eq!(interior.name.decode(), "Test Cell");

        let exterior = order
            .cell(&CellId::exterior(-3, 12))
            .expect("exterior by grid");
        assert_eq!(exterior.data.grid_y, 12);

        assert!(order.cell(&CellId::interior("nowhere")).is_none());
        assert!(order.cell(&CellId::exterior(99, 99)).is_none());
    }

    #[test]
    fn lands_resolve_by_grid() {
        let order = synthetic_order();

        let land = order.land(-3, 12).expect("land by grid");
        assert_eq!((land.grid_x, land.grid_y), (-3, 12));
        assert!(order.land(99, 99).is_none());
    }

    #[test]
    fn ltexs_resolve_by_texture_index() {
        let order = synthetic_order();

        let ltex = order.ltex(2).expect("ltex by INTV index");
        assert_eq!(ltex.texture.decode(), "Tx_Sand_01.tga");
        assert!(order.ltex(99).is_none());
    }

    #[test]
    fn later_plugin_overrides_earlier() {
        // Two plugins define the same static id; the later one in load order wins.
        let earlier = EsmDirectory {
            header: Default::default(),
            records: vec![Record::Stat(Stat {
                id: l1("shared_stat"),
                model: l1(r"a\one.nif"),
            })],
        };
        let later = EsmDirectory {
            header: Default::default(),
            records: vec![Record::Stat(Stat {
                id: l1("Shared_Stat"),
                model: l1(r"b\two.nif"),
            })],
        };
        let order =
            TesLoadOrder::from_esms(vec![Esm::from_static(earlier), Esm::from_static(later)]);

        assert_eq!(
            order.object("shared_stat").unwrap().model.as_deref(),
            Some(r"b\two.nif"),
            "later plugin's definition should win"
        );
    }
}
