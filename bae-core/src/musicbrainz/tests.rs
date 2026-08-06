use super::*;

#[test]
fn test_release_search_params_build_query() {
    let params = ReleaseSearchParams {
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        year: Some("2000".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params.build_query(),
        "artist:\"Test Artist\" AND release:\"Test Album\" AND date:2000",
    );
    let params2 = ReleaseSearchParams {
        artist: Some("Another Artist".to_string()),
        catalog_number: Some("TL-1234".to_string()),
        ..Default::default()
    };
    assert_eq!(
        params2.build_query(),
        "artist:\"Another Artist\" AND catno:\"TL-1234\""
    );
}

#[test]
fn release_search_params_ignore_blank_fields() {
    let blank_params = ReleaseSearchParams {
        artist: Some("   ".to_string()),
        album: Some("\n\t".to_string()),
        ..Default::default()
    };
    assert!(!blank_params.has_any_field());
    assert_eq!(blank_params.build_query(), "");

    let params = ReleaseSearchParams {
        artist: Some("  Artist Name  ".to_string()),
        year: Some(" 2000 ".to_string()),
        ..Default::default()
    };
    assert!(params.has_any_field());
    assert_eq!(params.build_query(), "artist:\"Artist Name\" AND date:2000");
}

#[test]
fn release_search_params_escape_quoted_lucene_values() {
    assert_eq!(
        QueryValueFormat::Quoted.render("release", r#"Quoted "Middle" Phrase"#),
        r#"release:"Quoted \"Middle\" Phrase""#,
    );
    assert_eq!(
        QueryValueFormat::Quoted.render("release", r#"Backslash at end\"#),
        r#"release:"Backslash at end\\""#,
    );
    assert_eq!(
        QueryValueFormat::Quoted.render("artist", "Artist Name"),
        r#"artist:"Artist Name""#,
    );
}

#[test]
fn release_search_params_build_query_with_escaped_phrase() {
    let params = ReleaseSearchParams {
        artist: Some("Artist Name".to_string()),
        album: Some(r#"Quoted "Middle" Phrase"#.to_string()),
        ..Default::default()
    };

    assert_eq!(
        params.build_query(),
        r#"artist:"Artist Name" AND release:"Quoted \"Middle\" Phrase""#,
    );
}

#[test]
fn first_discogs_release_url_skips_master_urls_and_missing_urls() {
    let relations = vec![
        MbRelation {
            url: Some(MbUrlResource {
                resource: Some("https://www.discogs.com/master/12345".to_string()),
            }),
            ..Default::default()
        },
        MbRelation {
            url: Some(MbUrlResource {
                resource: Some("https://www.discogs.com/release/67890".to_string()),
            }),
            ..Default::default()
        },
        MbRelation {
            url: None,
            ..Default::default()
        },
    ];

    assert_eq!(
        first_discogs_release_url(&relations),
        Some("https://www.discogs.com/release/67890".to_string())
    );
}

#[test]
fn test_deserialize_mb_release_response() {
    let json = r#"{
        "id": "f9469bd8-a413-43f1-bee3-e3baabfb91cc",
        "title": "Super Hits of the 70s",
        "date": "2002",
        "country": null,
        "barcode": "8711638222024",
        "artist-credit": [{
            "name": "All Star Cover Band",
            "artist": {
                "id": "53ebb100-5cfb-42e7-9ae3-453464420840",
                "name": "All Star Cover Band",
                "sort-name": "All Star Cover Band"
            }
        }],
        "release-group": {
            "id": "ded0036e-243a-4ae4-8c65-7ec37aae4bd9",
            "first-release-date": "2002",
            "secondary-types": [],
            "secondary-type-ids": []
        },
        "label-info": [{
            "catalog-number": "3822202",
            "label": { "name": "Galaxy Music" }
        }],
        "media": [{
            "format": "CD",
            "tracks": [
                { "position": 1, "title": "Track One Title", "length": 216000 },
                { "position": 2, "title": "Track Two Title", "length": 241000 }
            ]
        }],
        "relations": [{
            "url": { "resource": "https://www.discogs.com/release/67890" }
        }],
        "cover-art-archive": {
            "count": 2, "artwork": true, "front": true, "back": true,
            "darkened": false
        }
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "f9469bd8-a413-43f1-bee3-e3baabfb91cc");
    assert_eq!(response.title, "Super Hits of the 70s");
    assert_eq!(response.date.as_deref(), Some("2002"));
    assert!(response.country.is_none());
    assert_eq!(response.barcode.as_deref(), Some("8711638222024"));
    assert_eq!(response.artist_credit.len(), 1);
    assert_eq!(response.artist_credit[0].name, "All Star Cover Band");
    assert_eq!(response.media.len(), 1);
    assert_eq!(response.media[0].tracks.len(), 2);
    assert_eq!(
        response.media[0].tracks[0].title.as_deref(),
        Some("Track One Title")
    );
    assert_eq!(response.label_info.len(), 1);
    assert_eq!(
        response.label_info[0].catalog_number.as_deref(),
        Some("3822202")
    );
    assert_eq!(response.relations.len(), 1);
    assert!(response.has_front_cover());
}

#[test]
fn test_deserialize_mb_release_response_minimal() {
    // Minimal response: every field the type requires, every optional array
    // absent. The `cover-art-archive` block is required because every endpoint
    // this type is parsed from — the release lookup, the disc-ID lookup, the
    // release browse — returns it.
    let json = r#"{
        "id": "abc-123",
        "title": "Minimal Release",
        "cover-art-archive": {
            "count": 0, "artwork": false, "front": false, "back": false,
            "darkened": false
        }
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "abc-123");
    assert_eq!(response.title, "Minimal Release");
    assert!(response.date.is_none());
    assert!(response.artist_credit.is_empty());
    assert!(response.media.is_empty());
    assert!(response.relations.is_empty());
    assert!(!response.has_front_cover());
}

/// A takedown darkens the whole release's art: the archive serves nothing for
/// it, whatever `front` says.
#[test]
fn a_darkened_release_serves_no_front_cover() {
    let json = r#"{
        "id": "darkened-1",
        "title": "Album Title",
        "cover-art-archive": {
            "count": 3, "artwork": true, "front": true, "back": false,
            "darkened": true
        }
    }"#;

    let response: MbReleaseResponse = serde_json::from_str(json).unwrap();
    assert!(!response.has_front_cover());
    assert!(crate::import::cover_art::musicbrainz_covers(&response).is_empty());
}

// ── fetch_mb_xref ────────────────────────────────────────────────
//
// Seeded through the caches so no test hits the network. The caches are
// process-global LRUs, so each test uses a unique Discogs release ID to keep
// another test's seed from bleeding in.

fn make_mb_response(id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: id.to_string(),
        title: "Test Album".to_string(),
        date: None,
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: release_group_id.map(|rg| MbReleaseGroupRef {
            id: rg.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

#[test]
fn release_group_fallback_error_is_logged_with_group_id() {
    let mut result = None;

    let logs = crate::test_logs::capture_warn_logs(|| {
        result = Some(release_group_discogs_url(
            "rg-error",
            Err(MusicBrainzError::Other("transient fetch".to_string())),
        ));
    });

    assert!(logs.contains("rg-error"));
    assert!(logs.contains("transient fetch"));
    assert!(result.expect("closure ran").is_none());
}

#[tokio::test]
async fn test_fetch_mb_xref_with_backlink_returns_response_and_metadata() {
    let discogs_id = "fetch-mb-xref-hit-1";
    let mb_release_id = "mb-release-hit-1";
    let mb_group_id = "mb-group-hit-1";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        (
            make_mb_response(mb_release_id, Some(mb_group_id)),
            None,
            r#"{"id":"mb-release-hit-1"}"#.to_string(),
        ),
    );
    seed_release_group_json_cache(mb_group_id, r#"{"id":"mb-group-hit-1"}"#.to_string());

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    let (response, pairs) = result.expect("expected cross-link to be found");
    assert_eq!(response.id, mb_release_id);
    assert_eq!(
        response.release_group.as_ref().map(|rg| rg.id.as_str()),
        Some(mb_group_id)
    );
    // Two documents: the MB release, re-keyed under the Discogs release the
    // lookup started from, and its release group under its own id.
    assert_eq!(pairs.len(), 2);
    assert_eq!(
        pairs[0].source,
        crate::import::PayloadSource::MusicBrainzDiscogsXref
    );
    assert_eq!(pairs[0].source_release_id, discogs_id);
    assert_eq!(
        pairs[1].source,
        crate::import::PayloadSource::MusicBrainzReleaseGroup
    );
    assert_eq!(pairs[1].source_release_id, mb_group_id);
}

#[tokio::test]
async fn test_fetch_mb_xref_no_backlink_returns_none() {
    let discogs_id = "fetch-mb-xref-miss-1";
    seed_discogs_url_lookup(discogs_id, None);

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    assert!(
        result.is_none(),
        "expected None when MB has no back-link, got Some"
    );
}

#[tokio::test]
async fn test_fetch_mb_xref_release_without_group_still_returns_response() {
    // A missing release group is not a fetch-time failure: `fetch_mb_xref`
    // returns whatever MB gave it. The mapper is what gates on `release_group`,
    // and only emits an MB identity row when one is present.
    let discogs_id = "fetch-mb-xref-no-rg";
    let mb_release_id = "mb-release-no-rg";

    seed_discogs_url_lookup(discogs_id, Some(mb_release_id.to_string()));
    seed_release_cache(
        mb_release_id,
        (
            make_mb_response(mb_release_id, None),
            None,
            r#"{"id":"mb-release-no-rg"}"#.to_string(),
        ),
    );

    let result = fetch_mb_xref(discogs_id, CallPriority::Interactive).await;

    let (response, pairs) = result.expect("expected response even without release_group");
    assert_eq!(response.id, mb_release_id);
    assert!(response.release_group.is_none());
    // Only one document (no release-group JSON to fetch).
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].source,
        crate::import::PayloadSource::MusicBrainzDiscogsXref
    );
    assert_eq!(pairs[0].source_release_id, discogs_id);
}

/// Only a failure a retry could fix is retried. A `NotFound` is the ordinary
/// answer for a disc MusicBrainz doesn't have, and each extra attempt costs
/// another round trip plus a 1s rate-limit wait; the "at least one search field"
/// error is raised before a request is even built.
#[test]
fn only_transient_musicbrainz_failures_are_retried() {
    assert!(should_retry_mb(&MusicBrainzError::Timeout));
    assert!(should_retry_mb(&MusicBrainzError::Network(
        "refused".into()
    )));
    assert!(should_retry_mb(&MusicBrainzError::Provider {
        status: Some(503)
    }));
    assert!(should_retry_mb(&MusicBrainzError::Provider {
        status: Some(429)
    }));

    assert!(!should_retry_mb(&MusicBrainzError::NotFound("disc".into())));
    assert!(!should_retry_mb(&MusicBrainzError::Provider {
        status: Some(404)
    }));
    assert!(!should_retry_mb(&MusicBrainzError::Provider {
        status: Some(400)
    }));
    assert!(!should_retry_mb(&MusicBrainzError::Other(
        "At least one search field must be provided".into()
    )));
}

// ── fetch_release_with_metadata ─────────────────────────────────────────────

fn mb_release(release_id: &str, release_group_id: Option<&str>) -> MbReleaseResponse {
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("1999".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![],
        release_group: release_group_id.map(|id| MbReleaseGroupRef {
            id: id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// The archival pairs every MB import path writes: the release under
/// `musicbrainz`, its release-group under `musicbrainz_release_group`. Both the
/// direct import and the Discogs cross-reference fetch through here, so a change
/// to this shape reaches both.
#[tokio::test]
async fn fetch_release_with_metadata_archives_release_and_group() {
    let release_id = "fetch-with-metadata-rel";
    let group_id = "fetch-with-metadata-group";
    seed_release_cache(
        release_id,
        (
            mb_release(release_id, Some(group_id)),
            Some("https://www.discogs.com/release/1".to_string()),
            r#"{"id":"release"}"#.to_string(),
        ),
    );
    seed_release_group_json_cache(group_id, r#"{"id":"group"}"#.to_string());

    let fetched = fetch_release_with_metadata(release_id, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(fetched.response.id, release_id);
    assert_eq!(
        fetched.discogs_url.as_deref(),
        Some("https://www.discogs.com/release/1")
    );
    assert_eq!(fetched.raw_json, r#"{"id":"release"}"#);
    assert_eq!(
        fetched.release_group,
        Some(crate::import::SourcePayload::new(
            crate::import::PayloadSource::MusicBrainzReleaseGroup,
            group_id,
            r#"{"id":"group"}"#.to_string()
        ))
    );
}

/// A release with no release group archives just its own JSON. The group is
/// supplementary — its absence is not an import failure.
#[tokio::test]
async fn fetch_release_with_metadata_without_group_archives_only_the_release() {
    let release_id = "fetch-with-metadata-no-group";
    seed_release_cache(
        release_id,
        (
            mb_release(release_id, None),
            None,
            r#"{"id":"release"}"#.to_string(),
        ),
    );

    let fetched = fetch_release_with_metadata(release_id, CallPriority::Interactive)
        .await
        .unwrap();

    assert_eq!(fetched.discogs_url, None);
    assert_eq!(fetched.raw_json, r#"{"id":"release"}"#);
    assert_eq!(fetched.release_group, None);
}

// ── label_and_catno ────────────────────────────────────────────────────────

#[test]
fn label_and_catno_reads_the_first_label_info() {
    let label_info = vec![
        MbLabelInfo {
            label: Some(MbLabel {
                name: Some("First Label".to_string()),
            }),
            catalog_number: Some("CAT-1".to_string()),
        },
        MbLabelInfo {
            label: Some(MbLabel {
                name: Some("Second Label".to_string()),
            }),
            catalog_number: Some("CAT-2".to_string()),
        },
    ];
    assert_eq!(
        label_and_catno(&label_info),
        (Some("First Label".to_string()), Some("CAT-1".to_string()))
    );

    // No label info at all, and an entry with neither field, both read as unknown.
    assert_eq!(label_and_catno(&[]), (None, None));
    assert_eq!(
        label_and_catno(&[MbLabelInfo {
            label: None,
            catalog_number: None,
        }]),
        (None, None)
    );
}
