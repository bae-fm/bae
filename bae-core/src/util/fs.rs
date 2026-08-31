/// The content hash for a host-provided blob whose bytes are already held for
/// `put_blob`. Coven owns hashing user-provided files during their preparation.
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
