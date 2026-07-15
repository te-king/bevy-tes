//! The layered game-data virtual file system behind the `tes://` asset source.
//!
//! Morrowind resolves a data path (`meshes\i\in_de_shack_01.nif`) by checking loose files
//! in the `Data Files` directory first, then the registered BSA archives. [`TesVfs`]
//! reproduces that with **one** case-insensitive index over both: loose files and every
//! archive entry share a single [`HashMap`] keyed on [`TesPath`], where **loose files win
//! over archives** and **later archives win over earlier ones** (the game's load-order
//! rule — `Bloodmoon.bsa` overrides `Morrowind.bsa`).
//!
//! The map is built once at construction. Archive entries key on a [`TesPath`] borrowed
//! straight from the (mmap-backed) archive and point at a zero-copy slice of it; loose
//! entries key on an owned [`TesPathBuf`] and point at their `root`-relative on-disk path,
//! read on demand. Because [`TesPath`] compares and hashes in the game's path normal form,
//! lookups are case-insensitive and `/`-vs-`\` agnostic on every platform.
//!
//! [`TesVfsReader`] adapts the VFS to Bevy's [`AssetReader`] so the whole layered view is
//! served as a single asset source — archive files as borrowed [`SliceReader`]s (no copy),
//! loose files through Bevy's own [`FileAssetReader`], whose reads are async and streaming
//! (and fd-limited) rather than a blocking slurp into a `Vec`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::{AssetReader, AssetReaderError, PathStream, Reader, SliceReader, VecReader};
use self_cell::self_cell;
use tes_core::tes_path::normalize;
use tes_core::{TesPath, TesPathBuf};
use tes3_bsa::Bsa;

/// A layered, case-insensitive view over a Morrowind `Data Files` directory and its BSA
/// archives. See the [module docs](self) for the precedence rules.
pub struct TesVfs {
    /// The data root loose entries are relative to; `None` for the empty VFS.
    root: Option<PathBuf>,
    internal: TesVfsInternal,
}

self_cell!(
    struct TesVfsInternal {
        owner: Box<[Bsa]>,

        #[covariant]
        dependent: TesVfsDirectory,
    }
);

struct TesVfsDirectory<'a> {
    /// The unified index. Keys are [`TesPath`]s — borrowed from an archive mapping for
    /// archived entries, owned for loose ones — so lookups normalize (case-fold, `/`→`\`)
    /// on the fly.
    table: HashMap<Cow<'a, TesPath>, Source<'a>>,
}

/// Where a resolved path's bytes come from.
enum Source<'a> {
    /// A zero-copy slice into a BSA archive mapping.
    Archived(&'a [u8]),
    /// A loose file, as its path relative to the VFS `root` (case preserved as on disk),
    /// read on demand.
    Loose(PathBuf),
}

