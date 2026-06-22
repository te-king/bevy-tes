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
//!     .add_plugins((AssetPlugin::default(), BethPlugin))
//!     .add_systems(Startup, load);
//! ```
//!
//! Decoding meshes, textures and other content *out of* the loaded data (NIF → Bevy
//! meshes/materials, textures → images, …) is future work; the [`convert`] module is its
//! placeholder. For now this layer only exposes the parsed structures as Bevy assets.
//!
//! [`AssetServer`]: bevy::asset::AssetServer

use bevy::app::{App, Plugin};
use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetApp, AssetLoader, LoadContext};
use bevy::reflect::TypePath;

use tes_nif::{Nif, NifError};
use tes3_bsa::{Bsa, BsaError};
use tes3_esm::{EsmError, Plugin as TesPlugin};

pub mod convert;

pub use tes_nif;
/// Re-exports of the underlying parser crates, so downstream code can name the parsed
/// types ([`tes3_esm::Record`], [`tes3_bsa::FileEntry`], [`tes_nif::Nif`], …) without
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
/// Wraps an owned [`tes3_bsa::Bsa`]; access the file entries via the `0` field.
#[derive(Asset, TypePath, Debug)]
pub struct BsaAsset(pub Bsa);

/// A parsed NIF model (`.nif`) wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`tes_nif::Nif`]; access the header via the `0` field. (Currently the
/// parser decodes only the header — see [`tes_nif`] for scope.)
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
        &["esm", "esp"]
    }
}

/// Loads `.bsa` archives into [`BsaAsset`].
#[derive(Default, TypePath)]
struct BsaLoader;

impl AssetLoader for BsaLoader {
    type Asset = BsaAsset;
    type Settings = ();
    type Error = BsaError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<BsaAsset, BsaError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_err(BsaError::Io)?;
        Ok(BsaAsset(Bsa::parse(&bytes)?))
    }

    fn extensions(&self) -> &[&str] {
        &["bsa"]
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
        &["nif"]
    }
}

/// Bevy plugin that registers the TES3 asset types and their loaders.
///
/// Requires Bevy's [`AssetPlugin`](bevy::asset::AssetPlugin) to be present (it is part of
/// `DefaultPlugins`); add it explicitly in headless apps.
pub struct BethPlugin;

impl Plugin for BethPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<EsmAsset>()
            .init_asset::<BsaAsset>()
            .init_asset::<NifAsset>()
            .init_asset_loader::<EsmLoader>()
            .init_asset_loader::<BsaLoader>()
            .init_asset_loader::<NifLoader>();
    }
}
