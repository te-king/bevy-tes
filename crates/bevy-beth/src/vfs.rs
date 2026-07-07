//! The layered game-data virtual file system behind the `tes://` asset source.
//!
//! Morrowind resolves a data path (`meshes\i\in_de_shack_01.nif`) by checking loose files
//! in the `Data Files` directory first, then the registered BSA archives. [`TesVfs`]
//! reproduces that as two layers:
//!
//! - **Loose files** are probed live on the filesystem — no index, no startup walk. Each
//!   lookup tries the normal form (lowercase, `/`-separated) first and then the path
//!   exactly as written. On case-insensitive filesystems (macOS, Windows — the platforms
//!   the game itself targets) that resolves any casing, like the original engine; on
//!   case-sensitive filesystems a loose file is found when the on-disk tree is lowercase
//!   (the common shipping convention) or the reference matches its exact case.
//! - **Archives** are indexed once at open: every entry of every archive goes into one
//!   map from normal-form path to the file's bytes — zero-copy slices into the mmaps,
//!   with the keys borrowed straight from them too (a `self_cell` pairing) — inserted in
//!   load order so later archives win (the game's rule: `Bloodmoon.bsa` overrides
//!   `Morrowind.bsa`). Archive lookups are therefore always case-insensitive and
//!   separator-agnostic, and precedence costs nothing per lookup.
//!
//! Loose files win over archives. [`TesVfsReader`] adapts the VFS to Bevy's
//! [`AssetReader`] so the whole layered view is served as a single asset source, with
//! loose reads delegated to Bevy's own [`FileAssetReader`].

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, SliceReader, VecReader};
use tes_core::paths::normalize;
use tes_core::{L1Str, L1String};
use tes3_bsa::Bsa;

/// The merged directory of all open archives: normal-form path → file bytes, inserted in
/// load order so later archives win. Keys borrow the stored names straight out of the
/// archive mappings — vanilla archives store names in normal form already — falling back
/// to owned normalized copies for archives that don't (the engine tolerates those, since
/// it looks up purely by hash).
///
/// `L1Str` keys compare Windows-1252 bytes while queries arrive as UTF-8, so a stored
/// name with non-ASCII bytes only matches a byte-identical query. Vanilla names are all
/// ASCII; this is a documented edge, not a behavior change.
struct ArchiveIndex<'a> {
    map: HashMap<Cow<'a, L1Str>, &'a [u8]>,
}

impl<'a> ArchiveIndex<'a> {
    fn build(archives: &'a [Bsa]) -> ArchiveIndex<'a> {
        let mut map = HashMap::with_capacity(archives.iter().map(Bsa::len).sum());
        for bsa in archives {
            for file in bsa.files() {
                let raw = file.name.as_bytes();
                let key = if raw.iter().any(|&b| b == b'/' || b.is_ascii_uppercase()) {
                    Cow::Owned(L1String::from_bytes(
                        normalize(&file.name.decode()).into_bytes(),
                    ))
                } else {
                    Cow::Borrowed(file.name)
                };
                map.insert(key, bsa.bytes(file));
            }
        }
        ArchiveIndex { map }
    }
}

self_cell::self_cell!(
    struct ArchivesCell {
        owner: Vec<Bsa>,

        #[covariant]
        dependent: ArchiveIndex,
    }
);

/// A layered, case-tolerant view over a Morrowind `Data Files` directory: loose files
/// over BSA archives. See the [module docs](self) for the layering and case rules.
pub struct TesVfs {
    /// The loose-file tree; `None` for the empty VFS, so probes can't accidentally hit
    /// paths relative to the working directory.
    root: Option<PathBuf>,
    /// The open archives coupled to their merged lookup index.
    archives: ArchivesCell,
}

/// A successful loose-file probe: where the file is, and which query form found it.
struct LooseHit {
    on_disk: PathBuf,
    /// The forward-slash form that hit — normal form, or the path as given when only
    /// its exact case exists on a case-sensitive filesystem.
    form: String,
}

impl TesVfs {
    /// Open a VFS over `root` with an explicit archive load order (paths resolved
    /// relative to the process, not `root`; later archives override earlier ones). An
    /// unreadable archive is an error, since an explicit list is a statement of intent;
    /// `root` itself is only probed lazily, so a missing directory just means every
    /// loose lookup misses.
    pub fn new(
        root: impl AsRef<Path>,
        archives: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> io::Result<TesVfs> {
        let archives = archives
            .into_iter()
            .map(|p| Bsa::open(p.as_ref()).map_err(io::Error::other))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(TesVfs::assemble(
            Some(root.as_ref().to_path_buf()),
            archives,
        ))
    }

    /// Open a VFS over `root`, discovering `*.bsa` archives at its top level and
    /// ordering them by modification time, oldest first — which reproduces the vanilla
    /// game's effective order (`Morrowind.bsa` < `Tribunal.bsa` < `Bloodmoon.bsa`). For
    /// modded setups with different conventions, use [`TesVfs::new`] with an explicit
    /// order instead. Fails only if `root` can't be listed; archives that fail to open
    /// are skipped with a warning on stderr.
    pub fn open(root: impl AsRef<Path>) -> io::Result<TesVfs> {
        let root = root.as_ref();
        let mut bsas: Vec<PathBuf> = std::fs::read_dir(root)?
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa")))
            .collect();
        bsas.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());

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
        Ok(TesVfs::assemble(Some(root.to_path_buf()), archives))
    }

