//! Block-body parsers, one per block type Morrowind ships.
//!
//! Because 4.0.0.2 stores no block sizes, [`parse_block`] must decode every body exactly
//! to reach the next block — a mis-sized parser desyncs the stream and garbles the next
//! type-name read (the footer check in [`Nif::parse`](crate::Nif::parse) catches a desync
//! in the last block). Blocks the crate doesn't model are decoded just far enough to walk
//! past and returned as [`Block::Other`].

use tes_core::L1String;

use crate::NifError;
use crate::blocks::{
    AlphaProperty, Block, BlockRef, Material, NifHeader, NifTransform, Node, SourceTexture,
    TexturingProperty, TriMesh, TriShape,
};
use crate::reader::{RResult, ReadError, Reader};

/// Parse the version-4.0.0.2 header: identifier line (terminated by `\n`), then the
/// version and block-count `u32`s.
pub(crate) fn nif_header(r: &mut Reader) -> RResult<NifHeader> {
    let ident = L1String::from_bytes(r.line()?.to_vec());
    let version = r.u32()?;
    let num_blocks = r.u32()?;
    Ok(NifHeader {
        ident,
        version,
        num_blocks,
    })
}

/// Dispatch a block body by its type name.
pub(crate) fn parse_block(r: &mut Reader, ty: &str) -> Result<Block, NifError> {
    parse_block_inner(r, ty).map_err(|e| match e {
        Some(re) => re.at(ty.to_string()),
        None => NifError::Parse(format!("unsupported block type {ty:?}")),
    })
}

