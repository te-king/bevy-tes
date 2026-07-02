//! `tes-nif` — parser for NetImmerse/Gamebryo `.nif` model files (TES3 / Morrowind).
//!
//! # Scope
//!
//! Morrowind ships NIF version **4.0.0.2** (`0x0400_0002`). Unlike the later 20.x NIFs
//! (Oblivion+), a 4.0.0.2 file has **no block-type table and no block-size table** in its
//! header. Instead each block is preceded inline by its type name (a length-prefixed
//! string) and block sizes are *implicit*: the only way to find where one block ends is to
//! fully decode the current block's body. So traversal requires a body parser per block
//! type — there is no generic "skip unknown" path.
//!
//! This crate decodes the **static-mesh block graph** — the blocks needed to get triangle
//! geometry out of a model: [`Block::Node`] (`NiNode` and friends), [`Block::TriShape`]
//! (`NiTriShape`) and [`Block::TriShapeData`] (`NiTriShapeData`, the actual mesh). The
//! various property / texture / extra-data blocks are decoded only far enough to walk past
//! them (kept as [`Block::Other`] so block indices — which references rely on — stay
//! aligned). Block types outside this set (particle systems, controllers, …) cause
//! [`Nif::parse`] to fail with [`NifError::Parse`] naming the unsupported type.
//!
//! ```no_run
//! let bytes = std::fs::read("model.nif").unwrap();
//! let nif = tes_nif::Nif::parse(&bytes).unwrap();
//! assert_eq!(nif.header.version, tes_nif::VERSION_TES3);
//! for (transform, mesh) in nif.tri_shapes() {
//!     println!("{} vertices, {} triangles", mesh.vertices.len(), mesh.triangles.len());
//! }
//! ```

use nom::IResult;
use nom::bytes::complete::take;
use nom::number::complete::le_u32;
use std::fmt;
use tes_core::L1String;

/// The NIF version Morrowind/Tribunal/Bloodmoon use: `4.0.0.2`.
pub const VERSION_TES3: u32 = 0x0400_0002;

/// Error returned when reading or parsing a NIF file.
#[derive(Debug)]
pub enum NifError {
    /// I/O failure while reading the file from disk.
    Io(std::io::Error),
    /// The byte stream could not be parsed as a supported NIF.
    Parse(String),
}

impl fmt::Display for NifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NifError::Io(e) => write!(f, "I/O error: {e}"),
            NifError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for NifError {}

impl From<std::io::Error> for NifError {
    fn from(e: std::io::Error) -> Self {
        NifError::Io(e)
    }
}

/// The NIF header (version 4.0.0.2 layout): a newline-terminated identifier string, the
/// numeric version, and the number of blocks that follow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NifHeader {
    /// The version identifier line, e.g. `NetImmerse File Format, Version 4.0.0.2`
    /// (without the trailing newline).
    pub ident: L1String,
    /// Numeric version, e.g. [`VERSION_TES3`].
    pub version: u32,
    /// Number of blocks following the header.
    pub num_blocks: u32,
}

/// A local node transform: translation, a row-major 3×3 rotation matrix, and a uniform
/// scale. Taken straight from the block's `NiAVObject` fields; it is *not* composed with
/// ancestor transforms (full scene-graph composition is left to the caller).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NifTransform {
    pub translation: [f32; 3],
    /// Row-major rotation matrix (`rotation[row][col]`).
    pub rotation: [[f32; 3]; 3],
    pub scale: f32,
}

impl Default for NifTransform {
    fn default() -> Self {
        NifTransform {
            translation: [0.0; 3],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            scale: 1.0,
        }
    }
}

impl NifTransform {
    /// Apply this transform to a point: `translation + scale * (rotation * point)`.
    pub fn apply_point(&self, p: [f32; 3]) -> [f32; 3] {
        let r = self.apply_vector(p);
        [
            r[0] * self.scale + self.translation[0],
            r[1] * self.scale + self.translation[1],
            r[2] * self.scale + self.translation[2],
        ]
    }

    /// Apply only the rotation to a direction vector (no translation, no scale).
    pub fn apply_vector(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.rotation;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }
}

