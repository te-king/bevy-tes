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
//! This crate decodes the **static-mesh block graph** — the blocks needed to get textured
//! triangle geometry out of a model: [`Node`] (`NiNode` and friends), [`TriShape`]
//! (`NiTriShape`), [`Block::TriShapeData`] (`NiTriShapeData`, the actual mesh), the
//! base-texture chain [`TexturingProperty`] → [`SourceTexture`] (which retains the diffuse
//! texture filename), and [`Block::MaterialProperty`] (surface colours). [`Nif::instances`]
//! walks the scene graph from its roots, composing transforms and resolving each shape's
//! texture and material. Other property / extra-data blocks are decoded only far enough to
//! walk past them (kept as [`Block::Other`] so block indices — which [`BlockRef`]s rely on —
//! stay aligned). Block types outside this set (particle systems, controllers, …) cause
//! [`Nif::parse`] to fail with [`NifError::Parse`] naming the unsupported type.
//!
//! ```no_run
//! let bytes = std::fs::read("model.nif").unwrap();
//! let nif = tes_nif::Nif::parse(&bytes).unwrap();
//! assert_eq!(nif.header.version, tes_nif::VERSION_TES3);
//! for shape in nif.instances() {
//!     let mesh = shape.mesh;
//!     println!("{} vertices, {} triangles", mesh.vertices.len(), mesh.triangles.len());
//!     if let Some(tex) = shape.base_texture {
//!         println!("  textured with {}", tex.decode());
//!     }
//! }
//! ```

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

/// A reference from one block to another: an index into [`Nif::blocks`], with `-1` (on
/// disk, any negative value) meaning "no block" ([`BlockRef::NONE`]). Resolve it with
/// [`Nif::block`] or [`BlockRef::index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockRef(i32);

impl BlockRef {
    /// The absent reference (`-1` on disk).
    pub const NONE: BlockRef = BlockRef(-1);

    /// `true` when this reference points at no block.
    pub fn is_none(self) -> bool {
        self.0 < 0
    }

    /// The index into [`Nif::blocks`], or `None` for [`BlockRef::NONE`].
    pub fn index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

impl Default for BlockRef {
    fn default() -> Self {
        BlockRef::NONE
    }
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

    /// Compose this (parent) transform with a `child`, yielding the child's transform in this
    /// transform's frame: `self ∘ child`. The result applied to a point equals
    /// `self.apply_point(child.apply_point(p))` (exact because the scale is uniform).
    pub fn compose(&self, child: &NifTransform) -> NifTransform {
        NifTransform {
            translation: self.apply_point(child.translation),
            rotation: mat3_mul(&self.rotation, &child.rotation),
            scale: self.scale * child.scale,
        }
    }
}

/// Row-major 3×3 matrix product `a * b`.
fn mat3_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut m = [[0.0f32; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    m
}

/// Surface material decoded from a `NiMaterialProperty`: the fixed-function colours and
/// factors Morrowind authored per shape. Colours are RGB in the game's (sRGB-ish) space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    /// Specular exponent (0–128 range in practice).
    pub glossiness: f32,
    /// Opacity, `0.0`–`1.0`. Below 1 the shape is meant to be drawn translucent.
    pub alpha: f32,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            ambient: [1.0; 3],
            diffuse: [1.0; 3],
            specular: [0.0; 3],
            emissive: [0.0; 3],
            glossiness: 0.0,
            alpha: 1.0,
        }
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

/// `NiNode` (and node-like blocks): a transform, its child block references, its attached
/// property references (inherited by descendants), and traversal flags.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub transform: NifTransform,
    pub children: Vec<BlockRef>,
    /// References to attached property blocks, inherited by descendant shapes.
    pub properties: Vec<BlockRef>,
    /// `NiAVObject` "hidden" flag (bit 0): the node and its subtree should not be drawn.
    pub hidden: bool,
    /// `true` for a `RootCollisionNode`: its subtree is collision geometry, not visuals.
    pub collision: bool,
}

