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
//! texture filename), [`Block::MaterialProperty`] (surface colours) and
//! [`Block::AlphaProperty`] (blend/cutout settings). [`Nif::instances`] walks the scene
//! graph from its roots, composing transforms and resolving each shape's texture,
//! material and alpha settings.
//!
//! Every other block type Morrowind ships — animation controllers and their key data,
//! skinning, particle systems and their modifiers, dynamic effects, cameras, embedded
//! textures, and the assorted property / extra-data blocks — is decoded exactly far
//! enough to walk past it, kept as [`Block::Other`] so block indices (which [`BlockRef`]s
//! rely on) stay aligned. Skinned and animated models therefore parse and yield their
//! bind-pose geometry; particle emitters parse but carry no drawable shapes. The whole
//! vanilla corpus (Morrowind, Tribunal and Bloodmoon archives plus loose files) parses
//! this way; a genuinely unknown block type still fails [`Nif::parse`] with
//! [`NifError::Parse`] naming it, as does a file whose blocks don't line up exactly with
//! its footer.
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
//!
//! # Crate layout
//!
//! - `blocks` (re-exported here) — the decoded data model: [`Block`] and the typed
//!   structures inside it.
//! - `reader` (private) — the bounds-checked byte cursor.
//! - `parse` (private) — one body parser per block type Morrowind ships.
//! - This root module — [`Nif`]: the parse entry point and scene-graph traversal.

use tes_core::L1String;

mod blocks;
mod parse;
mod reader;

pub use blocks::*;

use reader::Reader;

/// The NIF version Morrowind/Tribunal/Bloodmoon use: `4.0.0.2`.
pub const VERSION_TES3: u32 = 0x0400_0002;

/// Error returned when reading or parsing a NIF file.
#[derive(Debug, thiserror::Error)]
pub enum NifError {
    /// I/O failure while reading the file from disk.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The byte stream could not be parsed as a supported NIF.
    #[error("parse error: {0}")]
    Parse(String),
}

/// A parsed NIF file: its [`NifHeader`], the decoded [`Block`] graph, and the root block
/// references the scene is traversed from.
#[derive(Debug)]
pub struct Nif {
    pub header: NifHeader,
    /// Decoded blocks, one per block in the file, in file order.
    pub blocks: Vec<Block>,
    /// Root block references from the file footer; scene traversal starts here. When the
    /// list is empty [`Nif::instances`] falls back to block 0.
    pub roots: Vec<BlockRef>,
}

impl Nif {
    /// Parse a NIF from an in-memory byte slice. Validates the version is [`VERSION_TES3`]
    /// and decodes every block (failing on any block type outside the supported set).
    pub fn parse(input: &[u8]) -> Result<Nif, NifError> {
        let mut r = Reader::new(input);
        let header = parse::nif_header(&mut r).map_err(|e| e.at("header"))?;
        if header.version != VERSION_TES3 {
            return Err(NifError::Parse(format!(
                "unsupported NIF version {:#010x} (expected {:#010x})",
                header.version, VERSION_TES3
            )));
        }

        let mut blocks = Vec::with_capacity(header.num_blocks as usize);
        for i in 0..header.num_blocks {
            let ty = r.string().map_err(|e| e.at(format!("block {i} type")))?;
            let block = parse::parse_block(&mut r, &ty.decode()).map_err(|e| match e {
                NifError::Parse(msg) => NifError::Parse(format!("block {i}: {msg}")),
                other => other,
            })?;
            blocks.push(block);
        }
        // The footer (root count + root refs) follows the blocks and must end the file
        // exactly. This is a deliberate tripwire: block sizes are implicit, so a mis-sized
        // block parser would desync the stream — and the footer check catches a desync in
        // the *last* block, which no following block-type read would.
        let roots = r.refs().map_err(|e| e.at("footer"))?;
        if r.remaining() != 0 {
            return Err(NifError::Parse(format!(
                "{} trailing bytes after the footer",
                r.remaining()
            )));
        }

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
                    alpha: self.alpha_property(&props),
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

    /// Resolve a shape's [`AlphaProperty`] from the first [`Block::AlphaProperty`] in its
    /// (inheritance-ordered) property references. `None` when none applies — the shape
    /// renders opaque.
    ///
    /// See [`Nif::base_texture`] for the expected ordering of `properties`.
    pub fn alpha_property(&self, properties: &[BlockRef]) -> Option<AlphaProperty> {
        for &p in properties {
            if let Some(Block::AlphaProperty(a)) = self.block(p) {
                return Some(*a);
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
    /// Alpha rendering settings (first inherited `NiAlphaProperty`), if any — `None`
    /// means the shape draws fully opaque.
    pub alpha: Option<AlphaProperty>,
}