/// Returns `Err(None)` for an unsupported type, `Err(Some(_))` for a short read.
fn parse_block_inner(r: &mut Reader, ty: &str) -> Result<Block, Option<ReadError>> {
    let block = match ty {
        // Node-like blocks all share the NiNode body in 4.0.0.2 (a billboard node's mode
        // lives in its flags at this version).
        "NiNode" | "RootCollisionNode" | "AvoidNode" | "NiBSAnimationNode" | "NiBSParticleNode"
        | "NiBillboardNode" => {
            let av = av_object(r)?;
            let hidden = av.hidden();
            let children = r.refs()?;
            let _effects = r.refs()?;
            Block::Node(Node {
                transform: av.transform,
                children,
                properties: av.properties,
                hidden,
                collision: ty == "RootCollisionNode",
            })
        }
        "NiTriShape" => {
            let av = av_object(r)?;
            let hidden = av.hidden();
            let data = r.block_ref()?;
            r.block_ref()?; // skin instance ref
            Block::TriShape(TriShape {
                transform: av.transform,
                data,
                properties: av.properties,
                hidden,
            })
        }
        "NiTriShapeData" => Block::TriShapeData(tri_shape_data(r)?),

        // Particle-system leaves share the NiGeometry body (same as NiTriShape); their
        // data blocks are the geometry-data prefix plus a per-particle tail. Walked past:
        // this crate doesn't simulate particles, so they carry no drawable geometry here.
        "NiParticles" | "NiRotatingParticles" | "NiAutoNormalParticles" => {
            av_object(r)?;
            r.block_ref()?; // data
            r.block_ref()?; // skin instance
            Block::Other
        }
        "NiParticlesData" | "NiAutoNormalParticlesData" => {
            particles_data(r)?;
            Block::Other
        }
        "NiRotatingParticlesData" => {
            let nv = particles_data(r)?;
            if r.boolean()? {
                r.skip(nv * 16)?; // per-particle rotation quaternions
            }
            Block::Other
        }

        "NiTexturingProperty" => Block::TexturingProperty(texturing_property(r)?),
        "NiMaterialProperty" => Block::MaterialProperty(material_property(r)?),
        "NiAlphaProperty" => {
            ni_object_net(r)?;
            let flags = r.u16()?;
            let threshold = r.u8()?;
            Block::AlphaProperty(AlphaProperty { flags, threshold })
        }
        "NiVertexColorProperty" => {
            ni_object_net(r)?;
            r.skip(2 + 4 + 4)?; // flags + vertex mode + lighting mode
            Block::Other
        }
        "NiZBufferProperty"
        | "NiSpecularProperty"
        | "NiWireframeProperty"
        | "NiShadeProperty"
        | "NiDitherProperty"
        | "NiFogProperty" => {
            ni_object_net(r)?;
            r.skip(2)?; // flags
            Block::Other
        }
        "NiSourceTexture" => Block::SourceTexture(SourceTexture {
            file_name: source_texture(r)?,
        }),
        "NiStringExtraData" => {
            r.block_ref()?; // next extra data
            r.u32()?; // bytes remaining
            r.string()?;
            Block::Other
        }
        "NiTextKeyExtraData" => {
            r.block_ref()?; // next extra data
            r.u32()?; // bytes remaining
            let n = r.u32()?;
            for _ in 0..n {
                r.f32()?; // time
                r.string()?; // value
            }
            Block::Other
        }
        "NiVertWeightsExtraData" => {
            r.block_ref()?; // next extra data
            r.u32()?; // bytes remaining
            let n = r.u16()? as usize;
            r.skip(n * 4)?; // per-vertex weights
            Block::Other
        }

        // Animation controllers: the NiTimeController body plus a type-specific tail.
        // Walked past — this crate doesn't animate.
        "NiKeyframeController"
        | "NiVisController"
        | "NiAlphaController"
        | "NiMaterialColorController"
        | "NiRollController" => {
            time_controller(r)?;
            r.block_ref()?; // data
            Block::Other
        }
        "NiUVController" => {
            time_controller(r)?;
            r.u16()?; // texture set
            r.block_ref()?; // data
            Block::Other
        }
        "NiGeomMorpherController" => {
            time_controller(r)?;
            r.block_ref()?; // data
            r.u8()?; // always update
            Block::Other
        }
        "NiPathController" => {
            time_controller(r)?;
            r.i32()?; // bank direction
            r.f32()?; // max bank angle
            r.f32()?; // smoothing
            r.u16()?; // follow axis
            r.block_ref()?; // path (position) data
            r.block_ref()?; // percent (float) data
            Block::Other
        }
        "NiLookAtController" => {
            time_controller(r)?;
            r.block_ref()?; // look-at target
            Block::Other
        }
        "NiFlipController" => {
            time_controller(r)?;
            r.u32()?; // texture slot
            r.f32()?; // accumulated time
            r.f32()?; // delta between flips
            r.refs()?; // texture sources
            Block::Other
        }
        "NiParticleSystemController" | "NiBSPArrayController" => {
            particle_system_controller(r)?;
            Block::Other
        }

        // Animation key data.
        "NiKeyframeData" => {
            keyframe_data(r)?;
            Block::Other
        }
        "NiFloatData" => {
            key_group(r, 4)?;
            Block::Other
        }
        "NiPosData" => {
            key_group(r, 12)?;
            Block::Other
        }
        "NiColorData" => {
            key_group(r, 16)?;
            Block::Other
        }
        "NiUVData" => {
            // U/V translation then U/V scale key groups.
            for _ in 0..4 {
                key_group(r, 4)?;
            }
            Block::Other
        }
        "NiVisData" => {
            let n = r.u32()? as usize;
            r.skip(n * 5)?; // time f32 + visibility byte per key
            Block::Other
        }
        "NiMorphData" => {
            let num_morphs = r.u32()? as usize;
            let nv = r.u32()? as usize;
            r.u8()?; // relative targets
            for _ in 0..num_morphs {
                let num_keys = r.u32()? as usize;
                // Unlike a KeyGroup, a morph's key type is present even with zero keys.
                let interpolation = r.u32()?;
                keys(r, num_keys, interpolation, 4)?;
                r.skip(nv * 12)?; // morph target vertex offsets
            }
            Block::Other
        }

        // Skinning.
        "NiSkinInstance" => {
            r.block_ref()?; // data
            r.block_ref()?; // skeleton root
            r.refs()?; // bones
            Block::Other
        }
        "NiSkinData" => {
            skin_transform(r)?;
            let num_bones = r.u32()? as usize;
            r.block_ref()?; // skin partition (optional hardware-skinning data)
            for _ in 0..num_bones {
                skin_transform(r)?;
                r.skip(12 + 4)?; // bounding sphere center + radius
                let nv = r.u16()? as usize;
                r.skip(nv * 6)?; // vertex index (u16) + weight (f32) pairs
            }
            Block::Other
        }

        // Dynamic effects, cameras and embedded textures.
        "NiTextureEffect" => {
            dynamic_effect(r)?;
            r.skip(36 + 12)?; // model projection matrix + translation
            r.skip(4 * 4)?; // filtering + clamping + texture type + coordinate generation
            r.block_ref()?; // source texture
            r.u8()?; // clipping-plane enable
            r.skip(16)?; // clipping plane (normal + constant)
            r.skip(2 + 2 + 2)?; // PS2 L + PS2 K + unknown short
            Block::Other
        }
        "NiCamera" => {
            av_object(r)?;
            r.skip(6 * 4)?; // frustum left/right/top/bottom/near/far
            r.skip(4 * 4)?; // viewport left/right/top/bottom
            r.f32()?; // LOD adjust
            r.block_ref()?; // scene
            r.u32()?; // num screen polygons (always 0)
            Block::Other
        }
        "NiPixelData" => {
            r.skip(4 + 4 * 4 + 4 + 8)?; // format + channel masks + bpp + fast-compare bytes
            r.block_ref()?; // palette
            let num_mipmaps = r.u32()? as usize;
            r.u32()?; // bytes per pixel
            r.skip(num_mipmaps * 12)?; // width/height/offset per mipmap
            let num_bytes = r.u32()? as usize;
            r.skip(num_bytes)?; // pixel data
            Block::Other
        }

        // Particle modifiers: a next-modifier/controller header plus fixed fields.
        "NiGravity" => {
            particle_modifier(r)?;
            r.skip(4 + 4 + 4 + 12 + 12)?; // decay + force + field type + position + direction
            Block::Other
        }
        "NiParticleGrowFade" => {
            particle_modifier(r)?;
            r.skip(4 + 4)?; // grow time + fade time
            Block::Other
        }
        "NiParticleColorModifier" => {
            particle_modifier(r)?;
            r.block_ref()?; // color data
            Block::Other
        }
        "NiParticleRotation" => {
            particle_modifier(r)?;
            r.skip(1 + 12 + 4)?; // random-axis flag + initial axis + rotation speed
            Block::Other
        }
        "NiParticleBomb" => {
            particle_modifier(r)?;
            r.skip(4 * 4 + 4 + 12 + 12)?; // decay/duration/deltaV/start + decay type + position + direction
            Block::Other
        }
        "NiPlanarCollider" => {
            particle_modifier(r)?;
            r.f32()?; // bounce
            r.skip(4 + 4 + 12 + 12 + 12 + 16)?; // height + width + position + X/Y axes + plane
            Block::Other
        }
        "NiSphericalCollider" => {
            particle_modifier(r)?;
            r.f32()?; // bounce
            r.skip(4 + 12)?; // radius + position
            Block::Other
        }
        _ => return Err(None),
    };
    Ok(block)
}

