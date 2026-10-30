use super::*;

fn named(subject: coven::PendingSubject) -> PendingSubjectName {
    PendingItem::before_open(coven::PendingRecord::new(
        subject,
        coven::PendingReason::NoStorage,
    ))
    .subject
}

/// Before a library is open every subject names itself by what it is, and a
/// file, which only this device's upload queue can name, is a file.
#[test]
fn subjects_name_what_they_are_before_a_library_opens() {
    use coven::PendingSubject as S;
    assert_eq!(named(S::Connection), PendingSubjectName::Connection);
    assert_eq!(
        named(S::Audience(coven::Audience::Store)),
        PendingSubjectName::LibraryData
    );
    assert_eq!(
        named(S::Operation {
            id: coven::OperationId(4),
            kind: coven::OperationKind::Invite,
        }),
        PendingSubjectName::Operation(coven::OperationKind::Invite)
    );
    for file in [
        S::Upload {
            device: coven::DeviceId(2),
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        },
        S::Download {
            device: coven::DeviceId(2),
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        },
        S::Keep {
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        },
        S::Move {
            id: coven::MoveId(3),
        },
    ] {
        assert_eq!(named(file), PendingSubjectName::File);
    }
}

/// A file's record is named by the row it is the file of, as the library
/// holds it now; a file no row here names is a file.
#[test]
fn a_files_record_is_named_by_its_row() {
    let named = |row| {
        PendingItem::named(
            coven::PendingRecord::new(
                coven::PendingSubject::Upload {
                    device: coven::DeviceId(2),
                    file: coven::FileId(uuid::Uuid::from_u128(9)),
                },
                coven::PendingReason::NoStorage,
            ),
            row,
        )
        .subject
    };
    assert_eq!(
        named(Some(FileRow::ReleaseFile {
            release_id: "release".into(),
            file_name: "01 Track.flac".into(),
            album_title: "Album Title".into(),
        })),
        PendingSubjectName::ReleaseFile {
            file_name: "01 Track.flac".into(),
            album_title: "Album Title".into(),
        }
    );
    assert_eq!(
        named(Some(FileRow::Cover {
            album_title: "Album Title".into()
        })),
        PendingSubjectName::Cover {
            album_title: "Album Title".into()
        }
    );
    assert_eq!(
        named(Some(FileRow::DeletedReleaseFile)),
        PendingSubjectName::DeletedReleaseFile
    );
    assert_eq!(named(None), PendingSubjectName::File);
}
