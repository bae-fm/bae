//! The pictures of the import list: an image file in a candidate's folder, or
//! art embedded in one of its audio files. A screen asks for one at a pixel
//! size and gets a small encoded copy, read from the original once, in chunks
//! that stop when nobody wants the picture, off the app's threads, and kept
//! with its candidate until the candidate leaves the list. A viewer that
//! decodes at full size asks for the original's own bytes through the same
//! reader.

use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;

use bae_pictures::{
    FlightError, Flights, IsCancelled, PictureCopy, PictureSize, Priority, Volumes,
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
    flights: Flights<(ImportPicture, PictureSize), Vec<u8>, PictureError>,
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
    /// same picture and size share one job, and the job ends when the last of
    /// them is dropped.
    pub(crate) async fn read(
        &self,
        source: ImportPicture,
        pixels: Option<u32>,
        priority: Priority,
    ) -> Result<Vec<u8>, ImportError> {
        let size = PictureSize::covering(pixels);
        let job = {
            let (services, volumes, source) =
                (self.services.clone(), self.volumes.clone(), source.clone());
            move |cancelled: IsCancelled| make(services, volumes, source, size, priority, cancelled)
        };
        match self.flights.request((source, size), job).await {
            Ok(bytes) => Ok(Arc::try_unwrap(bytes).unwrap_or_else(|shared| (*shared).clone())),
            Err(FlightError::Failed(error)) => Err(ImportError::Picture(error)),
            Err(FlightError::Panicked) => Err(ImportError::Internal {
                detail: "the job that makes a picture panicked".to_string(),
            }),
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

/// The bytes of `source` at `size`: the kept copy when the candidate keeps
/// one, else a read of the original that ends when `cancel` is set, kept for
/// the next request when it is a copy.
async fn make(
    services: PictureServices,
    volumes: Volumes,
    source: ImportPicture,
    size: PictureSize,
    priority: Priority,
    cancelled: IsCancelled,
) -> Result<Vec<u8>, PictureError> {
    let owner = services.tables.picture_owner(&source).await?.ok_or_else(|| match &source {
        ImportPicture::Folder { file } | ImportPicture::Embedded { source: file, .. } => {
            PictureError::NotInImport(file.path.clone())
        }
    })?;
    if let PictureSize::Copy(step) = size {
        if let Some(kept) = services.tables.load_picture_copy(&owner, step).await? {
            return Ok(kept);
        }
    }
    let origin = match &source {
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
    };
    // Visible pictures read before prefetched ones, and a share is read a few
    // files at a time.
    let _turn =
        volumes.read_place(&owner.watched_folder_path, reads_at_once(owner.volume), priority).await;
    let lease = bae_store::lease_one(&services.files).await;
    let bytes = services
        .diagnostics
        .blocking("make a picture copy", move || read_picture(origin, size, cancelled, lease))
        .await??;
    if let (PictureSize::Copy(step), Made::Copy(copy)) = (size, &bytes) {
        services.tables.store_picture_copy(&owner, step, copy).await?;
    }
    Ok(match bytes {
        Made::Copy(copy) => copy.bytes,
        Made::Original(bytes) => bytes,
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

/// What reading a picture made.
enum Made {
    Copy(PictureCopy),
    Original(Vec<u8>),
}

/// Read the picture from `origin` at `size`, on a blocking thread.
fn read_picture(
    origin: Origin,
    size: PictureSize,
    cancelled: IsCancelled,
    lease: coven::FileLease,
) -> Result<Made, PictureError> {
    let read = |origin: Box<dyn Read + Send>| bae_pictures::read_original(origin, &cancelled);
    match (origin, size) {
        (Origin::File(version), PictureSize::Copy(step)) => {
            let file = open(&version, &lease)?;
            Ok(Made::Copy(bae_pictures::make_copy(file, cancelled, step)?))
        }
        (Origin::File(version), PictureSize::Original) => {
            Ok(Made::Original(read(Box::new(open(&version, &lease)?))?))
        }
        (Origin::Art(art), PictureSize::Copy(step)) => {
            Ok(Made::Copy(bae_pictures::make_copy(Cursor::new(art), cancelled, step)?))
        }
        (Origin::Art(art), PictureSize::Original) => Ok(Made::Original(art)),
    }
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
