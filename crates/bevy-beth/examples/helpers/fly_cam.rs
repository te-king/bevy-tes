//! Fly-camera controller shared by the render examples via `#[path]` inclusion (Cargo
//! doesn't compile files in `examples/` subdirectories as targets).

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

/// Fly camera (interactive mode): yaw/pitch track the mouse-look orientation, `speed` is
/// the base fly speed in world units per second, scaled to the scene at framing time.
#[derive(Component)]
pub struct FreeCam {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

/// Fly-camera controller (interactive mode). Mouse-look engages only while the right
/// button is held — the cursor locks for clean relative deltas and releases with the
/// button, so the window stays usable. Movement is camera-relative except for the
/// vertical axis, which is world-up (the usual editor-freecam convention).
pub fn free_cam(
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
    // from lockpick-sized to building-sized scenes. Clamped per frame because trackpads
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