/// `NiTriShape`: a transform, a reference to its [`Block::TriShapeData`], the references
/// of its attached properties (used to resolve its texture and material), and the
/// "hidden" flag.
#[derive(Debug, Clone, PartialEq)]
pub struct TriShape {
    pub transform: NifTransform,
    /// Reference to the geometry data ([`Block::TriShapeData`]).
    pub data: BlockRef,
    /// References to attached property blocks (texturing, material, alpha, …).
    pub properties: Vec<BlockRef>,
    /// `NiAVObject` "hidden" flag (bit 0): the shape should not be drawn.
    pub hidden: bool,
}

/// `NiTexturingProperty`: retains the reference of the base (first-slot) texture, i.e. the
/// [`SourceTexture`] providing the diffuse map. [`BlockRef::NONE`] when that slot is unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TexturingProperty {
    pub base_texture: BlockRef,
}

/// `NiSourceTexture`: an external texture reference, keeping the filename it names
/// (e.g. `Tx_BeerStein.dds`). Empty for the internal-pixel-data case, which this crate
/// doesn't decode.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceTexture {
    pub file_name: L1String,
}

/// One decoded NIF block. Every block in the file produces exactly one entry (in file
/// order), so an index into [`Nif::blocks`] matches the [`BlockRef`]s stored in other
/// blocks (e.g. [`TriShape::data`]). Blocks this crate doesn't model are kept as
/// [`Block::Other`] purely to preserve that indexing.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// `NiNode` and friends — see [`Node`].
    Node(Node),
    /// `NiTriShape` — see [`TriShape`].
    TriShape(TriShape),
    /// `NiTriShapeData`: the actual triangle geometry.
    TriShapeData(TriMesh),
    /// `NiTexturingProperty` — see [`TexturingProperty`].
    TexturingProperty(TexturingProperty),
    /// `NiMaterialProperty`: the decoded surface [`Material`].
    MaterialProperty(Material),
    /// `NiSourceTexture` — see [`SourceTexture`].
    SourceTexture(SourceTexture),
    /// A block parsed past but not represented (alpha, extra data, …).
    Other,
}

/// A parsed NIF file: its [`NifHeader`], the decoded [`Block`] graph, and the root block
/// references the scene is traversed from.
#[derive(Debug)]
pub struct Nif {
    pub header: NifHeader,
    /// Decoded blocks, one per block in the file, in file order.
    pub blocks: Vec<Block>,
    /// Root block references from the file footer; scene traversal starts here. Empty when
    /// the footer is absent, in which case [`Nif::instances`] falls back to block 0.
    pub roots: Vec<BlockRef>,
}

impl Nif {
    /// Parse a NIF from an in-memory byte slice. Validates the version is [`VERSION_TES3`]
    /// and decodes every block (failing on any block type outside the supported set).
    pub fn parse(input: &[u8]) -> Result<Nif, NifError> {
        let mut r = Reader::new(input);
        let header = nif_header(&mut r).map_err(|e| e.at("header"))?;
        if header.version != VERSION_TES3 {
            return Err(NifError::Parse(format!(
                "unsupported NIF version {:#010x} (expected {:#010x})",
                header.version, VERSION_TES3
            )));
        }

        let mut blocks = Vec::with_capacity(header.num_blocks as usize);
        for i in 0..header.num_blocks {
            let ty = r.string().map_err(|e| e.at(format!("block {i} type")))?;
            let block = parse_block(&mut r, &ty.decode()).map_err(|e| match e {
                NifError::Parse(msg) => NifError::Parse(format!("block {i}: {msg}")),
                other => other,
            })?;
            blocks.push(block);
        }
        // The footer (root count + root refs) follows the blocks. It's optional for us: read
        // it if present, else traversal falls back to block 0.
        let roots = r.refs().unwrap_or_default();

        Ok(Nif {
            header,
            blocks,
            roots,
        })
    }

    /// Resolve a [`BlockRef`] to its block, or `None` for [`BlockRef::NONE`] or an
    /// out-of-range index.
    pub fn block(&self, r: BlockRef) -> Option<&Block> {
        self.blocks.get(r.index()?)
    }

    /// The references scene traversal starts from: the footer's [`Nif::roots`], or block 0
    /// when the footer is absent.
    pub fn scene_roots(&self) -> &[BlockRef] {
        if self.roots.is_empty() {
            &[BlockRef(0)]
        } else {
            &self.roots
        }
    }

