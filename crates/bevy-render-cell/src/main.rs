//! Render a TES3 (Morrowind) cell — a whole interior or exterior grid square — with
//! Bevy, spawned from an ESM through `bevy_tes`'s `CellSeed`:
//!
//! ```text
//! cargo run --release -p bevy-render-cell -- "Balmora, Guild of Mages"
//! cargo run --release -p bevy-render-cell -- --exterior=-3,-2
//! ```
//!
//! By default this waits until the cell's models have streamed in, frames the scene,
//! saves one PNG and exits — printing the screenshot's absolute path. Pass `-o out.png`
//! to choose where it lands, `--esm <file>` / `--data <dir>` for non-default data,
//! `--load-order <file>` for a multi-plugin load order read from a plain-text file (one
//! plugin per line, earliest first), or `--interactive` for a live window with Bevy's
//! free camera (hold RMB to look, WASD to fly, E/Q up/down, Shift to run, scroll for
//! speed).
//!
//! Cell resolution, reference placement, light spawning and NIF/texture loading all
//! happen inside `bevy_tes`; this binary stages a camera and lighting around the
//! spawned entities. Exterior cells include their LAND terrain — texture-splatted from
//! the VTEX grid via `TerrainPlugin`, tinted by the vertex colors — and a sea-level
//! water plane where the ground dips below zero.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::atmosphere::ScatteringMedium;
use bevy::light::{
    Atmosphere, AtmosphereEnvironmentMapLight, CascadeShadowConfigBuilder, ShadowFilteringMethod,
};
use bevy::pbr::{AtmosphereSettings, ScreenSpaceAmbientOcclusion};
use bevy::post_process::auto_exposure::{AutoExposure, AutoExposurePlugin};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{ExitCondition, WindowResolution};
use clap::Parser;

use bevy_tes::{
    CellEnvironment, CellId, CellSeed, CellSpawnFailed, CellSpawned, LoadOrderHandle,
    TerrainPlugin, TesPlugin,
};

/// Render a TES3 cell with Bevy.
#[derive(Parser, Debug)]
struct Args {
    /// Interior cell name, e.g. `Balmora, Guild of Mages` (case-insensitive).
    #[arg(default_value = "Balmora, Guild of Mages", conflicts_with = "exterior")]
    cell: String,

    /// Render an exterior grid cell instead, as `X,Y` (e.g. `--exterior=-3,-2`).
    #[arg(long, allow_hyphen_values = true)]
    exterior: Option<String>,

    /// The plugin to read the cell from, as a game-data path (a load order of one).
    #[arg(long, default_value = "Morrowind.esm")]
    esm: String,

    /// A plain-text load-order file instead: one plugin filename per line (relative to
    /// `--data`), earliest first (later plugins win); `#` comments and blank lines are
    /// skipped.
    #[arg(long, conflicts_with = "esm")]
    load_order: Option<PathBuf>,

    /// The Morrowind `Data Files` directory to serve (loose files + `*.bsa`).
    #[arg(long, default_value = "data")]
    data: PathBuf,

    /// Where to write the screenshot PNG (screenshot mode only). Defaults to the system
    /// temp dir as `render_cell-<name>.png`.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Open a live window with a fly camera instead of capturing a single screenshot.
    #[arg(long)]
    interactive: bool,
}

/// How many consecutive frames the drawable-mesh count must hold still before the scene
/// counts as settled — NIF instances splice in progressively as their assets load.
const STABLE_FRAMES: u32 = 20;
/// Give up waiting for more meshes after this many frames and frame whatever arrived.
const SETTLE_TIMEOUT: u32 = 1_200;

/// Set once `frame_cell` has staged the camera and lights around the spawned cell.
#[derive(Resource, Default)]
struct Framed(bool);

/// Screenshot destination; absent in `--interactive` mode.
#[derive(Resource)]
struct Capture {
    path: PathBuf,
}

