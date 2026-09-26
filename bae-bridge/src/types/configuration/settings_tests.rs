use super::*;
use bae_core::import::{Catalog, ImportError};

#[test]
fn unexpected_import_data_is_distinct_from_domain_refusals() {
    for error in [
        ImportError::SourceData {
            catalog: Catalog::MusicBrainz,
            detail: "missing track title".into(),
        },
        ImportError::CoverArt {
            detail: "unsupported image decoder".into(),
        },
        ImportError::MusicBrainz(bae_core::musicbrainz::MusicBrainzError::Other(
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
    let error = ImportError::Internal {
        detail: "missing prepared bytes".into(),
    };
    assert!(matches!(BridgeError::from(error), BridgeError::Diagnostic {
        category: BridgeErrorCategory::Internal, detail
    } if detail.contains("missing prepared bytes")));
}

#[test]
fn wrapped_library_failures_preserve_the_library_category() {
    use bae_core::library::LibraryError;
    let cases: [(fn() -> LibraryError, BridgeErrorCategory); 3] = [
        (
            || LibraryError::Database(coven::DbError::Message("read failed".into())),
            BridgeErrorCategory::Database,
        ),
        (
            || LibraryError::Internal("prepared state missing".into()),
            BridgeErrorCategory::Internal,
        ),
        (
            || LibraryError::Import("identity needs review".into()),
            BridgeErrorCategory::Import,
        ),
    ];
    for (make_error, category) in cases {
        let direct = BridgeError::from(make_error());
        assert!(
            matches!(&direct, BridgeError::Diagnostic { category: actual, .. } if *actual == category)
        );
        assert_eq!(BridgeError::from(ImportError::Db(make_error())), direct);
    }
}

#[test]
fn expected_provider_errors_remain_domain_failures() {
    use bae_core::{discogs::client::DiscogsError, musicbrainz::MusicBrainzError};
    for error in [
        ImportError::MusicBrainz(MusicBrainzError::NotFound("release".into())),
        ImportError::MusicBrainz(MusicBrainzError::Provider {
            status: Some(500),
            told_wait: None,
        }),
        ImportError::MusicBrainz(MusicBrainzError::Network("connection closed".into())),
        ImportError::MusicBrainz(MusicBrainzError::Timeout),
        ImportError::Discogs(DiscogsError::NotFound),
        ImportError::Discogs(DiscogsError::Provider {
            status: 500u16.try_into().unwrap(),
            told_wait: None,
        }),
        ImportError::Discogs(DiscogsError::RateLimit { told_wait: None }),
        ImportError::Discogs(DiscogsError::InvalidApiKey),
    ] {
        assert!(matches!(
            BridgeError::from(error),
            BridgeError::Diagnostic {
                category: BridgeErrorCategory::Import,
                ..
            }
        ));
    }
}

#[test]
fn expected_local_and_draft_failures_keep_their_presentation() {
    for (error, expected) in [
        (
            ImportError::LocalCover {
                detail: "selected image disappeared".into(),
            },
            BridgeErrorCategory::Import,
        ),
        (
            ImportError::LocalCover {
                detail: "permission denied reading selected image".into(),
            },
            BridgeErrorCategory::Import,
        ),
        (
            ImportError::MetadataTrackCount {
                metadata_tracks: 15,
                audio_tracks: 14,
            },
            BridgeErrorCategory::MetadataTrackCount,
        ),
        (
            ImportError::CandidateImportInProgress,
            BridgeErrorCategory::CandidateImportInProgress,
        ),
        (
            ImportError::CandidateBeingIdentified,
            BridgeErrorCategory::CandidateBeingIdentified,
        ),
        (
            ImportError::CandidateAlreadyImported,
            BridgeErrorCategory::CandidateAlreadyImported,
        ),
        (
            ImportError::Config {
                detail: "failed to open key store".into(),
            },
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
    use bae_core::signals::LookupFailure;
    for failure in [
        LookupFailure::Network,
        LookupFailure::Timeout,
        LookupFailure::Provider { status: Some(404) },
        LookupFailure::Provider { status: Some(500) },
    ] {
        let error = ImportError::CoverArtRequest {
            failure,
            detail: "artwork request failed".into(),
        };
        assert!(matches!(
            BridgeError::from(error),
            BridgeError::Diagnostic {
                category: BridgeErrorCategory::Import,
                ..
            }
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
        assert!(
            received > 0,
            "client must send a request before the response"
        );
        let body = "invalid image ".repeat(30);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    let error = bae_core::import::cover_art::RemoteImageCache::for_test(
        bae_core::util::http::Http::for_test().serve("images.example", &origin),
    )
    .fetch("https://images.example/cover")
    .await
    .unwrap_err();
    let detail = error.to_string();
    assert!(detail.contains("not a valid image"), "{detail}");
    assert_eq!(
        BridgeError::from(error),
        BridgeError::Diagnostic {
            category: BridgeErrorCategory::ImportData,
            detail
        }
    );
    server.await.unwrap();
}
