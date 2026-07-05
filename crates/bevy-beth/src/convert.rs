//! Pure conversions from parsed NIF data into Bevy engine types.
//!
//! These are the stateless building blocks the scene builder (`scene` module) assembles:
//! geometry, transforms and surface parameters, each mapped 1:1 from the parser types.
//! The parser crates know nothing of Bevy; only this crate bridges the two.

use bevy::asset::{Handle, RenderAssetUsages};
use bevy::color::{Color, LinearRgba};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::{Mat3, Quat, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::pbr::StandardMaterial;
use bevy::transform::components::Transform;
use tes_nif::{NifTransform, TriMesh};

/// Convert a `NiTriShapeData` triangle mesh into a Bevy [`Mesh`], in the shape's **local
/// space** (the scene builder puts the transform on the entity instead).
///
/// Normals fall back to +Z and UVs to the origin where absent, so attribute lengths stay
/// aligned with the vertex count.
pub fn trimesh_to_mesh(tri: &TriMesh) -> Mesh {
    let positions: Vec<[f32; 3]> = tri.vertices.clone();
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(tri.vertices.len());
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(tri.vertices.len());
    for i in 0..tri.vertices.len() {
        normals.push(tri.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]));
        uvs.push(tri.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
    }
    let indices: Vec<u32> = tri
        .triangles
        .iter()
        .flat_map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
        .collect();

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Convert a NIF local transform into a Bevy [`Transform`].
///
/// `NifTransform::rotation` is a **row-major** matrix while [`Mat3::from_cols_array_2d`]
/// takes columns, hence the transpose. The NIF's Z-up convention is *not* handled here —
/// the scene builder applies one Z-up→Y-up rotation at the scene root instead of touching
/// every node.
pub fn nif_transform(t: &NifTransform) -> Transform {
    let rotation = Mat3::from_cols_array_2d(&t.rotation).transpose();
    Transform {
        translation: Vec3::from(t.translation),
        rotation: Quat::from_mat3(&rotation),
        scale: Vec3::splat(t.scale),
    }
}

/// Build the [`StandardMaterial`] for a shape from its resolved base-colour texture, NIF
/// material and alpha property, mirroring how the game shades fixed-function surfaces:
///
/// - with a material: diffuse tint (multiplied with the texture), emissive, and the
///   material alpha carried in the tint;
/// - textured but no material: white, so the texture shows unmodified;
/// - neither: a neutral tan stand-in.
///
/// Transparency follows the game's rule: it exists **only** when an `NiAlphaProperty`
/// asks for it — without one the shape is opaque no matter what its material alpha says.
/// See [`alpha_mode`] for how the property's flags map onto Bevy.
///
/// Morrowind geometry is frequently single-sided-authored but viewed from both sides
/// (tent flaps, foliage), so materials render double-sided without culling.
pub fn nif_material(
    base_color_texture: Option<Handle<Image>>,
    material: Option<&tes_nif::Material>,
    alpha: Option<tes_nif::AlphaProperty>,
) -> StandardMaterial {
    let textured = base_color_texture.is_some();
    let (base_color, emissive) = match material {
        Some(m) => (
            Color::srgba(m.diffuse[0], m.diffuse[1], m.diffuse[2], m.alpha),
            LinearRgba::from(Color::srgb(m.emissive[0], m.emissive[1], m.emissive[2])),
        ),
        None if textured => (Color::WHITE, LinearRgba::BLACK),
        None => (Color::srgb(0.8, 0.7, 0.6), LinearRgba::BLACK),
    };
    StandardMaterial {
        base_color,
        base_color_texture,
        emissive,
        alpha_mode: alpha_mode(alpha),
        // Morrowind's fixed-function look: matte unless a future glossiness mapping says
        // otherwise.
        perceptual_roughness: 0.9,
        double_sided: true,
        cull_mode: None,
        ..Default::default()
    }
}

/// Map an `NiAlphaProperty` onto Bevy's [`AlphaMode`]:
///
/// - alpha **testing** with a keep-if-above function → [`AlphaMode::Mask`] at the
///   property's threshold (crisp cutouts with correct depth — foliage leaf cards);
/// - otherwise alpha **blending** → [`AlphaMode::Add`] when the destination factor is
///   `ONE` (fire/glow effects accumulate light), else [`AlphaMode::Blend`];
/// - no property, or one with both features disabled → [`AlphaMode::Opaque`].
///
/// A shape with both testing and blending enabled becomes a mask: Bevy renders one mode
/// per material, and cutout-with-depth is the better approximation for the foliage that
/// combination appears on.
fn alpha_mode(alpha: Option<tes_nif::AlphaProperty>) -> AlphaMode {
    // `NiAlphaProperty` test functions (see `AlphaProperty::test_function`).
    const TEST_GREATER: u16 = 4;
    const TEST_GREATER_EQUAL: u16 = 6;
    // Blend factors (see `AlphaProperty::dest_blend_mode`).
    const BLEND_ONE: u16 = 0;

    match alpha {
        Some(a)
            if a.testing() && matches!(a.test_function(), TEST_GREATER | TEST_GREATER_EQUAL) =>
        {
            AlphaMode::Mask(a.threshold as f32 / 255.0)
        }
        Some(a) if a.blending() => {
            if a.dest_blend_mode() == BLEND_ONE {
                AlphaMode::Add
            } else {
                AlphaMode::Blend
            }
        }
        _ => AlphaMode::Opaque,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_property_maps_to_alpha_mode() {
        use tes_nif::AlphaProperty;
        let mode = |flags, threshold| alpha_mode(Some(AlphaProperty { flags, threshold }));

        // No property at all → opaque; material alpha alone never blends (game rule).
        assert_eq!(alpha_mode(None), AlphaMode::Opaque);
        // Testing GREATER + blending (typical foliage flags): the cutout wins.
        assert_eq!(
            mode(1 | (6 << 1) | (7 << 5) | 0x0200 | (4 << 10), 128),
            AlphaMode::Mask(128.0 / 255.0)
        );
        // Plain src-alpha / inv-src-alpha blending (windows, ghosts).
        assert_eq!(mode(1 | (6 << 1) | (7 << 5), 0), AlphaMode::Blend);
        // Destination factor ONE → additive (fire and glow effects).
        assert_eq!(mode(1 | (6 << 1), 0), AlphaMode::Add);
        // A property with everything switched off is explicitly opaque.
        assert_eq!(mode(0, 128), AlphaMode::Opaque);
    }

    #[test]
    fn transform_conversion_matches_nif_application() {
        // +90° about Z (row-major), scale 2, translation — the composed Bevy transform
        // must move points exactly like the NIF transform does.
        let nif_t = NifTransform {
            translation: [1.0, 2.0, 3.0],
            rotation: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            scale: 2.0,
        };
        let bevy_t = nif_transform(&nif_t);
        for p in [[1.0, 0.0, 0.0], [0.5, -3.0, 2.0]] {
            let expected = Vec3::from(nif_t.apply_point(p));
            let got = bevy_t.transform_point(Vec3::from(p));
            assert!(
                (expected - got).length() < 1e-5,
                "{p:?}: {expected} vs {got}"
            );
        }
    }
}
