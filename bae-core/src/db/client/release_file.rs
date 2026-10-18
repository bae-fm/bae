//! Readers for unattached device-local audio and coven-managed attachments.
use super::*;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub(crate) enum ReleaseFileStream {
    Attached(coven::FileStream),
    Local {
        file: tokio::sync::Mutex<tokio::fs::File>,
        source: coven::UserFile,
    },
}
impl ReleaseFileStream {
    pub(crate) fn plaintext_size(&self) -> u64 {
        match self {
            Self::Attached(stream) => stream.plaintext_size(),
            Self::Local { source, .. } => source.size,
        }
    }
    pub(crate) async fn read_at(&self, offset: u64, len: u64) -> Result<Vec<u8>, DbError> {
        match self {
            Self::Attached(stream) => Ok(stream.read_at(offset, len).await?),
            Self::Local { file, source } => {
                if offset.checked_add(len).is_none_or(|end| end > source.size) {
                    return Err(DbError::Message(format!(
                        "read {offset}+{len} exceeds {} bytes",
                        source.size
                    )));
                }
                let mut file = file.lock().await;
                check_source(&file.metadata().await?, source)?;
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                let mut bytes = vec![
                    0;
                    usize::try_from(len)
                        .map_err(|error| DbError::Message(error.to_string()))?
                ];
                file.read_exact(&mut bytes).await?;
                check_source(&file.metadata().await?, source)?;
                Ok(bytes)
            }
        }
    }
}
fn check_source(metadata: &std::fs::Metadata, source: &coven::UserFile) -> Result<(), DbError> {
    if metadata.len() != source.size || metadata.modified()? != source.modified_at {
        return Err(DbError::LocalFileChanged(source.path.clone()));
    }
    Ok(())
}
impl Database {
    pub(crate) async fn open_release_file(&self, id: &str) -> Result<ReleaseFileStream, DbError> {
        let row = self
            .find_file_by_id(id)
            .await?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if row.attached {
            let file = self.file_ref("release_files", id).await?;
            return Ok(ReleaseFileStream::Attached(
                self.open_file_stream(&file).await?,
            ));
        }
        let source = self
            .external_blob(id)
            .await?
            .ok_or_else(|| DbError::FileNotUploaded(id.to_string()))?;
        let file = match tokio::fs::File::open(&source.path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(DbError::LocalFileMissing(source.path.clone()));
            }
            Err(error) => return Err(error.into()),
        };
        check_source(&file.metadata().await?, &source)?;
        Ok(ReleaseFileStream::Local {
            file: tokio::sync::Mutex::new(file),
            source,
        })
    }

    pub(crate) async fn read_release_file(&self, id: &str) -> Result<Vec<u8>, DbError> {
        let stream = self.open_release_file(id).await?;
        stream.read_at(0, stream.plaintext_size()).await
    }
}
