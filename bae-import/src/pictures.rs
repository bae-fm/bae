//! The pictures of the import list: an image file in a candidate's folder, or
//! art embedded in one of its audio files. A screen asks for one at a pixel
//! size and gets a small encoded copy, read from the original once, in chunks
//! that stop when nobody wants the picture, off the app's threads, and kept
//! with its candidate until the candidate leaves the list. The viewer's tiles
//! are cut from the original, which the same reader opens.

use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;

use bae_pictures::{
    AskedTurn, FlightError, Flights, IsCancelled, PictureCopy, PictureStep, PictureTurn, Volumes,
};
use coven::LeasedFile;

use crate::folder_scanner::FileVersion;
use crate::{CoverRefusal, ImportError, ImportPicture};

/// Why a picture could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PictureError {
    /// The file is no image of a candidate the import list holds: it left the
    /// list or its folder after the screen asked, or was never one of them.
    #[error("{} is not an image of a candidate in the import list", .0.display())]
    NotInImport(std::path::PathBuf),
    /// The file is not the version the scan read: the person changed it
    /// since, and a copy of it would be kept under the old version.
    #[error("{} changed after its import candidate was scanned", .0.display())]
    ChangedSinceScan(std::path::PathBuf),
    #[error("opening {} failed: {error}", path.display())]
    Open { path: std::path::PathBuf, error: std::io::Error },
    /// The opened file's modification time could not be read to compare it
    /// with the scan's.
    #[error(transparent)]
    ModifiedTime(#[from] crate::folder_scanner::FolderScanError),
    #[error(transparent)]
    Pictures(#[from] bae_pictures::PicturesError),
    #[error(transparent)]
    Task(#[from] coven::task::TaskFailed),
    #[error(transparent)]
    Db(#[from] bae_store::DbError),
    #[error(transparent)]
    NoArt(#[from] CoverRefusal),
}

/// What a picture's job reads and writes: the import's tables, where its
/// copies are kept, the places open files take, and the reporter of a bug in
/// the job.
#[derive(Clone)]
struct PictureServices {
    tables: crate::tables::ImportTables,
    files: coven::FileBudget,
    diagnostics: bae_diagnostics::Diagnostics,
}

/// The jobs making pictures now, and the reads each volume takes at once.
#[derive(Clone)]
pub(crate) struct ImportPictures {
    flights: Flights<(ImportPicture, PictureStep), PictureCopy, PictureError>,
    volumes: Volumes,
    services: PictureServices,
}

impl ImportPictures {
    pub(crate) fn new(
        tables: crate::tables::ImportTables,
        files: coven::FileBudget,
        diagnostics: bae_diagnostics::Diagnostics,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            flights: Flights::new(diagnostics.clone(), runtime),
            volumes: Volumes::new(),
            services: PictureServices { tables, files, diagnostics },
        }
    }

    /// The bytes of `source` at the size `pixels` asks for. Requests for the
    /// same picture and size share one job, which reads at the highest turn
    /// any of them asks and ends when the last of them is dropped.
    pub(crate) async fn read(
        &self,
        source: ImportPicture,
        pixels: u32,
        turn: AskedTurn,
    ) -> Result<Vec<u8>, ImportError> {
        Ok(self.read_copy(source, PictureStep::covering(pixels), turn).await?.bytes)
    }

    /// The copy of `source` at `step`, with its format, which the import
    /// writes into its release.
    pub(crate) async fn read_copy(
        &self,
        source: ImportPicture,
        step: PictureStep,
        turn: AskedTurn,
    ) -> Result<PictureCopy, ImportError> {
        let job = {
            let (services, volumes, source) =
                (self.services.clone(), self.volumes.clone(), source.clone());
            move |cancelled: IsCancelled, turn| {
                make(services, volumes, source, step, turn, cancelled)
            }
        };
        match self.flights.request((source, step), turn, job).await {
            Ok(copy) => Ok(Arc::try_unwrap(copy).unwrap_or_else(|shared| (*shared).clone())),
            Err(FlightError::Failed(error)) => Err(ImportError::Picture(error)),
            Err(FlightError::Panicked) => Err(ImportError::Internal {
                detail: "the job that makes a picture panicked".to_string(),
            }),
        }
    }

    /// The original of `source` for a cut that reads it to its end, such as
    /// the viewer's tiles: the image file at the version the scan read, or
    /// the art the candidate's snapshot stored. The file is opened off the
    /// app's threads, and holds one of its volume's places for reads, as the
    /// picture the person looks at, until the origin is dropped.
    pub(crate) async fn open_original(
        &self,
        source: ImportPicture,
    ) -> Result<Box<dyn bae_pictures::PictureOrigin>, ImportError> {
        let services = &self.services;
        let owner = services
            .tables
            .picture_owner(&source, None)
            .await?
            .ok_or_else(|| picture_error(PictureError::NotInImport(source.version().path.clone())))?
            .owner;
        match source_origin(services, &source).await.map_err(picture_error)? {
            Origin::Art(art) => Ok(Box::new(Cursor::new(art))),
            Origin::File(version) => {
                let place = self
                    .volumes
                    .read_place(
                        &owner.watched_folder_path,
                        reads_at_once(owner.volume),
                        AskedTurn::unchanging(PictureTurn::Focused),
                    )
                    .await;
                let lease = bae_store::lease_one(&services.files).await;
                let file = services
                    .diagnostics
                    .blocking("open a picture's original", move || open(&version, &lease))
                    .await
                    .map_err(|failed| picture_error(PictureError::Task(failed)))?
                    .map_err(picture_error)?;
                Ok(Box::new(bae_pictures::HeldOrigin::new(file, place)))
            }
        }
    }
}

/// The file an original is read from, which holds one of the installation's
/// places for open files while it is open.
struct FileOrigin(LeasedFile<File>);

impl Read for FileOrigin {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buffer)
    }
}

impl Seek for FileOrigin {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        (&*self.0).seek(position)
    }
}

