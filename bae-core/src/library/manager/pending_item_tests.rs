use super::*;

fn named(subject: coven::PendingSubject) -> PendingSubjectName {
    PendingItem::before_open(coven::PendingRecord {
        subject,
        reason: coven::PendingReason::NoStorage,
    })
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
    assert_eq!(
        named(S::File {
            device: coven::DeviceId(2),
            file: coven::FileId(uuid::Uuid::from_u128(9)),
        }),
        PendingSubjectName::File
    );
}

/// A queued file is named by its release's album; once that release is gone
/// it is a file of a deleted release, never a name made up for it.
#[test]
fn an_upload_is_named_by_its_release_while_the_release_exists() {
    assert_eq!(
        upload_name(
            UploadFileLabel::Filename("01 Track.flac".into()),
            Some("Album Title".into())
        ),
        PendingSubjectName::ReleaseFile {
            file_name: "01 Track.flac".into(),
            album_title: "Album Title".into(),
        }
    );
    assert_eq!(
        upload_name(UploadFileLabel::Cover, Some("Album Title".into())),
        PendingSubjectName::Cover {
            album_title: "Album Title".into()
        }
    );
    for label in [
        UploadFileLabel::Filename("01 Track.flac".into()),
        UploadFileLabel::Cover,
    ] {
        assert_eq!(
            upload_name(label, None),
            PendingSubjectName::DeletedReleaseFile
        );
    }
    assert_eq!(
        upload_name(UploadFileLabel::ArtistImage, None),
        PendingSubjectName::ArtistImage
    );
}
