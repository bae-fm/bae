use crate::import::folder_scanner::ScannedFile;

/// Prove that every physical file still has the identity captured by the scan.
pub(super) fn validate_scanned_file_identities(
    files: &[ScannedFile],
) -> Result<(), crate::import::ImportError> {
    for file in files {
        let metadata = std::fs::metadata(&file.path).map_err(|error| {
            crate::import::ImportError::UnusableFile {
                detail: format!("failed to stat {}: {error}", file.path.display()),
            }
        })?;
        let modified_at_ns = crate::import::folder_scanner::file_modified_at_ns(
            &file.path, &metadata,
        )
        .map_err(|error| crate::import::ImportError::UnusableFile {
            detail: error.to_string(),
        })?;
        let content_digest = crate::util::fs::hash_file(&file.path).map_err(|error| {
            crate::import::ImportError::UnusableFile {
                detail: format!("failed to hash {}: {error}", file.path.display()),
            }
        })?;
        if metadata.len() != file.size
            || modified_at_ns != file.modified_at_ns
            || content_digest != file.content_digest
        {
            return Err(crate::import::ImportError::UnusableFile {
                detail: format!(
                    "{} changed after its import candidate was scanned; rescan before importing",
                    file.path.display()
                ),
            });
        }
    }
    Ok(())
}
