//! Bevy integration for Bethesda Creation Engine (TES3 / Morrowind) data files.
//!
//! [`TesPlugin`] registers a **`tes://` asset source** backed by [`TesVfs`] — a layered,
//! case-insensitive view of a Morrowind `Data Files` directory where loose files override
//! BSA archives — plus [`AssetLoader`]s for the game's formats. Once the plugin is added,
//! any file the game could resolve is loadable through the regular
//! [`AssetServer`], whether it exists loose on disk or only inside an archive:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_tes::{TesPlugin, LoadOrderAsset, NifAsset};
//!
//! fn load(asset_server: Res<AssetServer>) {
//!     let _esm: Handle<LoadOrderAsset> = asset_server.load("tes://Morrowind.esm");
//!     let _nif: Handle<NifAsset> = asset_server.load("tes://meshes/i/in_de_shack_01.nif");
//! }
//!
//! App::new()
//!     // TesPlugin FIRST: asset sources must be registered before AssetPlugin
//!     // (part of DefaultPlugins) builds the AssetServer.
//!     .add_plugins((TesPlugin::new("data"), DefaultPlugins))
//!     .add_systems(Startup, load)
//!     .run();
//! ```
//!
//! With the `scene` feature (on by default), a NIF load additionally emits
//! spawnable content as labeled sub-assets — `Mesh`es, `StandardMaterial`s (their
//! textures resolved through the VFS, so archive-only textures work), and a
//! `WorldAsset` scene preserving the model's node hierarchy:
//!
//! ```ignore
//! // (requires the `scene` feature)
//! use bevy::world_serialization::WorldAssetRoot;
//!
//! fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     commands.spawn(WorldAssetRoot(
//!         asset_server.load("tes://meshes/i/in_de_shack_01.nif#Scene"),
//!     ));
//! }
//! ```
//!
//! A full **load order** — several plugins merged so later ones override earlier — is
//! configured on the plugin itself:
//! `TesPlugin::new("data").with_plugins(["Morrowind.esm", "Tribunal.esm"])` parses the
//! listed files during startup and shares the result as the [`LoadOrderHandle`]
//! resource ([`read_load_order`] fills the list from a plain-text file).
//!
//! Whole **cells** (interiors or exterior grid squares) spawn the same way from a loaded
//! load order — one child entity per placed object, each loading its own NIF scene (see
//! [`cell`]). Everything spawns Y-up and **in meters** — game-frame axes and game units
//! convert exactly once, at the [`convert`] boundary (see
//! [`METERS_PER_UNIT`](convert::METERS_PER_UNIT)):
//!
//! ```ignore
//! use bevy_tes::{CellId, CellSeed};
//!
//! fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     commands.spawn(CellSeed {
//!         load_order: asset_server.load("tes://Morrowind.esm"),
//!         cell: CellId::interior("Balmora, Guild of Mages"),
//!     });
//! }
//! ```
//!
//! # Plugin ordering
//!
//! `TesPlugin` **must be added before** Bevy's `AssetPlugin` (i.e. before
//! `DefaultPlugins`): Bevy requires custom asset sources to be registered before the
//! `AssetServer` is built. The plugin asserts this at startup. Headless apps that build
//! the `App` manually must also call `app.finish()` for loaders to be registered.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::app::{App, Plugin};
use bevy::asset::io::{AssetSourceBuilder, AssetSourceId, Reader};
use bevy::asset::{Asset, AssetApp, AssetLoader, AssetServer, Assets, Handle, LoadContext};
use bevy::ecs::resource::Resource;
use bevy::reflect::TypePath;

use tes_nif::{Nif, NifError};
use tes3_esm::records::cell::{Cell, Reference};
use tes3_esm::records::land::Land;
use tes3_esm::records::ltex::Ltex;
use tes3_esm::{Esm, EsmDirectory, EsmError};

pub mod tes_loadorder;
pub mod tes_vfs;

