/// The content hash for a host-provided file whose bytes are already held for
/// `put_file`. Coven owns hashing user-provided files during their preparation.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// `error` with the path it happened at in its text: a bare "Permission denied"
/// or "Access is denied" names no file, and a user report of it is
/// undiagnosable. The kind is kept.
pub fn io_error_at(path: &std::path::Path, error: std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;
