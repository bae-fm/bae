use super::*;
use bae_catalogs::musicbrainz::MusicBrainzError;
use bae_catalogs::CatalogError;

/// Each reason a release cannot be read onto the audio is the import's
/// own refusal, naming what it names.
#[test]
fn a_reading_refusal_is_the_import_refusal_it_names() {
    use bae_catalogs::reading::ReadingRefusal;
    assert!(matches!(
        ImportError::from(ReadingRefusal::Tied { mediums: vec![1, 3] }),
        ImportError::TiedMediums { mediums } if mediums == vec![1, 3]
    ));
    assert!(matches!(
        ImportError::from(ReadingRefusal::TrackCount { folder: 2, release: 7 }),
        ImportError::MetadataTrackCount { folder_tracks: 2, release_tracks: 7 }
    ));
    assert!(matches!(
        ImportError::from(ReadingRefusal::Unreadable {
            catalog: bae_catalogs::catalog::Catalog::Discogs,
            detail: "a track with no title".to_string(),
        }),
        ImportError::Catalog(CatalogError::SourceData {
            catalog: bae_catalogs::catalog::Catalog::Discogs,
            ..
        })
    ));
}

/// A remote-image fetch that failed shows as an import failure when the
/// artwork host answered or could not be reached, as unusable import
/// data when the bytes are no image, and as bae's own failure when bae
/// could not build the request or finish its cache task.
#[tokio::test]
async fn a_failed_remote_image_fetch_shows_as_whose_failure_it_is() {
    use bae_catalogs::remote_image::RemoteImageError;
    use bae_diagnostics::UiErrorCategory;
    let category = |error: RemoteImageError| ImportError::from(error).category();
    let transport = |error: reqwest::Error| RemoteImageError::Transport {
        context: "Failed to fetch image",
        error,
    };

    let answered = RemoteImageError::Status { status: 503, detail: "unavailable".to_string() };
    assert_eq!(category(answered), UiErrorCategory::Import);
    let absent = RemoteImageError::NoImage { url: "https://example.com/front".to_string() };
    assert_eq!(category(absent), UiErrorCategory::Import);
    let unreached = reqwest::Client::new()
        .get("http://127.0.0.1:1/cover.jpg")
        .send()
        .await
        .expect_err("nothing listens on port 1");
    assert_eq!(category(transport(unreached)), UiErrorCategory::Import);
    let unbuilt =
        reqwest::Client::new().get("not a URL").build().expect_err("a relative URL does not build");
    assert_eq!(category(transport(unbuilt)), UiErrorCategory::Internal);
    let unusable =
        RemoteImageError::Unusable { detail: "Downloaded file is not a valid image".to_string() };
    assert_eq!(category(unusable), UiErrorCategory::ImportData);
    let unfinished = RemoteImageError::CacheTask(bae_util::task_failure::TaskFailed::Panicked(
        bae_util::task_failure::Bug {
            operation: "read the remote image cache",
            message: "the cache task fails".to_string(),
        },
    ));
    assert_eq!(category(unfinished), UiErrorCategory::Internal);
}

/// A MusicBrainz timeout raised with `?` arrives at the terminal consumer as
/// a matchable `MusicBrainz(Timeout)`, so a retry policy can read the
/// retryability straight off the variant.
#[test]
fn musicbrainz_error_is_preserved_unflattened() {
    let err: ImportError = MusicBrainzError::Timeout.into();
    assert!(matches!(
        err,
        ImportError::Catalog(CatalogError::MusicBrainz(MusicBrainzError::Timeout))
    ));

    let err: ImportError = MusicBrainzError::Provider { status: Some(503), told_wait: None }.into();
    assert!(matches!(
        err,
        ImportError::Catalog(CatalogError::MusicBrainz(MusicBrainzError::Provider {
            status: Some(503),
            ..
        }))
    ));
}

/// A credit the library refuses shows the refusal's own line under an
/// import, as it does anywhere else.
#[test]
fn a_refused_credit_shows_its_own_line() {
    use bae_diagnostics::UiErrorCategory as C;
    use bae_records::{IdentityRefusal, IdentityWriteError};
    let refusal = IdentityRefusal::ArtistGone("gone".to_string());
    let error = ImportError::from(IdentityWriteError::from(refusal));
    assert_eq!(error.category(), C::ChangedMeanwhile, "{error}");
}

/// A request whose candidate or watched folders changed under it says so,
/// a folder that overlaps a watched one says that, and a pane's edit of what
/// its draft doesn't hold is bae's own fault: none of them is an import.
#[test]
fn a_refused_request_is_not_shown_as_an_import() {
    use crate::tables::{CandidateStateRefusal, PreparationRefusal, WatchedFolderRefusal};
    use bae_diagnostics::UiErrorCategory as C;
    use std::path::PathBuf;
    let cases: [(ImportError, C); 6] = [
        (CandidateStateRefusal::Superseded("hash".into()).into(), C::ChangedMeanwhile),
        (
            CandidateStateRefusal::MetadataChanged { content_hash: "hash".into(), revision: 3 }
                .into(),
            C::ChangedMeanwhile,
        ),
        (PreparationRefusal::Imported("hash".into()).into(), C::ChangedMeanwhile),
        (WatchedFolderRefusal::NotWatched(PathBuf::from("/music")).into(), C::ChangedMeanwhile),
        (
            WatchedFolderRefusal::Overlap {
                folder: PathBuf::from("/music/a"),
                watched: PathBuf::from("/music"),
            }
            .into(),
            C::WatchedFolderOverlap,
        ),
        (crate::preparations::TrackEditRefusal::NoTrackNamed.into(), C::Internal),
    ];
    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error}");
    }
}

