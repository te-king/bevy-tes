//! The TES3 game-data path convention.
//!
//! The engine treats data paths — BSA directory entries, ESM model/icon references, NIF
//! texture names, loose files on disk — as case-insensitive with interchangeable `/` and
//! `\` separators. Every index or comparison therefore goes through one **normal form**:
//! lowercase, backslash-separated (the form BSA directories store natively).

/// Normalize a game-data path for lookup or comparison: ASCII-lowercase, `/` → `\`.
///
/// This is the shared normal form used by `tes3_bsa`'s archive index and `bevy_beth`'s
/// VFS — indexes built with it and keys looked up through it always agree.
pub fn normalize(path: &str) -> String {
    path.to_ascii_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_separators() {
        assert_eq!(normalize("Textures/TX_Wood.DDS"), r"textures\tx_wood.dds");
        assert_eq!(normalize(r"meshes\i\Shack.NIF"), r"meshes\i\shack.nif");
        assert_eq!(normalize(""), "");
    }
}