#[cfg(feature = "scene")]
pub mod cell;
#[cfg(feature = "scene")]
pub mod convert;
#[cfg(feature = "scene")]
mod scene;
#[cfg(feature = "scene")]
pub mod terrain;

#[cfg(feature = "scene")]
pub use cell::{
    CellEnvironment, CellReference, CellSeed, CellSpawnFailed, CellSpawned, CellTerrain, CellWater,
};
#[cfg(feature = "scene")]
pub use convert::{CELL_SIZE_METERS, METERS_PER_UNIT};
#[cfg(feature = "scene")]
pub use terrain::{TerrainPlugin, TerrainSplatMaterial};
pub use tes_loadorder::{CellId, ObjectKind, ObjectRef, TesLoadOrder};
pub use tes_vfs::{TesVfs, TesVfsReader};

pub use tes_nif;
/// Re-exports of the underlying parser crates, so downstream code can name the parsed
/// types ([`tes3_esm::Record`], [`tes3_bsa::Bsa`], [`tes_nif::Nif`], …) without taking a
/// separate dependency.
pub use tes3_bsa;
pub use tes3_esm;

/// The name of the asset source [`TesPlugin`] registers: load game data with paths like
/// `tes://meshes/i/in_de_shack_01.nif`.
pub const TES_SOURCE: &str = "tes";

/// Shared handle to the game-data VFS, inserted by [`TesPlugin`] so systems (and the
/// NIF loader) can query the same layered view the `tes://` source serves.
#[derive(Resource, Clone)]
pub struct TesVfsHandle(pub Arc<TesVfs>);

/// The app's load order, inserted by [`TesPlugin`] when it was given a plugin list
/// (see [`TesPlugin::with_plugins`]); absent when the list is empty. The handle starts
/// loading during plugin finish — poll [`AssetServer::load_state`], or just seed a
/// [`CellSeed`](cell::CellSeed) with it and let `spawn_cells` wait.
#[derive(Resource, Clone, Debug)]
pub struct LoadOrderHandle(pub Handle<LoadOrderAsset>);

/// A TES3 load order — one or more parsed plugins (`.esm`/`.esp`) with merged lookup
/// tables — wrapped as a Bevy [`Asset`].
///
/// It holds a [`TesLoadOrder`]: the owned plugin buffers plus lookup tables borrowing
/// their records, merged earliest-first so later plugins win on id/grid collision.
/// Loading a plugin file directly (`asset_server.load("tes://Morrowind.esm")`) yields a
/// one-plugin load order; [`TesPlugin::with_plugins`] builds the app's full load order
/// and shares it as [`LoadOrderHandle`].
#[derive(Asset, TypePath)]
pub struct LoadOrderAsset {
    load_order: TesLoadOrder,
}

impl LoadOrderAsset {
    /// Parse `bytes` as a single plugin and build the lookup tables. The bytes stay
    /// alive inside the asset; the parsed records borrow them.
    pub fn parse(bytes: Vec<u8>) -> Result<LoadOrderAsset, EsmError> {
        let esm = Esm::parse(bytes)?;
        Ok(LoadOrderAsset {
            load_order: TesLoadOrder::from_esms(vec![esm]),
        })
    }

    /// Build a load order from already-parsed plugins, earliest first (later plugins
    /// override earlier ones on id/grid collision).
    pub fn from_esms(esms: Vec<Esm>) -> LoadOrderAsset {
        LoadOrderAsset {
            load_order: TesLoadOrder::from_esms(esms),
        }
    }

    /// Wrap an in-memory [`EsmDirectory<'static>`] (e.g. a synthetic test plugin built
    /// from `&'static` literals) without a backing buffer.
    pub fn from_static(directory: EsmDirectory<'static>) -> LoadOrderAsset {
        LoadOrderAsset::from_esms(vec![Esm::from_static(directory)])
    }

    /// The load order backing this asset.
    pub fn load_order(&self) -> &TesLoadOrder {
        &self.load_order
    }

