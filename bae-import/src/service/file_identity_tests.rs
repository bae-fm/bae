use super::*;
use bae_util::test_files::TestFolder;

#[test]
fn validation_returns_the_audio_observations_it_already_read() {
    let dir = TestFolder::new();
    let audio_path = dir.path().join("01.flac");
    let image_path = dir.path().join("cover.jpg");
    dir.write(&audio_path, b"audio");
    dir.write(&image_path, b"image");
    let scanned = |path: &std::path::Path, relative_path: &str| {
        let metadata = dir.metadata(path);
        ScannedFile::new(
            path.to_path_buf(),
            relative_path.to_string(),
            metadata.len(),
            crate::folder_scanner::file_modified_at_ns(path, &metadata).unwrap(),
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