/// Triangle mesh geometry decoded from a `NiTriShapeData` block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    pub vertices: Vec<[f32; 3]>,
    /// Per-vertex normals; empty when the block has none.
    pub normals: Vec<[f32; 3]>,
    /// First UV set, per vertex; empty when the block has none.
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u16; 3]>,
}

/// One decoded NIF block. Every block in the file produces exactly one entry (in file
/// order), so an index into [`Nif::blocks`] matches the block references stored in other
/// blocks (e.g. [`Block::TriShape::data`]). Blocks this crate doesn't model are kept as
/// [`Block::Other`] purely to preserve that indexing.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// `NiNode` (and node-like blocks): a transform plus child block references.
    Node {
        transform: NifTransform,
        children: Vec<i32>,
    },
    /// `NiTriShape`: a transform plus a reference to its [`Block::TriShapeData`].
    TriShape {
        transform: NifTransform,
        /// Block index of the geometry data, or `-1` for none.
        data: i32,
    },
    /// `NiTriShapeData`: the actual triangle geometry.
    TriShapeData(TriMesh),
    /// A block parsed past but not represented (properties, textures, extra data, …).
    Other,
}

/// A parsed NIF file: its [`NifHeader`] and the decoded [`Block`] graph.
#[derive(Debug)]
pub struct Nif {
    pub header: NifHeader,
    /// Decoded blocks, one per block in the file, in file order.
    pub blocks: Vec<Block>,
}

impl Nif {
    /// Parse a NIF from an in-memory byte slice. Validates the version is [`VERSION_TES3`]
    /// and decodes every block (failing on any block type outside the supported set).
    pub fn parse(input: &[u8]) -> Result<Nif, NifError> {
        let (rest, header) =
            nif_header(input).map_err(|e| NifError::Parse(format!("header: {e:?}")))?;
        if header.version != VERSION_TES3 {
            return Err(NifError::Parse(format!(
                "unsupported NIF version {:#010x} (expected {:#010x})",
                header.version, VERSION_TES3
            )));
        }

        let mut r = Reader::new(rest);
        let mut blocks = Vec::with_capacity(header.num_blocks as usize);
        for i in 0..header.num_blocks {
            let ty = r.string().map_err(|e| e.at(format!("block {i} type")))?;
            let block = parse_block(&mut r, &ty.decode()).map_err(|e| match e {
                NifError::Parse(msg) => NifError::Parse(format!("block {i}: {msg}")),
                other => other,
            })?;
            blocks.push(block);
        }
        // A footer (root count + root refs) follows the blocks; it's not needed for
        // geometry, so we simply ignore whatever trails the last block.

        Ok(Nif { header, blocks })
    }

    /// Iterate every `NiTriShape` paired with its geometry, as
    /// `(transform, &TriMesh)`. Only the shape's own local transform is reported; ancestor
    /// `NiNode` transforms are not composed in.
    pub fn tri_shapes(&self) -> impl Iterator<Item = (NifTransform, &TriMesh)> {
        self.blocks.iter().filter_map(|b| match b {
            Block::TriShape { transform, data } => {
                let mesh = self.blocks.get(*data as usize).and_then(|d| match d {
                    Block::TriShapeData(m) => Some(m),
                    _ => None,
                })?;
                Some((*transform, mesh))
            }
            _ => None,
        })
    }
}

/// Parse the version-4.0.0.2 header: identifier line (terminated by `\n`), then the
/// version and block-count `u32`s.
fn nif_header(input: &[u8]) -> IResult<&[u8], NifHeader> {
    let nl = input
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| nom_fail(input))?;
    let ident = L1String::from_bytes(input[..nl].to_vec());
    let input = &input[nl + 1..];

    let (input, version) = le_u32(input)?;
    let (input, num_blocks) = le_u32(input)?;
    Ok((
        input,
        NifHeader {
            ident,
            version,
            num_blocks,
        },
    ))
}

/// Read a block's inline type name: a little-endian `u32` length prefix followed by that
/// many bytes (e.g. `NiNode`). This is the framing every 4.0.0.2 block begins with.
pub fn block_type(input: &[u8]) -> IResult<&[u8], L1String> {
    let (input, len) = le_u32(input)?;
    let (input, bytes) = take(len as usize)(input)?;
    Ok((input, L1String::from_bytes(bytes.to_vec())))
}

