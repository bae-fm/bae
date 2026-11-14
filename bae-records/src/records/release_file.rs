//! Readers for a release file: the coven file its row references, read
//! through coven from wherever coven has it.
use super::*;

impl Records {
    /// The release file `id`'s stream, which holds one place of `lease` until
    /// it drops.
    pub async fn open_release_file(
        &self,
        id: &str,
        lease: &coven::FileLease,
    ) -> Result<bae_store::FileStream, DbError> {
        let row = self.find_file_by_id(id).await?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let file = self.store.file_ref(row.file).await?;
        Ok(self.store.open_file_stream(&file, lease).await?)
    }

    /// The release file `id`'s bytes, read in one place of `lease`.
    pub async fn read_release_file(
        &self,
        id: &str,
        lease: &coven::FileLease,
    ) -> Result<Vec<u8>, DbError> {
        let stream = self.open_release_file(id, lease).await?;
        Ok(stream.read_at(0, stream.plaintext_size()).await?)
    }
}
