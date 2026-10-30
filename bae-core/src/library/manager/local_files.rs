//! Copies owned by an audio detachment are removed unless its write commits.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

struct LocalCopies {
    root: PathBuf,
    files: Vec<PathBuf>,
    directories: Vec<PathBuf>,
}

impl LocalCopies {
    fn create_parent(&mut self, path: &std::path::Path) -> Result<(), LibraryError> {
        let metadata = if path != self.root && path.starts_with(&self.root) {
            std::fs::symlink_metadata(path)
        } else {
            std::fs::metadata(path)
        };
        match metadata {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
            Ok(_) => Err(LibraryError::Storage(format!(
                "destination parent is not a directory: {}",
                path.display()
            ))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .ok_or_else(|| {
                        LibraryError::Storage(format!(
                            "destination has no parent: {}",
                            path.display()
                        ))
                    })?;
                self.create_parent(parent)?;
                std::fs::create_dir(path)?;
                self.directories.push(path.into());
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn create(&mut self, path: &std::path::Path) -> Result<std::fs::File, LibraryError> {
        let parent = path
            .parent()
            .ok_or_else(|| LibraryError::Storage("destination has no parent".into()))?;
        // The user chooses the root; synced row paths may not redirect beneath it.
        for ancestor in parent
            .ancestors()
            .take_while(|path| *path != self.root && path.starts_with(&self.root))
        {
            match std::fs::symlink_metadata(ancestor) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(LibraryError::Storage(format!(
                        "destination contains a symlink: {}",
                        ancestor.display()
                    )))
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.create_parent(parent)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(path)?;
        self.files.push(path.into());
        Ok(file)
    }

    fn rollback(&mut self) -> Result<(), LibraryError> {
        let mut failure = None;
        for path in self.files.iter().rev() {
            if let Err(error) = std::fs::remove_file(path) {
                failure = Some(combine_failure(failure, error.into()));
            }
        }
        for path in self.directories.iter().rev() {
            if let Err(error) = std::fs::remove_dir(path) {
                failure = Some(combine_failure(failure, error.into()));
            }
        }
        self.files.clear();
        self.directories.clear();
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn commit(&mut self) {
        self.files.clear();
        self.directories.clear();
    }
}

fn combine_failure(operation: Option<LibraryError>, cleanup: LibraryError) -> LibraryError {
    match operation {
        Some(operation) => LibraryError::Cleanup {
            operation: Box::new(operation),
            cleanup: Box::new(cleanup),
        },
        None => cleanup,
    }
}

impl Drop for LocalCopies {
    fn drop(&mut self) {
        if let Err(error) = self.rollback() {
            tracing::error!(%error, "failed to remove uncommitted local release copies");
        }
    }
}

impl LibraryManager {
    pub(super) async fn copy_release_to_local(
        &self,
        release: &str,
        root: PathBuf,
        destinations: HashMap<String, PathBuf>,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let mut copies = LocalCopies {
            root,
            files: Vec::new(),
            directories: Vec::new(),
        };
        let result = self
            .write_local_copies(release, destinations, cancel, &mut copies)
            .await;
        match result {
            Ok(()) => {
                copies.commit();
                Ok(())
            }
            Err(operation) => match copies.rollback() {
                Ok(()) => Err(operation),
                Err(cleanup) => Err(combine_failure(Some(operation), cleanup)),
            },
        }
    }

    async fn write_local_copies(
        &self,
        release: &str,
        destinations: HashMap<String, PathBuf>,
        cancel: &crate::library::CancellationToken,
        copies: &mut LocalCopies,
    ) -> Result<(), LibraryError> {
        let mut originals = Vec::with_capacity(destinations.len());
        for (id, path) in destinations {
            if cancel.is_cancelled() {
                return Err(LibraryError::Storage(
                    "making release local was cancelled".into(),
                ));
            }
            let reference = self.release_file_ref(&id).await?;
            let coven::FilePlace::Cloud { content_hash, .. } = reference.place() else {
                return Err(LibraryError::Storage(format!(
                    "release file {id} is not in the cloud"
                )));
            };
            let expected = *content_hash.as_bytes();
            let stream = self
                .database
                .open_file_stream(&reference)
                .await
                .map_err(|error| LibraryError::FileRead {
                    operation: "read file for local copy".into(),
                    error: Box::new(error),
                })?;
            let mut output = copies.create(&path)?;
            let mut offset = 0;
            while offset < stream.plaintext_size() {
                let bytes = tokio::select! {
                    _ = cancel.cancelled() => return Err(LibraryError::Storage("making release local was cancelled".into())),
                    bytes = stream.read_at(offset, (4 * 1024 * 1024).min(stream.plaintext_size() - offset)) => bytes.map_err(|error| LibraryError::FileRead { operation: "copy file to local folder".into(), error: Box::new(error) })?,
                };
                offset += bytes.len() as u64;
                output = blocking_io(move || {
                    output.write_all(&bytes)?;
                    Ok(output)
                })
                .await?;
            }
            let source = blocking_io(move || {
                use std::io::{Seek, SeekFrom};
                output.sync_all()?;
                output.seek(SeekFrom::Start(0))?;
                let before = output.metadata()?;
                let mut hash = Sha256::new();
                let mut buffer = [0; coven::CHUNK_SIZE];
                loop {
                    let count = output.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    hash.update(&buffer[..count]);
                }
                let actual: [u8; 32] = hash.finalize().into();
                let after = output.metadata()?;
                if actual != expected
                    || after.len() != offset
                    || before.modified()? != after.modified()?
                {
                    return Err(LibraryError::Storage(format!(
                        "local copy verification failed: {}",
                        path.display()
                    )));
                }
                bae_util::fs::sync_directory(path.parent().expect("validated destination parent"))?;
                Ok(coven::UserFile {
                    path,
                    size: after.len(),
                    modified_at: after.modified()?,
                })
            })
            .await?;
            originals.push((id, reference.id(), source));
        }
        if cancel.is_cancelled() {
            return Err(LibraryError::Storage(
                "making release local was cancelled".into(),
            ));
        }
        self.database
            .detach_release_files(release, originals)
            .await?;
        Ok(())
    }
}

/// Run blocking IO (files, the keychain, the config file) on a blocking thread.
pub(super) async fn blocking_io<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, LibraryError> + Send + 'static,
) -> Result<T, LibraryError> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}