    /// Look up a placeable object by editor id (any case).
    pub fn object(&self, id: &str) -> Option<ObjectRef<'_>> {
        self.load_order.object(id)
    }

    /// Look up a cell record by id (interior names match case-insensitively).
    pub fn cell(&self, id: &CellId) -> Option<&Cell<'_>> {
        self.load_order.cell(id)
    }

    /// The placed references for a cell, in authored order — see
    /// [`TesLoadOrder::references`] for why this beats reaching into
    /// [`Cell::references`].
    pub fn references<'s>(&'s self, id: &CellId) -> impl Iterator<Item = &'s Reference<'s>> {
        self.load_order.references(id)
    }

    /// Look up an exterior cell's `LAND` record by grid coordinates.
    pub fn land(&self, x: i32, y: i32) -> Option<&Land<'_>> {
        self.load_order.land(x, y)
    }

    /// Look up a landscape texture by its `LTEX` index (what a LAND `VTEX` value − 1
    /// refers to).
    pub fn ltex(&self, index: u32) -> Option<&Ltex<'_>> {
        self.load_order.ltex(index)
    }
}

// Manual: the owned plugin buffers must not leak into the asset's Debug output.
impl std::fmt::Debug for LoadOrderAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadOrderAsset")
            .field("load_order", &self.load_order)
            .finish()
    }
}

/// A parsed NIF model (`.nif`) wrapped as a Bevy [`Asset`].
///
/// [`NifAsset::nif`] is the raw parsed block graph. With the `scene` feature the loader
/// also emits renderable labeled sub-assets, reachable through the handle fields here or
/// by label (`#Scene`, `#Mesh0`, `#Material0`, …) — mirroring Bevy's glTF loader.
#[derive(Asset, TypePath, Debug)]
pub struct NifAsset {
    /// The parsed NIF block graph.
    pub nif: Nif,
    /// The model as a spawnable scene (labeled `Scene`): the NIF node hierarchy as
    /// entities with `Transform`s, meshes and materials attached, under a Z-up→Y-up root.
    #[cfg(feature = "scene")]
    pub scene: bevy::asset::Handle<bevy::world_serialization::WorldAsset>,
    /// One mesh per drawable `NiTriShape`, in traversal order (labeled `Mesh{i}`).
    #[cfg(feature = "scene")]
    pub meshes: Vec<bevy::asset::Handle<bevy::mesh::Mesh>>,
    /// The distinct materials used by the shapes (labeled `Material{i}`).
    #[cfg(feature = "scene")]
    pub materials: Vec<bevy::asset::Handle<bevy::pbr::StandardMaterial>>,
}

/// Loads a single `.esm`/`.esp` file into a one-plugin [`LoadOrderAsset`].
#[derive(Default, TypePath)]
struct EsmLoader;

impl AssetLoader for EsmLoader {
    type Asset = LoadOrderAsset;
    type Settings = ();
    type Error = EsmError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<LoadOrderAsset, EsmError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(EsmError::Io)?;
        LoadOrderAsset::parse(bytes)
    }

    fn extensions(&self) -> &[&str] {
        // Bevy matches extensions case-sensitively; game data mixes cases freely.
        &["esm", "esp", "ESM", "ESP"]
    }
}

/// Loads `.nif` models into [`NifAsset`], emitting scene sub-assets under the `scene`
/// feature. Holds the VFS to resolve texture references (extension swaps, archive-only
/// textures) at load time.
#[derive(TypePath)]
struct NifLoader {
    #[cfg_attr(not(feature = "scene"), allow(dead_code))]
    vfs: Arc<TesVfs>,
}

impl AssetLoader for NifLoader {
    type Asset = NifAsset;
    type Settings = ();
    type Error = NifError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<NifAsset, NifError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(NifError::Io)?;
        let nif = Nif::parse(&bytes)?;

