//! Render a TES3 (Morrowind) `.nif` model with Bevy, through the `tes://` asset pipeline.
//!
//! The model is addressed by its **game data path** (as the game would name it) and can
//! live loose in the data directory or inside a BSA archive — the VFS layers both:
//!
//! ```text
//! cargo run -p bevy-beth --example render_nif --features render -- meshes/i/in_de_shack_01.nif
//! ```
//!
//! By default this renders one frame off a fixed three-quarter view, saves it to a PNG,
//! and exits — printing the screenshot's absolute path so the image can be inspected.
//! Pass `-o out.png` to choose where the screenshot lands, `--data <dir>` for a
//! non-default data directory, `--dark` to stage a night scene (self-illumination such
//! as glow maps stands out against moonlight-level lighting), or `--interactive` to
//! instead open a live window with a fly camera: hold the right mouse button to look,
//! WASD to fly, Space/Q for up/down, Shift to go faster, scroll to rescale the base
//! speed.
//!
//! All the loading heavy lifting — scene-graph traversal, texture resolution through
//! loose files and archives, material construction — happens inside `bevy_beth`'s NIF
//! loader; this example only spawns the scene and stages a camera, lights and ground
//! around whatever geometry shows up.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{CursorGrabMode, CursorOptions, ExitCondition, PrimaryWindow, WindowResolution};
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use clap::Parser;

use bevy_beth::{BethPlugin, NifAsset};

/// Render a TES3 `.nif` model with Bevy.
#[derive(Parser, Debug)]
struct Args {
    /// Game-data path of the model, e.g. `meshes/i/in_de_shack_01.nif` (loose file or
    /// BSA-archived — both resolve).
    path: String,

    /// The Morrowind `Data Files` directory to serve (loose files + `*.bsa`).
    #[arg(long, default_value = "data")]
    data: PathBuf,

    /// Where to write the screenshot PNG (screenshot mode only). Defaults to the system
    /// temp dir as `render_nif-<name>.png`.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Open a live window with a fly camera instead of capturing a single screenshot.
    #[arg(long)]
    interactive: bool,

    /// Stage a night scene (moonlight-level key and ambient light) instead of full
    /// daylight — how the game shows glow maps and other self-illumination.
    #[arg(long)]
    dark: bool,
}

/// Handles to the loading model: the spawnable scene and the primary NIF asset (used to
/// notice empty models and report load failures).
#[derive(Resource)]
struct ModelScene {
    scene: Handle<WorldAsset>,
    nif: Handle<NifAsset>,
    path: String,
}

/// Set once `frame_scene` has staged the camera/lights/ground around the spawned model.
#[derive(Resource, Default)]
struct Framed(bool);

/// `--dark`: stage the set at night-time light levels.
#[derive(Resource, Default)]
struct DarkScene(bool);

/// Screenshot destination; absent in `--interactive` mode.
#[derive(Resource)]
struct Capture {
    path: PathBuf,
}

/// Set by the [`ScreenshotCaptured`] observer once the PNG has been written to disk.
#[derive(Resource, Default)]
struct CaptureDone(bool);

/// The pivot the model hangs off; screenshot mode turns it to the three-quarter view.
#[derive(Component)]
struct Pivot;

/// Child of the pivot holding the scene; `frame_scene` shifts it so the model's center
/// sits on the pivot (and thus on the world origin the camera is staged around).
#[derive(Component)]
struct Holder;

/// Fly camera (interactive mode): yaw/pitch track the mouse-look orientation, `speed` is
/// the base fly speed in world units per second, scaled to the model at framing time.
#[derive(Component)]
struct FreeCam {
    yaw: f32,
    pitch: f32,
    speed: f32,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut app = App::new();
    // BethPlugin must precede DefaultPlugins: asset sources register before AssetPlugin.
    app.add_plugins(BethPlugin::new(args.data.clone()));

