//! Plain numeric primitives shared by the format parsers.
//!
//! These are deliberately minimal, dependency-light, and free of any `glam`/Bevy
//! dependency so the parser crates stay light. Downstream (e.g. `bevy-beth`) converts
//! them into engine types.

use nom::IResult;
use nom::bytes::complete::take;

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

#[cfg(test)]
mod tests {
    use super::*;

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
