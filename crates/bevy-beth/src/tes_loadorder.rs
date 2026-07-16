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
use tes_core::{L1Str, TesId, TesPath};
use tes3_esm::records::cell::{Cell, CellFlags, Reference};
use tes3_esm::records::land::Land;
use tes3_esm::records::ligh::LightData;
use tes3_esm::records::ltex::Ltex;
use tes3_esm::records::{
    acti::Acti, alch::Alch, appa::Appa, armo::Armo, body::Body, book::Book, clot::Clot, cont::Cont,
    crea::Crea, door::Door, ingr::Ingr, levc::Levc, levi::Levi, ligh::Ligh, lock::Lock, misc::Misc,
    npc::Npc, prob::Prob, repa::Repa, stat::Stat, weap::Weap,
};
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

/// What a cell reference's editor id resolves to: a borrowed view of the whole record
/// that defined it (in whichever plugin defined it last).
///
/// Fields shared across record types have accessors ([`id`](ObjectRef::id),
/// [`kind`](ObjectRef::kind), [`model`](ObjectRef::model), [`light`](ObjectRef::light));
/// anything record-specific — a door's destinations, a container's inventory, a leveled
/// list's entries — is reached by matching the variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectRef<'a> {
    Static(&'a Stat<'a>),
    Activator(&'a Acti<'a>),
    Container(&'a Cont<'a>),
    Door(&'a Door<'a>),
    Light(&'a Ligh<'a>),
    Misc(&'a Misc<'a>),
    Weapon(&'a Weap<'a>),
    Armor(&'a Armo<'a>),
    Clothing(&'a Clot<'a>),
    Book(&'a Book<'a>),
    Ingredient(&'a Ingr<'a>),
    Potion(&'a Alch<'a>),
    Apparatus(&'a Appa<'a>),
    Lockpick(&'a Lock<'a>),
    Probe(&'a Prob<'a>),
    Repair(&'a Repa<'a>),
    Creature(&'a Crea<'a>),
    Npc(&'a Npc<'a>),
    BodyPart(&'a Body<'a>),
    LeveledCreature(&'a Levc<'a>),
    LeveledItem(&'a Levi<'a>),
}

impl<'a> ObjectRef<'a> {
    /// The fields every placeable record shares, extracted in one dispatch: editor id,
    /// kind tag, and the raw `MODL` value (absent on leveled lists, optional on a few
    /// record types).
    fn parts(self) -> (&'a L1Str, ObjectKind, Option<&'a L1Str>) {
        match self {
            ObjectRef::Static(r) => (r.id, ObjectKind::Static, Some(r.model)),
            ObjectRef::Activator(r) => (r.id, ObjectKind::Activator, Some(r.model)),
            ObjectRef::Container(r) => (r.id, ObjectKind::Container, Some(r.model)),
            ObjectRef::Door(r) => (r.id, ObjectKind::Door, Some(r.model)),
            ObjectRef::Light(r) => (r.id, ObjectKind::Light, r.model),
            ObjectRef::Misc(r) => (r.id, ObjectKind::Misc, Some(r.model)),
            ObjectRef::Weapon(r) => (r.id, ObjectKind::Weapon, Some(r.model)),
            ObjectRef::Armor(r) => (r.id, ObjectKind::Armor, Some(r.model)),
            ObjectRef::Clothing(r) => (r.id, ObjectKind::Clothing, Some(r.model)),
            ObjectRef::Book(r) => (r.id, ObjectKind::Book, Some(r.model)),
            ObjectRef::Ingredient(r) => (r.id, ObjectKind::Ingredient, Some(r.model)),
            ObjectRef::Potion(r) => (r.id, ObjectKind::Potion, r.model),
            ObjectRef::Apparatus(r) => (r.id, ObjectKind::Apparatus, r.model),
            ObjectRef::Lockpick(r) => (r.id, ObjectKind::Lockpick, Some(r.model)),
            ObjectRef::Probe(r) => (r.id, ObjectKind::Probe, Some(r.model)),
            ObjectRef::Repair(r) => (r.id, ObjectKind::Repair, Some(r.model)),
            ObjectRef::Creature(r) => (r.id, ObjectKind::Creature, Some(r.model)),
            ObjectRef::Npc(r) => (r.id, ObjectKind::Npc, r.model),
            ObjectRef::BodyPart(r) => (r.id, ObjectKind::BodyPart, Some(r.model)),
            ObjectRef::LeveledCreature(r) => (r.id, ObjectKind::LeveledCreature, None),
            ObjectRef::LeveledItem(r) => (r.id, ObjectKind::LeveledItem, None),
        }
    }

