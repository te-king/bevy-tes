//! Windows-1252 (a.k.a. "Latin-1" in the TES3 docs) string types with lazy decoding.
//!
//! TES3 stores text as Windows-1252 bytes. Rather than eagerly transcoding to UTF-8 at
//! parse time, [`L1Str`] and [`L1String`] hold the raw bytes and decode only when
//! requested via [`L1Str::decode`]. They mirror the standard [`str`]/[`String`] pair:
//! `L1Str` is an unsized borrowed view over `[u8]`, `L1String` owns a `Vec<u8>` and
//! derefs to `L1Str`, and `L1Str: ToOwned<Owned = L1String>` so `Cow<L1Str>` works too.
//!
//! Unlike `str`, there is no validity invariant: every byte sequence is a valid
//! Windows-1252 string, so wrapping is infallible and free.

use std::borrow::{Borrow, Cow};
use std::fmt;

/// A borrowed Windows-1252 string slice. The byte-for-byte analogue of [`str`].
#[repr(transparent)]
pub struct L1Str([u8]);

impl L1Str {
    /// Wrap raw Windows-1252 bytes as an `&L1Str` without copying or decoding.
    pub fn from_bytes(bytes: &[u8]) -> &L1Str {
        // SAFETY: `L1Str` is `repr(transparent)` over `[u8]`, so a `&[u8]` and a
        // `&L1Str` have identical layout (including the length metadata), and every
        // byte sequence is a valid Windows-1252 string.
        unsafe { &*(bytes as *const [u8] as *const L1Str) }
    }

    /// The underlying Windows-1252 bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether every byte is ASCII (in which case decoding is a zero-copy borrow).
    pub fn is_ascii(&self) -> bool {
        self.0.is_ascii()
    }

    /// Lazily decode to UTF-8. Pure-ASCII content (the overwhelmingly common case)
    /// borrows the bytes directly; only content with Windows-1252 high bytes allocates,
    /// since those must be re-encoded to valid UTF-8.
    pub fn decode(&self) -> Cow<'_, str> {
        if self.0.is_ascii() {
            Cow::Borrowed(std::str::from_utf8(&self.0).expect("ASCII is valid UTF-8"))
        } else {
            Cow::Owned(self.0.iter().map(|&b| cp1252_to_char(b)).collect())
        }
    }

    /// Copy into an owned [`L1String`].
    pub fn to_l1string(&self) -> L1String {
        L1String(self.0.to_vec())
    }
}

impl Default for &L1Str {
    fn default() -> Self {
        L1Str::from_bytes(&[])
    }
}

impl PartialEq for L1Str {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for L1Str {}

impl std::hash::Hash for L1Str {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Compare against a UTF-8 `str` by decoding (so high-byte content compares correctly).
impl PartialEq<str> for L1Str {
    fn eq(&self, other: &str) -> bool {
        self.decode() == other
    }
}

impl PartialEq<&str> for L1Str {
    fn eq(&self, other: &&str) -> bool {
        self.decode() == *other
    }
}

impl fmt::Debug for L1Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.decode(), f)
    }
}

impl fmt::Display for L1Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.decode(), f)
    }
}

impl ToOwned for L1Str {
    type Owned = L1String;
    fn to_owned(&self) -> L1String {
        self.to_l1string()
    }
}

/// An owned Windows-1252 string. The byte-for-byte analogue of [`String`].
#[repr(transparent)]
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct L1String(Vec<u8>);

impl L1String {
    pub fn new() -> L1String {
        L1String(Vec::new())
    }

    /// Take ownership of raw Windows-1252 bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> L1String {
        L1String(bytes)
    }

    /// Borrow as an [`L1Str`].
    pub fn as_l1str(&self) -> &L1Str {
        L1Str::from_bytes(&self.0)
    }

    /// The underlying Windows-1252 bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl std::ops::Deref for L1String {
    type Target = L1Str;
    fn deref(&self) -> &L1Str {
        self.as_l1str()
    }
}

impl Borrow<L1Str> for L1String {
    fn borrow(&self) -> &L1Str {
        self.as_l1str()
    }
}

impl From<&L1Str> for L1String {
    fn from(s: &L1Str) -> L1String {
        s.to_l1string()
    }
}

impl fmt::Debug for L1String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_l1str(), f)
    }
}

impl fmt::Display for L1String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_l1str(), f)
    }
}

/// Map a single Windows-1252 byte to its Unicode `char`.
///
/// Bytes `0x00..=0x7F` are ASCII and `0xA0..=0xFF` share Latin-1 code points; only the
/// `0x80..=0x9F` range needs the cp1252 mapping table (with a handful of unassigned
/// slots that fall back to the replacement character).
fn cp1252_to_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        // Unassigned in cp1252.
        0x81 | 0x8D | 0x8F | 0x90 | 0x9D => '\u{FFFD}',
        // ASCII and Latin-1 ranges map directly to the matching code point.
        other => other as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_decodes_borrowed() {
        let s = L1Str::from_bytes(b"hello");
        assert!(matches!(s.decode(), Cow::Borrowed("hello")));
        assert_eq!(s, L1Str::from_bytes(b"hello"));
        assert_eq!(s, "hello"); // PartialEq<&str>
    }

    #[test]
    fn high_bytes_decode_owned() {
        // 0xE9 is Latin-1 'é'; 0x99 is cp1252 '™'.
        assert!(matches!(L1Str::from_bytes(&[0xE9]).decode(), Cow::Owned(_)));
        assert_eq!(L1Str::from_bytes(&[0xE9]).decode(), "é");
        assert_eq!(L1Str::from_bytes(&[0x99]).decode(), "™");
    }

    #[test]
    fn owned_derefs_to_borrowed() {
        let owned = L1Str::from_bytes(b"abc").to_l1string();
        assert_eq!(owned.len(), 3);
        assert_eq!(owned.decode(), "abc");
        assert_eq!(&*owned, L1Str::from_bytes(b"abc"));
    }
}
