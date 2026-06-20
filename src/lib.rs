//! Bevy integration for Bethesda Creation Engine (TES3 / Morrowind) data files.
//!
//! This crate adds [`BethPlugin`], which registers Bevy [`AssetLoader`]s backed by
//! [`beth_rs`]. Once the plugin is added, `.esm`/`.esp` plugins load as [`EsmAsset`] and
//! `.bsa` archives load as [`BsaAsset`] through the regular [`AssetServer`]:
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
//! Decoding meshes, textures and other content *out of* the loaded data is intentionally
//! out of scope for now — this layer only exposes the parsed structures as Bevy assets.
//!
//! [`AssetServer`]: bevy::asset::AssetServer

use bevy::app::{App, Plugin};
use bevy::asset::io::Reader;
use bevy::asset::{Asset, AssetApp, AssetLoader, LoadContext};
use bevy::reflect::TypePath;

use beth_rs::{Bsa, BsaError, EsmError, Plugin as TesPlugin};

/// Re-export of the underlying parser crate, so downstream code can name the parsed
/// types ([`beth_rs::Record`], [`beth_rs::FileEntry`](beth_rs::bsa::FileEntry), …)
/// without taking a separate dependency.
pub use beth_rs;

/// A parsed TES3 plugin (`.esm`/`.esp`) wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`beth_rs::Plugin`]; access the records via the `0` field.
#[derive(Asset, TypePath, Debug)]
pub struct EsmAsset(pub TesPlugin);

/// A parsed TES3 BSA archive wrapped as a Bevy [`Asset`].
///
/// Wraps an owned [`beth_rs::Bsa`]; access the file entries via the `0` field.
#[derive(Asset, TypePath, Debug)]
pub struct BsaAsset(pub Bsa);

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

/// Bevy plugin that registers the TES3 asset types and their loaders.
///
/// Requires Bevy's [`AssetPlugin`](bevy::asset::AssetPlugin) to be present (it is part of
/// `DefaultPlugins`); add it explicitly in headless apps.
pub struct BethPlugin;

impl Plugin for BethPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<EsmAsset>()
            .init_asset::<BsaAsset>()
            .init_asset_loader::<EsmLoader>()
            .init_asset_loader::<BsaLoader>();
    }
}