    /// Walk the scene graph from the roots, yielding a [`ShapeInstance`] for every drawable
    /// `NiTriShape` with its **world** transform composed down the tree. Hidden nodes/shapes
    /// and `RootCollisionNode` subtrees are skipped, and each shape's texture and material are
    /// resolved with property inheritance (the shape's own properties take precedence over an
    /// ancestor's).
    pub fn instances(&self) -> Vec<ShapeInstance<'_>> {
        let mut out = Vec::new();
        for &root in self.scene_roots() {
            self.collect_instances(root, &NifTransform::default(), &[], &mut out);
        }
        out
    }

    /// Depth-first traversal helper. `world` is the accumulated transform above `block`;
    /// `inherited` is the ancestors' property references, nearest first.
    fn collect_instances<'a>(
        &'a self,
        block: BlockRef,
        world: &NifTransform,
        inherited: &[BlockRef],
        out: &mut Vec<ShapeInstance<'a>>,
    ) {
        match self.block(block) {
            Some(Block::Node(node)) => {
                if node.hidden || node.collision {
                    return;
                }
                let node_world = world.compose(&node.transform);
                // A node's own properties sit nearer to descendants than its ancestors'.
                let mut props = node.properties.clone();
                props.extend_from_slice(inherited);
                for &child in &node.children {
                    self.collect_instances(child, &node_world, &props, out);
                }
            }
            Some(Block::TriShape(shape)) => {
                if shape.hidden {
                    return;
                }
                let Some(Block::TriShapeData(mesh)) = self.block(shape.data) else {
                    return;
                };
                let mut props = shape.properties.clone();
                props.extend_from_slice(inherited);
                out.push(ShapeInstance {
                    transform: world.compose(&shape.transform),
                    mesh,
                    base_texture: self.base_texture(&props),
                    material: self.material(&props),
                });
            }
            _ => {}
        }
    }

    /// Resolve a shape's base-colour texture filename by walking its (inheritance-ordered)
    /// property references to the first [`TexturingProperty`], then following that to its
    /// [`SourceTexture`]. `None` when no external base texture applies.
    ///
    /// The property list must be inheritance-ordered — the shape's own properties first,
    /// then each ancestor's, nearest first — as built during scene traversal (this is the
    /// list [`Nif::instances`] resolves internally; it is public so custom traversals can
    /// reuse the exact same precedence rules).
    pub fn base_texture(&self, properties: &[BlockRef]) -> Option<&L1String> {
        for &p in properties {
            if let Some(Block::TexturingProperty(tp)) = self.block(p)
                && let Some(Block::SourceTexture(st)) = self.block(tp.base_texture)
                && !st.file_name.decode().is_empty()
            {
                return Some(&st.file_name);
            }
        }
        None
    }

    /// Resolve a shape's [`Material`] from the first [`Block::MaterialProperty`] in its
    /// (inheritance-ordered) property references. `None` when none applies.
    ///
    /// See [`Nif::base_texture`] for the expected ordering of `properties`.
    pub fn material(&self, properties: &[BlockRef]) -> Option<Material> {
        for &p in properties {
            if let Some(Block::MaterialProperty(m)) = self.block(p) {
                return Some(*m);
            }
        }
        None
    }
}

/// A drawable `NiTriShape` located in the scene, produced by [`Nif::instances`].
#[derive(Debug, Clone, Copy)]
pub struct ShapeInstance<'a> {
    /// The shape's **world** transform, composed from the root down the scene graph.
    pub transform: NifTransform,
    /// The triangle geometry from the shape's `NiTriShapeData`, in the shape's local space.
    pub mesh: &'a TriMesh,
    /// Base-colour texture filename (first inherited `NiTexturingProperty` slot), if any —
    /// e.g. `Tx_BeerStein.dds`. Resolve it against a texture directory or `Morrowind.bsa`.
    pub base_texture: Option<&'a L1String>,
    /// Surface material (first inherited `NiMaterialProperty`), if any.
    pub material: Option<Material>,
}

// --- parsing -----------------------------------------------------------------------------