/// Build a nom error anchored at `input` for use with the `?` operator.
fn nom_fail(input: &[u8]) -> nom::Err<nom::error::Error<&[u8]>> {
    nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
}

// --- block body parsing -----------------------------------------------------------------

/// A simple sequential cursor over the byte stream with bounds-checked little-endian reads.
/// All reads advance the cursor; a short read produces a [`ReadError`].
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// A short read while decoding a block body. Carries a description set via [`ReadError::at`].
struct ReadError(String);

impl ReadError {
    fn at(self, context: impl Into<String>) -> NifError {
        NifError::Parse(format!("{}: {}", context.into(), self.0))
    }
}

type RResult<T> = Result<T, ReadError>;

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> RResult<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let slice = &self.data[self.pos..end];
                self.pos = end;
                Ok(slice)
            }
            None => Err(ReadError(format!(
                "unexpected end of data (wanted {n} bytes at offset {})",
                self.pos
            ))),
        }
    }

    fn u8(&mut self) -> RResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> RResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> RResult<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> RResult<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> RResult<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// A boolean. In NIF ≤ 4.0.0.2 a `bool` is serialized as a 4-byte `uint`.
    fn boolean(&mut self) -> RResult<bool> {
        Ok(self.u32()? != 0)
    }

    /// A `u32`-length-prefixed string (the framing used by block type names and by every
    /// `SizedString` field; not null-terminated).
    fn string(&mut self) -> RResult<L1String> {
        let len = self.u32()? as usize;
        Ok(L1String::from_bytes(self.take(len)?.to_vec()))
    }

    fn vec3(&mut self) -> RResult<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn skip(&mut self, n: usize) -> RResult<()> {
        self.take(n).map(|_| ())
    }

    /// A `u32` count followed by that many `i32` block references.
    fn refs(&mut self) -> RResult<Vec<i32>> {
        let n = self.u32()? as usize;
        (0..n).map(|_| self.i32()).collect()
    }
}

/// Dispatch a block body by its type name.
fn parse_block(r: &mut Reader, ty: &str) -> Result<Block, NifError> {
    parse_block_inner(r, ty).map_err(|e| match e {
        Some(re) => re.at(ty.to_string()),
        None => NifError::Parse(format!("unsupported block type {ty:?}")),
    })
}

/// Returns `Err(None)` for an unsupported type, `Err(Some(_))` for a short read.
fn parse_block_inner(r: &mut Reader, ty: &str) -> Result<Block, Option<ReadError>> {
    let block = match ty {
        // Node-like blocks all share the NiNode body in 4.0.0.2.
        "NiNode" | "RootCollisionNode" | "AvoidNode" | "NiBSAnimationNode"
        | "NiBSParticleNode" => {
            let (transform, _props) = av_object(r)?;
            let children = r.refs()?;
            let _effects = r.refs()?;
            Block::Node {
                transform,
                children,
            }
        }
        "NiTriShape" => {
            let (transform, _props) = av_object(r)?;
            let data = r.i32()?;
            let _skin = r.i32()?;
            Block::TriShape { transform, data }
        }
        "NiTriShapeData" => Block::TriShapeData(tri_shape_data(r)?),

        // Properties: decoded only enough to skip past them.
        "NiTexturingProperty" => {
            texturing_property(r)?;
            Block::Other
        }
        "NiMaterialProperty" => {
            ni_object_net(r)?;
            r.skip(2)?; // flags
            r.skip(4 * 4 * 3 + 4 * 2)?; // 4 colors (rgb) + glossiness + alpha
            Block::Other
        }
        "NiAlphaProperty" => {
            ni_object_net(r)?;
            r.skip(2 + 1)?; // flags + threshold
            Block::Other
        }
        "NiVertexColorProperty" => {
            ni_object_net(r)?;
            r.skip(2 + 4 + 4)?; // flags + vertex mode + lighting mode
            Block::Other
        }
        "NiZBufferProperty" | "NiSpecularProperty" | "NiWireframeProperty"
        | "NiShadeProperty" | "NiDitherProperty" | "NiFogProperty" => {
            ni_object_net(r)?;
            r.skip(2)?; // flags
            Block::Other
        }
        "NiSourceTexture" => {
            source_texture(r)?;
            Block::Other
        }
        "NiStringExtraData" => {
            r.i32()?; // next extra data ref
            r.u32()?; // bytes remaining
            r.string()?;
            Block::Other
        }
        "NiTextKeyExtraData" => {
            r.i32()?; // next extra data ref
            r.u32()?; // bytes remaining
            let n = r.u32()?;
            for _ in 0..n {
                r.f32()?; // time
                r.string()?; // value
            }
            Block::Other
        }
        _ => return Err(None),
    };
    Ok(block)
}