/// Where the bytes of one picture are read from.
enum Origin {
    /// The image file, at the version the scan read.
    File(FileVersion),
    /// The art the candidate's tag snapshot stored.
    Art(Vec<u8>),
}

/// `source` at `step`: the kept copy when the candidate keeps one, else a
/// read of the original that ends when `cancelled` is set, kept for the next
/// request.
async fn make(
    services: PictureServices,
    volumes: Volumes,
    source: ImportPicture,
    step: PictureStep,
    turn: AskedTurn,
    cancelled: IsCancelled,
) -> Result<PictureCopy, PictureError> {
    let picture = services
        .tables
        .picture_owner(&source, Some(step))
        .await?
        .ok_or_else(|| PictureError::NotInImport(source.version().path.clone()))?;
    if let Some(kept) = picture.kept {
        return Ok(kept);
    }
    let owner = picture.owner;
    let origin = source_origin(&services, &source).await?;
    // A share is read a few files at a time, and the picture's turn says
    // where it stands in the line for one of those reads.
    let _place =
        volumes.read_place(&owner.watched_folder_path, reads_at_once(owner.volume), turn).await;
    let lease = bae_store::lease_one(&services.files).await;
    let copy = services
        .diagnostics
        .blocking("make a picture copy", move || read_picture(origin, step, cancelled, lease))
        .await??;
    services.tables.store_picture_copy(&owner, step, &copy).await?;
    Ok(copy)
}

fn picture_error(error: PictureError) -> ImportError {
    ImportError::Picture(Arc::new(error))
}

/// Where the bytes of `source` are read from: the art its candidate's
/// snapshot stored, or the image file at the version the scan read.
async fn source_origin(
    services: &PictureServices,
    source: &ImportPicture,
) -> Result<Origin, PictureError> {
    Ok(match source {
        ImportPicture::Embedded { candidate, source_file_id, .. } => Origin::Art(
            services
                .tables
                .load_embedded_cover_bytes(candidate, source_file_id)
                .await?
                .ok_or_else(|| CoverRefusal::NoTagArtwork {
                    source_file_id: source_file_id.clone(),
                })?,
        ),
        ImportPicture::Folder { file } => Origin::File(file.clone()),
    })
}

/// How many pictures of one watched folder are read at once: a few from a
/// disk, fewer from a share, which answers each read slowly.
fn reads_at_once(volume: crate::VolumeKind) -> usize {
    match volume {
        crate::VolumeKind::Local => 4,
        crate::VolumeKind::Network => 2,
    }
}

/// Read the copy of the picture from `origin` at `step`, on a blocking thread.
fn read_picture(
    origin: Origin,
    step: PictureStep,
    cancelled: IsCancelled,
    lease: coven::FileLease,
) -> Result<PictureCopy, PictureError> {
    Ok(match origin {
        Origin::File(version) => bae_pictures::make_copy(open(&version, &lease)?, cancelled, step)?,
        Origin::Art(art) => bae_pictures::make_copy(Cursor::new(art), cancelled, step)?,
    })
}

/// Opens the file the scan read as `version`, refused when it is no longer that
/// version: a copy of a file the person replaced since would be kept under the
/// version the scan read.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: the picture is the person's own file in a watched folder, read before coven records anything of it"
)]
fn open(version: &FileVersion, lease: &coven::FileLease) -> Result<FileOrigin, PictureError> {
    let path = &version.path;
    #[cfg(test)]
    crate::tests::reads::record(crate::tests::reads::FileRead::PictureOriginal, path);
    let open_error = |error| PictureError::Open { path: path.clone(), error };
    let file = lease.place_blocking().open(path).map_err(open_error)?;
    // The opened file is the one that is read, so its size and time are the
    // ones to compare.
    let metadata = file.metadata().map_err(open_error)?;
    let modified_at_ns = crate::folder_scanner::file_modified_at_ns(path, &metadata)?;
    if metadata.len() != version.size || modified_at_ns != version.modified_at_ns {
        return Err(PictureError::ChangedSinceScan(path.clone()));
    }
    Ok(FileOrigin(file))
}