    if args.interactive {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("render_nif — {}", args.path),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Update, free_cam);
    } else {
        // Screenshot mode: a hidden window is enough to drive the render target; we
        // capture one frame and exit. `close_when_requested` is off so nothing races our
        // AppExit.
        let path = args
            .output
            .clone()
            .unwrap_or_else(|| default_screenshot_path(&args.path));
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

    let model_path = args.path.clone();
    app.insert_resource(DarkScene(args.dark))
        .init_resource::<Framed>()
        .add_systems(
            Startup,
            move |mut commands: Commands, asset_server: Res<AssetServer>| {
                setup(&mut commands, &asset_server, &model_path);
            },
        )
        .add_systems(Update, frame_scene);

    match app.run() {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(_) => ExitCode::FAILURE,
    }
}

/// Spawn the model — pivot → holder → scene instance — and record the handles. Camera,
/// lights and ground wait until the scene has spawned and can be measured
/// ([`frame_scene`]).
fn setup(commands: &mut Commands, asset_server: &AssetServer, path: &str) {
    let scene: Handle<WorldAsset> = asset_server.load(format!("tes://{path}#Scene"));
    let nif: Handle<NifAsset> = asset_server.load(format!("tes://{path}"));

    let pivot = commands
        .spawn((
            Transform::default(),
            // A non-renderable pivot still needs Visibility so the child meshes, which
            // inherit it, aren't culled.
            Visibility::default(),
            Pivot,
        ))
        .id();
    commands.spawn((
        Transform::default(),
        Visibility::default(),
        Holder,
        WorldAssetRoot(scene.clone()),
        ChildOf(pivot),
    ));

    commands.insert_resource(ModelScene {
        scene,
        nif,
        path: path.to_string(),
    });
}

