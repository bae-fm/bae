//! What a read names as not got: a source that failed, and a document on a
//! catalog the read did not ask.

use super::*;

/// What a fetch followed a link to and did not get is named with the
/// release: a source that failed, and a Discogs document with no key to ask
/// with. A catalog answering that there is none is not missing anything.
#[tokio::test]
async fn documents_a_fetch_could_not_get_are_named() {
    let selected = LookupRef::new(LookupCatalog::MusicBrainz, "missing-parts-release");
    let server = ProviderServer::start(HashMap::from([
        (
            "/ws/2/release/missing-parts-release".into(),
            (
                200,
                release_json(
                    &selected.key,
                    Some("missing-parts-group"),
                    &["https://www.discogs.com/release/950001"],
                ),
            ),
        ),
        ("/ws/2/release-group/missing-parts-group".into(), (400, "Rejected group request".into())),
    ]))
    .await;
    let fetched = server
        .providers
        .fetch_payloads(
            None,
            &LookupCatalog::ALL,
            &selected,
            ReadDepth::Complete,
            CallClass::REQUESTED,
        )
        .await
        .unwrap();
    let mut unfetched = fetched.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::Discogs,
                key: "950001".into(),
                reason: UnfetchedReason::NotAsked,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzReleaseGroup,
                key: "missing-parts-group".into(),
                reason: UnfetchedReason::Failed,
            },
        ]
    );
    let stored = fetched.extract().unwrap();
    assert!(
        stored.fetch_could_add(ReadDepth::Complete, &[LookupCatalog::MusicBrainz]),
        "the failed group can be asked again"
    );
    let mut only_the_key_missing = stored.clone();
    only_the_key_missing.unfetched.retain(|document| document.reason == UnfetchedReason::NotAsked);
    assert!(
        !only_the_key_missing.fetch_could_add(ReadDepth::Complete, &[LookupCatalog::MusicBrainz]),
        "without a key, asking again gets no Discogs document"
    );
    assert!(
        only_the_key_missing.fetch_could_add(ReadDepth::Complete, &LookupCatalog::ALL),
        "with a key, the Discogs document can be asked for"
    );
}

/// A session with Discogs switched off reads a MusicBrainz release without
/// asking Discogs for the release and master its pages link, though it holds
/// a key: each is named as not asked, and reading again adds them once
/// Discogs is switched on.
#[tokio::test]
async fn a_read_asks_a_switched_off_discogs_nothing() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let session = crate::providers::CatalogSession::new(
        Some(Arc::new(server.fakes.discogs_client("fixture-token"))),
        vec![LookupCatalog::MusicBrainz],
        server.providers.clone(),
    );
    let payloads = session
        .fetch_payloads(
            &LookupRef::new(LookupCatalog::MusicBrainz, "cycle-release"),
            ReadDepth::Complete,
            CallClass::REQUESTED,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([
            ("/ws/2/release/cycle-release".into(), 1),
            ("/ws/2/release-group/cycle-group".into(), 1),
        ])
    );
    let mut unfetched = payloads.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::Discogs,
                key: "920001".into(),
                reason: UnfetchedReason::NotAsked,
            },
            UnfetchedDocument {
                document: PayloadSource::DiscogsMaster,
                key: "920002".into(),
                reason: UnfetchedReason::NotAsked,
            },
        ]
    );
    let stored = payloads.extract().unwrap();
    assert!(!stored.fetch_could_add(ReadDepth::Complete, session.asked()));
    assert!(stored.fetch_could_add(ReadDepth::Complete, &LookupCatalog::ALL));
}

/// A session with MusicBrainz switched off reads a Discogs release and its
/// master without asking MusicBrainz which of its releases and release
/// groups link them.
#[tokio::test]
async fn a_read_asks_a_switched_off_musicbrainz_nothing() {
    let server = ProviderServer::start(linked_pair_answers()).await;
    let session = crate::providers::CatalogSession::new(
        Some(Arc::new(server.fakes.discogs_client("fixture-token"))),
        vec![LookupCatalog::Discogs],
        server.providers.clone(),
    );
    let payloads = session
        .fetch_payloads(
            &LookupRef::new(LookupCatalog::Discogs, "920001"),
            ReadDepth::Complete,
            CallClass::REQUESTED,
        )
        .await
        .unwrap();

    assert_eq!(
        server.requests(),
        HashMap::from([("/releases/920001".into(), 1), ("/masters/920002".into(), 1)])
    );
    let mut unfetched = payloads.unfetched.clone();
    unfetched.sort_by_key(|document| document.key.clone());
    assert_eq!(
        unfetched,
        vec![
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsXref,
                key: "920001".into(),
                reason: UnfetchedReason::NotAsked,
            },
            UnfetchedDocument {
                document: PayloadSource::MusicBrainzDiscogsMasterXref,
                key: "920002".into(),
                reason: UnfetchedReason::NotAsked,
            },
        ]
    );
}
