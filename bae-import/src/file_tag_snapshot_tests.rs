use super::*;
use crate::file_tag_reader::FileTagRead;
use std::path::Path;

#[derive(Default)]
struct CountingReader(std::sync::atomic::AtomicUsize);

impl FileTagReader for CountingReader {
    fn read(&self, _path: &Path) -> Result<FileTagRead, crate::file_tag_reader::TagReadFailure> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(FileTagRead {
            tags: FileTags {
                title: Some("Track Alpha".to_string()),
                track_artists: vec!["Artist Alpha".to_string()],
                album_title: Some("Album Alpha".to_string()),
                album_artists: vec!["Artist Alpha".to_string()],
                years: vec![2001],
                numbers: crate::track_numbers::TrackNumbers { disc: None, track: Some(1) },
                ..FileTags::untagged()
            },
            delivery: DeliveryTags::default(),
            embedded_cover: None,
        })
    }
}

#[test]
fn extraction_reads_every_file_before_returning_one_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let paths = [dir.path().join("01.flac"), dir.path().join("02.flac")];
    for path in &paths {
        std::fs::write(path, b"audio").unwrap();
    }
    let files = paths
        .iter()
        .enumerate()
        .map(|(index, path)| ScannedFile::new(path.clone(), format!("{:02}.flac", index + 1), 5, 1))
        .collect::<Vec<_>>();
    let reader = CountingReader::default();

    let snapshot = extract_file_tag_snapshot(&files, 8, 2, &reader).unwrap();

    assert_eq!(snapshot.files.len(), 2);
    assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(
        snapshot.files.iter().map(|fact| &fact.observation).collect::<Vec<_>>(),
        observe_audio_files(&files).unwrap().iter().collect::<Vec<_>>()
    );
}

#[test]
fn extraction_refuses_a_file_whose_scanned_size_changed() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("01.flac");
    std::fs::write(&path, b"changed").unwrap();
    let file = ScannedFile::new(path, "01.flac".to_string(), 5, 1);
    let reader = CountingReader::default();

    let error = extract_file_tag_snapshot(&[file], 1, 0, &reader).unwrap_err();

    assert!(matches!(error, ImportError::FileTags { detail } if detail.contains("changed after")));
    assert_eq!(reader.0.load(std::sync::atomic::Ordering::Relaxed), 0);
}
