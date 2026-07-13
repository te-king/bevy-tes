//! `LAND` — landscape data for an exterior cell.
//!
//! Each exterior cell defines 65×65 arrays of vertex heights, normals and colors plus
//! a 16×16 texture-index grid and a 9×9 world-map height grid. These large arrays are
//! kept as owned `Vec<u8>` byte blobs; typed views are provided by [`Land::decode_heights`]
//! (running-sum decode of the delta-encoded grid), [`Land::decode_normals`],
//! [`Land::decode_colors`] and [`Land::decode_textures`] (block de-swizzle).

use crate::common::{Subrecord, finish, flags, le_f32, le_i32};
use nom::IResult;

/// Vertices per side of the LAND height/normal/color grids (64 quads + 1).
pub const LAND_GRID: usize = 65;
/// Texels per side of the `VTEX` texture-index grid (each texel covers 4×4 quads).
pub const VTEX_GRID: usize = 16;
/// Exterior cell edge length in game units (64 quads × 128 units per quad).
pub const CELL_SIZE: f32 = 8192.0;
/// One VHGT height unit is 8 game units.
pub const HEIGHT_SCALE: f32 = 8.0;

bitflags::bitflags! {
    /// Landscape flags (`DATA`) indicating which optional arrays are present.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct LandFlags: u32 {
        /// `VNML`, `VHGT`, `WNAM`.
        const HAS_HEIGHTS = 0x01;
        /// `VCLR`.
        const HAS_COLORS = 0x02;
        /// `VTEX`.
        const HAS_TEXTURES = 0x04;
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Land<'a> {
    pub grid_x: i32,
    pub grid_y: i32,
    /// Which of the arrays below are populated.
    pub data_types: LandFlags,
    /// 65×65×3 vertex normals (`VNML`), as raw signed bytes.
    pub normals: Option<&'a [u8]>,
    /// Per-cell height offset from `VHGT`.
    pub height_offset: Option<f32>,
    /// 65×65 delta-encoded vertex heights from `VHGT`, as raw signed bytes.
    pub heights: Option<&'a [u8]>,
    /// 9×9 world-map heights (`WNAM`).
    pub world_map_heights: Option<&'a [u8]>,
    /// 65×65×3 vertex colors (`VCLR`).
    pub colors: Option<&'a [u8]>,
    /// Raw 16×16 texture-index grid bytes (`VTEX`), in the file's swizzled block order;
    /// decode via [`Land::decode_textures`] (or [`Land::texture_indices`] for raw order).
    pub texture_data: Option<&'a [u8]>,
}

fn coords(input: &[u8]) -> IResult<&[u8], (i32, i32)> {
    let (input, x) = le_i32(input)?;
    let (input, y) = le_i32(input)?;
    Ok((input, (x, y)))
}

/// Parse the `VHGT` payload into its height offset and the 65×65 delta-height grid
/// (the trailing 3 junk bytes are ignored).
fn vhgt(input: &[u8]) -> IResult<&[u8], (f32, &[u8])> {
    let (rest, offset) = le_f32(input)?;
    let heights_end = rest.len().min(65 * 65);
    Ok((&rest[heights_end..], (offset, &rest[..heights_end])))
}

