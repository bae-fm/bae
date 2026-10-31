use super::*;

/// An original written at `modified`, and the record of it at `recorded`.
fn original_at(
    dir: &std::path::Path,
    modified: std::time::SystemTime,
    recorded: std::time::SystemTime,
) -> RecordedOriginal {
    let path = dir.join("01.flac");
    let file = std::fs::File::create(&path).unwrap();
    std::io::Write::write_all(&mut &file, &[7; 1_000]).unwrap();
    file.set_modified(modified).unwrap();
    let (modified_seconds, modified_nanos) = coven::unix_seconds_and_nanos(recorded);
    RecordedOriginal {
        path,
        size: 1_000,
        modified_seconds,
        modified_nanos,
    }
}

#[test]
fn an_original_saved_within_the_same_millisecond_reads_changed() {
    let dir = tempfile::tempdir().unwrap();
    let recorded = std::time::UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_000);
    let saved = recorded + std::time::Duration::from_nanos(300);

    let unchanged = original_at(dir.path(), recorded, recorded);
    assert_eq!(on_disk(&unchanged).unwrap(), OriginalOnDisk::Unchanged);
    let changed = original_at(dir.path(), saved, recorded);
    assert_eq!(on_disk(&changed).unwrap(), OriginalOnDisk::Changed);
}
