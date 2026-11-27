//! The tile cache: the tiles of each picture a viewer zoomed, for library and
//! import pictures alike, one folder per picture in the library's cache
//! folder on this device and one row per folder in `cached_tiles`, with a
//! 512 MiB budget. A picture is cut once, by one job that every request for
//! it shares, and reopening it reads the kept tiles.
//!
//! Its table is `tile_cache.sql`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_diagnostics::Diagnostics;
use bae_store::query::to_i64;
use bae_store::{DbError, Store, WriteStatements};
use coven::CovenWrite;
use tracing::{debug, warn};

use crate::tiles::{cut_tiles, tile_path, Pyramid};
use crate::{
    AskedTurn, CopyFormat, FlightError, Flights, IsCancelled, PictureOrigin, PictureTurn,
    PicturesError,
};

// The size of each picture's tiles that the cache keeps.
sqlive_compose::fragment!(
    CachedTileBytes,
    r#"
    SELECT tiles.bytes
    FROM cached_tiles AS tiles
"#
);

/// The bytes of tiles the cache keeps on a device.
const TILE_CACHE_BUDGET: u64 = 512 * 1024 * 1024;

/// Why an origin could not be opened for a cut: the caller's own error, with
/// the category a surface words it in.
#[derive(Debug)]
pub struct OpenError {
    pub category: bae_diagnostics::UiErrorCategory,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl OpenError {
    pub fn new(
        category: bae_diagnostics::UiErrorCategory,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self { category, source: Box::new(source) }
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Why a picture's tiles could not be read.
#[derive(Debug, thiserror::Error)]
pub enum TilesError {
    #[error("opening the picture failed: {0}")]
    Open(OpenError),
    #[error(transparent)]
    Pictures(#[from] PicturesError),
    #[error(transparent)]
    Store(#[from] DbError),
    #[error(transparent)]
    Task(#[from] coven::task::TaskFailed),
    #[error("the job that cuts a picture's tiles failed: {0}")]
    Shared(Arc<TilesError>),
    #[error("the job that cuts a picture's tiles panicked")]
    Panicked,
    /// The request named a tile outside the picture's levels.
    #[error("the picture has no tile at level {level}, column {x}, row {y}")]
    NoTile { level: u32, x: u32, y: u32 },
    /// The cache keeps a picture's row, and its tile is not in the folder the
    /// row names.
    #[error("the cache keeps the picture but not its tile at level {level}, column {x}, row {y}")]
    Missing { level: u32, x: u32, y: u32 },
}

/// The tiles of the pictures viewers zoomed, and the cuts running now.
#[derive(Clone)]
pub struct PictureTiles {
    disk: Arc<TileDisk>,
    cuts: Flights<String, Pyramid, TilesError>,
    #[cfg(any(test, feature = "test-utils"))]
    _owned_dir: Option<Arc<bae_util::test_files::TestFolder>>,
}

impl PictureTiles {
    /// The tile cache of the library at `library_path`, whose rows `store`
    /// holds.
    pub fn new(
        library_path: &Path,
        store: Store,
        diagnostics: Diagnostics,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self::in_dir(
            library_path.join("cache").join("picture-tiles"),
            TILE_CACHE_BUDGET,
            store,
            diagnostics,
            runtime,
        )
    }

    /// A cache over a temporary folder, whose rows `store` holds.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(
        store: Store,
        diagnostics: Diagnostics,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        let folder = bae_util::test_files::TestFolder::new();
        let mut tiles =
            Self::in_dir(folder.join("tiles"), TILE_CACHE_BUDGET, store, diagnostics, runtime);
        tiles._owned_dir = Some(Arc::new(folder));
        tiles
    }

    fn in_dir(
        dir: PathBuf,
        budget: u64,
        store: Store,
        diagnostics: Diagnostics,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            cuts: Flights::new(diagnostics.clone(), runtime),
            disk: Arc::new(TileDisk {
                dir,
                budget,
                store,
                diagnostics,
                changing: tokio::sync::Mutex::new(()),
            }),
            #[cfg(any(test, feature = "test-utils"))]
            _owned_dir: None,
        }
    }

    /// The tiles of the picture whose bytes `identity` names: the kept ones,
    /// marked as the most recently used, else the ones a cut of what `open`
    /// opens makes. Requests for one picture share one cut, which stops
    /// between two reads of the original when the last of them is dropped.
    pub async fn pyramid<F, Fut>(&self, identity: &str, open: F) -> Result<Pyramid, TilesError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn PictureOrigin>, OpenError>> + Send + 'static,
    {
        let folder = folder_for(identity);
        if let Some(pyramid) = self.disk.kept(&folder, Mark::Used).await? {
            return Ok(pyramid);
        }
        self.cut(folder, open).await
    }

    /// The bytes of the tile at column `x` and row `y` of level `level` (0
    /// is the picture's own size) of the picture `identity` names, cut first
    /// when the cache keeps none of it.
    pub async fn tile<F, Fut>(
        &self,
        identity: &str,
        level: u32,
        x: u32,
        y: u32,
        open: F,
    ) -> Result<Vec<u8>, TilesError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn PictureOrigin>, OpenError>> + Send + 'static,
    {
        let folder = folder_for(identity);
        let (pyramid, open) = match self.disk.kept(&folder, Mark::Unused).await? {
            Some(pyramid) => (pyramid, Some(open)),
            None => (self.cut(folder.clone(), open).await?, None),
        };
        let has_tile = pyramid
            .levels()
            .into_iter()
            .zip(0..)
            .any(|(tiles, index)| index == level && x < tiles.columns && y < tiles.rows);
        if !has_tile {
            return Err(TilesError::NoTile { level, x, y });
        }
        match (self.disk.read_tile(&folder, pyramid.format, level, x, y).await, open) {
            (Err(TilesError::Pictures(PicturesError::Read(error))), Some(open))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                // An eviction deletes the row before the folder, so a tile
                // that is gone means the cache let the picture go after its
                // row was read, and it is cut again. A row that is still
                // there names a folder without the tile, which no step of the
                // cache produces.
                if self.disk.kept(&folder, Mark::Unused).await?.is_some() {
                    return Err(TilesError::Missing { level, x, y });
                }
                let pyramid = self.cut(folder.clone(), open).await?;
                self.disk.read_tile(&folder, pyramid.format, level, x, y).await
            }
            (read, _) => read,
        }
    }

    /// Remove every row and the folder with every tile in it.
    pub async fn clear(&self) -> Result<(), DbError> {
        self.disk.clear().await
    }

    async fn cut<F, Fut>(&self, folder: String, open: F) -> Result<Pyramid, TilesError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn PictureOrigin>, OpenError>> + Send + 'static,
    {
        let disk = self.disk.clone();
        let job = {
            let folder = folder.clone();
            move |cancelled: IsCancelled, _| async move {
                let origin = open().await.map_err(TilesError::Open)?;
                disk.cut_and_keep(&folder, origin, cancelled).await
            }
        };
        // A viewer cuts a picture once the person zooms into it.
        let turn = AskedTurn::unchanging(PictureTurn::Focused);
        match self.cuts.request(folder, turn, job).await {
            Ok(pyramid) => Ok(*pyramid),
            Err(FlightError::Failed(error)) => Err(TilesError::Shared(error)),
            Err(FlightError::Panicked) => Err(TilesError::Panicked),
        }
    }
}

/// Whether a lookup of a kept picture marks it as the most recently used.
#[derive(Clone, Copy)]
enum Mark {
    Used,
    Unused,
}

/// The cache's folders and rows. The cache writes a folder before its row,
/// and deletes a row before its folder, so every row names a folder that is
/// there. A crash between the two steps leaves a folder that no row names: it
/// takes space, no read finds it, and a later cut of its picture or Clear
/// cache replaces or removes it.
struct TileDisk {
    dir: PathBuf,
    budget: u64,
    store: Store,
    diagnostics: Diagnostics,
    /// Held by each change to the folders and their rows (a keep, a clear)
    /// from its first step to its last, so no change runs between another's
    /// folder step and its row step.
    changing: tokio::sync::Mutex<()>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
)]
impl TileDisk {
    /// The pyramid the cache keeps in `folder`, marked as the most recently
    /// used when `mark` says so; none when it keeps none.
    async fn kept(&self, folder: &str, mark: Mark) -> Result<Option<Pyramid>, TilesError> {
        let folder = folder.to_string();
        let kept = match mark {
            Mark::Used => self.store.call(move |sql| mark_used(sql, &folder)).await?,
            Mark::Unused => {
                self.store
                    .read(move |sql| {
                        pyramid_from(sql.query(PYRAMID, [&folder], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })?)
                    })
                    .await?
            }
        };
        Ok(kept)
    }

    /// Cut `origin` into a folder of its own, keep it as `folder`, then
    /// remove the least recently used folders until the rows' bytes fit the
    /// budget; the picture just kept stays, even when its tiles alone are
    /// past the budget. A failed or cancelled cut removes what it wrote. A
    /// picture whose row is there when the cut ends, because another cut kept
    /// it first, keeps the tiles of that cut.
    async fn cut_and_keep(
        &self,
        folder: &str,
        origin: Box<dyn PictureOrigin>,
        cancelled: IsCancelled,
    ) -> Result<Pyramid, TilesError> {
        let dir = self.dir.clone();
        let (spool, pyramid, bytes) = self
            .diagnostics
            .blocking("cut a picture's tiles", move || cut_into_spool(&dir, origin, cancelled))
            .await??;
        let _changing = self.changing.lock().await;
        if let Some(kept) = self.kept(folder, Mark::Unused).await? {
            return Ok(kept);
        }
        let path = self.dir.join(folder);
        // No row names this folder, so one that is there is a crash's
        // leftover, which the cut replaces.
        let kept = self
            .diagnostics
            .blocking("keep a picture's tiles", move || match std::fs::remove_dir_all(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                _ => std::fs::rename(spool.path().join("tiles"), &path),
            })
            .await?;
        kept.map_err(|error| TilesError::Pictures(PicturesError::Write(error)))?;
        let budget = self.budget;
        let evicted = {
            let folder = folder.to_string();
            self.store.call(move |sql| keep(sql, &folder, pyramid, bytes, budget)).await?
        };
        if !evicted.is_empty() {
            debug!(removed = evicted.len(), budget = self.budget, "Evicted pictures' tiles");
            self.remove_folders(evicted).await?;
        }
        Ok(pyramid)
    }

    /// The bytes of one tile of the pyramid kept in `folder`.
    async fn read_tile(
        &self,
        folder: &str,
        format: CopyFormat,
        level: u32,
        x: u32,
        y: u32,
    ) -> Result<Vec<u8>, TilesError> {
        let path = tile_path(&self.dir.join(folder), format, level, x, y);
        let read = self.diagnostics.blocking("read a picture's tile", move || std::fs::read(path));
        read.await?.map_err(|error| TilesError::Pictures(PicturesError::Read(error)))
    }

    /// Delete every row, then the folder with every picture's tiles in it,
    /// those no row names among them.
    async fn clear(&self) -> Result<(), DbError> {
        let _changing = self.changing.lock().await;
        self.store.call(forget_all).await?;
        let dir = self.dir.clone();
        self.diagnostics
            .blocking("clear the tile cache", move || match std::fs::remove_dir_all(&dir) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                removed => removed,
            })
            .await
            .map_err(std::io::Error::other)??;
        Ok(())
    }

