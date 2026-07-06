//! Lookup structures over a parsed plugin: editor id → placeable object, and cell
//! name/grid → `CELL` record.
//!
//! A plugin stores records as a flat, file-ordered list; placing a cell requires the
//! reverse mappings (a cell reference names its object by editor id, a cell is named by
//! interior name or exterior grid). [`EsmIndex::build`] derives both in one pass —
//! the ESM loader runs it once per plugin so systems get O(1) lookups.
//!
//! Editor ids and cell names are case-insensitive, matching the game.

use std::collections::HashMap;

use tes3_esm::records::cell::Cell;
use tes3_esm::records::cell::CellFlags;
use tes3_esm::records::ligh::LightData;
use tes3_esm::{L1Str, Plugin, Record};

/// Identifies a cell within a plugin.
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

/// Lookups over a parsed [`Plugin`]: editor id → [`ObjectInfo`], and [`CellId`] →
/// `CELL` record. Built once by the ESM loader; see the [module docs](self).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EsmIndex {
    /// Lowercased editor id → object info.
    objects: HashMap<String, ObjectInfo>,
    /// Lowercased interior cell name → index into `plugin.records`.
    interiors: HashMap<String, usize>,
    /// Exterior grid → index into `plugin.records`.
    exteriors: HashMap<(i32, i32), usize>,
}

impl EsmIndex {
    /// Build the index in one pass over the plugin's records. Later records win on id
    /// collision, mirroring the game's last-definition-wins rule within a file.
    pub fn build(plugin: &Plugin) -> EsmIndex {
        let mut index = EsmIndex::default();
        for (i, record) in plugin.records.iter().enumerate() {
            match record {
                Record::Cell(cell) => {
                    if cell.data.flags.contains(CellFlags::INTERIOR) {
                        index.interiors.insert(lower(&cell.name), i);
                    } else {
                        index
                            .exteriors
                            .insert((cell.data.grid_x, cell.data.grid_y), i);
                    }
                }
                Record::Stat(r) => index.object_entry(&r.id, ObjectKind::Static, Some(&r.model)),
                Record::Acti(r) => index.object_entry(&r.id, ObjectKind::Activator, Some(&r.model)),
                Record::Cont(r) => index.object_entry(&r.id, ObjectKind::Container, Some(&r.model)),
                Record::Door(r) => index.object_entry(&r.id, ObjectKind::Door, Some(&r.model)),
                Record::Misc(r) => index.object_entry(&r.id, ObjectKind::Misc, Some(&r.model)),
                Record::Weap(r) => index.object_entry(&r.id, ObjectKind::Weapon, Some(&r.model)),
                Record::Armo(r) => index.object_entry(&r.id, ObjectKind::Armor, Some(&r.model)),
                Record::Clot(r) => index.object_entry(&r.id, ObjectKind::Clothing, Some(&r.model)),
                Record::Book(r) => index.object_entry(&r.id, ObjectKind::Book, Some(&r.model)),
                Record::Ingr(r) => {
                    index.object_entry(&r.id, ObjectKind::Ingredient, Some(&r.model))
                }
                Record::Lock(r) => index.object_entry(&r.id, ObjectKind::Lockpick, Some(&r.model)),
                Record::Prob(r) => index.object_entry(&r.id, ObjectKind::Probe, Some(&r.model)),
                Record::Repa(r) => index.object_entry(&r.id, ObjectKind::Repair, Some(&r.model)),
                Record::Crea(r) => index.object_entry(&r.id, ObjectKind::Creature, Some(&r.model)),
                Record::Body(r) => index.object_entry(&r.id, ObjectKind::BodyPart, Some(&r.model)),
                Record::Alch(r) => {
                    index.object_entry(&r.id, ObjectKind::Potion, r.model.as_deref())
                }
                Record::Appa(r) => {
                    index.object_entry(&r.id, ObjectKind::Apparatus, r.model.as_deref())
                }
                Record::Npc(r) => index.object_entry(&r.id, ObjectKind::Npc, r.model.as_deref()),
                Record::Ligh(r) => {
                    index.objects.insert(
                        lower(&r.id),
                        ObjectInfo {
                            kind: ObjectKind::Light,
                            model: model_path(r.model.as_deref()),
                            light: Some(r.data),
                        },
                    );
                }
                // Indexed by kind only, so a skipped leveled-list reference logs as
                // "leveled list" rather than "unknown id".
                Record::Levi(r) => index.object_entry(&r.id, ObjectKind::LeveledItem, None),
                Record::Levc(r) => index.object_entry(&r.id, ObjectKind::LeveledCreature, None),
                _ => {}
            }
        }
        index
    }

