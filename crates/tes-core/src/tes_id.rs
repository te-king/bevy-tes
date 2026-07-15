//! Case-insensitive editor ids.
//!
//! TES3 records name each other by **editor id** — a short Windows-1252 string (an object's
//! id, a cell's name, a script name). The engine compares these case-insensitively, so
//! every id index or lookup goes through one normal form: ASCII-lowercase (the engine folds
//! case byte-wise, not with Unicode rules — a distinction that only matters for the
//! accented ids of localized game builds).
//!
//! [`TesId`]/[`TesIdBuf`] are the borrowed/owned view pair (mirroring [`L1Str`]/[`L1String`]
//! and [`TesPath`](crate::TesPath)/[`TesPathBuf`](crate::TesPathBuf)) that compare and hash
//! *as if* lowercased, without allocating. Unlike a path, an id has no separators, so no
//! `/`↔`\` rewriting happens.

use std::borrow::Borrow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use crate::{L1Str, L1String};

/// An editor id (`furn_de_table_02`, `Balmora, Guild of Mages`) whose [`PartialEq`]/[`Eq`]
/// and [`Hash`] fold ASCII case, so `Furn_DE_Table_02` and `furn_de_table_02` are equal and
/// hash alike. Derefs to the underlying [`L1Str`] for the original (un-folded) bytes.
///
/// Like [`L1Str`] over `[u8]` (and [`TesPath`](crate::TesPath) over [`L1Str`]), this is an
/// unsized view type: it is only ever handled by reference, and `&L1Str` → `&TesId` is a
/// free pointer cast. Case folding is applied lazily on comparison rather than stored,
/// keeping the view zero-copy over the source record bytes.
#[derive(Debug)]
#[repr(transparent)]
pub struct TesId(L1Str);

impl TesId {
    /// View a Windows-1252 id string as a `TesId` without copying or decoding. Case
    /// folding is deferred to the [`PartialEq`] and [`Hash`] impls.
    pub fn new(id: &L1Str) -> &TesId {
        // SAFETY: `TesId` is `repr(transparent)` over `L1Str`, so `&L1Str` and `&TesId`
        // have identical layout (including the length metadata), and every Windows-1252
        // string is a valid id view.
        unsafe { &*(id as *const L1Str as *const TesId) }
    }

    /// View raw Windows-1252 id bytes as a `TesId` without copying or decoding.
    pub fn from_bytes(bytes: &[u8]) -> &TesId {
        TesId::new(L1Str::from_bytes(bytes))
    }

    /// View raw Windows-1252 id bytes as a `TesId` ending at the first NUL byte (or the
    /// whole slice if there is none), without copying or decoding.
    pub fn from_bytes_until_null(bytes: &[u8]) -> &TesId {
        TesId::new(L1Str::from_bytes_until_null(bytes))
    }

    /// The id bytes in normal form: ASCII-lowercase. Both equality and hashing go through
    /// this so they always agree.
    fn iter_normalized(&self) -> impl Iterator<Item = u8> + '_ {
        self.0.as_bytes().iter().map(|b| b.to_ascii_lowercase())
    }
}

impl Deref for TesId {
    type Target = L1Str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq for TesId {
    fn eq(&self, other: &Self) -> bool {
        self.iter_normalized().eq(other.iter_normalized())
    }
}

impl Eq for TesId {}

impl Hash for TesId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter_normalized().for_each(|b| state.write_u8(b));
        state.write_usize(self.0.len());
    }
}

/// Displays the original (un-folded) id text.
impl fmt::Display for TesId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl ToOwned for TesId {
    type Owned = TesIdBuf;
    fn to_owned(&self) -> TesIdBuf {
        TesIdBuf(self.0.to_l1string())
    }
}

/// An owned editor id: the [`TesId`] counterpart that owns its bytes, mirroring
/// [`L1String`]/[`L1Str`] and [`TesPathBuf`](crate::TesPathBuf)/[`TesPath`](crate::TesPath).
///
/// Reach for it as a map key when the id can't borrow from longer-lived storage. It derefs
/// (and [`Borrow`]s) to [`TesId`], so a `HashMap<TesIdBuf, _>` can be probed with a borrowed
/// `&TesId` and both agree on the case-folded equality/hash.
#[derive(Clone)]
#[repr(transparent)]
pub struct TesIdBuf(L1String);

impl TesIdBuf {
    /// Take ownership of raw Windows-1252 id bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> TesIdBuf {
        TesIdBuf(L1String::from_bytes(bytes))
    }

    /// Borrow as a [`TesId`].
    pub fn as_tes_id(&self) -> &TesId {
        TesId::new(&self.0)
    }
}

impl Deref for TesIdBuf {
    type Target = TesId;
    fn deref(&self) -> &TesId {
        self.as_tes_id()
    }
}

impl Borrow<TesId> for TesIdBuf {
    fn borrow(&self) -> &TesId {
        self.as_tes_id()
    }
}

impl From<&TesId> for TesIdBuf {
    fn from(id: &TesId) -> TesIdBuf {
        id.to_owned()
    }
}

// Equality and hashing delegate to `TesId` so they fold case, and — crucially — so a
// `TesIdBuf` key and a borrowed `&TesId` probe agree, as `Borrow` requires.
impl PartialEq for TesIdBuf {
    fn eq(&self, other: &Self) -> bool {
        self.as_tes_id() == other.as_tes_id()
    }
}

impl Eq for TesIdBuf {}

impl Hash for TesIdBuf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_tes_id().hash(state);
    }
}

impl fmt::Debug for TesIdBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_tes_id(), f)
    }
}

/// Displays the original (un-folded) id text.
impl fmt::Display for TesIdBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_tes_id(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    fn hash_of(id: &TesId) -> u64 {
        let mut hasher = DefaultHasher::new();
        id.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equal_ignoring_case() {
        let a = TesId::from_bytes(b"Furn_DE_Table_02");
        let b = TesId::from_bytes(b"furn_de_table_02");
        assert_eq!(a, b);
        assert_eq!(hash_of(a), hash_of(b));
    }

    #[test]
    fn separators_are_significant() {
        // Unlike a path, an id does not fold `/` and `\` together.
        assert_ne!(TesId::from_bytes(b"a/b"), TesId::from_bytes(b"a\\b"));
    }

    #[test]
    fn distinct_ids_differ() {
        assert_ne!(TesId::from_bytes(b"rat"), TesId::from_bytes(b"cliff_racer"));
    }

    #[test]
    fn deref_exposes_original_bytes() {
        let id = TesId::from_bytes(b"Furn_Thing");
        assert_eq!(id.as_bytes(), b"Furn_Thing");
    }

    #[test]
    fn owned_key_probed_by_borrowed_id() {
        let mut map = std::collections::HashMap::new();
        map.insert(TesId::from_bytes(b"Light_Fire").to_owned(), 7);
        // Look the owned key up through a differently-cased borrowed `&TesId`.
        assert_eq!(map.get(TesId::from_bytes(b"light_fire")), Some(&7));
        assert_eq!(map.get(TesId::from_bytes(b"light_ice")), None);
    }
}