/// `NiTimeController` (the common controller body): next-controller ref, flags, timing
/// parameters and the target object.
fn time_controller(r: &mut Reader) -> RResult<()> {
    r.block_ref()?; // next controller
    r.u16()?; // flags
    r.f32()?; // frequency
    r.f32()?; // phase
    r.f32()?; // start time
    r.f32()?; // stop time
    r.block_ref()?; // target
    Ok(())
}

/// `NiParticleSystemController` (also `NiBSPArrayController`): emitter parameters plus the
/// saved per-particle state.
fn particle_system_controller(r: &mut Reader) -> RResult<()> {
    time_controller(r)?;
    r.skip(6 * 4)?; // speed/variation + declination/variation + planar angle/variation
    r.skip(12 + 16 + 4)?; // initial normal + initial color (rgba) + initial size
    r.skip(4 + 4)?; // emit start/stop time
    r.u8()?; // reset particle system
    r.skip(4 + 4 + 4)?; // birth rate + lifetime + lifetime variation
    r.u8()?; // use birth rate
    r.u8()?; // spawn on death
    r.skip(12)?; // emitter dimensions
    r.block_ref()?; // emitter object
    r.skip(2 + 4 + 2 + 4 + 4)?; // spawn generations + % spawned + multiplier + speed/dir chaos
    let num_particles = r.u16()? as usize;
    r.u16()?; // num valid
    r.skip(num_particles * 40)?; // saved per-particle state (NiParticleInfo)
    r.block_ref()?; // emitter modifier
    r.block_ref()?; // particle modifier chain
    r.block_ref()?; // particle collider
    r.u8()?; // static target bound
    Ok(())
}

