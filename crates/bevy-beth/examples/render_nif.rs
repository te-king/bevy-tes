//! Render a TES3 (Morrowind) `.nif` model with Bevy.
//!
//! By default this renders one frame off a fixed three-quarter view, saves it to a PNG, and
//! exits — printing the screenshot's absolute path so the image can be inspected:
//!
//! ```text
//! cargo run -p bevy-beth --example render_nif --features render -- path/to/model.nif
//! ```
//!
//! Try a local fixture from the `data/` tree (see `data/README.md`):
//!
//! ```text
//! cargo run -p bevy-beth --example render_nif --features render -- \
//!     data/meshes/cursor.nif
//! ```
//!
//! Pass `-o out.png` to choose where the screenshot lands, or `--interactive` to instead
//! open a live window that slowly rotates the model for inspection from all sides.
//!
//! The NIF's scene graph is walked into one drawable part per `NiTriShape` up front (see
//! [`bevy_beth::convert::nif_to_parts`]), each keeping its own composed transform, texture and
//! material — so a multi-part model renders with its distinct surfaces. Each referenced
//! base-colour texture is resolved against a sibling `textures/` directory (e.g.
//! `data/textures/`) and decoded via [`bevy_beth::convert::texture_to_image`]. Only
//! static-mesh NIFs are supported — animated, skinned and particle models report an
//! unsupported-block error.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{ExitCondition, WindowResolution};
use clap::Parser;

use bevy_beth::convert::{NifPart, nif_to_parts, texture_to_image};
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

/// A single drawable part, prepared before the Bevy app starts: its (world-space) mesh, the
/// name of its base texture if any, and the colours for its material.
struct PreparedPart {
    mesh: Mesh,
    /// Key into [`Model::textures`] for this part's base-colour map, if it has one.
    texture_name: Option<String>,
    /// Base colour (with opacity in the alpha channel): a diffuse tint over the texture, or
    /// the flat surface colour when untextured.
    base_color: Color,
    emissive: Color,
    /// Whether this part should be drawn translucent (material alpha below 1).
    translucent: bool,
}

/// The prepared model and camera framing, built before the Bevy app starts.
#[derive(Resource)]
struct Model {
    /// One entry per drawable `NiTriShape`.
    parts: Vec<PreparedPart>,
    /// Decoded base-colour textures, keyed by filename and shared across parts that reference
    /// the same one (so each is uploaded to the GPU once).
    textures: HashMap<String, Image>,
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

    let raw_parts = nif_to_parts(&nif);
    if raw_parts.is_empty() {
        eprintln!("{}: no renderable geometry", args.path.display());
        return ExitCode::FAILURE;
    }

    // Decode each distinct base texture once, resolving it against the model's directory.
    let mut textures: HashMap<String, Image> = HashMap::new();
    for part in &raw_parts {
        if let Some(name) = &part.base_texture
            && !textures.contains_key(name)
            && let Some(image) = load_texture(name, &args.path)
        {
            textures.insert(name.clone(), image);
        }
    }

    let parts: Vec<PreparedPart> = raw_parts.into_iter().map(|p| prepare_part(p, &textures)).collect();

    let (center, radius, min_y) = aggregate_bounds(&parts);
    // Parts are shifted by -center at spawn, so the lowest point lands at min_y - center.y.
    let floor_y = min_y - center.y;
    println!(
        "Loaded {} — {} part(s), {} texture(s), bounds r={radius:.1} about {center:?}",
        args.path.display(),
        parts.len(),
        textures.len(),
    );

