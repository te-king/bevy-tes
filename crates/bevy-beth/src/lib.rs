//! Bevy integration for Bethesda Creation Engine (TES3 / Morrowind) data files.
//!
//! This crate adds [`BethPlugin`], which registers Bevy [`AssetLoader`]s backed by the
//! format parsers ([`tes3_esm`], [`tes3_bsa`], [`tes_nif`]). Once the plugin is added,
//! `.esm`/`.esp` plugins load as [`EsmAsset`], `.bsa` archives as [`BsaAsset`], and `.nif`
//! models as [`NifAsset`] through the regular [`AssetServer`]:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_beth::{BethPlugin, EsmAsset};
//!
//! fn load(asset_server: Res<AssetServer>) {
//!     let _handle: Handle<EsmAsset> = asset_server.load("Morrowind.esm");
//! }
//!
//! App::new()
//!     .add_plugins((AssetPlugin::default(), BethPlugin::default()))
//!     .add_systems(Startup, load);
//! ```
//!
//! The `asset_root` on [`BethPlugin`] must match the `file_path` on [`AssetPlugin`] so
//! that [`BsaAsset`] loading can mmap the archive directly from disk. Both default to
//! `"assets"`.
//!
//! Decoding the loaded data into engine types — NIF scene graphs into per-shape meshes and
//! materials, texture bytes into images — lives in the `convert` module, behind the
//! `render` feature.
//!
//! [`AssetServer`]: bevy::asset::AssetServer

use std::path::PathBuf;

use bevy::app::{App, Plugin};
use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetApp, AssetLoader, LoadContext};
use bevy::reflect::TypePath;

use tes_nif::{Nif, NifError};
use tes3_bsa::{Bsa, BsaError};
use tes3_esm::{EsmError, Plugin as TesPlugin};

#[cfg(feature = "render")]
pub mod convert;

pub use tes_nif;
/// Re-exports of the underlying parser crates, so downstream code can name the parsed
/// types ([`tes3_esm::Record`], [`tes3_bsa::FileRecord`], [`tes_nif::Nif`], …) without
/// taking a separate dependency.
pub use tes3_bsa;
pub use tes3_esm;

/// A parsed TES3 plugin (`.esm`/`.esp`) wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`tes3_esm::Plugin`]; access the records via the `0` field.
#[derive(Asset, TypePath, Debug)]
pub struct EsmAsset(pub TesPlugin);

/// A parsed TES3 BSA archive wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`tes3_bsa::Bsa`]; access the file directory via the `0` field and read
/// file bytes via [`Bsa::get`] or [`Bsa::bytes`].
#[derive(Asset, TypePath, Debug)]
pub struct BsaAsset(pub Bsa);

/// A parsed NIF model (`.nif`) wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`tes_nif::Nif`]; access the block graph via the `0` field, or walk the
/// scene with [`Nif::instances`]. Convert to drawable meshes/materials via
/// `convert::nif_to_parts` (behind the `render` feature).
#[derive(Asset, TypePath, Debug)]
pub struct NifAsset(pub Nif);

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
        Ok(EsmAsset(TesPlugin::parse(&bytes)?))
    }

    fn extensions(&self) -> &[&str] {
        // Bevy matches extensions case-sensitively; game data mixes cases freely.
        &["esm", "esp", "ESM", "ESP"]
    }
}

/// Loads `.bsa` archives into [`BsaAsset`] by mmapping the archive file directly.
///
/// The `asset_root` must match the `file_path` configured on Bevy's [`AssetPlugin`] so
/// the loader can resolve the archive's OS path.
#[derive(TypePath)]
struct BsaLoader {
    asset_root: PathBuf,
}

impl AssetLoader for BsaLoader {
    type Asset = BsaAsset;
    type Settings = ();
    type Error = BsaError;

    async fn load(
        &self,
        _reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<BsaAsset, BsaError> {
        let path = self.asset_root.join(load_context.path().path());
        Ok(BsaAsset(Bsa::open(path)?))
    }

    fn extensions(&self) -> &[&str] {
        // Bevy matches extensions case-sensitively; game data mixes cases freely.
        &["bsa", "BSA"]
    }
}

/// Loads `.nif` models into [`NifAsset`].
#[derive(Default, TypePath)]
struct NifLoader;

impl AssetLoader for NifLoader {
    type Asset = NifAsset;
    type Settings = ();
    type Error = NifError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<NifAsset, NifError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(NifError::Io)?;
        Ok(NifAsset(Nif::parse(&bytes)?))
    }

    fn extensions(&self) -> &[&str] {
        // Bevy matches extensions case-sensitively, and Morrowind ships plenty of
        // upper-case `.NIF` files (e.g. `BM_Snow_01.NIF`).
        &["nif", "NIF"]
    }
}

/// Bevy plugin that registers the TES3 asset types and their loaders.
///
/// The `asset_root` field must match the `file_path` on Bevy's
/// [`AssetPlugin`](bevy::asset::AssetPlugin) — both default to `"assets"`. Set them to
/// the same value when using a non-default asset directory.
///
/// Requires Bevy's [`AssetPlugin`](bevy::asset::AssetPlugin) to be present (it is part of
/// `DefaultPlugins`); add it explicitly in headless apps.
pub struct BethPlugin {
    /// The filesystem root from which `.bsa` archives are resolved.
    /// Must match [`AssetPlugin::file_path`](bevy::asset::AssetPlugin::file_path).
    pub asset_root: PathBuf,
}

impl Default for BethPlugin {
    fn default() -> Self {
        Self {
            asset_root: PathBuf::from("assets"),
        }
    }
}

impl Plugin for BethPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<EsmAsset>()
            .init_asset::<BsaAsset>()
            .init_asset::<NifAsset>()
            .init_asset_loader::<EsmLoader>()
            .register_asset_loader(BsaLoader {
                asset_root: self.asset_root.clone(),
            })
            .init_asset_loader::<NifLoader>();
    }
}
