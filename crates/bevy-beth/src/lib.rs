//! Bevy integration for Bethesda Creation Engine (TES3 / Morrowind) data files.
//!
//! [`BethPlugin`] registers a **`tes://` asset source** backed by [`TesVfs`] — a layered,
//! case-insensitive view of a Morrowind `Data Files` directory where loose files override
//! BSA archives — plus [`AssetLoader`]s for the game's formats. Once the plugin is added,
//! any file the game could resolve is loadable through the regular
//! [`AssetServer`], whether it exists loose on disk or only inside an archive:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_beth::{BethPlugin, EsmAsset, NifAsset};
//!
//! fn load(asset_server: Res<AssetServer>) {
//!     let _esm: Handle<EsmAsset> = asset_server.load("tes://Morrowind.esm");
//!     let _nif: Handle<NifAsset> = asset_server.load("tes://meshes/i/in_de_shack_01.nif");
//! }
//!
//! App::new()
//!     // BethPlugin FIRST: asset sources must be registered before AssetPlugin
//!     // (part of DefaultPlugins) builds the AssetServer.
//!     .add_plugins((BethPlugin::new("data"), DefaultPlugins))
//!     .add_systems(Startup, load)
//!     .run();
//! ```
//!
//! With the `scene` feature (implied by `render`), a NIF load additionally emits
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
//! Whole **cells** (interiors or exterior grid squares) spawn the same way from a loaded
//! plugin — one child entity per placed object, each loading its own NIF scene (see
//! [`cell`]):
//!
//! ```ignore
//! use bevy_beth::{CellId, CellSeed};
//!
//! fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     commands.spawn(CellSeed {
//!         esm: asset_server.load("tes://Morrowind.esm"),
//!         cell: CellId::interior("Balmora, Guild of Mages"),
//!     });
//! }
//! ```
//!
//! # Plugin ordering
//!
//! `BethPlugin` **must be added before** Bevy's `AssetPlugin` (i.e. before
//! `DefaultPlugins`): Bevy requires custom asset sources to be registered before the
//! `AssetServer` is built. The plugin asserts this at startup. Headless apps that build
//! the `App` manually must also call `app.finish()` for loaders to be registered.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::app::{App, Plugin};
use bevy::asset::io::{AssetSourceBuilder, AssetSourceId, Reader};
use bevy::asset::{Asset, AssetApp, AssetLoader, AssetServer, Assets, LoadContext};
use bevy::ecs::resource::Resource;
use bevy::reflect::TypePath;

use tes_nif::{Nif, NifError};
use tes3_esm::{Esm, EsmDirectory, EsmError};

pub mod index;
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
pub use index::{CellId, EsmIndex, ObjectInfo, ObjectKind};
#[cfg(feature = "scene")]
pub use terrain::{TerrainPlugin, TerrainSplatMaterial};
pub use tes_vfs::{TesVfs, TesVfsReader};

pub use tes_nif;
/// Re-exports of the underlying parser crates, so downstream code can name the parsed
/// types ([`tes3_esm::Record`], [`tes3_bsa::Bsa`], [`tes_nif::Nif`], …) without taking a
/// separate dependency.
pub use tes3_bsa;
pub use tes3_esm;

/// The name of the asset source [`BethPlugin`] registers: load game data with paths like
/// `tes://meshes/i/in_de_shack_01.nif`.
pub const TES_SOURCE: &str = "tes";

/// Shared handle to the game-data VFS, inserted by [`BethPlugin`] so systems (and the
/// NIF loader) can query the same layered view the `tes://` source serves.
#[derive(Resource, Clone)]
pub struct TesVfsHandle(pub Arc<TesVfs>);

/// A parsed TES3 plugin (`.esm`/`.esp`) wrapped as a Bevy [`Asset`]: an owned [`Esm`]
/// (the file bytes plus the zero-copy [`EsmDirectory`] view borrowing them, reached via
/// [`EsmAsset::esm`]) alongside a lookup index over its records.
#[derive(Asset, TypePath)]
pub struct EsmAsset {
    esm: Esm,
    /// Lookups over the records (editor id → object, cell name/grid → `CELL`), built
    /// once at load time. Fully owned, so it sits beside the parsed view.
    pub index: EsmIndex,
}

impl EsmAsset {
    /// Parse `bytes` and build the index. The bytes stay alive inside the asset; the
    /// parsed records borrow them.
    pub fn parse(bytes: Vec<u8>) -> Result<EsmAsset, EsmError> {
        let esm = Esm::parse(bytes)?;
        let index = EsmIndex::build(esm.directory());
        Ok(EsmAsset { esm, index })
    }

    /// Wrap an in-memory [`EsmDirectory<'static>`] (e.g. a synthetic test plugin built
    /// from `&'static` literals) without a backing buffer.
    pub fn from_static(directory: EsmDirectory<'static>) -> EsmAsset {
        let esm = Esm::from_static(directory);
        let index = EsmIndex::build(esm.directory());
        EsmAsset { esm, index }
    }

    /// The parsed plugin directory: header plus all records in file order.
    pub fn esm(&self) -> &EsmDirectory<'_> {
        self.esm.directory()
    }
}

// Manual: the owned Esm's buffer must not leak into the asset's Debug output.
impl std::fmt::Debug for EsmAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EsmAsset")
            .field("esm", self.esm())
            .field("index", &self.index)
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

/// Loads `.esm`/`.esp` files into [`EsmAsset`].
#[derive(Default, TypePath)]
struct EsmLoader;

impl AssetLoader for EsmLoader {
    type Asset = EsmAsset;
    type Settings = ();
    type Error = EsmError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<EsmAsset, EsmError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(EsmError::Io)?;
        EsmAsset::parse(bytes)
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
pub struct BethPlugin {
    /// The Morrowind `Data Files` directory the VFS serves (loose files + `*.bsa`).
    pub data_root: PathBuf,
    /// Explicit archive load order (later archives override earlier ones). `None`
    /// discovers `*.bsa` at the root ordered by modification time, which reproduces the
    /// vanilla game's effective order.
    pub archives: Option<Vec<PathBuf>>,
}

impl BethPlugin {
    /// A plugin serving `data_root` with auto-discovered archives.
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        BethPlugin {
            data_root: data_root.into(),
            archives: None,
        }
    }
}

impl Default for BethPlugin {
    /// Serves `"data"`, the workspace's conventional game-data directory.
    fn default() -> Self {
        BethPlugin::new("data")
    }
}

impl Plugin for BethPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.world().get_resource::<AssetServer>().is_none(),
            "BethPlugin must be added before Bevy's AssetPlugin (add it before DefaultPlugins)"
        );

        // FileAssetReader (which TesVfsReader delegates loose reads to) resolves relative
        // roots against Bevy's base path — the executable's directory — not the working
        // directory. Absolutize once so the reader and the VFS index agree on one tree.
        let data_root =
            std::path::absolute(&self.data_root).unwrap_or_else(|_| self.data_root.clone());

        let vfs = match &self.archives {
            Some(list) => TesVfs::new(&data_root, list),
            None => TesVfs::open(&data_root),
        };
        let vfs = Arc::new(vfs.unwrap_or_else(|e| {
            // Keep dataless apps (tests, fresh checkouts) bootable: loads just miss.
            eprintln!(
                "bevy-beth: cannot open data root {}: {e}; `tes://` loads will find nothing",
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
        app.init_asset::<EsmAsset>()
            .init_asset::<NifAsset>()
            .init_asset_loader::<EsmLoader>()
            .register_asset_loader(NifLoader { vfs });
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
