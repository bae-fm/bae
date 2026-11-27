//! Readers for a release file: the coven file its row references, read
//! through coven from wherever coven has it.
use super::*;

impl Records {
    /// The stream of the release file whose row references the coven file
    /// `file`, which holds one place of `lease` until it drops.
    pub async fn open_release_file(
        &self,
        file: coven::FileId,
        lease: &coven::FileLease,
    ) -> Result<bae_store::FileStream, DbError> {
        let file = self.store.file_ref(file).await?;
        Ok(self.store.open_file_stream(&file, lease).await?)
    }

    /// The bytes of the release file whose row references the coven file
    /// `file`, read in one place of `lease`.
    pub async fn read_release_file(
        &self,
        file: coven::FileId,
        lease: &coven::FileLease,
    ) -> Result<Vec<u8>, DbError> {
        let stream = self.open_release_file(file, lease).await?;
        Ok(stream.read_at(0, stream.plaintext_size()).await?)
    }
}