        #[cfg(feature = "scene")]
        {
            let out = scene::build(&nif, &self.vfs, load_context);
            Ok(NifAsset {
                nif,
                scene: out.scene,
                meshes: out.meshes,
                materials: out.materials,
            })
        }
        #[cfg(not(feature = "scene"))]
        {
            let _ = load_context;
            Ok(NifAsset { nif })
        }
    }

    fn extensions(&self) -> &[&str] {
        // Bevy matches extensions case-sensitively, and Morrowind ships plenty of
        // upper-case `.NIF` files (e.g. `BM_Snow_01.NIF`).
        &["nif", "NIF"]
    }
}

/// Bevy plugin registering the `tes://` asset source and the TES3 asset loaders.
///
/// **Must be added before Bevy's `AssetPlugin`** (i.e. before `DefaultPlugins`) — asset
/// sources can only be registered before the `AssetServer` exists; the plugin asserts
/// this. See the [crate docs](crate) for a full example.
///
/// For texture-splatted terrain, also add [`TerrainPlugin`] **after** `DefaultPlugins`;
/// without it terrain spawns vertex-tinted white.
pub struct TesPlugin {
    /// The Morrowind `Data Files` directory the VFS serves (loose files + `*.bsa`).
    pub data_root: PathBuf,
    /// Explicit archive load order (later archives override earlier ones). `None`
    /// discovers `*.bsa` at the root ordered by modification time, which reproduces the
    /// vanilla game's effective order.
    pub archives: Option<Vec<PathBuf>>,
    /// Plugin files to parse into the app's load order, as paths relative to
    /// `data_root`, earliest first (later plugins override earlier ones). Empty: no
    /// [`LoadOrderHandle`] is inserted.
    pub plugins: Vec<PathBuf>,
}

impl TesPlugin {
    /// A plugin serving `data_root` with auto-discovered archives and no load order.
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        TesPlugin {
            data_root: data_root.into(),
            archives: None,
            plugins: Vec::new(),
        }
    }

    /// Builder: set the plugin load order (earliest first), enabling [`LoadOrderHandle`].
    /// See [`read_load_order`] for filling the list from a plain-text file.
    pub fn with_plugins(mut self, plugins: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.plugins = plugins.into_iter().map(Into::into).collect();
        self
    }

    /// `data_root` absolutized, so `build`'s VFS/reader and `finish`'s plugin reads all
    /// resolve against one tree regardless of Bevy's base-path rules.
    fn absolute_data_root(&self) -> PathBuf {
        std::path::absolute(&self.data_root).unwrap_or_else(|_| self.data_root.clone())
    }
}

impl Default for TesPlugin {
    /// Serves `"data"`, the workspace's conventional game-data directory.
    fn default() -> Self {
        TesPlugin::new("data")
    }
}

