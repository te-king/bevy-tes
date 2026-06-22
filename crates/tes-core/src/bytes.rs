//! Small [`nom`] helpers shared across format parsers.
//!
//! These cover two recurring needs: reading the format's Windows-1252 string fields into
//! owned [`L1String`]s, and running a field parser *tolerantly* — trailing bytes and
//! short/malformed payloads are common across format versions, so the helpers ignore
//! trailing data and fall back to defaults rather than erroring.

use crate::latin1::L1String;
use nom::IResult;
use nom::bytes::complete::take;

/// Copy bytes into an owned [`L1String`], stopping at the first NUL byte.
///
/// This handles both fixed-length NUL-padded `string` fields and NUL-terminated `zstring`
/// fields, since both are framed by an explicit size. No decoding happens here — the raw
/// Windows-1252 bytes are stored as-is and transcoded lazily via [`L1Str::decode`].
///
/// [`L1Str::decode`]: crate::L1Str::decode
pub fn l1(bytes: &[u8]) -> L1String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    L1String::from_bytes(bytes[..end].to_vec())
}

/// Parser combinator: read `n` bytes and copy them (NUL-trimmed) into an owned
/// [`L1String`]. Used for fixed-length `char[n]` fields (e.g. object/spell names).
pub fn fixed_l1str(n: usize) -> impl Fn(&[u8]) -> IResult<&[u8], L1String> {
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
/// `data` is borrowed only for the call; the produced `T` is owned.
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