    /// An empty VFS: every lookup misses. Used to keep an app bootable when the data
    /// directory is absent.
    pub fn empty() -> TesVfs {
        TesVfs::assemble(None, Vec::new())
    }

    fn assemble(root: Option<PathBuf>, archives: Vec<Bsa>) -> TesVfs {
        TesVfs {
            root,
            archives: ArchivesCell::new(archives, |a| ArchiveIndex::build(a)),
        }
    }

    /// Whether `path` (either separator; case rules per the [module docs](self))
    /// resolves to a file.
    pub fn contains(&self, path: &str) -> bool {
        self.loose(path).is_some() || self.archived(path).is_some()
    }

    /// Read a file's bytes: the loose file if present, else the *last* archive that
    /// contains the path. Archive content is a zero-copy borrow of the mapping; loose
    /// content is read from disk. `None` when nowhere.
    pub fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        if let Some(hit) = self.loose(path) {
            return std::fs::read(hit.on_disk).ok().map(Cow::Owned);
        }
        self.archived(path).map(Cow::Borrowed)
    }

    /// The archive layer's bytes for `path`, ignoring loose files: one lookup in the
    /// merged normal-form index (later archives already won at build time).
    fn archived(&self, path: &str) -> Option<&[u8]> {
        let key = normalize(path);
        self.archives
            .borrow_dependent()
            .map
            .get(L1Str::from_bytes(key.as_bytes()))
            .copied()
    }

    /// Probe the loose tree: normal form first (the canonical query form — and the only
    /// one that matters on the game's own case-insensitive platforms), then the path
    /// exactly as given, rescuing exact-case references into mixed-case trees on
    /// case-sensitive filesystems.
    fn loose(&self, path: &str) -> Option<LooseHit> {
        let root = self.root.as_ref()?;
        let normal = normalize(path).replace('\\', "/");
        let as_given = path.replace('\\', "/");

        let mut candidates = vec![normal];
        if as_given != candidates[0] {
            candidates.push(as_given);
        }
        for form in candidates {
            // Both forms have identical components apart from case, so one rejection
            // rejects the path outright.
            let on_disk = root.join(safe_relative(&form)?);
            if on_disk.is_file() {
                return Some(LooseHit { on_disk, form });
            }
        }
        None
    }

    /// Resolve the first candidate that exists to the forward-slash query form to load
    /// it by: normal form, unless only an exact-case loose file matched.
    fn resolve(&self, candidates: impl IntoIterator<Item = String>) -> Option<String> {
        for candidate in candidates {
            if let Some(hit) = self.loose(&candidate) {
                return Some(hit.form);
            }
            if self.archived(&candidate).is_some() {
                return Some(normalize(&candidate).replace('\\', "/"));
            }
        }
        None
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

        self.resolve(candidates)
    }

    /// Resolve an ESM model reference to the VFS path (forward-slash form) that actually
    /// exists, or `None`.
    ///
    /// `MODL` subrecord values are relative to `meshes\` without the prefix
    /// (`f\act_bm_firelake00.nif`) — the engine prepends it. The verbatim path is also
    /// tried as cheap robustness for mods that embed the prefix.
    pub fn resolve_model(&self, name: &str) -> Option<String> {
        self.resolve([format!("meshes\\{name}"), name.to_string()])
    }
}

/// Interpret a forward-slash game-data path as a relative filesystem path, refusing
/// anything that could escape the data root (absolute paths, `..` components — data
/// references shouldn't contain either, but they come from untrusted files).
fn safe_relative(path: &str) -> Option<PathBuf> {
    let rel: PathBuf = path.split('/').collect();
    rel.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then_some(rel)
}

/// [`AssetReader`] serving the `tes://` source: loose files through Bevy's own
/// [`FileAssetReader`], archive content as zero-copy [`SliceReader`]s over the shared
/// [`TesVfs`].
pub struct TesVfsReader {
    vfs: Arc<TesVfs>,
    loose: FileAssetReader,
}

impl TesVfsReader {
    /// A reader over `vfs` whose loose layer is served from `root`. Pass an **absolute**
    /// root: [`FileAssetReader`] resolves relative paths against Bevy's base path (the
    /// executable's directory), not the working directory.
    pub fn new(vfs: Arc<TesVfs>, root: impl AsRef<Path>) -> TesVfsReader {
        TesVfsReader {
            vfs,
            loose: FileAssetReader::new(root),
        }
    }
}

impl AssetReader for TesVfsReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let query = path.to_string_lossy();

        // Loose layer, probing in the same order as `TesVfs::loose`. `FileAssetReader`
        // ties the returned reader's lifetime to the path argument and our candidates
        // are locals, so a hit is drained into an owned `VecReader` — the same single
        // copy a filesystem read costs anyway.
        let normal = normalize(&query).replace('\\', "/");
        let as_given = query.replace('\\', "/");
        let mut candidates = vec![normal];
        if as_given != candidates[0] {
            candidates.push(as_given);
        }
        for form in &candidates {
            if safe_relative(form).is_none() {
                break;
            }
            match self.loose.read(Path::new(form)).await {
                Ok(mut reader) => {
                    let mut bytes = Vec::new();
                    reader
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|e| AssetReaderError::Io(Arc::new(e)))?;
                    return Ok(Box::new(VecReader::new(bytes)) as Box<dyn Reader + 'a>);
                }
                Err(AssetReaderError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }

        // Archive layer: a zero-copy view straight into the mmap.
        match self.vfs.archived(&query) {
            Some(bytes) => Ok(Box::new(SliceReader::new(bytes)) as Box<dyn Reader + 'a>),
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