impl TesVfs {
    /// Open a VFS over `root` with an explicit archive load order (paths resolved
    /// relative to the process, not `root`; later archives override earlier ones, and
    /// loose files under `root` override them all). Fails if `root` can't be walked or an
    /// archive can't be opened — an explicit list is a statement of intent.
    pub fn new(
        root: impl AsRef<Path>,
        archives: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> io::Result<TesVfs> {
        let root = root.as_ref().to_path_buf();
        let archives = archives
            .into_iter()
            .map(|p| Bsa::open(p.as_ref()).map_err(io::Error::other))
            .collect::<io::Result<Vec<_>>>()?
            .into_boxed_slice();
        let internal =
            TesVfsInternal::try_new(archives, |archives| build_directory(archives, &root))?;
        Ok(TesVfs {
            root: Some(root),
            internal,
        })
    }

    /// Open a VFS over `root`, discovering `*.bsa` archives at its top level and ordering
    /// them by modification time, oldest first — which reproduces the vanilla game's
    /// effective order (`Morrowind.bsa` < `Tribunal.bsa` < `Bloodmoon.bsa`). For modded
    /// setups with different conventions, use [`TesVfs::new`] with an explicit order.
    /// Archives that fail to open are skipped with a warning on stderr.
    pub fn open(root: impl AsRef<Path>) -> io::Result<TesVfs> {
        let root = root.as_ref().to_path_buf();
        let mut bsas: Vec<PathBuf> = std::fs::read_dir(&root)?
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
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let internal =
            TesVfsInternal::try_new(archives, |archives| build_directory(archives, &root))?;
        Ok(TesVfs {
            root: Some(root),
            internal,
        })
    }

    /// An empty VFS: every lookup misses. Used to keep an app bootable when the data
    /// directory is absent.
    pub fn empty() -> TesVfs {
        let internal = TesVfsInternal::new(Box::new([]), |_| TesVfsDirectory {
            table: HashMap::new(),
        });
        TesVfs {
            root: None,
            internal,
        }
    }

    /// Whether `path` (any case, `/` or `\` separators) resolves to a file. I/O-free.
    pub fn contains(&self, path: &str) -> bool {
        self.internal
            .borrow_dependent()
            .table
            .contains_key(TesPath::from_bytes(path.as_bytes()))
    }

    /// Read a file's bytes: a copy of the archive slice, or the loose file read from disk.
    /// `None` when the path isn't in the VFS (or a loose file can't be read).
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self
            .internal
            .borrow_dependent()
            .table
            .get(TesPath::from_bytes(path.as_bytes()))?
        {
            Source::Archived(bytes) => Some(bytes.to_vec()),
            Source::Loose(rel) => std::fs::read(self.root.as_ref()?.join(rel)).ok(),
        }
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

    /// Resolve an ESM model reference to the VFS path (forward-slash form) that actually
    /// exists, or `None`.
    ///
    /// `MODL` subrecord values are relative to `meshes\` without the prefix
    /// (`f\act_bm_firelake00.nif`) — the engine prepends it. The verbatim path is also
    /// tried as cheap robustness for mods that embed the prefix.
    pub fn resolve_model(&self, name: &str) -> Option<String> {
        [format!("meshes\\{name}"), name.to_string()]
            .into_iter()
            .find(|c| self.contains(c))
            .map(|c| normalize(&c).replace('\\', "/"))
    }
}

/// Build the unified directory: archive entries first (in load order, so later archives
/// overwrite earlier ones), then loose files (so they win over every archive).
fn build_directory<'a>(archives: &'a [Bsa], root: &Path) -> io::Result<TesVfsDirectory<'a>> {
    let mut table: HashMap<Cow<'a, TesPath>, Source<'a>> = HashMap::new();
    for bsa in archives {
        for (name, data) in bsa.files() {
            table.insert(Cow::Borrowed(name), Source::Archived(data));
        }
    }
    index_loose_files(root, &mut table)?;
    Ok(TesVfsDirectory { table })
}

/// Walk `root` recursively, inserting each loose file under its normalized relative path,
/// overwriting any archive entry at the same path.
fn index_loose_files<'a>(
    root: &Path,
    table: &mut HashMap<Cow<'a, TesPath>, Source<'a>>,
) -> io::Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                let key = TesPathBuf::from_bytes(rel.as_os_str().as_bytes().to_vec());
                table.insert(Cow::Owned(key), Source::Loose(rel.to_path_buf()));
            }
        }
    }
    Ok(())
}

/// [`AssetReader`] serving the `tes://` source from a shared [`TesVfs`]. Archive files
/// are served as zero-copy [`SliceReader`]s; loose files are delegated to Bevy's own
/// [`FileAssetReader`], rooted at the same data directory the VFS indexes.
pub struct TesVfsReader {
    vfs: Arc<TesVfs>,
    loose: FileAssetReader,
}

impl TesVfsReader {
    /// A reader over `vfs`, serving loose files from `root`. Pass an **absolute** root:
    /// [`FileAssetReader`] resolves relative roots against Bevy's base path (the
    /// executable's directory), not the working directory — so an absolute root is what
    /// keeps it pointing at the same tree the VFS walked.
    pub fn new(vfs: Arc<TesVfs>, root: impl AsRef<Path>) -> TesVfsReader {
        TesVfsReader {
            vfs,
            loose: FileAssetReader::new(root),
        }
    }
}

impl AssetReader for TesVfsReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        let key = TesPath::from_bytes(path.as_os_str().as_bytes());
        match self.vfs.internal.borrow_dependent().table.get(key) {
            // Zero-copy view straight into the archive mapping.
            Some(Source::Archived(bytes)) => {
                Ok(Box::new(SliceReader::new(bytes)) as Box<dyn Reader + 'a>)
            }
            // Bevy's file reader: async, streaming, fd-limited. The stored path is already
            // `root`-relative, which is exactly what `FileAssetReader` expects.
            Some(Source::Loose(rel)) => self
                .loose
                .read(rel)
                .await
                .map(|reader| Box::new(reader) as Box<dyn Reader + 'a>),
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
