use super::*;

fn file_record(
    device: coven::DeviceId,
    file: coven::FileId,
    reason: coven::PendingReason,
) -> coven::PendingRecord {
    coven::PendingRecord::new(coven::PendingSubject::File { device, file }, reason)
}

fn waits_for(path: coven::ObjectPath) -> coven::PendingReason {
    coven::PendingReason::Waits(coven::Prerequisite::Object(path))
}

/// A file's record waiting for the object at its own fixed path is that
/// file arriving from its uploader; any other reason, or a wait for another
/// object, stays a record that needs attention.
#[test]
fn only_a_wait_for_the_files_own_upload_is_an_arrival() {
    let store = coven::StoreId(uuid::Uuid::from_u128(1));
    let device = coven::DeviceId(7);
    let file = coven::FileId(uuid::Uuid::from_u128(9));
    let own = coven::ObjectPath::file(store, device, file);
    assert_eq!(
        ArrivingFile::of(&file_record(device, file, waits_for(own))),
        Some(ArrivingFile {
            uploader: device,
            file
        })
    );
    let other_file =
        coven::ObjectPath::file(store, device, coven::FileId(uuid::Uuid::from_u128(3)));
    let other_device = coven::ObjectPath::file(store, coven::DeviceId(8), file);
    for reason in [
        waits_for(other_file),
        waits_for(other_device),
        coven::PendingReason::FileUnavailable(coven::FileMissingReason::DeviceRemoved),
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure: coven::StorageFailure::Network,
            answered: false,
        },
        coven::PendingReason::Waits(coven::Prerequisite::SequenceCatchUp),
    ] {
        assert_eq!(ArrivingFile::of(&file_record(device, file, reason)), None);
    }
    assert_eq!(
        ArrivingFile::of(&coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            waits_for(coven::ObjectPath::file(store, device, file)),
        )),
        None
    );
}

fn arriving(device: u64, file: u128) -> ArrivingFile {
    ArrivingFile {
        uploader: coven::DeviceId(device),
        file: coven::FileId(uuid::Uuid::from_u128(file)),
    }
}

fn release_row(release: &str, album: &str, title: &str) -> crate::db::ArrivingReleaseRow {
    crate::db::ArrivingReleaseRow {
        release_id: release.into(),
        album_id: album.into(),
        album_title: title.into(),
    }
}

/// Arriving files group under the device each arrives from, by release:
/// a cover marks its release, a release file names the tracks it plays, and
/// artist images and avatars are counted apart.
#[test]
fn arriving_files_group_by_device_and_release() {
    let rows = [
        (FileTable::Covers, "release-b", arriving(1, 1)),
        (FileTable::ReleaseFiles, "file-a1", arriving(1, 2)),
        (FileTable::ReleaseFiles, "file-a2", arriving(1, 3)),
        (FileTable::ArtistImages, "artist", arriving(1, 4)),
        (FileTable::People, "person", arriving(2, 5)),
        (FileTable::Covers, "release-a", arriving(2, 6)),
    ];
    let found = ArrivingReleaseRows {
        releases: vec![
            release_row("release-a", "album-a", "Abbey Road"),
            release_row("release-b", "album-b", "Blue"),
        ],
        file_releases: vec![
            ("file-a1".into(), "release-a".into()),
            ("file-a2".into(), "release-a".into()),
        ],
        file_tracks: vec![
            ("file-a1".into(), "track-2".into()),
            ("file-a1".into(), "track-1".into()),
            ("file-a2".into(), "track-3".into()),
        ],
    };
    let devices = HashMap::from([
        (coven::DeviceId(1), "Studio Mac".to_string()),
        (coven::DeviceId(2), "Ana's Phone".to_string()),
    ]);
    let arrivals = group(&rows, &found, &devices).unwrap();
    assert_eq!(
        arrivals,
        Arrivals {
            devices: vec![
                DeviceArrivals {
                    device: coven::DeviceId(2),
                    device_name: "Ana's Phone".into(),
                    releases: vec![ReleaseArrival {
                        release_id: "release-a".into(),
                        album_id: "album-a".into(),
                        album_title: "Abbey Road".into(),
                        cover: true,
                        track_ids: Vec::new(),
                    }],
                    other_files: 1,
                },
                DeviceArrivals {
                    device: coven::DeviceId(1),
                    device_name: "Studio Mac".into(),
                    releases: vec![
                        ReleaseArrival {
                            release_id: "release-a".into(),
                            album_id: "album-a".into(),
                            album_title: "Abbey Road".into(),
                            cover: false,
                            track_ids: vec!["track-1".into(), "track-2".into(), "track-3".into()],
                        },
                        ReleaseArrival {
                            release_id: "release-b".into(),
                            album_id: "album-b".into(),
                            album_title: "Blue".into(),
                            cover: true,
                            track_ids: Vec::new(),
                        },
                    ],
                    other_files: 1,
                },
            ],
        }
    );
    let releases = arrivals.releases();
    let (device, release_a) = &releases["release-a"];
    assert_eq!(device, "Ana's Phone");
    assert!(release_a.cover);
    assert_eq!(release_a.track_ids.len(), 3);
    assert_eq!(arrivals.albums()["album-b"], "Studio Mac");
}

/// A file arriving from a device no member has is a broken library, not a
/// nameless arrival.
#[test]
fn a_file_from_an_unknown_device_is_an_error() {
    let found = ArrivingReleaseRows {
        releases: vec![release_row("release", "album", "Title")],
        file_releases: Vec::new(),
        file_tracks: Vec::new(),
    };
    assert!(group(
        &[(FileTable::Covers, "release", arriving(5, 1))],
        &found,
        &HashMap::new()
    )
    .is_err());
}