/// The tail of a `Key<T>` sequence: each key is a time plus a `elem`-byte value, with
/// quadratic keys (type 2) adding forward/backward tangents and TBC keys (type 3) adding
/// tension/bias/continuity.
fn keys(r: &mut Reader, count: usize, interpolation: u32, elem: usize) -> RResult<()> {
    let per_key = 4
        + elem
        + match interpolation {
            2 => elem * 2, // forward + backward tangents
            3 => 12,       // tension + bias + continuity
            _ => 0,
        };
    r.skip(count * per_key)
}

/// `KeyGroup<T>`: a key count, the interpolation type (only when non-empty), and the keys,
/// with `elem`-byte values.
fn key_group(r: &mut Reader, elem: usize) -> RResult<()> {
    let n = r.u32()? as usize;
    if n > 0 {
        let interpolation = r.u32()?;
        keys(r, n, interpolation, elem)?;
    }
    Ok(())
}

/// `NiKeyframeData`: quaternion (or per-axis) rotation keys, then translation and scale
/// key groups.
fn keyframe_data(r: &mut Reader) -> RResult<()> {
    let num_rotations = r.u32()? as usize;
    if num_rotations > 0 {
        let rotation_type = r.u32()?;
        if rotation_type == 4 {
            // Euler rotation: an axis-order float then a key group per axis.
            r.f32()?;
            for _ in 0..3 {
                key_group(r, 4)?;
            }
        } else {
            // Quaternion keys never carry tangents; TBC keys (type 3) add the TBC triple.
            let per_key = 4 + 16 + if rotation_type == 3 { 12 } else { 0 };
            r.skip(num_rotations * per_key)?;
        }
    }
    key_group(r, 12)?; // translations
    key_group(r, 4)?; // scales
    Ok(())
}

/// The `NiTransform` layout used by skinning data: a 3×3 rotation, translation and scale.
fn skin_transform(r: &mut Reader) -> RResult<()> {
    r.skip(36 + 12 + 4)
}

/// `NiParticlesData` (also its auto-normal alias): the geometry-data prefix plus particle
/// radius/activity and optional per-particle sizes. Returns the vertex (particle) count
/// for tails that need it.
fn particles_data(r: &mut Reader) -> RResult<usize> {
    let g = geometry_data(r)?;
    r.u16()?; // num particles
    r.f32()?; // particle radius
    r.u16()?; // num active
    if r.boolean()? {
        r.skip(g.nv * 4)?; // per-particle sizes
    }
    Ok(g.nv)
}

/// `NiDynamicEffect` (4.0.0.2): the `NiAVObject` body plus the affected-node list, stored
/// at this version as stale pointer hashes rather than block refs.
fn dynamic_effect(r: &mut Reader) -> RResult<()> {
    av_object(r)?;
    let n = r.u32()? as usize;
    r.skip(n * 4)?;
    Ok(())
}

/// `NiParticleModifier` (the common modifier body): next-modifier ref and controller ref.
fn particle_modifier(r: &mut Reader) -> RResult<()> {
    r.block_ref()?; // next modifier
    r.block_ref()?; // parent controller
    Ok(())
}

/// `NiObjectNET`: name string + extra-data ref + controller ref.
fn ni_object_net(r: &mut Reader) -> RResult<L1String> {
    let name = r.string()?;
    r.block_ref()?; // extra data
    r.block_ref()?; // controller
    Ok(name)
}

/// The decoded common `NiAVObject` fields: local transform, attached property refs, and the
/// flags word (bit 0 = hidden).
struct AvObject {
    transform: NifTransform,
    properties: Vec<BlockRef>,
    flags: u16,
}

impl AvObject {
    /// The `NiAVObject` "hidden" flag (bit 0): the object and its subtree are not drawn.
    fn hidden(&self) -> bool {
        self.flags & 0x0001 != 0
    }
}

