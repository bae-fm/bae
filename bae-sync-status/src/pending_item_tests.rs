use super::*;

fn named(subject: coven::PendingSubject) -> PendingSubjectName {
    PendingItem::before_open(coven::PendingRecord::new(subject, coven::PendingReason::NoStorage))
        .subject
}

/// Before a library is open every subject names itself by what it is, and a
/// file, which only this device's upload queue can name, is a file.
#[test]
fn subjects_name_what_they_are_before_a_library_opens() {
    use coven::PendingSubject as S;
    assert_eq!(named(S::Connection), PendingSubjectName::Connection);
    assert_eq!(named(S::Audience(coven::Audience::Store)), PendingSubjectName::LibraryData);
    assert_eq!(
        named(S::Operation { id: coven::OperationId(4), kind: coven::OperationKind::Invite }),
        PendingSubjectName::Operation(coven::OperationKind::Invite)
    );
    for file in [
        S::Upload { device: coven::DeviceId(2), object: coven::ObjectId(uuid::Uuid::from_u128(9)) },
        S::Download {
            device: coven::DeviceId(2),
            object: coven::ObjectId(uuid::Uuid::from_u128(9)),
        },
        S::Keep { file: coven::FileId(uuid::Uuid::from_u128(9)) },
        S::Move { id: coven::MoveId(3) },
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
                    object: coven::ObjectId(uuid::Uuid::from_u128(9)),
                },
                coven::PendingReason::NoStorage,
            ),
            row,
            None,
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
        named(Some(FileRow::Cover { album_title: "Album Title".into() })),
        PendingSubjectName::Cover { album_title: "Album Title".into() }
    );
    assert_eq!(named(Some(FileRow::DeletedReleaseFile)), PendingSubjectName::DeletedReleaseFile);
    assert_eq!(named(None), PendingSubjectName::File);
}

/// The upload of a cover, artist image or avatar holds this device's writes,
/// and its item names the file whose row the person can give up; the upload
/// of a release file, which trails, holds nothing, and no other record does.
#[test]
fn only_the_upload_of_a_bound_image_names_a_held_image() {
    let file = coven::FileId(uuid::Uuid::from_u128(5));
    let upload = coven::PendingSubject::Upload {
        device: coven::DeviceId(2),
        object: coven::ObjectId(uuid::Uuid::from_u128(9)),
    };
    let held = |subject: &coven::PendingSubject, row| {
        PendingItem::named(
            coven::PendingRecord::new(subject.clone(), coven::PendingReason::NoStorage),
            row,
            Some(file),
        )
        .held_image
    };
    assert_eq!(
        held(&upload, Some(FileRow::Cover { album_title: "Album".into() })),
        Some(HeldImage { file, kind: HeldImageKind::Cover })
    );
    assert_eq!(
        held(&upload, Some(FileRow::ArtistImage)),
        Some(HeldImage { file, kind: HeldImageKind::ArtistImage })
    );
    assert_eq!(
        held(&upload, Some(FileRow::Avatar)),
        Some(HeldImage { file, kind: HeldImageKind::Avatar })
    );
    assert_eq!(
        held(
            &upload,
            Some(FileRow::ReleaseFile {
                release_id: "release".into(),
                file_name: "01 Track.flac".into(),
                album_title: "Album".into(),
            })
        ),
        None
    );
    assert_eq!(held(&upload, Some(FileRow::DeletedReleaseFile)), None);
    assert_eq!(held(&upload, None), None);
    assert_eq!(
        held(&coven::PendingSubject::Move { id: coven::MoveId(3) }, Some(FileRow::Avatar)),
        None
    );
}