/// A simple sequential cursor over the byte stream with bounds-checked little-endian reads.
/// All reads advance the cursor; a short read produces a [`ReadError`].
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// A short read while decoding the stream. Carries a description set via [`ReadError::at`].
#[derive(Debug)]
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

    /// Read up to and including the next `\n`, returning the bytes before it.
    fn line(&mut self) -> RResult<&'a [u8]> {
        let len = self.data[self.pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| ReadError(format!("no newline after offset {}", self.pos)))?;
        let line = self.take(len)?;
        self.skip(1)?; // the newline itself
        Ok(line)
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

    /// A single block reference (an `i32`, negative meaning "none").
    fn block_ref(&mut self) -> RResult<BlockRef> {
        Ok(BlockRef(self.i32()?))
    }

    /// A `u32` count followed by that many block references.
    fn refs(&mut self) -> RResult<Vec<BlockRef>> {
        let n = self.u32()? as usize;
        (0..n).map(|_| self.block_ref()).collect()
    }
}

/// Parse the version-4.0.0.2 header: identifier line (terminated by `\n`), then the
/// version and block-count `u32`s.
fn nif_header(r: &mut Reader) -> RResult<NifHeader> {
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
        "NiNode" | "RootCollisionNode" | "AvoidNode" | "NiBSAnimationNode" | "NiBSParticleNode" => {
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

        "NiTexturingProperty" => Block::TexturingProperty(TexturingProperty {
            base_texture: texturing_property(r)?,
        }),
        "NiMaterialProperty" => Block::MaterialProperty(material_property(r)?),
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
        _ => return Err(None),
    };
    Ok(block)
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

/// `NiTexturingProperty`: walk past flags, apply mode and the texture descriptors, returning
/// the reference of the base (slot 0) texture's `NiSourceTexture` (or [`BlockRef::NONE`] if
/// unused).
fn texturing_property(r: &mut Reader) -> RResult<BlockRef> {
    ni_object_net(r)?;
    r.u16()?; // flags
    r.u32()?; // apply mode
    let texture_count = r.u32()? as usize;
    let mut base_texture = BlockRef::NONE;
    for slot in 0..texture_count {
        if r.boolean()? {
            // TexDesc: source ref + clamp + filter + uv set + ps2 l/k + unknown1
            let source = r.block_ref()?;
            if slot == 0 {
                base_texture = source;
            }
            r.skip(4 + 4 + 4 + 2 + 2 + 2)?; // clamp + filter + uv set + ps2 l/k + unknown1
            // The bump-map slot (index 5) carries an extra scale/offset and 2x2 matrix.
            if slot == 5 {
                r.skip(4 * 6)?;
            }
        }
    }
    Ok(base_texture)
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
        let name = b"NiParticleSystemController";
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name);
        let err = Nif::parse(&bytes).unwrap_err();
        assert!(format!("{err}").contains("NiParticleSystemController"));
    }

    #[test]
    fn block_ref_resolves_only_valid_indices() {
        assert_eq!(BlockRef::NONE.index(), None);
        assert!(BlockRef::NONE.is_none());
        assert_eq!(BlockRef(-7).index(), None);
        assert_eq!(BlockRef(2).index(), Some(2));
        assert!(!BlockRef(0).is_none());
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

    #[test]
    fn compose_matches_nested_application() {
        // A parent that rotates +90° about Z, scales 2×, and translates; a child that rotates
        // +90° about X and translates. Composing then applying must equal applying in turn.
        let parent = NifTransform {
            translation: [10.0, 0.0, -5.0],
            rotation: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            scale: 2.0,
        };
        let child = NifTransform {
            translation: [1.0, 2.0, 3.0],
            rotation: [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
            scale: 0.5,
        };
        let composed = parent.compose(&child);
        for p in [[1.0, 0.0, 0.0], [0.3, -2.0, 4.0], [-1.0, 5.0, 2.5]] {
            let nested = parent.apply_point(child.apply_point(p));
            let direct = composed.apply_point(p);
            for k in 0..3 {
                assert!(
                    (nested[k] - direct[k]).abs() < 1e-4,
                    "axis {k}: {nested:?} vs {direct:?}"
                );
            }
        }
        assert!((composed.scale - 1.0).abs() < 1e-6);
    }
}
