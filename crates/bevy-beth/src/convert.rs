//! Pure conversions from parsed NIF and ESM data into Bevy engine types.
//!
//! These are the stateless building blocks the scene builder (`scene` module) and cell
//! spawner (`cell` module) assemble: geometry, transforms and surface parameters, each
//! mapped 1:1 from the parser types. The parser crates know nothing of Bevy; only this
//! crate bridges the two.

use bevy::asset::{Handle, RenderAssetUsages};
use bevy::color::{Color, ColorToComponents, LinearRgba};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::{Mat3, Quat, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::pbr::StandardMaterial;
use bevy::transform::components::Transform;
use tes_nif::{NifTransform, TriMesh};
use tes3_esm::records::cell::ReferenceTransform;
use tes3_esm::records::land::{CELL_SIZE, LAND_GRID, Land};

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

/// Convert a cell reference's placement into a Bevy Y-up [`Transform`], for an entity
/// whose child is a NIF `#Scene`.
///
/// The game frame is Z-up; positions are in game units and rotations are XYZ Euler
/// radians with the game's clockwise-positive convention (each axis rotation is applied
/// **negated**, Z then Y then X — the same construction OpenMW uses). Every NIF `#Scene`
/// already carries the Z-up→Y-up rotation `C` on its own root, so the game-frame rotation
/// is *conjugated* into the Y-up frame here rather than converted per-axis: a content
/// point `p` then passes `(C·q·C⁻¹)·C·p = C·(q·p)` — the game placement, axis-converted
/// exactly once. `scale` is the reference's `XSCL`, clamped to the engine's `[0.5, 2.0]`.
pub fn cell_reference_transform(t: &ReferenceTransform, scale: f32) -> Transform {
    let c = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let [rx, ry, rz] = t.rotation;
    let q_game =
        Quat::from_rotation_x(-rx) * Quat::from_rotation_y(-ry) * Quat::from_rotation_z(-rz);
    Transform {
        translation: c * Vec3::from(t.position), // (x, y, z) → (x, z, -y)
        rotation: c * q_game * c.inverse(),
        scale: Vec3::splat(scale.clamp(0.5, 2.0)),
    }
}

/// The Y-up world transform of a cell's terrain mesh: the cell's **south-west corner**
/// mapped through the game→Bevy axis conversion `(x, y, z) → (x, z, -y)`. Pair with
/// [`land_mesh`], which builds vertices relative to that corner.
pub fn land_transform(grid_x: i32, grid_y: i32) -> Transform {
    Transform::from_xyz(grid_x as f32 * CELL_SIZE, 0.0, -(grid_y as f32) * CELL_SIZE)
}

/// Build a Bevy [`Mesh`] from a `LAND` record's vertex grids, in **cell-local Y-up**
/// coordinates with the origin at the cell's south-west corner (pair with
/// [`land_transform`]). `None` when the record has no decodable heights.
///
/// Grid vertex `(x, y)` sits at game-frame offset `(x·128, y·128, height)` from the
/// corner; through the axis map that is `[x·128, height, -y·128]`. Because the map is a
/// proper rotation (determinant +1) baked entirely into the positions — including the
/// negated z — game-frame counter-clockwise-up triangle winding carries over unchanged,
/// so front faces point +Y and default backface culling shows the terrain from above.
/// Each quad uses a fixed diagonal (vanilla alternates them checkerboard-style; the
/// visual difference is negligible at terrain scale).
///
/// `VNML` normals map through the same axis conversion; when absent the mesh computes
/// smooth normals from the geometry instead. `VCLR` vertex colors are gamma-space bytes
/// and are converted sRGB→linear for Bevy's pipeline ([`StandardMaterial`] modulates by
/// vertex color automatically); the attribute is omitted when `VCLR` is absent.
pub fn land_mesh(land: &Land) -> Option<Mesh> {
    const N: usize = LAND_GRID;
    let heights = land.decode_heights()?;
    let spacing = CELL_SIZE / (N - 1) as f32;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(N * N);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(N * N);
    for y in 0..N {
        for x in 0..N {
            positions.push([
                x as f32 * spacing,
                heights[y * N + x],
                -(y as f32) * spacing,
            ]);
            uvs.push([x as f32 / (N - 1) as f32, 1.0 - y as f32 / (N - 1) as f32]);
        }
    }

    let mut indices: Vec<u32> = Vec::with_capacity((N - 1) * (N - 1) * 6);
    for y in 0..N - 1 {
        for x in 0..N - 1 {
            let (a, b, c, d) = (
                (y * N + x) as u32,
                (y * N + x + 1) as u32,
                ((y + 1) * N + x) as u32,
                ((y + 1) * N + x + 1) as u32,
            );
            indices.extend([a, b, c, b, d, c]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    if let Some(normals) = land.decode_normals() {
        let normals: Vec<[f32; 3]> = normals.iter().map(|[x, y, z]| [*x, *z, -*y]).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    } else {
        mesh.compute_smooth_normals();
    }

    if let Some(colors) = land.decode_colors() {
        let colors: Vec<[f32; 4]> = colors
            .iter()
            .map(|&[r, g, b]| Color::srgb_u8(r, g, b).to_linear().to_f32_array())
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }

    Some(mesh)
}

/// Build the [`StandardMaterial`] for a shape from its resolved base-colour and glow-map
/// textures, NIF material and alpha property, mirroring how the game shades
/// fixed-function surfaces:
///
/// - with a material: diffuse tint (multiplied with the texture), emissive, and the
///   material alpha carried in the tint;
/// - textured but no material: white, so the texture shows unmodified;
/// - neither: a neutral tan stand-in.
///
/// A glow map becomes the [`emissive_texture`](StandardMaterial::emissive_texture). Bevy
/// *multiplies* it with the `emissive` factor while the game's fixed-function pipeline
/// *adds* the glow stage independently of the material's emissive colour — so with a glow
/// map present the factor is forced to white (the map alone drives self-illumination),
/// which matches vanilla assets: their glow-mapped shapes author black material emissive.
///
/// Transparency follows the game's rule: it exists **only** when an `NiAlphaProperty`
/// asks for it — without one the shape is opaque no matter what its material alpha says.
/// See [`alpha_mode`] for how the property's flags map onto Bevy.
///
/// Morrowind geometry is frequently single-sided-authored but viewed from both sides
/// (tent flaps, foliage), so materials render double-sided without culling.
pub fn nif_material(
    base_color_texture: Option<Handle<Image>>,
    emissive_texture: Option<Handle<Image>>,
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
    let emissive = if emissive_texture.is_some() {
        LinearRgba::WHITE
    } else {
        emissive
    };
    StandardMaterial {
        base_color,
        base_color_texture,
        emissive,
        emissive_texture,
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

    /// Quaternion equality via the dot product (handles the q/−q double cover;
    /// `angle_between`'s `acos` turns f32 noise into ~1e-4 near identity).
    fn assert_quat_eq(got: Quat, expected: Quat) {
        assert!(
            got.dot(expected).abs() > 1.0 - 1e-6,
            "{got:?} vs {expected:?}"
        );
    }

    #[test]
    fn cell_reference_position_maps_zup_to_yup() {
        let t = ReferenceTransform {
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0],
        };
        let out = cell_reference_transform(&t, 1.0);
        assert!((out.translation - Vec3::new(1.0, 3.0, -2.0)).length() < 1e-6);
        assert_quat_eq(out.rotation, Quat::IDENTITY);
    }

    #[test]
    fn cell_reference_yaw_pins_to_a_yup_yaw() {
        // A game-frame yaw (about Z-up) must become a Bevy yaw (about Y-up) of the same
        // handedness: 90° clockwise seen from above = Ry(-90°) in Bevy.
        let t = ReferenceTransform {
            position: [0.0; 3],
            rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
        };
        let out = cell_reference_transform(&t, 1.0);
        let expected = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        assert_quat_eq(out.rotation, expected);
    }

    #[test]
    fn cell_reference_transform_composes_with_the_nif_scene_root() {
        // Full chain check: a point in NIF model space passes the #Scene root's internal
        // Z-up→Y-up rotation, then this transform. The result must equal the game-frame
        // placement (rotate, scale, translate in Z-up) converted to Y-up once.
        let c = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let t = ReferenceTransform {
            position: [10.0, 20.0, 30.0],
            rotation: [0.1, 0.2, 0.3],
        };
        let out = cell_reference_transform(&t, 2.0);
        let q_game =
            Quat::from_rotation_x(-0.1) * Quat::from_rotation_y(-0.2) * Quat::from_rotation_z(-0.3);
        for p in [Vec3::X, Vec3::new(0.5, -3.0, 2.0)] {
            let got = out.transform_point(c * p);
            let expected = c * (q_game * (2.0 * p) + Vec3::from(t.position));
            assert!((got - expected).length() < 1e-4, "{p}: {got} vs {expected}");
        }
    }

    /// Buffers backing a synthetic LAND (the `Land` view borrows them; see
    /// [`SyntheticLand::land`]).
    struct SyntheticLand {
        base: f32,
        deltas: Vec<u8>,
        normals: Option<Vec<u8>>,
        colors: Option<Vec<u8>>,
    }

    impl SyntheticLand {
        fn land(&self) -> Land<'_> {
            Land {
                height_offset: Some(self.base),
                heights: Some(&self.deltas),
                normals: self.normals.as_deref(),
                colors: self.colors.as_deref(),
                ..Land::default()
            }
        }
    }

    /// A synthetic LAND: flat `base` height except vertex (1, 2) raised by `bump` VHGT
    /// units, with optional uniform normals/colors.
    fn synthetic_land(
        base: f32,
        bump: i8,
        normals: Option<[u8; 3]>,
        colors: bool,
    ) -> SyntheticLand {
        let mut deltas = vec![0u8; LAND_GRID * LAND_GRID];
        // Raise (1, 2) then restore the running sum at (2, 2) so the rest stays flat.
        deltas[2 * LAND_GRID + 1] = bump as u8;
        deltas[2 * LAND_GRID + 2] = (-bump) as u8;
        SyntheticLand {
            base,
            deltas,
            normals: normals.map(|n| {
                n.iter()
                    .copied()
                    .cycle()
                    .take(LAND_GRID * LAND_GRID * 3)
                    .collect()
            }),
            colors: colors.then(|| {
                [255u8, 0, 128]
                    .iter()
                    .copied()
                    .cycle()
                    .take(LAND_GRID * LAND_GRID * 3)
                    .collect()
            }),
        }
    }

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
    }

    #[test]
    fn land_mesh_layout_and_positions() {
        let mesh = land_mesh(&synthetic_land(0.0, 5, None, false).land()).unwrap();
        let positions = positions(&mesh);
        assert_eq!(positions.len(), 65 * 65);
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("expected U32 indices");
        };
        assert_eq!(indices.len(), 64 * 64 * 6);
        // Vertex (1, 2): east 1 step, north 2 steps, bumped 5 VHGT units = 40 game units.
        assert_eq!(positions[2 * 65 + 1], [128.0, 40.0, -256.0]);
        // Its east neighbor is back at the base height.
        assert_eq!(positions[2 * 65 + 2], [256.0, 0.0, -256.0]);
        // UVs: (64, 0) is the south-east corner → image-space bottom-right.
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x2(v) => v,
            other => panic!("unexpected UV format: {other:?}"),
        };
        assert_eq!(uvs[64], [1.0, 1.0]);
        assert_eq!(uvs[64 * 65], [0.0, 0.0]);
    }

    #[test]
    fn land_mesh_winding_faces_up() {
        let mesh = land_mesh(&synthetic_land(0.0, 0, None, false).land()).unwrap();
        let positions = positions(&mesh);
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("expected U32 indices");
        };
        // Counter-clockwise front faces must point +Y (up) so terrain survives default
        // backface culling when viewed from above.
        for tri in indices.chunks_exact(3).take(4) {
            let [p0, p1, p2] = [
                Vec3::from(positions[tri[0] as usize]),
                Vec3::from(positions[tri[1] as usize]),
                Vec3::from(positions[tri[2] as usize]),
            ];
            let normal = (p1 - p0).cross(p2 - p0);
            assert!(normal.y > 0.0, "triangle {tri:?} faces {normal}");
        }
    }

    #[test]
    fn land_mesh_maps_normals_and_colors() {
        // Game +Z normals become Bevy +Y.
        let mesh = land_mesh(&synthetic_land(0.0, 0, Some([0, 0, 127]), true).land()).unwrap();
        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x3(v) => v,
            other => panic!("unexpected normal format: {other:?}"),
        };
        assert_eq!(normals[0], [0.0, 1.0, 0.0]);

        // VCLR bytes are gamma-space: (255, 0, 128) → linear (1.0, 0.0, ~0.2158).
        let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x4(v) => v,
            other => panic!("unexpected color format: {other:?}"),
        };
        let expected = Color::srgb_u8(255, 0, 128).to_linear().to_f32_array();
        assert_eq!(colors[0], expected);
        assert!((colors[0][2] - 0.2158) < 1e-3);

        // VNML absent → normals still present (computed); VCLR absent → no color attribute.
        let bare = land_mesh(&synthetic_land(0.0, 0, None, false).land()).unwrap();
        assert!(bare.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        assert!(bare.attribute(Mesh::ATTRIBUTE_COLOR).is_none());
    }

    #[test]
    fn land_mesh_requires_heights() {
        assert!(land_mesh(&Land::default()).is_none());
    }

    #[test]
    fn land_transform_maps_grid_corner() {
        let t = land_transform(-3, -2);
        assert_eq!(t.translation, Vec3::new(-24576.0, 0.0, 16384.0));
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
