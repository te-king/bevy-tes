//! Small [`nom`] helpers shared across format parsers.
//!
//! These cover two recurring needs: reading the format's Windows-1252 string fields as
//! borrowed [`L1Str`] views into the input, and running a field parser *tolerantly* —
//! trailing bytes and short/malformed payloads are common across format versions, so the
//! helpers ignore trailing data and fall back to defaults rather than erroring.

use crate::latin1::L1Str;
use nom::IResult;
use nom::bytes::complete::take;

/// View bytes as a borrowed [`L1Str`], stopping at the first NUL byte. Zero-copy.
///
/// This handles both fixed-length NUL-padded `string` fields and NUL-terminated `zstring`
/// fields, since both are framed by an explicit size. No decoding happens here — the raw
/// Windows-1252 bytes are viewed as-is and transcoded lazily via [`L1Str::decode`].
pub fn l1(bytes: &[u8]) -> &L1Str {
    L1Str::from_bytes_until_null(bytes)
}

/// Parser combinator: read `n` bytes and view them (NUL-trimmed) as a borrowed
/// [`L1Str`]. Used for fixed-length `char[n]` fields (e.g. object/spell names).
pub fn fixed_l1str(n: usize) -> impl Fn(&[u8]) -> IResult<&[u8], &L1Str> {
    move |input| {
        let (input, bytes) = take(n)(input)?;
        Ok((input, l1(bytes)))
    }
}

/// Run a nom parser over a complete field payload, discarding any trailing bytes, and
/// return the value. Trailing bytes are common (alignment padding, version variants), so
/// they are intentionally ignored rather than treated as an error.
pub fn finish<T>(result: IResult<&[u8], T>) -> Option<T> {
    result.ok().map(|(_, value)| value)
}

/// Run a nom parser over a field payload, falling back to `T::default()` if the payload is
/// too short or malformed. Keeps decoding total (infallible) for real data.
///
/// The produced `T` may borrow from `data` (e.g. string fields).
pub fn parse_or_default<'a, T: Default>(
    parser: impl Fn(&'a [u8]) -> IResult<&'a [u8], T>,
    data: &'a [u8],
) -> T {
    finish(parser(data)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l1_stops_at_nul() {
        assert_eq!(l1(b"a\x00bc").as_bytes(), b"a");
        assert_eq!(l1(b"abc").as_bytes(), b"abc");
    }

    #[test]
    fn fixed_reads_exactly_n() {
        let (rest, s) = fixed_l1str(4)(b"ab\x00\x00XY").unwrap();
        assert_eq!(s, "ab");
        assert_eq!(rest, b"XY");
    }
}
