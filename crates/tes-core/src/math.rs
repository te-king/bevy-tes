//! Plain numeric primitives used by the geometry-bearing formats.
//!
//! These are deliberately minimal, `#[repr(C)]`, and free of any `glam`/Bevy dependency
//! so the parser crates stay light. Downstream (e.g. `bevy-beth`) converts them into
//! engine types. Each type has a matching little-endian [`nom`] reader.

use nom::IResult;
use nom::bytes::complete::take;
use nom::number::complete::le_f32;

/// An RGB(A) color stored as four bytes. The TES3 `rgb` field type occupies 4 bytes (the
/// 4th is padding/alpha).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Parse a 4-byte `rgb` color.
pub fn color(input: &[u8]) -> IResult<&[u8], Color> {
    let (input, bytes) = take(4usize)(input)?;
    Ok((
        input,
        Color {
            r: bytes[0],
            g: bytes[1],
            b: bytes[2],
            a: bytes[3],
        },
    ))
}

/// A 2-component float vector (e.g. NIF texture coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// A 3-component float vector (positions, normals, …).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A 4-component float vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// A 3×3 matrix in row-major order (NIF's `Matrix33`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix3 {
    pub m: [[f32; 3]; 3],
}

impl Default for Matrix3 {
    /// The identity matrix.
    fn default() -> Self {
        Matrix3 {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

/// A quaternion stored in `w, x, y, z` order (NIF's on-disk layout).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quaternion {
    /// The identity rotation.
    fn default() -> Self {
        Quaternion {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }
}

/// Read two little-endian `f32`s as a [`Vec2`].
pub fn vec2(input: &[u8]) -> IResult<&[u8], Vec2> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    Ok((input, Vec2 { x, y }))
}

/// Read three little-endian `f32`s as a [`Vec3`].
pub fn vec3(input: &[u8]) -> IResult<&[u8], Vec3> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    Ok((input, Vec3 { x, y, z }))
}

/// Read four little-endian `f32`s as a [`Vec4`].
pub fn vec4(input: &[u8]) -> IResult<&[u8], Vec4> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    let (input, w) = le_f32(input)?;
    Ok((input, Vec4 { x, y, z, w }))
}

/// Read nine little-endian `f32`s (row-major) as a [`Matrix3`].
pub fn matrix3(input: &[u8]) -> IResult<&[u8], Matrix3> {
    let mut m = [[0.0f32; 3]; 3];
    let mut input = input;
    for row in &mut m {
        for cell in row.iter_mut() {
            let (rest, v) = le_f32(input)?;
            *cell = v;
            input = rest;
        }
    }
    Ok((input, Matrix3 { m }))
}

/// Read four little-endian `f32`s (`w, x, y, z`) as a [`Quaternion`].
pub fn quaternion(input: &[u8]) -> IResult<&[u8], Quaternion> {
    let (input, w) = le_f32(input)?;
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    Ok((input, Quaternion { w, x, y, z }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vec3() {
        let bytes = [0, 0, 0x80, 0x3f, 0, 0, 0, 0x40, 0, 0, 0x40, 0x40];
        let (_, v) = vec3(&bytes).unwrap();
        assert_eq!(
            v,
            Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0
            }
        );
    }

    #[test]
    fn color_reads_four_bytes() {
        let (rest, c) = color(&[1, 2, 3, 4, 9]).unwrap();
        assert_eq!(
            c,
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }
        );
        assert_eq!(rest, &[9]);
    }
}