    /// The record's editor id, as authored.
    pub fn id(self) -> &'a L1Str {
        self.parts().0
    }

    /// The record kind, as a plain tag for skip lists and diagnostics.
    pub fn kind(self) -> ObjectKind {
        self.parts().1
    }

    /// Model path as authored — a [`TesPath`] view borrowing the record's `MODL` bytes,
    /// so it compares and hashes case- and separator-insensitively (e.g.
    /// `F\Act_BM_FireLake00.NIF`), relative to `meshes\`. `None` for model-less records
    /// and empty `MODL` subrecords.
    pub fn model(self) -> Option<&'a TesPath> {
        model_path(self.parts().2)
    }

    /// The light parameters, for [`ObjectRef::Light`] records only.
    pub fn light(self) -> Option<&'a LightData> {
        match self {
            ObjectRef::Light(r) => Some(&r.data),
            _ => None,
        }
    }
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
    /// Editor id → the defining record. Keys borrow the record id bytes and fold case,
    /// so lookups need no allocation.
    objects: HashMap<&'a TesId, ObjectRef<'a>>,
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
    pub fn object(&self, id: &str) -> Option<ObjectRef<'_>> {
        self.table()
            .objects
            .get(TesId::from_bytes(id.as_bytes()))
            .copied()
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

    /// The placed references for a cell, in authored order. Empty for unknown cells.
    ///
    /// Consumers should iterate this rather than reaching into [`Cell::references`]:
    /// once exterior-reference accumulation across plugins lands (`MVRF`/`MOVE`), a
    /// grid's references will merge from several `CELL` records, and only this accessor
    /// will reflect that.
    pub fn references<'s>(&'s self, id: &CellId) -> impl Iterator<Item = &'s Reference<'s>> {
        self.cell(id)
            .map(|cell| cell.references.iter())
            .into_iter()
            .flatten()
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
                Record::Stat(r) => table.insert_object(ObjectRef::Static(r)),
                Record::Acti(r) => table.insert_object(ObjectRef::Activator(r)),
                Record::Cont(r) => table.insert_object(ObjectRef::Container(r)),
                Record::Door(r) => table.insert_object(ObjectRef::Door(r)),
                Record::Ligh(r) => table.insert_object(ObjectRef::Light(r)),
                Record::Misc(r) => table.insert_object(ObjectRef::Misc(r)),
                Record::Weap(r) => table.insert_object(ObjectRef::Weapon(r)),
                Record::Armo(r) => table.insert_object(ObjectRef::Armor(r)),
                Record::Clot(r) => table.insert_object(ObjectRef::Clothing(r)),
                Record::Book(r) => table.insert_object(ObjectRef::Book(r)),
                Record::Ingr(r) => table.insert_object(ObjectRef::Ingredient(r)),
                Record::Alch(r) => table.insert_object(ObjectRef::Potion(r)),
                Record::Appa(r) => table.insert_object(ObjectRef::Apparatus(r)),
                Record::Lock(r) => table.insert_object(ObjectRef::Lockpick(r)),
                Record::Prob(r) => table.insert_object(ObjectRef::Probe(r)),
                Record::Repa(r) => table.insert_object(ObjectRef::Repair(r)),
                Record::Crea(r) => table.insert_object(ObjectRef::Creature(r)),
                Record::Npc(r) => table.insert_object(ObjectRef::Npc(r)),
                Record::Body(r) => table.insert_object(ObjectRef::BodyPart(r)),
                // Leveled lists are indexed too (not yet spawnable), so a skipped
                // reference resolves as "leveled list" rather than "unknown id".
                Record::Levc(r) => table.insert_object(ObjectRef::LeveledCreature(r)),
                Record::Levi(r) => table.insert_object(ObjectRef::LeveledItem(r)),
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

impl<'a> TesLoadOrderTable<'a> {
    fn insert_object(&mut self, object: ObjectRef<'a>) {
        self.objects.insert(TesId::new(object.id()), object);
    }
}

/// View a `MODL` value as a [`TesPath`] for VFS lookup; empty strings (some records carry
/// an empty subrecord) become `None`. Case/separator folding is deferred to `TesPath`, so
/// this neither copies nor normalizes.
fn model_path(model: Option<&L1Str>) -> Option<&TesPath> {
    let model = model?;
    if model.is_empty() {
        return None;
    }
    Some(TesPath::new(model))
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
                    references: vec![Reference {
                        id: 1,
                        object: l1("T_Stat"),
                        ..Default::default()
                    }],
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
        assert_eq!(stat.kind(), ObjectKind::Static);
        let model = stat.model().expect("stat has a model");
        // The view borrows the original MODL bytes unchanged (no normalizing copy)...
        assert_eq!(model.as_bytes(), br"F\Furn_Thing.NIF");
        // ...but as a TesPath it still matches case- and separator-folded.
        assert_eq!(model, TesPath::from_bytes(br"f/furn_thing.nif"));
        assert_eq!(stat.light(), None);

        let light = order.object("Light_Fire").expect("light by mixed-case id");
        assert_eq!(light.kind(), ObjectKind::Light);
        assert_eq!(light.model(), None, "empty MODL must not become a path");
        assert_eq!(light.light().unwrap().radius, 256);

        // Matching the variant reaches the whole defining record, not a projection.
        let rat = order.object("Rat_Cave").expect("creature by mixed-case id");
        assert_eq!(rat.kind(), ObjectKind::Creature);
        let ObjectRef::Creature(crea) = rat else {
            panic!("expected a creature record, got {rat:?}");
        };
        assert_eq!(crea.id.decode(), "rat_cave");

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
    fn references_iterate_through_the_table() {
        let order = synthetic_order();

        let refs: Vec<_> = order.references(&CellId::interior("tEsT cElL")).collect();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].object.decode(), "T_Stat");

        assert_eq!(order.references(&CellId::interior("nowhere")).count(), 0);
        assert_eq!(order.references(&CellId::exterior(99, 99)).count(), 0);
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
            order
                .object("shared_stat")
                .unwrap()
                .model()
                .unwrap()
                .as_bytes(),
            br"b\two.nif",
            "later plugin's definition should win"
        );
    }
}
