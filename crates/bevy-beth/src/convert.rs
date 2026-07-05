//! Conversions from parsed Bethesda data into Bevy engine types.
//!
//! This is the home for the Bevy-coupled translation layer — NIF block graphs into
//! `Mesh` / `StandardMaterial` / `Scene`, and texture blobs (DDS/TGA) into `Image`. It
//! keeps the dependency direction one-way: the parser crates know nothing of Bevy; only
//! this crate bridges the two.
//!
//! Today it exposes [`nif_to_parts`] (one drawable mesh + material per shape) and
//! [`texture_to_image`] (decoding a texture's bytes) — both behind the `render` feature. DDS
//! decoding is delegated to Bevy's own image loader rather than hand-rolling it.

#[cfg(feature = "render")]
use bevy::asset::RenderAssetUsages;
#[cfg(feature = "render")]
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageSampler, ImageSamplerDescriptor,
    ImageType,
};
#[cfg(feature = "render")]
use bevy::render::mesh::{Indices, Mesh, PrimitiveTopology};
#[cfg(feature = "render")]
use tes_nif::Nif;

/// One drawable piece of a model — a single `NiTriShape` converted for Bevy, carrying its own
/// mesh, texture reference and material so distinct shapes keep their distinct surfaces.
#[cfg(feature = "render")]
pub struct NifPart {
    /// Geometry in Bevy world space: the shape's composed scene-graph transform and the
    /// NIF→Bevy axis change (Z-up → Y-up) are baked into the vertices and normals, so the
    /// part can be spawned with an identity transform.
    pub mesh: Mesh,
    /// Base-colour texture filename this part references (e.g. `Tx_BeerStein.dds`), if any.
    pub base_texture: Option<String>,
    /// Surface material (diffuse tint, emissive, opacity), if the shape had one.
    pub material: Option<NifMaterial>,
}

/// The subset of a NIF [`Material`](tes_nif::Material) that maps onto a Bevy
/// `StandardMaterial`.
#[cfg(feature = "render")]
pub struct NifMaterial {
    /// Diffuse tint, multiplied with the base texture. sRGB.
    pub diffuse: [f32; 3],
    /// Emissive colour. sRGB.
    pub emissive: [f32; 3],
    /// Opacity, `0.0`–`1.0`; below 1 the part should be drawn translucent.
    pub alpha: f32,
}

/// Convert a parsed NIF into one [`NifPart`] per drawable `NiTriShape`, walking the scene
/// graph so each part's world transform is composed and its texture/material resolved.
///
/// Shapes with no geometry are skipped, so the result may be empty. NIF/Morrowind is Z-up
/// while Bevy is Y-up, so each part's geometry is rotated -90° about X (Z→Y) as it is baked.
#[cfg(feature = "render")]
pub fn nif_to_parts(nif: &Nif) -> Vec<NifPart> {
    let mut parts = Vec::new();
    for shape in nif.instances() {
        let tri = shape.mesh;
        if tri.vertices.is_empty() || tri.triangles.is_empty() {
            continue;
        }
        let transform = shape.transform;

        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(tri.vertices.len());
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(tri.vertices.len());
        let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(tri.vertices.len());

        for &v in &tri.vertices {
            positions.push(z_up_to_y_up(transform.apply_point(v)));
        }
        for i in 0..tri.vertices.len() {
            // Normals are optional; fall back to a default so attribute lengths stay aligned.
            let n = tri.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]);
            normals.push(z_up_to_y_up(transform.apply_vector(n)));
            uvs.push(tri.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
        }
        let indices: Vec<u32> = tri
            .triangles
            .iter()
            .flat_map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect();

        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));

        parts.push(NifPart {
            mesh,
            base_texture: shape.base_texture.map(|n| n.decode().into_owned()),
            material: shape.material.map(|m| NifMaterial {
                diffuse: m.diffuse,
                emissive: m.emissive,
                alpha: m.alpha,
            }),
        });
    }
    parts
}

/// Re-map a NIF (Z-up) position/direction into Bevy's (Y-up) convention: rotate -90° about
/// the X axis, i.e. `(x, y, z) -> (x, z, -y)`.
#[cfg(feature = "render")]
fn z_up_to_y_up([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, -y]
}

/// Decode texture bytes into a Bevy [`Image`], treating them as sRGB base-colour data.
///
/// Morrowind ships textures as BC-compressed (DXT1/3/5) DDS and as TGA; decoding is Bevy's own
/// image loaders (the `dds`/`tga` features, pulled in by this crate's `render` feature), so
/// compressed data is uploaded to the GPU as-is. `format` is the file extension (e.g. `"dds"`
/// or `"tga"`). The image samples with **repeat** addressing, since Morrowind UVs tile beyond
/// `[0, 1]` (clamping instead smears edge texels into streaks). Returns `None` if the bytes
/// can't be decoded in that format.
#[cfg(feature = "render")]
pub fn texture_to_image(bytes: &[u8], format: &str) -> Option<Image> {
    let mut sampler = ImageSamplerDescriptor::default();
    sampler.set_address_mode(ImageAddressMode::Repeat);
    Image::from_buffer(
        bytes,
        ImageType::Extension(format),
        // Morrowind textures are BC (S3TC); every desktop GPU Bevy targets supports it.
        CompressedImageFormats::BC,
        true, // base-colour maps are authored in sRGB
        ImageSampler::Descriptor(sampler),
        RenderAssetUsages::default(),
    )
    .ok()
}