    /// Remove `folders`, whose rows are gone. A folder that stays only takes
    /// space, so the failure is logged.
    async fn remove_folders(&self, folders: Vec<String>) -> Result<(), TilesError> {
        let dir = self.dir.clone();
        self.diagnostics
            .blocking("remove pictures' tiles", move || {
                for folder in folders {
                    match std::fs::remove_dir_all(dir.join(&folder)) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => warn!("Could not remove the tiles in {folder}: {error}"),
                    }
                }
            })
            .await?;
        Ok(())
    }
}

/// Cut `origin` into the `tiles` folder of a new spool folder in `dir`, and
/// return the spool, which removes itself when dropped, with the pyramid and
/// its bytes on disk.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
)]
fn cut_into_spool(
    dir: &Path,
    origin: Box<dyn PictureOrigin>,
    cancelled: IsCancelled,
) -> Result<(tempfile::TempDir, Pyramid, u64), TilesError> {
    let write = |error| TilesError::Pictures(PicturesError::Write(error));
    std::fs::create_dir_all(dir).map_err(write)?;
    let spool = tempfile::Builder::new().prefix(".cutting-").tempdir_in(dir).map_err(write)?;
    let tiles = spool.path().join("tiles");
    let pyramid = cut_tiles(origin, cancelled, &tiles)?;
    let bytes = folder_bytes(&tiles).map_err(write)?;
    Ok((spool, pyramid, bytes))
}

