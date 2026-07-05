//! The decoded NIF data model: the [`Block`] enum and the typed structures inside it.
//!
//! These are plain data types — parsing lives in the `parse` module, scene traversal on
//! [`Nif`](crate::Nif). Everything here is re-exported from the crate root.

use tes_core::L1String;

/// The NIF header (version 4.0.0.2 layout): a newline-terminated identifier string, the
/// numeric version, and the number of blocks that follow.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NifHeader {
    /// The version identifier line, e.g. `NetImmerse File Format, Version 4.0.0.2`
    /// (without the trailing newline).
    pub ident: L1String,
    /// Numeric version, e.g. [`VERSION_TES3`](crate::VERSION_TES3).
    pub version: u32,
    /// Number of blocks following the header.
    pub num_blocks: u32,
}

/// A reference from one block to another: an index into [`Nif::blocks`](crate::Nif::blocks),
/// with `-1` (on disk, any negative value) meaning "no block" ([`BlockRef::NONE`]). Resolve
/// it with [`Nif::block`](crate::Nif::block) or [`BlockRef::index`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockRef(pub(crate) i32);

impl BlockRef {
    /// The absent reference (`-1` on disk).
    pub const NONE: BlockRef = BlockRef(-1);

    /// `true` when this reference points at no block.
    pub fn is_none(self) -> bool {
        self.0 < 0
    }

    /// The index into [`Nif::blocks`](crate::Nif::blocks), or `None` for [`BlockRef::NONE`].
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

/// `NiAlphaProperty`: how a shape's alpha channel is rendered — blending and/or cutout
/// testing. Without one of these on a shape, Morrowind draws it fully opaque no matter
/// what its material's alpha says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlphaProperty {
    /// The raw flags word; see the accessor methods for the fields inside it.
    pub flags: u16,
    /// Alpha-test reference value, `0`–`255`: with [`AlphaProperty::testing`] enabled,
    /// only fragments whose alpha passes [`AlphaProperty::test_function`] against this
    /// are drawn.
    pub threshold: u8,
}

impl AlphaProperty {
    /// Alpha blending enabled (bit 0).
    pub fn blending(self) -> bool {
        self.flags & 0x0001 != 0
    }

    /// Source blend mode (bits 1–4), a D3D-style blend factor: `0` ONE, `1` ZERO,
    /// `2` SRC_COLOR, `3` INV_SRC_COLOR, `4` DEST_COLOR, `5` INV_DEST_COLOR,
    /// `6` SRC_ALPHA, `7` INV_SRC_ALPHA, `8` DEST_ALPHA, `9` INV_DEST_ALPHA,
    /// `10` SRC_ALPHA_SATURATE.
    pub fn source_blend_mode(self) -> u16 {
        (self.flags >> 1) & 0x000F
    }

    /// Destination blend mode (bits 5–8); same encoding as
    /// [`AlphaProperty::source_blend_mode`].
    pub fn dest_blend_mode(self) -> u16 {
        (self.flags >> 5) & 0x000F
    }

    /// Alpha (cutout) testing enabled (bit 9).
    pub fn testing(self) -> bool {
        self.flags & 0x0200 != 0
    }

    /// Alpha-test comparison function (bits 10–12): `0` ALWAYS, `1` LESS, `2` EQUAL,
    /// `3` LESS_EQUAL, `4` GREATER, `5` NOT_EQUAL, `6` GREATER_EQUAL, `7` NEVER.
    /// Morrowind foliage is `GREATER` in practice.
    pub fn test_function(self) -> u16 {
        (self.flags >> 10) & 0x0007
    }
}

/// One decoded NIF block. Every block in the file produces exactly one entry (in file
/// order), so an index into [`Nif::blocks`](crate::Nif::blocks) matches the [`BlockRef`]s
/// stored in other blocks (e.g. [`TriShape::data`]). Blocks this crate doesn't model are
/// kept as [`Block::Other`] purely to preserve that indexing.
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
    /// `NiAlphaProperty` — see [`AlphaProperty`].
    AlphaProperty(AlphaProperty),
    /// `NiSourceTexture` — see [`SourceTexture`].
    SourceTexture(SourceTexture),
    /// A block parsed past but not represented (alpha, extra data, …).
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

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
