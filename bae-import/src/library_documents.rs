//! The text documents of a library release, read whole through coven: the
//! track sheets, rip logs and AccurateRip reports that re-identifying the
//! release and saving a CUE sheet's binding read. The readers take each
//! document's bytes, so nothing is copied to a folder first.

use crate::cue_sheet::{parse_cue_content, CueSheet, CueSheetError};
use crate::left_out::{FolderInputs, InputRead, LeftOut};
use bae_records::{DbFile, Records};
use bae_store::DbError;
use bae_util::text_encoding::{check_document_size, decode_text, DecodedText};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Why a document of a library release could not be read.
#[derive(Debug, thiserror::Error)]
pub enum LibraryDocumentError {
    /// coven could not read the file.
    #[error("bae could not read the release's file {name}: {error}")]
    Coven { name: String, error: DbError },
    /// The file is a text document that bae refuses to read whole, for its
    /// size.
    #[error("{name} could not be read: {error}")]
    Refused { name: String, error: std::io::Error },
}

impl From<LibraryDocumentError> for crate::ImportError {
    fn from(error: LibraryDocumentError) -> Self {
        match error {
            LibraryDocumentError::Coven { error, .. } => error.into(),
            LibraryDocumentError::Refused { name, error } => {
                Self::DocumentRead { file: name, error }
            }
        }
    }
}

impl From<LibraryDocumentError> for LeftOut {
    fn from(error: LibraryDocumentError) -> Self {
        let logged = error.to_string();
        match error {
            LibraryDocumentError::Coven { error, .. } => {
                Self::library(logged, bae_diagnostics::database_category(&error))
            }
            LibraryDocumentError::Refused { error, .. } => {
                Self::of_io(logged, error.kind(), error.raw_os_error())
            }
        }
    }
}

/// The whole of `file`, read through coven in one place of `lease`.
async fn read_whole(
    records: &Records,
    lease: &coven::FileLease,
    file: &DbFile,
) -> Result<Vec<u8>, LibraryDocumentError> {
    let name = &file.original_filename;
    let coven = |error: DbError| LibraryDocumentError::Coven { name: name.clone(), error };
    let stream = records.open_release_file(file.file, lease).await.map_err(coven)?;
    let size = stream.plaintext_size();
    check_document_size(name, size)
        .map_err(|error| LibraryDocumentError::Refused { name: name.clone(), error })?;
    stream.read_at(0, size).await.map_err(|error| coven(error.into()))
}

/// The bytes of some of a library release's files, by the path each file's
/// scanned file has: its name in the release.
pub(crate) struct LibraryDocuments {
    by_path: HashMap<PathBuf, Vec<u8>>,
}

impl LibraryDocuments {
    /// Read each of `files` whole, one after the other, in one place of
    /// `lease`. A file over the text document limit is refused before any of
    /// it is read.
    pub(crate) async fn read(
        records: &Records,
        lease: &coven::FileLease,
        files: &[&DbFile],
    ) -> Result<Self, LibraryDocumentError> {
        let mut by_path = HashMap::new();
        for file in files.iter().copied() {
            let bytes = read_whole(records, lease, file).await?;
            by_path.insert(PathBuf::from(&file.original_filename), bytes);
        }
        Ok(Self { by_path })
    }

    /// [`Self::read`] for identification, which leaves out through `inputs`
    /// each file it can't read and holds the others.
    pub(crate) async fn read_to_identify(
        records: &Records,
        lease: &coven::FileLease,
        files: &[&DbFile],
        inputs: &FolderInputs,
    ) -> Self {
        let mut by_path = HashMap::new();
        for file in files.iter().copied() {
            let path = PathBuf::from(&file.original_filename);
            match read_whole(records, lease, file).await {
                Ok(bytes) => {
                    by_path.insert(path, bytes);
                }
                Err(error) => inputs.leave_out(InputRead::LibraryFile, &path, error.into()),
            }
        }
        Self { by_path }
    }

    fn bytes(&self, path: &Path) -> &[u8] {
        self.by_path.get(path).expect("a library release's documents hold each file read from it")
    }

    /// The text of the document at `path`, decoded whatever its encoding.
    /// `read` refused each document over the size limit.
    pub(crate) fn text(&self, path: &Path) -> DecodedText {
        decode_text(self.bytes(path))
    }

    /// The track sheet at `path`, parsed.
    pub(crate) fn parse_sheet(&self, path: &Path) -> Result<CueSheet, CueSheetError> {
        parse_cue_content(&self.text(path).text)
    }
}