/// Set by the [`ScreenshotCaptured`] observer once the PNG has been written to disk.
#[derive(Resource, Default)]
struct CaptureDone(bool);

fn main() -> ExitCode {
    let args = Args::parse();
    let cell = match &args.exterior {
        Some(grid) => match parse_grid(grid) {
            Some((x, y)) => CellId::exterior(x, y),
            None => {
                eprintln!("--exterior expects X,Y grid coordinates, got {grid:?}");
                return ExitCode::FAILURE;
            }
        },
        None => CellId::interior(&args.cell),
    };
    let title = match &cell {
        CellId::Interior(name) => name.clone(),
        CellId::Exterior { x, y } => format!("exterior {x},{y}"),
    };

    let plugins: Vec<PathBuf> = match &args.load_order {
        Some(path) => match bevy_tes::read_load_order(path) {
            Ok(list) if !list.is_empty() => list,
            Ok(_) => {
                eprintln!("load-order file {} lists no plugins", path.display());
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("cannot read load order {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => vec![PathBuf::from(&args.esm)],
    };

    let mut app = App::new();
    // TesPlugin must precede DefaultPlugins: asset sources register before AssetPlugin.
    app.add_plugins(TesPlugin::new(args.data.clone()).with_plugins(plugins));

    if args.interactive {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("render_cell — {title}"),
                ..default()
            }),
            ..default()
        }))
        // TerrainPlugin goes after DefaultPlugins (it registers a render material);
        // AutoExposurePlugin isn't part of DefaultPlugins.
        .add_plugins((TerrainPlugin, FreeCameraPlugin, AutoExposurePlugin));
    } else {
        // Screenshot mode: a hidden window is enough to drive the render target; we
        // capture one frame and exit. `close_when_requested` is off so nothing races our
        // AppExit.
        let path = args
            .output
            .clone()
            .unwrap_or_else(|| default_screenshot_path(&title));
        // Clear any stale screenshot so a failed run can't leave a misleading old image.
        let _ = std::fs::remove_file(&path);
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(768, 768),
                ..default()
            }),
            exit_condition: ExitCondition::DontExit,
            close_when_requested: false,
            ..default()
        }))
        // TerrainPlugin goes after DefaultPlugins (it registers a render material);
        // AutoExposurePlugin isn't part of DefaultPlugins.
        .add_plugins((TerrainPlugin, AutoExposurePlugin))
        .insert_resource(Capture { path })
        .init_resource::<CaptureDone>()
        .add_systems(Update, capture);
    }

    app.init_resource::<Framed>()
        .add_systems(
            Startup,
            // TesPlugin finished before Startup, so the load-order handle exists.
            move |mut commands: Commands, load_order: Res<LoadOrderHandle>| {
                commands.spawn(CellSeed {
                    load_order: load_order.0.clone(),
                    cell: cell.clone(),
                });
            },
        )
        .add_systems(Update, frame_cell);

    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(_) => ExitCode::FAILURE,
    }
}

/// `X,Y` → grid coordinates.
fn parse_grid(s: &str) -> Option<(i32, i32)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Game units per meter: Morrowind's 64 units = 1 yard. The atmosphere works in meters,
/// so the planet entity is scaled by this to keep sky curvature and aerial perspective
/// at the right world scale.
const UNITS_PER_METER: f32 = 70.0;

