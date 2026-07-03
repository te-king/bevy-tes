//! Render a TES3 (Morrowind) `.nif` model with Bevy.
//!
//! By default this renders one frame off a fixed three-quarter view, saves it to a PNG, and
//! exits — printing the screenshot's absolute path so the image can be inspected:
//!
//! ```text
//! cargo run -p bevy-beth --example render_nif --features render -- path/to/model.nif
//! ```
//!
//! Try a bundled fixture (needs no game data):
//!
//! ```text
//! cargo run -p bevy-beth --example render_nif --features render -- \
//!     crates/tes-nif/tests/cursor.nif
//! ```
//!
//! Pass `-o out.png` to choose where the screenshot lands, or `--interactive` to instead
//! open a live window that slowly rotates the model for inspection from all sides.
//!
//! The NIF is parsed and converted to a Bevy [`Mesh`] up front (see
//! [`bevy_beth::convert::nif_to_mesh`]). Only static-mesh NIFs are supported — animated,
//! skinned and particle models will report an unsupported-block error.

use std::f32::consts::PI;
use std::path::PathBuf;
use std::process::ExitCode;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{ExitCondition, WindowResolution};
use clap::Parser;

use bevy_beth::convert::nif_to_mesh;
use tes_nif::Nif;

/// Render a TES3 `.nif` model with Bevy.
#[derive(Parser, Debug)]
struct Args {
    /// Path to the `.nif` file to render.
    path: PathBuf,

    /// Where to write the screenshot PNG (screenshot mode only). Defaults to the system
    /// temp dir as `render_nif-<name>.png`.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Open a live, slowly-rotating window instead of capturing a single screenshot.
    #[arg(long)]
    interactive: bool,
}

/// The mesh and a camera framing distance, prepared before the Bevy app starts.
#[derive(Resource)]
struct Model {
    mesh: Mesh,
    /// Radius of the model's bounding sphere about its center, used to place the camera.
    radius: f32,
    center: Vec3,
    /// Height of the model's lowest point relative to its center, where the ground plane
    /// sits. Negative (the feet are below the center).
    floor_y: f32,
}

/// Screenshot destination; absent in `--interactive` mode.
#[derive(Resource)]
struct Capture {
    path: PathBuf,
}

/// Set by the [`ScreenshotCaptured`] observer once the PNG has been written to disk.
#[derive(Resource, Default)]
struct CaptureDone(bool);

/// Marks the pivot entity so the spin system (interactive mode) can find it.
#[derive(Component)]
struct Spin;

fn main() -> ExitCode {
    let args = Args::parse();

    let bytes = match std::fs::read(&args.path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args.path.display());
            return ExitCode::FAILURE;
        }
    };

    let nif = match Nif::parse(&bytes) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("cannot parse {}: {e}", args.path.display());
            return ExitCode::FAILURE;
        }
    };

    let Some(mesh) = nif_to_mesh(&nif) else {
        eprintln!("{}: no renderable geometry", args.path.display());
        return ExitCode::FAILURE;
    };

    let (center, radius, min_y) = bounding_sphere(&mesh);
    // The model is shifted by -center at spawn, so its lowest point lands at min_y - center.y.
    let floor_y = min_y - center.y;
    println!(
        "Loaded {} — {} tri shape(s), bounds r={radius:.1} about {center:?}",
        args.path.display(),
        nif.tri_shapes().count(),
    );

    let mut app = App::new();
    app.insert_resource(Model {
        mesh,
        radius,
        center,
        floor_y,
    })
    .add_systems(Startup, setup);

    if args.interactive {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("render_nif — {}", args.path.display()),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Update, spin);
    } else {
        // Screenshot mode: a hidden window is enough to drive the render target; we capture
        // one frame and exit. `close_when_requested` is off so nothing races our AppExit.
        let path = args.output.unwrap_or_else(|| default_screenshot_path(&args.path));
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
        .insert_resource(Capture { path })
        .init_resource::<CaptureDone>()
        .add_systems(Update, capture);
    }

    app.run();
    ExitCode::SUCCESS
}

fn setup(
    mut commands: Commands,
    model: Res<Model>,
    capture: Option<Res<Capture>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(model.mesh.clone());
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.7, 0.6),
        perceptual_roughness: 0.9,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // A pivot at the model's center; the mesh hangs off it shifted by -center so rotating
    // the pivot spins the model about its own middle rather than orbiting it. In screenshot
    // mode the pivot holds a fixed three-quarter yaw; in interactive mode `spin` drives it.
    let yaw = if capture.is_some() { -0.6 } else { 0.0 };
    commands
        .spawn((
            Transform::from_rotation(Quat::from_rotation_y(yaw)),
            // A non-renderable pivot still needs Visibility so the child mesh, which inherits
            // it, isn't culled (avoids the B0004 warning and a blank render).
            Visibility::default(),
            Spin,
        ))
        .with_child((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(-model.center),
        ));

    let r = model.radius.max(0.001);

    // A ground plane at the model's feet to catch its shadow. It's sized generously so the
    // shadow never falls off the edge, and sits a hair below the lowest vertex.
    let ground = meshes.add(Plane3d::default().mesh().size(r * 20.0, r * 20.0));
    let ground_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.35, 0.38),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(ground),
        MeshMaterial3d(ground_material),
        Transform::from_xyz(0.0, model.floor_y - r * 0.01, 0.0),
    ));

    // Frame the model: pull the camera back proportionally to its size.
    let cam_pos = Vec3::new(0.0, r * 0.6, r * 2.5);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(cam_pos).looking_at(Vec3::ZERO, Vec3::Y),
        // Ambient fill so faces turned away from the key light aren't pure black. Kept low so
        // the cast shadow still reads.
        AmbientLight {
            brightness: 200.0,
            ..default()
        },
    ));

    // Key light: casts shadows, with the cascade sized to the model so the shadow map has
    // enough resolution over the scene regardless of model scale.
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            maximum_distance: r * 8.0,
            ..default()
        }
        .build(),
    ));
}

fn spin(time: Res<Time>, mut q: Query<&mut Transform, With<Spin>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.4 * PI);
    }
}

/// Screenshot-mode driver: let a few frames render so the scene is warm, request one
/// screenshot, then exit once its observer reports the PNG has been written.
fn capture(
    mut commands: Commands,
    capture: Res<Capture>,
    done: Res<CaptureDone>,
    mut frame: Local<u32>,
    mut shot_requested: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;

    // Wait a few frames for meshes to upload and the first frame to render, then ask for the
    // screenshot. `save_to_disk` writes the file; the second observer flags completion so we
    // don't exit mid-readback.
    if *frame == 10 && !*shot_requested {
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

/// Default screenshot path: system temp dir, named after the model file.
fn default_screenshot_path(nif: &std::path::Path) -> PathBuf {
    let stem = nif.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    std::env::temp_dir().join(format!("render_nif-{stem}.png"))
}

/// Compute the model's bounding-sphere center and radius, plus its axis-aligned minimum
/// `y` (its lowest point), from the vertex positions.
fn bounding_sphere(mesh: &Mesh) -> (Vec3, f32, f32) {
    let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return (Vec3::ZERO, 1.0, -1.0);
    };
    if positions.is_empty() {
        return (Vec3::ZERO, 1.0, -1.0);
    }

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for &[x, y, z] in positions {
        let p = Vec3::new(x, y, z);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    let radius = positions
        .iter()
        .map(|&[x, y, z]| Vec3::new(x, y, z).distance(center))
        .fold(0.0_f32, f32::max);
    (center, radius, min.y)
}