impl<'a> Land<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Land<'a> {
        let mut out = Land::default();
        for sub in subs {
            match &sub.tag.0 {
                b"INTV" => {
                    if let Some((x, y)) = finish(coords(sub.data)) {
                        out.grid_x = x;
                        out.grid_y = y;
                    }
                }
                b"DATA" => out.data_types = finish(flags(sub.data)).unwrap_or_default(),
                b"VNML" => out.normals = Some(sub.data),
                b"VHGT" => {
                    if let Some((offset, heights)) = finish(vhgt(sub.data)) {
                        out.height_offset = Some(offset);
                        out.heights = Some(heights);
                    }
                }
                b"WNAM" => out.world_map_heights = Some(sub.data),
                b"VCLR" => out.colors = Some(sub.data),
                b"VTEX" => out.texture_data = Some(sub.data),
                _ => {}
            }
        }
        out
    }

    /// Iterate the `VTEX` texture indices as little-endian `u16`s (the raw bytes are not
    /// guaranteed to be 2-byte aligned, so they are read rather than transmuted).
    pub fn texture_indices(&self) -> impl Iterator<Item = u16> + '_ {
        self.texture_data
            .unwrap_or(&[])
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
    }

    /// Decode `VTEX` into the 16×16 texture-index grid, row-major from the cell's
    /// **south-west** corner: index = `y * 16 + x`, `x` west→east, `y` south→north.
    ///
    /// Storage order is swizzled (UESP's LAND notes; OpenMW's `loadland`): the grid is
    /// stored as a row-major 4×4 array of blocks, each block a row-major 4×4 array of
    /// texels. Each value is the referencing `LTEX` record's index **plus one**; 0 means
    /// no explicit texture (the engine's default land texture, `_land_default.tga`).
    ///
    /// `None` when `VTEX` is absent or not exactly 16×16 `u16`s.
    pub fn decode_textures(&self) -> Option<Vec<u16>> {
        if self.texture_data?.len() != VTEX_GRID * VTEX_GRID * 2 {
            return None;
        }
        let mut out = vec![0u16; VTEX_GRID * VTEX_GRID];
        for (stored_pos, value) in self.texture_indices().enumerate() {
            let (block, texel) = (stored_pos / 16, stored_pos % 16);
            let (bx, by) = (block % 4, block / 4);
            let (tx, ty) = (texel % 4, texel / 4);
            out[(by * 4 + ty) * VTEX_GRID + (bx * 4 + tx)] = value;
        }
        Some(out)
    }

    /// Decode the delta-encoded `VHGT` grid into 65×65 absolute vertex heights in game
    /// units, row-major from the cell's **south-west** corner: index = `y * 65 + x`,
    /// `x` west→east, `y` south→north.
    ///
    /// Format: the running sum starts at [`Land::height_offset`]; the first byte of each
    /// row is a delta from the previous row's **first** vertex (row 0's first byte is a
    /// delta from the offset itself), and each subsequent byte is a delta from its left
    /// neighbor. The accumulated value × [`HEIGHT_SCALE`] is the height in game units.
    ///
    /// `None` when `VHGT` is absent or truncated.
    pub fn decode_heights(&self) -> Option<Vec<f32>> {
        let deltas = self.heights?;
        if deltas.len() != LAND_GRID * LAND_GRID {
            return None;
        }
        let mut out = Vec::with_capacity(LAND_GRID * LAND_GRID);
        let mut row = self.height_offset?;
        for y in 0..LAND_GRID {
            row += (deltas[y * LAND_GRID] as i8) as f32;
            let mut h = row;
            out.push(h * HEIGHT_SCALE);
            for x in 1..LAND_GRID {
                h += (deltas[y * LAND_GRID + x] as i8) as f32;
                out.push(h * HEIGHT_SCALE);
            }
        }
        Some(out)
    }

    /// Decode `VNML` into 65×65 unit normals in the game's Z-up frame, same ordering as
    /// [`Land::decode_heights`]. The raw `i8` triples are re-normalized (authored data is
    /// only approximately unit length); degenerate zero vectors become +Z.
    ///
    /// `None` when `VNML` is absent or truncated.
    pub fn decode_normals(&self) -> Option<Vec<[f32; 3]>> {
        let bytes = self.normals?;
        if bytes.len() != LAND_GRID * LAND_GRID * 3 {
            return None;
        }
        Some(
            bytes
                .chunks_exact(3)
                .map(|c| {
                    let [x, y, z] = [c[0], c[1], c[2]].map(|b| (b as i8) as f32);
                    let len = (x * x + y * y + z * z).sqrt();
                    if len == 0.0 {
                        [0.0, 0.0, 1.0]
                    } else {
                        [x / len, y / len, z / len]
                    }
                })
                .collect(),
        )
    }

    /// Decode `VCLR` into 65×65 RGB byte triples, same ordering as
    /// [`Land::decode_heights`]. The bytes are gamma-space (sRGB-like) 0–255 channels;
    /// color-space conversion is the consumer's concern.
    ///
    /// `None` when `VCLR` is absent or truncated.
    pub fn decode_colors(&self) -> Option<Vec<[u8; 3]>> {
        let bytes = self.colors?;
        if bytes.len() != LAND_GRID * LAND_GRID * 3 {
            return None;
        }
        Some(bytes.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn land_with_heights(offset: f32, deltas: &[u8]) -> Land<'_> {
        Land {
            height_offset: Some(offset),
            heights: Some(deltas),
            ..Land::default()
        }
    }

    #[test]
    fn decode_heights_follows_row_and_column_delta_rules() {
        let mut deltas = vec![0u8; LAND_GRID * LAND_GRID];
        deltas[0] = 1;
        deltas[1] = 2;
        deltas[65] = 0xFF; // -1
        deltas[66] = 3;
        let h = land_with_heights(10.0, &deltas).decode_heights().unwrap();
        assert_eq!(h.len(), 4225);
        // Row 0's first byte deltas from the offset: (10 + 1) * 8.
        assert_eq!(h[0], 88.0);
        // Subsequent bytes delta from the left neighbor: (11 + 2) * 8.
        assert_eq!(h[1], 104.0);
        // A row start deltas from the previous row's FIRST vertex (11), not its last.
        assert_eq!(h[65], 80.0);
        assert_eq!(h[66], 104.0);
        // Zero deltas propagate the row-start value to the far corner.
        assert_eq!(h[4224], 80.0);
    }

    #[test]
    fn decode_heights_rejects_truncated_or_offsetless_data() {
        assert!(land_with_heights(0.0, &[0; 100]).decode_heights().is_none());
        let mut land = land_with_heights(0.0, &[0; LAND_GRID * LAND_GRID]);
        land.height_offset = None;
        assert!(land.decode_heights().is_none());
        assert!(Land::default().decode_heights().is_none());
    }

    #[test]
    fn decode_normals_scales_and_renormalizes() {
        let mut bytes = vec![0u8; LAND_GRID * LAND_GRID * 3];
        bytes[2] = 127; // vertex 0: (0, 0, 127) → +Z
        bytes[3] = 127; // vertex 1: (127, 127, 0) → diagonal, needs renormalizing
        bytes[4] = 127;
        // vertex 2 stays (0, 0, 0) → degenerate, falls back to +Z
        let land = Land {
            normals: Some(&bytes),
            ..Land::default()
        };
        let normals = land.decode_normals().unwrap();
        assert_eq!(normals.len(), 4225);
        assert_eq!(normals[0], [0.0, 0.0, 1.0]);
        let d = std::f32::consts::FRAC_1_SQRT_2;
        assert!((normals[1][0] - d).abs() < 1e-6 && (normals[1][1] - d).abs() < 1e-6);
        assert_eq!(normals[2], [0.0, 0.0, 1.0]);

        let truncated = Land {
            normals: Some(&[0; 10]),
            ..Land::default()
        };
        assert!(truncated.decode_normals().is_none());
    }

    #[test]
    fn decode_textures_deswizzles_blocks() {
        // Store each texel's own storage position as its value: decode then reads back
        // "which stored slot landed here", pinning the block layout exactly.
        let bytes: Vec<u8> = (0..256u16).flat_map(u16::to_le_bytes).collect();
        let land = Land {
            texture_data: Some(&bytes),
            ..Land::default()
        };
        let grid = land.decode_textures().unwrap();
        assert_eq!(grid.len(), 256);
        // Stored position 0 is block (0,0) texel (0,0) → logical (0,0).
        assert_eq!(grid[0], 0);
        // Logical (x=4, y=1) is block (1,0) texel (0,1) → stored 1*16 + 4 = 20.
        assert_eq!(grid[VTEX_GRID + 4], 20);
        // Logical (x=3, y=3) is block (0,0)'s last texel → stored 15.
        assert_eq!(grid[3 * VTEX_GRID + 3], 15);
        // The far corner (15,15) is block (3,3)'s last texel → stored 255.
        assert_eq!(grid[255], 255);
        // Second row of the first block: logical (1,1) → stored texel 1*4+1 = 5.
        assert_eq!(grid[VTEX_GRID + 1], 5);
    }

    #[test]
    fn decode_textures_rejects_absent_or_truncated_data() {
        assert!(Land::default().decode_textures().is_none());
        let truncated = Land {
            texture_data: Some(&[0; 100]),
            ..Land::default()
        };
        assert!(truncated.decode_textures().is_none());
    }

    #[test]
    fn decode_colors_chunks_rgb() {
        let mut bytes = vec![255u8; LAND_GRID * LAND_GRID * 3];
        bytes[0] = 1;
        bytes[1] = 2;
        bytes[2] = 3;
        let land = Land {
            colors: Some(&bytes),
            ..Land::default()
        };
        let colors = land.decode_colors().unwrap();
        assert_eq!(colors.len(), 4225);
        assert_eq!(colors[0], [1, 2, 3]);
        assert_eq!(colors[4224], [255, 255, 255]);

        let truncated = Land {
            colors: Some(&[0; 10]),
            ..Land::default()
        };
        assert!(truncated.decode_colors().is_none());
    }
}
