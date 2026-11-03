//! How an import's error reads once it crosses into a library error.

use crate::library::LibraryError;
use bae_diagnostics::UiErrorCategory;
use bae_import::ImportError;

/// An import error that crosses into a library error stays the refusal it
/// is, so the library's category is the import's own, and a store failure
/// import met is the library's store failure.
#[test]
fn an_import_error_keeps_its_kind_through_a_library_error() {
    let library = LibraryError::from(ImportError::TiedMediums { mediums: vec![1, 3] });
    assert_eq!(library.category(), UiErrorCategory::TiedMediums { mediums: vec![1, 3] });

    let library = LibraryError::from(ImportError::MetadataTrackCount {
        folder_tracks: 10,
        release_tracks: 12,
    });
    assert_eq!(
        library.category(),
        UiErrorCategory::MetadataTrackCount { folder_tracks: 10, release_tracks: 12 }
    );

    let from_db =
        LibraryError::from(ImportError::Database(bae_db::DbError::Message("read failed".into())));
    assert!(matches!(from_db, LibraryError::Database(_)), "{from_db:?}");
}
