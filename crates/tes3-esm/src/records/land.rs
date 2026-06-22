//! `LAND` — landscape data for an exterior cell.
//!
//! Each exterior cell defines 65×65 arrays of vertex heights, normals and colors plus
//! a 16×16 texture-index grid and a 9×9 world-map height grid. These large arrays are
//! kept as owned `Vec<u8>` byte blobs; the height grid is delta-encoded per the format.
//! Signed `i8` arrays are exposed as raw bytes (reinterpret with `as i8`), and the `u16`
//! texture grid via [`Land::texture_indices`].

use crate::common::{Subrecord, finish, le_f32, le_i32, le_u32};
use nom::IResult;

/// Bit flags in the `DATA` field indicating which optional arrays are present.
pub const LAND_HAS_HEIGHTS: u32 = 0x01; // VNML, VHGT, WNAM
pub const LAND_HAS_COLORS: u32 = 0x02; // VCLR
pub const LAND_HAS_TEXTURES: u32 = 0x04; // VTEX

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Land {
    pub grid_x: i32,
    pub grid_y: i32,
    /// Bitfield describing which arrays below are populated.
    pub data_types: u32,
    /// 65×65×3 vertex normals (`VNML`), as raw signed bytes.
    pub normals: Option<Vec<u8>>,
    /// Per-cell height offset from `VHGT`.
    pub height_offset: Option<f32>,
    /// 65×65 delta-encoded vertex heights from `VHGT`, as raw signed bytes.
    pub heights: Option<Vec<u8>>,
    /// 9×9 world-map heights (`WNAM`).
    pub world_map_heights: Option<Vec<u8>>,
    /// 65×65×3 vertex colors (`VCLR`).
    pub colors: Option<Vec<u8>>,
    /// Raw 16×16 texture-index grid bytes (`VTEX`); decode via [`Land::texture_indices`].
    pub texture_data: Option<Vec<u8>>,
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

impl Land {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Land {
        let mut out = Land::default();
        for sub in subs {
            match &sub.tag {
                b"INTV" => {
                    if let Some((x, y)) = finish(coords(sub.data)) {
                        out.grid_x = x;
                        out.grid_y = y;
                    }
                }
                b"DATA" => out.data_types = finish(le_u32(sub.data)).unwrap_or(0),
                b"VNML" => out.normals = Some(sub.data.to_vec()),
                b"VHGT" => {
                    if let Some((offset, heights)) = finish(vhgt(sub.data)) {
                        out.height_offset = Some(offset);
                        out.heights = Some(heights.to_vec());
                    }
                }
                b"WNAM" => out.world_map_heights = Some(sub.data.to_vec()),
                b"VCLR" => out.colors = Some(sub.data.to_vec()),
                b"VTEX" => out.texture_data = Some(sub.data.to_vec()),
                _ => {}
            }
        }
        out
    }

    /// Iterate the `VTEX` texture indices as little-endian `u16`s (the raw bytes are not
    /// guaranteed to be 2-byte aligned, so they are read rather than transmuted).
    pub fn texture_indices(&self) -> impl Iterator<Item = u16> + '_ {
        self.texture_data
            .as_deref()
            .unwrap_or(&[])
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
    }
}
