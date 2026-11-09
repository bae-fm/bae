use super::*;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::CatalogError;
use bae_import::ImportError;

#[test]
fn unexpected_import_data_is_distinct_from_domain_refusals() {
    for error in [
        ImportError::Catalog(CatalogError::SourceData {
            catalog: Catalog::MusicBrainz,
            detail: "missing track title".into(),
        }),
        ImportError::CoverArt { detail: "unsupported image decoder".into() },
        ImportError::from(bae_catalogs::musicbrainz::MusicBrainzError::Other(
            "invalid release JSON".into(),
        )),
    ] {
        let expected_detail = error.to_string();
        let BridgeError::Diagnostic { category, detail } = BridgeError::from(error) else {
            panic!("unexpected data must retain a diagnostic");
        };
        assert_eq!(detail, expected_detail);
        assert_eq!(category, BridgeErrorCategory::ImportData);
    }
}

#[test]
fn internal_import_failure_retains_its_category() {
    let error = ImportError::Internal { detail: "missing prepared bytes".into() };
    assert!(matches!(BridgeError::from(error), BridgeError::Diagnostic {
        category: BridgeErrorCategory::Internal, detail
    } if detail.contains("missing prepared bytes")));
}

/// A store failure, an internal fault or a refusal crosses with its class,
/// whether the library or import met it.
#[test]
fn store_internal_and_refused_failures_keep_their_category() {
    use bae_library::LibraryError;
    let cases: [(BridgeError, BridgeErrorCategory); 4] = [
        (
            LibraryError::Database(bae_store::DbError::unreadable("a column", "a value")).into(),
            BridgeErrorCategory::Database,
        ),
        (
            ImportError::Database(bae_store::DbError::unreadable("a column", "a value")).into(),
            BridgeErrorCategory::Database,
        ),
        (
            LibraryError::Internal("prepared state missing".into()).into(),
            BridgeErrorCategory::Internal,
        ),
        (
            ImportError::Internal { detail: "prepared state missing".into() }.into(),
            BridgeErrorCategory::Internal,
        ),
    ];
    for (error, category) in cases {
        assert!(
            matches!(&error, BridgeError::Diagnostic { category: actual, .. } if *actual == category),
            "{error:?}"
        );
    }
}

/// A library operation that ran an import step (the release editor's
/// catalog apply) crosses with the import refusal's own category, so every
/// app words that refusal rather than a generic import failure.
#[test]
fn an_import_refusal_inside_a_library_error_crosses_as_itself() {
    use bae_library::LibraryError;
    let cases = [
        (
            ImportError::MetadataTrackCount { folder_tracks: 10, release_tracks: 12 },
            BridgeErrorCategory::MetadataTrackCount { folder_tracks: 10, release_tracks: 12 },
        ),
        (
            ImportError::TiedMediums { mediums: vec![1, 3] },
            BridgeErrorCategory::TiedMediums { mediums: vec![1, 3] },
        ),
    ];
    for (error, category) in cases {
        assert!(matches!(
            BridgeError::from(LibraryError::from(error)),
            BridgeError::Diagnostic { category: actual, .. } if actual == category
        ));
    }
}

#[test]
fn expected_provider_errors_remain_domain_failures() {
    use bae_catalogs::{discogs::client::DiscogsError, musicbrainz::MusicBrainzError};
    for error in [
        ImportError::from(MusicBrainzError::NotFound("release".into())),
        ImportError::from(MusicBrainzError::Provider { status: Some(500), told_wait: None }),
        ImportError::from(MusicBrainzError::Timeout),
        ImportError::from(DiscogsError::NotFound),
        ImportError::from(DiscogsError::Provider {
            status: 500u16.try_into().unwrap(),
            told_wait: None,
        }),
        ImportError::from(DiscogsError::InvalidApiKey),
    ] {
        assert!(matches!(
            BridgeError::from(error),
            BridgeError::Diagnostic { category: BridgeErrorCategory::Import, .. }
        ));
    }
}

#[test]
fn expected_local_and_draft_failures_keep_their_presentation() {
    for (error, expected) in [
        (
            ImportError::LocalCover { detail: "selected image disappeared".into() },
            BridgeErrorCategory::Import,
        ),
        (
            ImportError::LocalCover { detail: "permission denied reading selected image".into() },
            BridgeErrorCategory::Import,
        ),
        (
            ImportError::MetadataTrackCount { folder_tracks: 14, release_tracks: 15 },
            BridgeErrorCategory::MetadataTrackCount { folder_tracks: 14, release_tracks: 15 },
        ),
        (ImportError::CandidateImportInProgress, BridgeErrorCategory::CandidateImportInProgress),
        (ImportError::CandidateBeingIdentified, BridgeErrorCategory::CandidateBeingIdentified),
        (
            ImportError::FilesAlreadyImported { release_id: "release-1".into() },
            BridgeErrorCategory::FilesAlreadyImported,
        ),
        (
            ImportError::from(bae_import::DiscogsKeyError::Keyring(coven::KeyError::StoreClosed)),
            BridgeErrorCategory::Config,
        ),
    ] {
        assert!(
            matches!(BridgeError::from(error), BridgeError::Diagnostic {category, ..} if category == expected)
        );
    }
    assert_eq!(
        bridge_error_category_key(BridgeErrorCategory::ImportData),
        bridge_error_category_key(BridgeErrorCategory::Import)
    );
}

#[test]
fn artwork_request_conditions_remain_expected_at_the_bridge() {
    for status in [403, 500] {
        let error = ImportError::from(bae_catalogs::remote_image::RemoteImageError::Status {
            status,
            detail: "artwork request failed".into(),
        });
        assert!(matches!(
            BridgeError::from(error),
            BridgeError::Diagnostic { category: BridgeErrorCategory::Import, .. }
        ));
    }
}

#[tokio::test]
async fn actual_artwork_data_failure_reaches_the_bridge_with_its_detail() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let received = stream.read(&mut request).await.unwrap();
        assert!(received > 0, "client must send a request before the response");
        let body = "invalid image ".repeat(30);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    let error = ImportError::from(
        bae_catalogs::remote_image::RemoteImageCache::for_test(
            bae_util::http::Http::for_test().serve("images.example", &origin),
            bae_diagnostics::Diagnostics::noop(),
        )
        .fetch("https://images.example/cover")
        .await
        .unwrap_err(),
    );
    let detail = error.to_string();
    assert!(detail.contains("not a valid image"), "{detail}");
    assert_eq!(
        BridgeError::from(error),
        BridgeError::Diagnostic { category: BridgeErrorCategory::ImportData, detail }
    );
    server.await.unwrap();
}

#[test]
fn a_queue_change_has_no_line() {
    let queue_changed =
        BridgeError::diagnostic(BridgeErrorCategory::QueueChanged, "the queue moved on");
    assert_eq!(bridge_error_line_key(&queue_changed), None);
    let export = BridgeError::diagnostic(BridgeErrorCategory::Export, "disk full");
    assert_eq!(
        bridge_error_line_key(&export),
        Some(bridge_error_category_key(BridgeErrorCategory::Export))
    );
}