/// `NiObjectNET`: name string + extra-data ref + controller ref.
fn ni_object_net(r: &mut Reader) -> RResult<L1String> {
    let name = r.string()?;
    r.i32()?; // extra data ref
    r.i32()?; // controller ref
    Ok(name)
}

/// `NiAVObject`: `NiObjectNET` + flags + local transform + velocity + property refs +
/// optional bounding box. Returns the local transform and the property references.
fn av_object(r: &mut Reader) -> RResult<(NifTransform, Vec<i32>)> {
    ni_object_net(r)?;
    r.u16()?; // flags
    let translation = r.vec3()?;
    let rotation = [r.vec3()?, r.vec3()?, r.vec3()?];
    let scale = r.f32()?;
    r.vec3()?; // velocity
    let properties = r.refs()?;
    if r.boolean()? {
        // bounding box: unknown u32 + center vec3 + 3x3 axes + 3 extents
        r.skip(4 + 12 + 36 + 12)?;
    }
    Ok((
        NifTransform {
            translation,
            rotation,
            scale,
        },
        properties,
    ))
}

/// `NiTriShapeData` (the 4.0.0.2 `NiGeometryData` layout, no inherited name field).
fn tri_shape_data(r: &mut Reader) -> RResult<TriMesh> {
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
        vertices,
        normals,
        uvs,
        triangles,
    })
}

/// `NiTexturingProperty`: walk past flags, apply mode and the texture descriptors.
fn texturing_property(r: &mut Reader) -> RResult<()> {
    ni_object_net(r)?;
    r.u16()?; // flags
    r.u32()?; // apply mode
    let texture_count = r.u32()? as usize;
    for slot in 0..texture_count {
        if r.boolean()? {
            // TexDesc: source ref + clamp + filter + uv set + ps2 l/k + unknown1
            r.skip(4 + 4 + 4 + 4 + 2 + 2 + 2)?;
            // The bump-map slot (index 5) carries an extra scale/offset and 2x2 matrix.
            if slot == 5 {
                r.skip(4 * 6)?;
            }
        }
    }
    Ok(())
}

/// `NiSourceTexture`: walk past the external filename or internal pixel data plus format
/// flags.
fn source_texture(r: &mut Reader) -> RResult<()> {
    ni_object_net(r)?;
    let use_external = r.u8()?;
    if use_external != 0 {
        r.string()?; // file name
    } else {
        r.u8()?; // unknown byte
        r.i32()?; // pixel data ref
    }
    r.skip(4 + 4 + 4 + 1)?; // pixel layout + mipmap format + alpha format + is static
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_header(num_blocks: u32) -> Vec<u8> {
        let mut bytes = b"NetImmerse File Format, Version 4.0.0.2\n".to_vec();
        bytes.extend_from_slice(&VERSION_TES3.to_le_bytes());
        bytes.extend_from_slice(&num_blocks.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_header_fields() {
        let (_, h) = nif_header(&synthetic_header(3)).unwrap();
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
        let name = b"NiParticleSystemController";
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name);
        let err = Nif::parse(&bytes).unwrap_err();
        assert!(format!("{err}").contains("NiParticleSystemController"));
    }

    #[test]
    fn transform_applies_translation_and_scale() {
        let t = NifTransform {
            translation: [1.0, 2.0, 3.0],
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            scale: 2.0,
        };
        assert_eq!(t.apply_point([1.0, 1.0, 1.0]), [3.0, 4.0, 5.0]);
    }
}
