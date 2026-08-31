use std::path::Path;

/// The content hash coven's blob declarations require on every blob-bearing
/// row (`BlobDecl::hash_column`): the lowercase-hex SHA-256 of the blob's
/// plaintext, signed on the row alongside its declared size and verified by
/// coven against the decrypted bytes on every cloud fetch (including the very
/// first one, right after upload — not only a later device's download). coven
/// computes this identically on its own side but doesn't re-export the
/// primitive (a production host never names its own crypto type), so bae
/// computes it here, over the on-disk file a `release_files` row's blob reads
/// from — streamed in fixed chunks so hashing a multi-hundred-megabyte FLAC
/// never holds the whole file in memory. `report` receives the number of bytes
/// consumed by each successful read, so callers can expose byte progress.
pub fn hash_file(path: &Path, mut report: impl FnMut(u64)) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        report(read as u64);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// The same content hash, over plaintext already in memory — for a
/// host-provided blob (a generated cover/artist image) whose bytes are already
/// held for `put_blob` rather than read from a path.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