/// Once the cell has spawned and its models have settled, stage the set: camera framed
/// on the spawned geometry, ambient from the cell's own values, a key light for
/// exteriors. Runs once; also the failure exit when the cell can't spawn.
#[allow(clippy::too_many_arguments)]
fn frame_cell(
    mut commands: Commands,
    mut framed: ResMut<Framed>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    mut settle: Local<(u32, usize, u32)>, // (stable frames, last count, total frames)
    seeds: Query<(&CellSpawned, &CellEnvironment)>,
    failures: Query<&CellSpawnFailed>,
    parts: Query<&GlobalTransform, With<Mesh3d>>,
    lights: Query<&GlobalTransform, With<PointLight>>,
    capture: Option<Res<Capture>>,
    mut exit: MessageWriter<AppExit>,
) {
    if framed.0 {
        return;
    }
    if let Ok(failure) = failures.single() {
        eprintln!("cannot spawn cell: {}", failure.0);
        exit.write(AppExit::error());
        return;
    }
    let Ok((spawned, environment)) = seeds.single() else {
        return; // ESM still loading
    };

    // Models stream in progressively (each reference's NIF and its textures load
    // asynchronously); wait until the drawable count holds still before measuring.
    let (stable, last_count, total) = &mut *settle;
    *total += 1;
    let count = parts.iter().count();
    if count == *last_count {
        *stable += 1;
    } else {
        (*stable, *last_count) = (0, count);
    }
    if count == 0 || (*stable < STABLE_FRAMES && *total < SETTLE_TIMEOUT) {
        if *total >= SETTLE_TIMEOUT {
            eprintln!("no geometry arrived for the cell ({spawned:?})");
            exit.write(AppExit::error());
        }
        return;
    }

    // Bounds over the spawned parts' positions — coarse (no vertex AABBs), but plenty to
    // frame a room or a town block.
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for gt in &parts {
        min = min.min(gt.translation());
        max = max.max(gt.translation());
    }
    let center = (min + max) * 0.5;
    let r = ((max - min).length() * 0.5).max(100.0);

    // Stage per cell kind: interiors are lit by their authored ambient plus the placed
    // lights; exteriors get daylight and a shadow-casting sun, with their ambient
    // supplied by the sky's environment map (below) instead of a flat term.
    let ambient = if environment.interior {
        // The authored AMBI colour is dark in linear space (the game adds it in gamma
        // space), so the scalar is large: it reproduces the game's ~15% brightness floor
        // after Bevy's exposure scaling.
        AmbientLight {
            color: environment.ambient.unwrap_or(Color::WHITE),
            brightness: 8_000.0,
            ..default()
        }
    } else {
        // Zero, not absent: an absent component would fall back to Bevy's global
        // default ambient on top of the sky light.
        AmbientLight {
            brightness: 0.0,
            ..default()
        }
    };
    // Interiors: stand at the bounds centre (hollow in a room) and look toward the
    // placed lights — they mark the inhabited part of the cell. Exteriors: elevated
    // three-quarter view.
    let camera_transform = if environment.interior {
        let lit: Vec<Vec3> = lights.iter().map(|gt| gt.translation()).collect();
        let target = if lit.is_empty() {
            center + Vec3::X
        } else {
            let mut t = lit.iter().sum::<Vec3>() / lit.len() as f32;
            // Keep the view near-horizontal: it's a room, not a floor inspection.
            t.y = t.y.clamp(center.y - r * 0.2, center.y + r * 0.2);
            if t.distance(center) < 1.0 {
                t += Vec3::X;
            }
            t
        };
        // Slightly below the bounds centre: roof geometry drags the raw centre up.
        Transform::from_translation(center - Vec3::Y * r * 0.15).looking_at(target, Vec3::Y)
    } else {
        Transform::from_translation(center + Vec3::new(r * 0.7, r * 0.8, r * 0.7))
            .looking_at(center, Vec3::Y)
    };
    // Procedural sky: one earth-like atmosphere, scaled from meters to game units. The
    // planet center sits `inner_radius` below the origin so the world sits on its
    // surface (the default-transform hook would do this unscaled; setting a scale means
    // placing it ourselves). The sky's look is driven by the directional sun below.
    // Staged for interiors and exteriors alike — an interior only shows it through door
    // gaps, and per-kind tuning can come later.
    let atmosphere = Atmosphere::earth(scattering_mediums.add(ScatteringMedium::earth(256, 256)));
    commands.spawn((
        Transform::from_translation(-Vec3::Y * atmosphere.inner_radius * UNITS_PER_METER)
            .with_scale(Vec3::splat(UNITS_PER_METER)),
        atmosphere,
    ));

    let mut camera = commands.spawn((
        Camera3d::default(),
        camera_transform,
        // Game units are big (~70/m): push the far plane out so a town block fits.
        Projection::Perspective(PerspectiveProjection {
            near: 1.0,
            far: 200_000.0,
            ..default()
        }),
        ambient,
        // Renders the sky (and implies an HDR camera); bloom gives the sun a disk.
        AtmosphereSettings::default(),
        Bloom::NATURAL,
        // Sky-driven image-based lighting: ambient diffuse comes from the atmosphere's
        // generated environment map (blue from above, responding to sun angle).
        AtmosphereEnvironmentMapLight::default(),
        // Contact shadows in clutter and under eaves. Requires (and auto-adds) the
        // depth/normal prepasses; both SSAO and TAA require MSAA off.
        ScreenSpaceAmbientOcclusion::default(),
        // Alpha-masked vegetation shimmers under MSAA (it can't help alpha-tested
        // edges); TAA resolves those. Converges over a few frames — screenshot mode
        // waits for it. CAS recovers the crispness TAA softens away.
        TemporalAntiAliasing::default(),
        Msaa::Off,
        ContrastAdaptiveSharpening::default(),
        // Softer sun-shadow edges, using the TAA-aware filter.
        ShadowFilteringMethod::Temporal,
        // Punchier filmic tonemapper than the default TonyMcMapface.
        Tonemapping::AcesFitted,
        // Eye adaptation: meters the frame and drifts exposure toward mid-grey, so the
        // shared interior/exterior staging each land at a sane brightness. Adapts over
        // ~a second — screenshot mode waits for it too. Metering only the brighter
        // half of the histogram keeps the moody, darker-than-mid-grey look from being
        // "corrected" into overexposure.
        AutoExposure {
            filter: 0.50..=0.95,
            ..default()
        },
    ));
    if capture.is_none() {
        // Bevy's free camera; the controller logs its controls on the first frame. The
        // metric default speeds are far too slow for game units (~70/m), so scale them
        // to cross a town block in a few seconds.
        camera.insert(FreeCamera {
            walk_speed: 500.0,
            run_speed: 2_000.0,
            ..default()
        });
    }

    if !environment.interior {
        commands.spawn((
            DirectionalLight {
                illuminance: 6_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_xyz(1.0, 2.0, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
            CascadeShadowConfigBuilder {
                maximum_distance: r * 4.0,
                ..default()
            }
            .build(),
        ));
    }

    println!(
        "Framed cell — {} references spawned ({} skipped), {count} meshes, r={r:.0} about {center:?}",
        spawned.spawned, spawned.skipped
    );
    framed.0 = true;
}

/// Screenshot-mode driver: once the cell is framed, let the frame settle — texture
/// uploads, TAA convergence, auto-exposure adaptation (the slowest, ~a second) — then
/// request one screenshot and exit once its observer reports the PNG has been written.
fn capture(
    mut commands: Commands,
    capture: Res<Capture>,
    framed: Res<Framed>,
    done: Res<CaptureDone>,
    mut frames_since_framed: Local<u32>,
    mut shot_requested: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    if !framed.0 {
        return;
    }
    *frames_since_framed += 1;

    if *frames_since_framed == 90 && !*shot_requested {
        *shot_requested = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(capture.path.clone()))
            .observe(|_: On<ScreenshotCaptured>, mut done: ResMut<CaptureDone>| done.0 = true);
        return;
    }

    if done.0 {
        let shown = capture
            .path
            .canonicalize()
            .unwrap_or_else(|_| capture.path.clone());
        println!("Screenshot written to {}", shown.display());
        exit.write(AppExit::Success);
    }
}

/// Default screenshot path: system temp dir, named after the cell.
fn default_screenshot_path(title: &str) -> PathBuf {
    let stem: String = Path::new(title)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("cell")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    std::env::temp_dir().join(format!("render_cell-{stem}.png"))
}