/// Once the scene instance has spawned, measure it and stage the set: recenter the model
/// on the pivot, then place ground plane, camera and key light proportionally to its
/// size. Runs once; also the failure exit for models that don't load or have no geometry.
#[allow(clippy::too_many_arguments)]
fn frame_scene(
    mut commands: Commands,
    mut framed: ResMut<Framed>,
    mut settle: Local<u32>,
    model: Res<ModelScene>,
    asset_server: Res<AssetServer>,
    nifs: Res<Assets<NifAsset>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    parts: Query<(&Mesh3d, &GlobalTransform)>,
    mut holder: Query<&mut Transform, (With<Holder>, Without<Pivot>)>,
    mut pivot: Query<&mut Transform, With<Pivot>>,
    capture: Option<Res<Capture>>,
    dark: Res<DarkScene>,
    mut exit: MessageWriter<AppExit>,
) {
    if framed.0 {
        return;
    }

    // Load failures (bad path, unsupported NIF) end the run with a message instead of a
    // silently empty window.
    if let bevy::asset::LoadState::Failed(err) = asset_server.load_state(&model.scene) {
        eprintln!("cannot load tes://{}: {err}", model.path);
        exit.write(AppExit::error());
        return;
    }
    // A model that parsed but has no drawable shapes would never populate the part query.
    if let Some(nif) = nifs.get(&model.nif)
        && nif.meshes.is_empty()
    {
        eprintln!("tes://{}: no renderable geometry", model.path);
        exit.write(AppExit::error());
        return;
    }

    if parts.is_empty() {
        return;
    }
    // Give transform propagation one frame after the instance spawns, so the
    // GlobalTransforms we measure are real.
    *settle += 1;
    if *settle < 2 {
        return;
    }

    // World-space bounds over every spawned part (pivot/holder are still identity here,
    // so world space == the model's Y-up space).
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let world_points = |mesh: &Mesh, gt: &GlobalTransform| {
        let points: Vec<Vec3> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v
                .iter()
                .map(|&[x, y, z]| gt.transform_point(Vec3::new(x, y, z)))
                .collect(),
            _ => Vec::new(),
        };
        points
    };
    for (mesh3d, gt) in &parts {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        for p in world_points(mesh, gt) {
            min = min.min(p);
            max = max.max(p);
        }
    }
    if !min.is_finite() {
        return;
    }
    let center = (min + max) * 0.5;
    let mut radius = 0.0_f32;
    for (mesh3d, gt) in &parts {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        for p in world_points(mesh, gt) {
            radius = radius.max(p.distance(center));
        }
    }
    let r = radius.max(0.001);

    // Recenter: shift the holder so the model's center sits on the pivot, then (in
    // screenshot mode) turn the pivot to the fixed three-quarter view. Rotation happens
    // about the origin the center now occupies, so the framing below stays valid.
    if let Ok(mut t) = holder.single_mut() {
        t.translation = -center;
    }
    if capture.is_some()
        && let Ok(mut t) = pivot.single_mut()
    {
        t.rotation = Quat::from_rotation_y(-0.6);
    }
    let floor_y = min.y - center.y;

    // A ground plane at the model's feet to catch its shadow, sized so it never falls off
    // the edge.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(r * 20.0, r * 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.35, 0.38),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, floor_y - r * 0.01, 0.0),
    ));

    // Frame the model: pull the camera back proportionally to its size, with ambient fill
    // so faces away from the key light aren't pure black. `--dark` drops both lights to
    // moonlight levels, leaving self-illumination (glow maps, emissive materials) as the
    // dominant source.
    let (ambient, illuminance) = if dark.0 {
        (12.0, 100.0)
    } else {
        (200.0, 6_000.0)
    };
    let camera_transform = Transform::from_translation(Vec3::new(0.0, r * 0.6, r * 2.5))
        .looking_at(Vec3::ZERO, Vec3::Y);
    let mut camera = commands.spawn((
        Camera3d::default(),
        camera_transform,
        AmbientLight {
            brightness: ambient,
            ..default()
        },
    ));
    if capture.is_none() {
        // Fly camera, starting from the framing view; base speed crosses the model in a
        // couple of seconds regardless of its scale.
        let (yaw, pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);
        camera.insert(FreeCam {
            yaw,
            pitch,
            speed: r * 1.5,
        });
        println!(
            "Controls: hold RMB to look, WASD to fly, Space/Q up/down, Shift boost, scroll for speed"
        );
    }

    // Key light: casts shadows, with the cascade sized to the model so the shadow map has
    // enough resolution regardless of model scale.
    commands.spawn((
        DirectionalLight {
            illuminance,
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

    println!("Framed tes://{} — r={r:.1} about {center:?}", model.path);
    framed.0 = true;
}

/// Fly-camera controller (interactive mode). Mouse-look engages only while the right
/// button is held — the cursor locks for clean relative deltas and releases with the
/// button, so the window stays usable. Movement is camera-relative except for the
/// vertical axis, which is world-up (the usual editor-freecam convention).
fn free_cam(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut cam: Query<(&mut Transform, &mut FreeCam)>,
) {
    let Ok((mut transform, mut cam)) = cam.single_mut() else {
        return;
    };

    let looking = buttons.pressed(MouseButton::Right);
    if let Ok(mut options) = cursor.single_mut() {
        let want = if looking {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        if options.grab_mode != want {
            options.grab_mode = want;
            options.visible = !looking;
        }
    }
    if looking && motion.delta != Vec2::ZERO {
        cam.yaw -= motion.delta.x * 0.003;
        // Stop just short of straight up/down so yaw stays well-defined.
        cam.pitch = (cam.pitch - motion.delta.y * 0.003).clamp(-1.54, 1.54);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
    }

    // Scroll rescales the base speed multiplicatively, so a few notches cover the range
    // from lockpick-sized to building-sized models. Clamped per frame because trackpads
    // report pixel deltas that would otherwise explode the exponent.
    if scroll.delta.y != 0.0 {
        cam.speed *= 1.15_f32.powf(scroll.delta.y.clamp(-4.0, 4.0));
    }

    let mut wish = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        wish += *transform.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        wish -= *transform.forward();
    }
    if keys.pressed(KeyCode::KeyA) {
        wish -= *transform.right();
    }
    if keys.pressed(KeyCode::KeyD) {
        wish += *transform.right();
    }
    if keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::KeyE) {
        wish += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::ControlLeft) {
        wish -= Vec3::Y;
    }
    if wish != Vec3::ZERO {
        let boost = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            4.0
        } else {
            1.0
        };
        transform.translation += wish.normalize() * cam.speed * boost * time.delta_secs();
    }
}

/// Screenshot-mode driver: once the scene is framed, let a few frames render so texture
/// uploads land, request one screenshot, then exit once its observer reports the PNG has
/// been written.
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

    if *frames_since_framed == 8 && !*shot_requested {
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
fn default_screenshot_path(model_path: &str) -> PathBuf {
    let stem = Path::new(model_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    std::env::temp_dir().join(format!("render_nif-{stem}.png"))
}