    /// Look up a placeable object by editor id (any case).
    pub fn object(&self, id: &str) -> Option<&ObjectInfo> {
        self.objects.get(&id.to_lowercase())
    }

    /// Look up a cell record by id (interior names match case-insensitively).
    pub fn cell<'p>(&self, plugin: &'p Plugin, id: &CellId) -> Option<&'p Cell> {
        let i = match id {
            CellId::Interior(name) => *self.interiors.get(&name.to_lowercase())?,
            CellId::Exterior { x, y } => *self.exteriors.get(&(*x, *y))?,
        };
        match plugin.records.get(i) {
            Some(Record::Cell(cell)) => Some(cell),
            _ => None,
        }
    }

    fn object_entry(&mut self, id: &L1Str, kind: ObjectKind, model: Option<&L1Str>) {
        self.objects.insert(
            lower(id),
            ObjectInfo {
                kind,
                model: model_path(model),
                light: None,
            },
        );
    }
}

fn lower(s: &L1Str) -> String {
    s.decode().to_lowercase()
}

/// Normalize a `MODL` value for VFS lookup; empty strings (some records carry an empty
/// subrecord) become `None`.
fn model_path(model: Option<&L1Str>) -> Option<String> {
    let model = model?;
    if model.is_empty() {
        return None;
    }
    Some(tes_core::paths::normalize(&model.decode()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tes3_esm::L1String;
    use tes3_esm::records::cell::{Cell, CellData};
    use tes3_esm::records::crea::Crea;
    use tes3_esm::records::ligh::Ligh;
    use tes3_esm::records::stat::Stat;

    fn l1(s: &str) -> L1String {
        L1String::from_bytes(s.as_bytes().to_vec())
    }

    fn synthetic_plugin() -> Plugin {
        Plugin {
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
            ],
        }
    }

    #[test]
    fn objects_resolve_case_insensitively_with_normalized_models() {
        let plugin = synthetic_plugin();
        let index = EsmIndex::build(&plugin);

        let stat = index.object("T_STAT").expect("stat by upper-cased id");
        assert_eq!(stat.kind, ObjectKind::Static);
        assert_eq!(stat.model.as_deref(), Some(r"f\furn_thing.nif"));
        assert_eq!(stat.light, None);

        let light = index.object("Light_Fire").expect("light by mixed-case id");
        assert_eq!(light.kind, ObjectKind::Light);
        assert_eq!(light.model, None, "empty MODL must not become a path");
        assert_eq!(light.light.unwrap().radius, 256);

        assert_eq!(index.object("Rat_Cave").unwrap().kind, ObjectKind::Creature);
        assert_eq!(index.object("nowhere"), None);
    }

    #[test]
    fn cells_resolve_by_name_and_grid() {
        let plugin = synthetic_plugin();
        let index = EsmIndex::build(&plugin);

        let interior = index
            .cell(&plugin, &CellId::interior("tEsT cElL"))
            .expect("interior by case-mismatched name");
        assert_eq!(interior.name.decode(), "Test Cell");

        let exterior = index
            .cell(&plugin, &CellId::exterior(-3, 12))
            .expect("exterior by grid");
        assert_eq!(exterior.data.grid_y, 12);

        assert!(index.cell(&plugin, &CellId::interior("nowhere")).is_none());
        assert!(index.cell(&plugin, &CellId::exterior(99, 99)).is_none());
    }
}