/// `NiAVObject`: `NiObjectNET` + flags + local transform + velocity + property refs +
/// optional bounding box.
fn av_object(r: &mut Reader) -> RResult<AvObject> {
    ni_object_net(r)?;
    let flags = r.u16()?;
    let translation = r.vec3()?;
    let rotation = [r.vec3()?, r.vec3()?, r.vec3()?];
    let scale = r.f32()?;
    r.vec3()?; // velocity
    let properties = r.refs()?;
    if r.boolean()? {
        // bounding box: unknown u32 + center vec3 + 3x3 axes + 3 extents
        r.skip(4 + 12 + 36 + 12)?;
    }
    Ok(AvObject {
        transform: NifTransform {
            translation,
            rotation,
            scale,
        },
        properties,
        flags,
    })
}

/// The common `NiGeometryData` prefix (4.0.0.2 layout, no inherited name field):
/// vertices, normals, bounding sphere, vertex colors and UV sets. Shared by
/// `NiTriShapeData` and the particle data blocks, which append their own tails.
struct GeometryData {
    /// Vertex count — the length unit for the per-vertex arrays that may follow.
    nv: usize,
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

fn geometry_data(r: &mut Reader) -> RResult<GeometryData> {
    let nv = r.u16()? as usize;

    let has_vertices = r.boolean()?;
    let vertices = if has_vertices {
        (0..nv).map(|_| r.vec3()).collect::<RResult<_>>()?
    } else {
        Vec::new()
    };

    let has_normals = r.boolean()?;
    let normals = if has_normals {
        (0..nv).map(|_| r.vec3()).collect::<RResult<_>>()?
    } else {
        Vec::new()
    };

    r.vec3()?; // bounding sphere center
    r.f32()?; // bounding sphere radius

    if r.boolean()? {
        // vertex colors: rgba per vertex
        r.skip(nv * 16)?;
    }

    let num_uv_sets = r.u16()? as usize;
    let has_uv = r.boolean()?;
    let mut uvs = Vec::new();
    if has_uv {
        for set in 0..num_uv_sets {
            if set == 0 {
                uvs = (0..nv)
                    .map(|_| Ok([r.f32()?, r.f32()?]))
                    .collect::<RResult<_>>()?;
            } else {
                r.skip(nv * 8)?;
            }
        }
    }

    Ok(GeometryData {
        nv,
        vertices,
        normals,
        uvs,
    })
}

/// `NiTriShapeData`: the geometry-data prefix plus triangles and match groups.
fn tri_shape_data(r: &mut Reader) -> RResult<TriMesh> {
    let g = geometry_data(r)?;

    let num_triangles = r.u16()? as usize;
    r.u32()?; // num triangle points (== 3 * num_triangles)
    let triangles = (0..num_triangles)
        .map(|_| Ok([r.u16()?, r.u16()?, r.u16()?]))
        .collect::<RResult<_>>()?;

    let num_match_groups = r.u16()? as usize;
    for _ in 0..num_match_groups {
        let count = r.u16()? as usize;
        r.skip(count * 2)?;
    }

    Ok(TriMesh {
        vertices: g.vertices,
        normals: g.normals,
        uvs: g.uvs,
        triangles,
    })
}

/// `NiMaterialProperty`: `NiObjectNET` + flags, then the four colours, glossiness and alpha.
fn material_property(r: &mut Reader) -> RResult<Material> {
    ni_object_net(r)?;
    r.u16()?; // flags
    let ambient = r.vec3()?;
    let diffuse = r.vec3()?;
    let specular = r.vec3()?;
    let emissive = r.vec3()?;
    let glossiness = r.f32()?;
    let alpha = r.f32()?;
    Ok(Material {
        ambient,
        diffuse,
        specular,
        emissive,
        glossiness,
        alpha,
    })
}

/// `NiTexturingProperty`: walk past flags, apply mode and the texture descriptors, keeping
/// the `NiSourceTexture` references of the slots the crate models — base (slot 0) and glow
/// (slot 4). Unused slots stay [`BlockRef::NONE`].
fn texturing_property(r: &mut Reader) -> RResult<TexturingProperty> {
    ni_object_net(r)?;
    r.u16()?; // flags
    r.u32()?; // apply mode
    let texture_count = r.u32()? as usize;
    let mut prop = TexturingProperty::default();
    for slot in 0..texture_count {
        if r.boolean()? {
            // TexDesc: source ref + clamp + filter + uv set + ps2 l/k + unknown1
            let source = r.block_ref()?;
            match slot {
                0 => prop.base_texture = source,
                4 => prop.glow_texture = source,
                _ => {}
            }
            r.skip(4 + 4 + 4 + 2 + 2 + 2)?; // clamp + filter + uv set + ps2 l/k + unknown1
            // The bump-map slot (index 5) carries an extra scale/offset and 2x2 matrix.
            if slot == 5 {
                r.skip(4 * 6)?;
            }
        }
    }
    Ok(prop)
}

/// `NiSourceTexture`: read the external filename (or walk past internal pixel data) plus the
/// format flags, returning the filename — empty when the texture is internally embedded.
fn source_texture(r: &mut Reader) -> RResult<L1String> {
    ni_object_net(r)?;
    let use_external = r.u8()?;
    let file_name = if use_external != 0 {
        r.string()?
    } else {
        r.u8()?; // unknown byte
        r.block_ref()?; // pixel data
        L1String::default()
    };
    r.skip(4 + 4 + 4 + 1)?; // pixel layout + mipmap format + alpha format + is static
    Ok(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Nif, VERSION_TES3};

    fn synthetic_header(num_blocks: u32) -> Vec<u8> {
        let mut bytes = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
        bytes.extend_from_slice(&VERSION_TES3.to_le_bytes());
        bytes.extend_from_slice(&num_blocks.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_header_fields() {
        let bytes = synthetic_header(3);
        let h = nif_header(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(h.version, VERSION_TES3);
        assert_eq!(h.num_blocks, 3);
        assert_eq!(h.ident, "NetImmerse File Format, Version 4.0.0.2");
    }

    #[test]
    fn rejects_wrong_version() {
        let mut bytes = b"NetImmerse File Format, Version 10.0.1.0\n".to_vec();
        bytes.extend_from_slice(&0x0A01_0000u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        assert!(Nif::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_unsupported_block() {
        let mut bytes = synthetic_header(1);
        let name = b"NiUnheardOfBlockType";
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name);
        let err = Nif::parse(&bytes).unwrap_err();
        assert!(format!("{err}").contains("NiUnheardOfBlockType"));
    }

    #[test]
    fn texturing_property_captures_base_and_glow_slots_exactly() {
        // A synthetic `NiTexturingProperty` body with the base (0), glow (4) and bump (5)
        // slots present. Pins both which refs are kept and — via the full-consumption
        // check — the exact byte walk, including the bump slot's extra scale/matrix data.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // name (empty)
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // extra data ref
        bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller ref
        bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
        bytes.extend_from_slice(&2u32.to_le_bytes()); // apply mode
        bytes.extend_from_slice(&7u32.to_le_bytes()); // texture count
        fn tex_desc(bytes: &mut Vec<u8>, source: i32, bump: bool) {
            bytes.extend_from_slice(&1u32.to_le_bytes()); // has texture
            bytes.extend_from_slice(&source.to_le_bytes());
            bytes.extend_from_slice(&[0u8; 18]); // clamp + filter + uv set + ps2 l/k + unknown1
            if bump {
                bytes.extend_from_slice(&[0u8; 24]); // luma scale/offset + 2x2 matrix
            }
        }
        tex_desc(&mut bytes, 3, false); // slot 0: base
        for _ in 1..4 {
            bytes.extend_from_slice(&0u32.to_le_bytes()); // slots 1-3 absent
        }
        tex_desc(&mut bytes, 5, false); // slot 4: glow
        tex_desc(&mut bytes, 6, true); // slot 5: bump (walked past)
        bytes.extend_from_slice(&0u32.to_le_bytes()); // slot 6 absent

        let mut r = Reader::new(&bytes);
        let prop = texturing_property(&mut r).unwrap();
        assert_eq!(prop.base_texture, BlockRef(3));
        assert_eq!(prop.glow_texture, BlockRef(5));
        assert_eq!(r.remaining(), 0, "body must be consumed exactly");
    }
}
