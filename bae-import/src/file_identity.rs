use crate::folder_scanner::ScannedFile;

/// Prove that every physical file still has the metadata identity captured by the scan.
pub(super) fn validate_scanned_file_identities(
    files: &[ScannedFile],
) -> Result<Vec<crate::file_tag_snapshot::FileObservation>, crate::ImportError> {
    let mut audio_observations = Vec::new();
    for file in files {
        let observation = validate_scanned_file_metadata(file)?;
        if file.source_audio.is_some() {
            audio_observations.push(observation);
        }
    }
    Ok(audio_observations)
}

fn validate_scanned_file_metadata(
    file: &ScannedFile,
) -> Result<crate::file_tag_snapshot::FileObservation, crate::ImportError> {
    let metadata =
        std::fs::metadata(&file.path).map_err(|error| crate::ImportError::UnusableFile {
            detail: format!("failed to stat {}: {error}", file.path.display()),
        })?;
    let modified_at_ns = crate::folder_scanner::file_modified_at_ns(&file.path, &metadata)
        .map_err(|error| crate::ImportError::UnusableFile { detail: error.to_string() })?;
    if metadata.len() != file.size || modified_at_ns != file.modified_at_ns {
        return Err(changed_after_scan(file));
    }
    Ok(crate::file_tag_snapshot::FileObservation {
        relative_path: file.relative_path.clone(),
        size: metadata.len(),
        modified_at_ns,
    })
}

/// Refuse an original whose size or modification time, as coven found them
/// when it observed or read the file to stage it, are not the ones the scan
/// recorded: the release's rows describe the file the scan read, and coven
/// records the one it found.
pub(super) fn require_scanned_identity(
    file: &ScannedFile,
    size: u64,
    modified_at: std::time::SystemTime,
) -> Result<(), crate::ImportError> {
    let scanned: std::time::SystemTime =
        chrono::DateTime::from_timestamp_nanos(file.modified_at_ns).into();
    if size != file.size || modified_at != scanned {
        return Err(changed_after_scan(file));
    }
    Ok(())
}

fn changed_after_scan(file: &ScannedFile) -> crate::ImportError {
    crate::ImportError::UnusableFile {
        detail: format!(
            "{} changed after its import candidate was scanned; rescan before importing",
            file.path.display()
        ),
    }
}

#[cfg(test)]
#[path = "file_identity_tests.rs"]
mod tests;
