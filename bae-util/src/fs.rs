/// The content hash for a host-provided file whose bytes are already held for
/// `put_file`. Coven owns hashing user-provided files during their preparation.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Make the entries just created in `directory` durable. Unix makes a new
/// entry durable only once its directory is synced, which std does by opening
/// the directory as a file. std's `File::open` can't open a directory on
/// Windows (access is denied); `OpenOptionsExt::custom_flags` with
/// `FILE_FLAG_BACKUP_SEMANTICS` can, which bae doesn't do, so there the synced
/// file is all bae syncs; coven syncs directories on Unix only as well.
pub fn sync_directory(directory: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(directory)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(())
    }
}
