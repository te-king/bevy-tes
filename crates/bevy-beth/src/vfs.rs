//! The layered game-data virtual file system behind the `tes://` asset source.
//!
//! Morrowind resolves a data path (`meshes\i\in_de_shack_01.nif`) by checking loose files
//! in the `Data Files` directory first, then the registered BSA archives. [`TesVfs`]
//! reproduces that: one eager, case-insensitive index over the loose file tree, layered
//! over any number of open (mmap-backed) [`Bsa`] archives, where **loose files win over
//! archives** and **later archives win over earlier ones** (the game's load-order rule —
//! `Bloodmoon.bsa` overrides `Morrowind.bsa`).
//!
//! Path lookups are case-insensitive and separator-agnostic on every platform: both
//! `TEXTURES\TX_WOOD.DDS` and `textures/tx_wood.dds` resolve to the same entry, matching
//! how the game (built for a case-insensitive file system) treats paths.
//!
//! [`TesVfsReader`] adapts the VFS to Bevy's [`AssetReader`] so the whole layered view is
//! served as a single asset source.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, VecReader};
use tes_core::paths::normalize;
use tes3_bsa::Bsa;

/// A layered, case-insensitive view over a Morrowind `Data Files` directory: loose files
/// over BSA archives. See the [module docs](self) for the precedence rules.
pub struct TesVfs {
    /// Normalized path → actual on-disk path, built by one walk of the root at
    /// construction. Makes lookups case-correct on case-sensitive file systems and
    /// existence checks I/O-free.
    loose: HashMap<String, PathBuf>,
    /// Open archives in load order; later archives take precedence.
    archives: Vec<Bsa>,
}

impl TesVfs {
    /// Open a VFS over `root` with an explicit archive load order (paths resolved
    /// relative to the process, not `root`; later archives override earlier ones).
    /// Fails only if `root` can't be walked; an unreadable archive is an error too,
    /// since an explicit list is a statement of intent.
    pub fn new(
        root: impl AsRef<Path>,
        archives: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> io::Result<TesVfs> {
        let loose = index_loose_files(root.as_ref())?;
        let archives = archives
            .into_iter()
            .map(|p| Bsa::open(p.as_ref()).map_err(io::Error::other))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(TesVfs { loose, archives })
    }

    /// Open a VFS over `root`, discovering `*.bsa` archives at its top level and
    /// ordering them by modification time, oldest first — which reproduces the vanilla
    /// game's effective order (`Morrowind.bsa` < `Tribunal.bsa` < `Bloodmoon.bsa`). For
    /// modded setups with different conventions, use [`TesVfs::new`] with an explicit
    /// order instead. Archives that fail to open are skipped with a warning on stderr.
    pub fn open(root: impl AsRef<Path>) -> io::Result<TesVfs> {
        let root = root.as_ref();
        let mut bsas: Vec<PathBuf> = std::fs::read_dir(root)?
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")))
            .collect();
        bsas.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());

        let loose = index_loose_files(root)?;
        let archives = bsas
            .iter()
            .filter_map(|p| match Bsa::open(p) {
                Ok(bsa) => Some(bsa),
                Err(e) => {
                    eprintln!(
                        "bevy-beth: skipping unreadable archive {}: {e}",
                        p.display()
                    );
                    None
                }
            })
            .collect();
        Ok(TesVfs { loose, archives })
    }

    /// An empty VFS: every lookup misses. Used to keep an app bootable when the data
    /// directory is absent.
    pub fn empty() -> TesVfs {
        TesVfs {
            loose: HashMap::new(),
            archives: Vec::new(),
        }
    }

    /// Whether `path` (any case, `/` or `\` separators) resolves to a file. I/O-free.
    pub fn contains(&self, path: &str) -> bool {
        let key = normalize(path);
        self.loose.contains_key(&key) || self.archives.iter().any(|a| a.get(&key).is_some())
    }

    /// Read a file's bytes: the loose file if present, else the *last* archive that
    /// contains the path. `None` when nowhere.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let key = normalize(path);
        if let Some(on_disk) = self.loose.get(&key) {
            return std::fs::read(on_disk).ok();
        }
        self.archives
            .iter()
            .rev()
            .find_map(|a| a.get(&key))
            .map(<[u8]>::to_vec)
    }

    /// Resolve a NIF texture reference to the VFS path (forward-slash form) that actually
    /// exists, or `None`.
    ///
    /// NIF `NiSourceTexture` names are usually bare filenames (`Tx_BeerStein.dds`) that
    /// the engine looks up under `textures\`, occasionally with the prefix embedded; and
    /// Morrowind routinely ships a `.tga`-named texture as `.dds` (or vice versa), so
    /// both extensions are tried.
    pub fn resolve_texture(&self, name: &str) -> Option<String> {
        let base = name.rsplit(['\\', '/']).next().unwrap_or(name);
        let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);

        let mut candidates = Vec::with_capacity(4);
        if base != name {
            // The reference embeds a path (e.g. `textures\foo.dds`): honour it verbatim.
            candidates.push(name.to_string());
        }
        candidates.push(format!("textures\\{base}"));
        candidates.push(format!("textures\\{stem}.dds"));
        candidates.push(format!("textures\\{stem}.tga"));

        candidates
            .into_iter()
            .find(|c| self.contains(c))
            .map(|c| normalize(&c).replace('\\', "/"))
    }
}

/// Walk `root` recursively, mapping each file's normalized relative path to its on-disk
/// path.
fn index_loose_files(root: &Path) -> io::Result<HashMap<String, PathBuf>> {
    let mut index = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                index.insert(normalize(&rel.to_string_lossy()), path);
            }
        }
    }
    Ok(index)
}

/// [`AssetReader`] serving the `tes://` source from a shared [`TesVfs`].
pub struct TesVfsReader(pub Arc<TesVfs>);

impl AssetReader for TesVfsReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        match self.0.read(&path.to_string_lossy()) {
            Some(bytes) => Ok(VecReader::new(bytes)),
            None => Err(AssetReaderError::NotFound(path.to_path_buf())),
        }
    }

    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        // Game data carries no .meta files; Bevy falls back to default meta on NotFound.
        Err::<VecReader, _>(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        // Folder loads aren't supported through the VFS (individual paths only).
        Err(AssetReaderError::NotFound(path.to_path_buf()))
    }

    async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
        Ok(false)
    }
}
