//! Render a TES3 (Morrowind) `.nif` model in a Bevy window.
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
//! The NIF is parsed and converted to a Bevy [`Mesh`] up front (see
//! [`bevy_beth::convert::nif_to_mesh`]); the model is then framed by a camera and slowly
//! rotated so it can be inspected from all sides. Only static-mesh NIFs are supported —
//! animated, skinned and particle models will report an unsupported-block error.

use std::f32::consts::PI;
use std::path::PathBuf;
use std::process::ExitCode;

use bevy::prelude::*;
use clap::Parser;

use bevy_beth::convert::nif_to_mesh;
use tes_nif::Nif;

/// Render a TES3 `.nif` model in a Bevy window.
#[derive(Parser, Debug)]
struct Args {
    /// Path to the `.nif` file to render.
    path: PathBuf,
}

/// The mesh and a camera framing distance, prepared before the Bevy app starts.
#[derive(Resource)]
struct Model {
    mesh: Mesh,
    /// Radius of the model's bounding sphere about its center, used to place the camera.
    radius: f32,
    center: Vec3,
}

/// Marks the spawned model so the spin system can find it.
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

    let (center, radius) = bounding_sphere(&mesh);
    println!(
        "Loaded {} — {} tri shape(s), bounds r={radius:.1} about {center:?}",
        args.path.display(),
        nif.tri_shapes().count(),
    );

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("render_nif — {}", args.path.display()),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Model {
            mesh,
            radius,
            center,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, spin)
        .run();

    ExitCode::SUCCESS
}

fn setup(
    mut commands: Commands,
    model: Res<Model>,
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

    // Spin the model about the vertical axis around its own center.
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(-model.center),
        Spin,
    ));

    // Frame the model: pull the camera back proportionally to its size.
    let r = model.radius.max(0.001);
    let cam_pos = Vec3::new(0.0, r * 0.6, r * 2.5);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(cam_pos).looking_at(Vec3::ZERO, Vec3::Y),
        // Ambient fill so faces turned away from the key light aren't pure black.
        AmbientLight {
            brightness: 400.0,
            ..default()
        },
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            ..default()
        },
        Transform::from_xyz(1.0, 2.0, 1.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spin(time: Res<Time>, mut q: Query<&mut Transform, With<Spin>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.4 * PI);
    }
}

/// Compute the model's bounding-sphere center and radius from its vertex positions.
fn bounding_sphere(mesh: &Mesh) -> (Vec3, f32) {
    let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return (Vec3::ZERO, 1.0);
    };
    if positions.is_empty() {
        return (Vec3::ZERO, 1.0);
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
    (center, radius)
}
