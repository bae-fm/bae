use crate::import::folder_scanner::ScannedFile;

/// Prove that every physical file still has the metadata identity captured by the scan.
pub(super) fn validate_scanned_file_identities(
    files: &[ScannedFile],
) -> Result<(), crate::import::ImportError> {
    for file in files {
        validate_scanned_file_metadata(file)?;
    }
    Ok(())
}

pub(super) fn hash_scanned_file_for_import(
    file: &ScannedFile,
    report: impl FnMut(u64),
) -> Result<String, crate::import::ImportError> {
    validate_scanned_file_metadata(file)?;
    let content_hash = crate::util::fs::hash_file(&file.path, report).map_err(|error| {
        crate::import::ImportError::UnusableFile {
            detail: format!("failed to hash {}: {error}", file.path.display()),
        }
    })?;
    validate_scanned_file_metadata(file)?;
    Ok(content_hash)
}

fn validate_scanned_file_metadata(file: &ScannedFile) -> Result<(), crate::import::ImportError> {
    let metadata = std::fs::metadata(&file.path).map_err(|error| {
        crate::import::ImportError::UnusableFile {
            detail: format!("failed to stat {}: {error}", file.path.display()),
        }
    })?;
    let modified_at_ns = crate::import::folder_scanner::file_modified_at_ns(&file.path, &metadata)
        .map_err(|error| crate::import::ImportError::UnusableFile {
            detail: error.to_string(),
        })?;
    if metadata.len() != file.size || modified_at_ns != file.modified_at_ns {
        return Err(crate::import::ImportError::UnusableFile {
            detail: format!(
                "{} changed after its import candidate was scanned; rescan before importing",
                file.path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_hash_is_the_exact_file_digest() {
        let temp = tempfile::tempdir().expect("temp directory");
        let path = temp.path().join("source.bin");
        let bytes = b"source bytes stored by the import";
        std::fs::write(&path, bytes).expect("write source file");
        let metadata = std::fs::metadata(&path).expect("source metadata");
        let modified_at_ns = crate::import::folder_scanner::file_modified_at_ns(&path, &metadata)
            .expect("source modification time");
        let file = ScannedFile::new(
            path,
            "source.bin".to_string(),
            metadata.len(),
            modified_at_ns,
        );

        let mut bytes_reported = 0u64;
        let hash = hash_scanned_file_for_import(&file, |read| bytes_reported += read)
            .expect("hash source file");

        assert_eq!(hash, crate::util::fs::hash_bytes(bytes));
        assert_eq!(bytes_reported, bytes.len() as u64);
    }
}