    let mut app = App::new();
    app.insert_resource(Model {
        parts,
        textures,
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
    mut images: ResMut<Assets<Image>>,
) {
    // Upload each distinct texture once, keyed by name, so parts sharing a texture share a
    // handle (and a single GPU upload).
    let texture_handles: HashMap<&str, Handle<Image>> = model
        .textures
        .iter()
        .map(|(name, image)| (name.as_str(), images.add(image.clone())))
        .collect();

    // A pivot at the model's center; every part hangs off it shifted by -center so rotating
    // the pivot spins the model about its own middle rather than orbiting it. In screenshot
    // mode the pivot holds a fixed three-quarter yaw; in interactive mode `spin` drives it.
    let yaw = if capture.is_some() { -0.6 } else { 0.0 };
    let pivot = commands
        .spawn((
            Transform::from_rotation(Quat::from_rotation_y(yaw)),
            // A non-renderable pivot still needs Visibility so the child meshes, which inherit
            // it, aren't culled (avoids the B0004 warning and a blank render).
            Visibility::default(),
            Spin,
        ))
        .id();

    for part in &model.parts {
        let base_color_texture = part
            .texture_name
            .as_deref()
            .and_then(|name| texture_handles.get(name).cloned());
        let material = materials.add(StandardMaterial {
            base_color: part.base_color,
            base_color_texture,
            emissive: part.emissive.into(),
            alpha_mode: if part.translucent {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            perceptual_roughness: 0.9,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(meshes.add(part.mesh.clone())),
            MeshMaterial3d(material),
            Transform::from_translation(-model.center),
            ChildOf(pivot),
        ));
    }

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
fn default_screenshot_path(nif: &Path) -> PathBuf {
    let stem = nif.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    std::env::temp_dir().join(format!("render_nif-{stem}.png"))
}

/// Turn a converted [`NifPart`] into a [`PreparedPart`], choosing its Bevy colours: a
/// diffuse tint over the texture, or the flat surface colour when untextured (falling back to
/// a neutral tan when the shape had neither texture nor material). `texture_name` is kept only
/// when the texture actually decoded, so a missing texture degrades to the tint.
fn prepare_part(part: NifPart, textures: &HashMap<String, Image>) -> PreparedPart {
    let texture_name = part.base_texture.filter(|n| textures.contains_key(n));
    let has_texture = texture_name.is_some();

    let (base_color, emissive, translucent) = match &part.material {
        Some(m) => {
            let [r, g, b] = m.diffuse;
            let [er, eg, eb] = m.emissive;
            (
                Color::srgba(r, g, b, m.alpha),
                Color::srgb(er, eg, eb),
                m.alpha < 0.999,
            )
        }
        // No material: white so a texture shows its true colours, or a neutral tan when there
        // is nothing else to go on.
        None if has_texture => (Color::WHITE, Color::BLACK, false),
        None => (Color::srgb(0.8, 0.7, 0.6), Color::BLACK, false),
    };

    PreparedPart {
        mesh: part.mesh,
        texture_name,
        base_color,
        emissive,
        translucent,
    }
}

/// Resolve, read and decode a NIF-referenced texture, printing what happened. Returns `None`
/// (having warned) when the file can't be found, read, or decoded — the part then renders
/// with its material tint instead of failing the run.
fn load_texture(name: &str, nif_path: &Path) -> Option<Image> {
    let Some(path) = resolve_texture(nif_path, name) else {
        eprintln!(
            "  texture {name:?} referenced but not found near {}",
            nif_path.display()
        );
        return None;
    };
    let bytes = std::fs::read(&path)
        .map_err(|e| eprintln!("  cannot read texture {}: {e}", path.display()))
        .ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("dds");
    match texture_to_image(&bytes, ext) {
        Some(image) => {
            println!("  texture: {}", path.display());
            Some(image)
        }
        None => {
            eprintln!("  failed to decode texture {}", path.display());
            None
        }
    }
}

/// Locate a NIF-referenced texture on disk. NIF texture names use Windows separators and may
/// carry a `textures\` prefix, so we reduce to the bare filename, then walk up the model's
/// ancestor directories looking for the file either loose in that directory or under a
/// `textures/` subdirectory — this finds `data/textures/foo.tga` for a mesh nested at
/// `data/meshes/i/bar.nif` (the game's `Data Files` layout). Morrowind sometimes ships a
/// `.tga`-named texture as `.dds` (and vice versa), so both extensions are tried.
fn resolve_texture(nif_path: &Path, tex_name: &str) -> Option<PathBuf> {
    let base = tex_name.rsplit(['\\', '/']).next().unwrap_or(tex_name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    let names = [base.to_string(), format!("{stem}.dds"), format!("{stem}.tga")];

    // `ancestors()` includes the file itself first; skip it to start at the model's directory.
    nif_path.ancestors().skip(1).find_map(|dir| {
        names.iter().find_map(|name| {
            let loose = dir.join(name);
            if loose.exists() {
                return Some(loose);
            }
            let in_textures = dir.join("textures").join(name);
            in_textures.exists().then_some(in_textures)
        })
    })
}

/// Aggregate bounding-sphere center and radius over all parts, plus the axis-aligned minimum
/// `y` (the model's lowest point), from their world-space vertex positions.
fn aggregate_bounds(parts: &[PreparedPart]) -> (Vec3, f32, f32) {
    let positions = || {
        parts.iter().filter_map(|p| {
            match p.mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(bevy::render::mesh::VertexAttributeValues::Float32x3(v)) => Some(v),
                _ => None,
            }
        })
    };

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for verts in positions() {
        for &[x, y, z] in verts {
            let p = Vec3::new(x, y, z);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if !min.is_finite() {
        return (Vec3::ZERO, 1.0, -1.0);
    }

    let center = (min + max) * 0.5;
    let radius = positions()
        .flatten()
        .map(|&[x, y, z]| Vec3::new(x, y, z).distance(center))
        .fold(0.0_f32, f32::max);
    (center, radius, min.y)
}
