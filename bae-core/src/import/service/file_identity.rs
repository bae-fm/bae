use crate::import::folder_scanner::ScannedFile;

/// Prove that every physical file still has the metadata identity captured by the scan.
pub(super) fn validate_scanned_file_identities(
    files: &[ScannedFile],
) -> Result<Vec<crate::import::file_tag_snapshot::FileObservation>, crate::import::ImportError> {
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
) -> Result<crate::import::file_tag_snapshot::FileObservation, crate::import::ImportError> {
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
    Ok(crate::import::file_tag_snapshot::FileObservation {
        relative_path: file.relative_path.clone(),
        size: metadata.len(),
        modified_at_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_returns_the_audio_observations_it_already_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let audio_path = dir.path().join("01.flac");
        let image_path = dir.path().join("cover.jpg");
        std::fs::write(&audio_path, b"audio").unwrap();
        std::fs::write(&image_path, b"image").unwrap();
        let scanned = |path: &std::path::Path, relative_path: &str| {
            let metadata = std::fs::metadata(path).unwrap();
            ScannedFile::new(
                path.to_path_buf(),
                relative_path.to_string(),
                metadata.len(),
                crate::import::folder_scanner::file_modified_at_ns(path, &metadata).unwrap(),
            )
        };
        let files = vec![
            scanned(&audio_path, "01.flac").with_test_flac_audio(),
            scanned(&image_path, "cover.jpg"),
        ];

        let observations = validate_scanned_file_identities(&files).unwrap();

        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].relative_path, "01.flac");
        assert_eq!(observations[0].size, 5);
    }
}