/// The bytes of every file under `folder`.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: the tile cache's own folders, files bae keeps itself, which coven does not own"
)]
fn folder_bytes(folder: &Path) -> std::io::Result<u64> {
    let mut bytes = 0;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        bytes += if kind.is_dir() { folder_bytes(&entry.path())? } else { entry.metadata()?.len() };
    }
    Ok(bytes)
}

/// The name of the folder that holds the tiles of the picture `identity`
/// names.
fn folder_for(identity: &str) -> String {
    bae_util::fs::hash_bytes(identity.as_bytes())
}

/// The statement that reads the pyramid a folder's row describes.
const PYRAMID: &str = "SELECT width, height, format FROM cached_tiles WHERE folder = ?1";

/// The pyramid the row [`PYRAMID`] read describes, when it read one.
fn pyramid_from(rows: Vec<(u32, u32, String)>) -> Result<Option<Pyramid>, DbError> {
    rows.into_iter()
        .next()
        .map(|(width, height, format)| {
            let format = CopyFormat::from_stored_name(&format)
                .ok_or_else(|| DbError::unreadable("a tile format", &format))?;
            Ok(Pyramid { width, height, format })
        })
        .transpose()
}

sqlive_compose::write!(
    MarkTilesUsed(folder: &'p str),
    r#"
    UPDATE cached_tiles SET used = (SELECT MAX(used) + 1 FROM cached_tiles)
    WHERE folder = :folder
"#
);

sqlive_compose::write!(
    KeepTiles(folder: &'p str, width: u32, height: u32, format: &'static str, bytes: i64),
    r#"
    INSERT INTO cached_tiles (folder, width, height, format, bytes, used)
    VALUES (:folder, :width, :height, :format, :bytes,
        (SELECT COALESCE(MAX(used), 0) + 1 FROM cached_tiles))
"#
);

// The least recently used rows that don't fit `budget` with the more recently
// used ones, never the most recently used row.
sqlive_compose::write!(
    DeleteTilesOverBudget(budget: i64),
    r#"
    DELETE FROM cached_tiles WHERE folder IN (
        SELECT folder FROM (
            SELECT folder, SUM(bytes) OVER (ORDER BY used DESC) AS held FROM cached_tiles
        ) WHERE held > :budget
    ) AND used < (SELECT MAX(used) FROM cached_tiles)
    RETURNING folder
"#
);

sqlive_compose::write!(DeleteAllTiles, r#"DELETE FROM cached_tiles"#);

/// Mark `folder`'s row as the most recently used, and return the pyramid it
/// describes; none when there is no row.
fn mark_used(sql: &CovenWrite<'_>, folder: &str) -> Result<Option<Pyramid>, DbError> {
    sql.run(&MarkTilesUsedParams { folder })?;
    pyramid_from(sql.query(PYRAMID, [folder], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?)
}

/// Record `folder`, holding `pyramid` in `bytes`, as the most recently used,
/// then delete the least recently used rows until the rest fit `budget`, and
/// return the folders whose rows it deleted.
fn keep(
    sql: &CovenWrite<'_>,
    folder: &str,
    pyramid: Pyramid,
    bytes: u64,
    budget: u64,
) -> Result<Vec<String>, DbError> {
    sql.run(&KeepTilesParams {
        folder,
        width: pyramid.width,
        height: pyramid.height,
        format: pyramid.format.stored_name(),
        bytes: to_i64(bytes, "a picture's tiles' size")?,
    })?;
    let budget = to_i64(budget, "the tile cache's budget")?;
    let deleted = sql.returning(&DeleteTilesOverBudgetParams { budget })?;
    Ok(deleted.into_iter().map(|row| row.folder).collect())
}

/// Delete every row.
fn forget_all(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.run(&DeleteAllTilesParams {})?;
    Ok(())
}

#[cfg(all(test, libvips))]
#[path = "tile_cache_tests.rs"]
mod tests;
