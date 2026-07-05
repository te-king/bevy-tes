//! Helpers shared by the integration tests (each test binary pulls this in with
//! `mod common;`).

use std::time::Duration;

use bevy::app::App;
use bevy::asset::{AssetPlugin, AssetServer, Handle, LoadState};
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};

use bevy_beth::BethPlugin;

/// The workspace `data/` directory holding the (gitignored) game data. Bevy resolves the
/// asset root relative to `CARGO_MANIFEST_DIR` (this crate) when run under cargo, and
/// `BethPlugin`'s VFS resolves against the process working directory — the same place
/// under cargo.
const DATA_ROOT: &str = "../../data";

/// A headless app with just enough asset machinery for the loaders under test: the
/// plugins register the `tes://` source and the ESM/NIF loaders; under the `scene`
/// feature, manual registrations stand in for the render plugins that would normally own
/// the `Image`/`Mesh`/material/scene asset types.
pub fn app_with_assets() -> App {
    // Asset loading runs on Bevy's task pools, which `DefaultPlugins` would set up. A
    // headless test must initialize them itself.
    IoTaskPool::get_or_init(Default::default);
    AsyncComputeTaskPool::get_or_init(Default::default);
    ComputeTaskPool::get_or_init(Default::default);

    let mut app = App::new();
    app.add_plugins((
        // BethPlugin must precede AssetPlugin: asset sources register before the server.
        BethPlugin::new(DATA_ROOT),
        AssetPlugin {
            file_path: DATA_ROOT.to_string(),
            ..Default::default()
        },
    ));
    #[cfg(feature = "scene")]
    {
        use bevy::asset::AssetApp;
        use bevy::image::{CompressedImageFormats, Image, ImageLoader};
        app.init_asset::<Image>()
            .init_asset::<bevy::mesh::Mesh>()
            .init_asset::<bevy::pbr::StandardMaterial>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .register_asset_loader(ImageLoader::new(CompressedImageFormats::BC));
    }
    // Headless apps must finish plugin setup themselves; BethPlugin registers its
    // loaders in `Plugin::finish`.
    app.finish();
    app
}

/// Pump the app until `handle` finishes loading (or a generous timeout expires).
pub fn pump_until_loaded<A: bevy::asset::Asset>(app: &mut App, handle: &Handle<A>) -> LoadState {
    let mut state = LoadState::NotLoaded;
    for _ in 0..2000 {
        app.update();
        state = app.world().resource::<AssetServer>().load_state(handle);
        if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    state
}
