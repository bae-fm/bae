//! Readers for a release file: the coven file its row references, read
//! through coven from wherever coven has it.
use super::*;

impl Database {
    pub(crate) async fn open_release_file(&self, id: &str) -> Result<coven::FileStream, DbError> {
        let row = self
            .find_file_by_id(id)
            .await?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let file = self.file_ref(row.file).await?;
        Ok(self.open_file_stream(&file).await?)
    }

    pub(crate) async fn read_release_file(&self, id: &str) -> Result<Vec<u8>, DbError> {
        let stream = self.open_release_file(id).await?;
        Ok(stream.read_at(0, stream.plaintext_size()).await?)
    }
}
