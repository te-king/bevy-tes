//! Conversions from parsed Bethesda data into Bevy engine types.
//!
//! This is the home for the Bevy-coupled translation layer — NIF block graphs into
//! `Mesh` / `StandardMaterial` / `Scene`, and texture blobs (DDS/TGA) into `Image`. It
//! keeps the dependency direction one-way: the parser crates know nothing of Bevy; only
//! this crate bridges the two.
//!
//! Today it exposes [`nif_to_mesh`] (behind the `render` feature). Material and texture
//! conversion are still to come; planned work leans on the `image` / `ddsfile` crates for
//! texture decoding rather than hand-rolling DDS.

#[cfg(feature = "render")]
use bevy::asset::RenderAssetUsages;
#[cfg(feature = "render")]
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
#[cfg(feature = "render")]
use tes_nif::Nif;

/// Convert every `NiTriShape` in a parsed NIF into a single merged Bevy [`Mesh`].
///
/// Each shape's own local transform is baked into its vertices (and rotation into its
/// normals); ancestor `NiNode` transforms are not yet composed in. Returns `None` when the
/// NIF carries no triangle geometry.
///
/// Note: NIF/Morrowind is Z-up while Bevy is Y-up, so the result is rotated -90° about X
/// (Z→Y) to stand upright in a Bevy scene.
#[cfg(feature = "render")]
pub fn nif_to_mesh(nif: &Nif) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (transform, tri) in nif.tri_shapes() {
        if tri.vertices.is_empty() || tri.triangles.is_empty() {
            continue;
        }
        let base = positions.len() as u32;

        for &v in &tri.vertices {
            positions.push(z_up_to_y_up(transform.apply_point(v)));
        }

        // Normals are optional; fall back to a default so attribute lengths stay aligned.
        for i in 0..tri.vertices.len() {
            let n = tri.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
            normals.push(z_up_to_y_up(transform.apply_vector(n)));
        }

        for i in 0..tri.vertices.len() {
            uvs.push(tri.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
        }

        for t in &tri.triangles {
            indices.push(base + t[0] as u32);
            indices.push(base + t[1] as u32);
            indices.push(base + t[2] as u32);
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

/// Re-map a NIF (Z-up) position/direction into Bevy's (Y-up) convention: rotate -90° about
/// the X axis, i.e. `(x, y, z) -> (x, z, -y)`.
#[cfg(feature = "render")]
fn z_up_to_y_up([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, -y]
}