/// A Wikidata request that never got an answer is a provider failure,
/// as a MusicBrainz one is, not release data that could not be mapped.
#[test]
fn wikidata_error_is_a_provider_failure() {
    let err: ImportError = bae_catalogs::wikidata::WikidataError::Timeout.into();
    assert!(matches!(
        err,
        ImportError::Catalog(CatalogError::Wikidata(
            bae_catalogs::wikidata::WikidataError::Timeout
        ))
    ));
    assert_eq!(err.category(), bae_diagnostics::UiErrorCategory::Import);
}

/// A Discogs server error likewise survives the conversion so a retry
/// policy can distinguish it from a hard failure.
#[test]
fn discogs_error_is_preserved_unflattened() {
    let err: ImportError = bae_catalogs::discogs::client::DiscogsError::Provider {
        status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
        told_wait: None,
    }
    .into();
    assert!(matches!(
        err,
        ImportError::Catalog(CatalogError::Discogs(
            bae_catalogs::discogs::client::DiscogsError::Provider {
                status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ))
    ));
}
#[test]
fn invalid_request_preserves_the_discogs_transport_cause() {
    let request_error = reqwest::Client::new().get("not a URL").build().unwrap_err();
    let error = super::ImportError::from(bae_catalogs::discogs::client::DiscogsError::Transport(
        request_error,
    ));
    assert!(error.to_string().contains("relative URL without a base"), "{error}");
    let bae_catalogs::Failure::Internal(failure) = error.failure("asking Discogs") else {
        panic!("a request bae could not build is its own failure");
    };
    assert!(failure.detail.contains("relative URL without a base"), "{}", failure.detail);
}

/// Only an error a provider answered with is a lookup that failed; one
/// bae met on its own side is not, however it happened on the way to a
/// lookup.
#[test]
fn only_a_provider_s_error_is_a_failed_lookup() {
    let refused: ImportError = bae_catalogs::musicbrainz::MusicBrainzError::Timeout.into();
    assert_eq!(refused.provider_failure(), Some(bae_catalogs::LookupFailure::Timeout));
    let local = ImportError::Internal {
        detail: "the release was stored and did not read back".to_string(),
    };
    assert_eq!(local.provider_failure(), None);
}

/// A lookup that never reached MusicBrainz names what failed underneath — the
/// DNS lookup, the refused connection — in the detail a surface shows, where
/// reqwest's own text says only that sending failed. The request's path and
/// query stay out of it.
#[tokio::test]
async fn a_musicbrainz_network_failure_names_its_cause_in_the_detail_a_surface_shows() {
    for (origin, cause) in
        [("http://bae-test.invalid", "dns error"), ("http://127.0.0.1:1", "tcp connect error")]
    {
        let musicbrainz = bae_catalogs::musicbrainz::MusicBrainz::for_test(
            bae_util::http::Http::for_test().serve_every_host(origin),
        );
        let error = musicbrainz
            .lookup_by_discid("disc-unreachable", bae_catalogs::rate_limiter::CallClass::REQUESTED)
            .await
            .expect_err("nothing answers there");
        assert!(matches!(error, MusicBrainzError::Network(_)), "{error}");
        let bae_diagnostics::UiError::Diagnostic { category, detail } =
            ImportError::from(error).ui_error();
        assert_eq!(category, bae_diagnostics::UiErrorCategory::Import);
        assert!(detail.contains(cause), "{detail}");
        assert!(!detail.contains("disc-unreachable"), "{detail}");
    }
}

/// A request that never reached Discogs names what failed underneath in the
/// detail a surface shows, where reqwest's own text says only that sending
/// failed.
#[tokio::test]
async fn a_discogs_transport_failure_names_its_cause_in_the_detail_a_surface_shows() {
    let client = bae_catalogs::discogs::client::DiscogsClient::new(
        std::sync::Arc::new(bae_catalogs::discogs::client::Discogs::for_test(
            bae_util::http::Http::for_test().serve("api.discogs.com", "http://127.0.0.1:1"),
        )),
        "token".to_string(),
    );
    let error = client
        .get_release("510001", bae_catalogs::rate_limiter::CallClass::REQUESTED)
        .await
        .expect_err("nothing listens on port 1");
    assert!(matches!(error, bae_catalogs::discogs::client::DiscogsError::Transport(_)), "{error}");
    let bae_diagnostics::UiError::Diagnostic { category, detail } =
        ImportError::from(error).ui_error();
    assert_eq!(category, bae_diagnostics::UiErrorCategory::Import);
    assert!(detail.contains("tcp connect error"), "{detail}");
}