impl Plugin for TesPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.world().get_resource::<AssetServer>().is_none(),
            "TesPlugin must be added before Bevy's AssetPlugin (add it before DefaultPlugins)"
        );

        // FileAssetReader (which TesVfsReader delegates loose reads to) resolves relative
        // roots against Bevy's base path — the executable's directory — not the working
        // directory. Absolutize once so the reader and the VFS index agree on one tree.
        let data_root = self.absolute_data_root();

        let vfs = match &self.archives {
            Some(list) => TesVfs::new(&data_root, list),
            None => TesVfs::open(&data_root),
        };
        let vfs = Arc::new(vfs.unwrap_or_else(|e| {
            // Keep dataless apps (tests, fresh checkouts) bootable: loads just miss.
            eprintln!(
                "bevy-tes: cannot open data root {}: {e}; `tes://` loads will find nothing",
                data_root.display()
            );
            TesVfs::empty()
        }));

        app.insert_resource(TesVfsHandle(vfs.clone()));
        app.register_asset_source(
            AssetSourceId::from(TES_SOURCE),
            AssetSourceBuilder::new(move || Box::new(TesVfsReader::new(vfs.clone(), &data_root))),
        );
    }

    // Loader registration needs the AssetServer, which only exists once AssetPlugin has
    // built — hence the build/finish split (asset sources before, loaders after).
    fn finish(&self, app: &mut App) {
        let vfs = app.world().resource::<TesVfsHandle>().0.clone();
        app.init_asset::<LoadOrderAsset>()
            .init_asset::<NifAsset>()
            .init_asset_loader::<EsmLoader>()
            .register_asset_loader(NifLoader { vfs });
        if !self.plugins.is_empty() {
            let data_root = self.absolute_data_root();
            let paths: Vec<PathBuf> = self.plugins.iter().map(|p| data_root.join(p)).collect();
            // Plugins are always loose files (never inside BSAs), so plain fs reads
            // against the absolutized root are correct. add_async drives the handle
            // through the regular Loaded/Failed states, so consumers can't tell it
            // apart from a path load.
            let handle = app.world().resource::<AssetServer>().add_async(async move {
                let mut esms = Vec::with_capacity(paths.len());
                for path in &paths {
                    let bytes = std::fs::read(path).map_err(|e| {
                        EsmError::Io(std::io::Error::new(
                            e.kind(),
                            format!("{}: {e}", path.display()),
                        ))
                    })?;
                    esms.push(Esm::parse(bytes)?);
                }
                Ok::<LoadOrderAsset, EsmError>(LoadOrderAsset::from_esms(esms))
            });
            app.insert_resource(LoadOrderHandle(handle));
        }
        #[cfg(feature = "scene")]
        {
            // The scene pipeline emits these asset types and `spawn_cells` borrows two of
            // them as system parameters. Render apps register them via their plugins (in
            // `build`, so before any `finish`); a headless app has none of that, and a
            // missing `Assets<T>` resource would fail system-param validation at runtime.
            init_asset_if_missing::<bevy::image::Image>(app);
            init_asset_if_missing::<bevy::mesh::Mesh>(app);
            init_asset_if_missing::<bevy::pbr::StandardMaterial>(app);
            init_asset_if_missing::<bevy::world_serialization::WorldAsset>(app);
            app.add_systems(bevy::app::Update, cell::spawn_cells);
        }
    }
}

/// Register `Assets<A>` only when nothing else (i.e. a render plugin) already has.
#[cfg(feature = "scene")]
fn init_asset_if_missing<A: bevy::asset::Asset>(app: &mut App) {
    if !app.world().contains_resource::<Assets<A>>() {
        app.init_asset::<A>();
    }
}

/// Read a plain-text load-order file for [`TesPlugin::with_plugins`]: one plugin
/// filename per line (relative to the data root), in load order (earliest first).
/// Blank lines and lines starting with `#` are skipped; surrounding whitespace
/// (including `\r`) is trimmed. There is no inline-comment syntax — `#` only comments
/// out whole lines.
pub fn read_load_order(path: impl AsRef<std::path::Path>) -> std::io::Result<Vec<PathBuf>> {
    Ok(parse_load_order(&std::fs::read_to_string(path)?))
}

fn parse_load_order(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_load_order;
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn parses_plugins_in_authored_order() {
        assert_eq!(
            parse_load_order("Morrowind.esm\nTribunal.esm\nBloodmoon.esm\n"),
            paths(&["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"])
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        assert_eq!(
            parse_load_order("# masters\n\nMorrowind.esm\n  # indented comment\n\nMod.esp\n"),
            paths(&["Morrowind.esm", "Mod.esp"])
        );
    }

    #[test]
    fn trims_whitespace_and_crlf() {
        assert_eq!(
            parse_load_order("  Morrowind.esm \r\nTribunal.esm\r\n"),
            paths(&["Morrowind.esm", "Tribunal.esm"])
        );
    }

    #[test]
    fn empty_input_yields_no_plugins() {
        assert!(parse_load_order("").is_empty());
        assert!(parse_load_order("\n# only a comment\n").is_empty());
    }
}
