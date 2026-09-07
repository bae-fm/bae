//! Synthetic MusicBrainz release documents, and seeding them into the caches
//! an import reads instead of the network.

/// One track on a synthetic MusicBrainz medium: numbered `position`, its title
/// carried by the recording (where a real MB document usually carries it).
pub fn mb_track(position: i64, title: &str) -> bae_core::musicbrainz::MbTrack {
    bae_core::musicbrainz::MbTrack {
        position: Some(position),
        number: Some(position.to_string()),
        title: None,
        length: None,
        recording: Some(bae_core::musicbrainz::MbRecording {
            id: None,
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    }
}

/// One CD medium holding `tracks`, with no disc ids.
pub fn mb_medium(tracks: Vec<bae_core::musicbrainz::MbTrack>) -> bae_core::musicbrainz::MbMedium {
    bae_core::musicbrainz::MbMedium {
        discs: vec![],
        format: Some("CD".to_string()),
        tracks,
    }
}

/// The MusicBrainz release document the import tests seed: one "Artist Name"
/// credit, one CD medium holding a single "Track One" recording, a 1996 US
/// pressing with no labels, no url-rels and no cover art.
///
/// A fixture that differs in one of those names that field and takes the rest
/// from here:
///
/// ```ignore
/// MbReleaseResponse {
///     media: vec![support::mb_medium(vec![support::mb_track(1, "Only Track")])],
///     ..support::mb_release(release_id, group_id, "Album Title")
/// }
/// ```
pub fn mb_release(
    release_id: &str,
    release_group_id: &str,
    title: &str,
) -> bae_core::musicbrainz::MbReleaseResponse {
    bae_core::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: title.to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![bae_core::musicbrainz::MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(bae_core::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(bae_core::musicbrainz::MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![mb_medium(vec![mb_track(1, "Track One")])],
        relations: vec![],
        cover_art_archive: bae_core::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// Seed `response` and a minimal release-group document into the MusicBrainz
/// caches, so a lookup for either resolves without touching the network, and
/// return the release id. The release's own archived JSON is its serialization,
/// so what the import stores and what it parsed cannot disagree.
pub fn seed_mb_release(
    response: bae_core::musicbrainz::MbReleaseResponse,
    release_group_id: &str,
) -> String {
    let release_id = response.id.clone();
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(&release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        release_group_id,
        serde_json::json!({ "id": release_group_id }).to_string(),
    );
    release_id
}
