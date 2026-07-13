use std::path::{Path, PathBuf};

/// The content hash coven's blob declarations require on every blob-bearing
/// row (`BlobDecl::hash_column`): the lowercase-hex SHA-256 of the blob's
/// plaintext, signed on the row alongside its declared size and verified by
/// coven against the decrypted bytes on every cloud fetch (including the very
/// first one, right after upload — not only a later device's download). coven
/// computes this identically on its own side but doesn't re-export the
/// primitive (a production host never names its own crypto type), so bae
/// computes it here, over the on-disk file a `release_files` row's blob reads
/// from — streamed in fixed chunks so hashing a multi-hundred-megabyte FLAC
/// never holds the whole file in memory.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
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

/// Move a directory to a new location. Tries rename (fast, same volume),
/// falls back to recursive copy + delete.
pub fn move_directory(source: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let folder_name = source
        .file_name()
        .ok_or_else(|| "Invalid source path".to_string())?;
    let dest = dest_dir.join(folder_name);

    if dest.exists() {
        return Err(format!("Destination already exists: {}", dest.display()));
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create destination directory: {e}"))?;
    }

    if std::fs::rename(source, &dest).is_err() {
        copy_dir_recursive(source, &dest).map_err(|e| format!("Failed to move directory: {e}"))?;
        std::fs::remove_dir_all(source)
            .map_err(|e| format!("Failed to clean up source after copy: {e}"))?;
    }

    Ok(dest)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}
