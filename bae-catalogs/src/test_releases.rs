//! Synthetic MusicBrainz and Discogs releases a test builds its fixtures
//! from, for [`crate::fake_catalogs::FakeCatalogs`] to answer with.

/// One track on a synthetic MusicBrainz medium: numbered `position`, its title
/// carried by the recording (where a real MB document usually carries it).
pub fn mb_track(position: i64, title: &str) -> crate::musicbrainz::MbTrack {
    crate::musicbrainz::MbTrack {
        position: Some(position),
        number: Some(position.to_string()),
        title: None,
        length: None,
        recording: Some(crate::musicbrainz::MbRecording {
            id: None,
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    }
}

/// One CD medium holding `tracks`, with no disc ids.
pub fn mb_medium(tracks: Vec<crate::musicbrainz::MbTrack>) -> crate::musicbrainz::MbMedium {
    crate::musicbrainz::MbMedium { discs: vec![], format: Some("CD".to_string()), tracks }
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
///     media: vec![test_releases::mb_medium(vec![test_releases::mb_track(1, "Only Track")])],
///     ..test_releases::mb_release(release_id, group_id, "Album Title")
/// }
/// ```
pub fn mb_release(
    release_id: &str,
    release_group_id: &str,
    title: &str,
) -> crate::musicbrainz::MbReleaseResponse {
    crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: title.to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        status: Some("Official".to_string()),
        packaging: None,
        barcode: None,
        artist_credit: vec![crate::musicbrainz::MbArtistCredit {
            joinphrase: None,
            name: "Artist Name".to_string(),
            artist: Some(crate::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![mb_medium(vec![mb_track(1, "Track One")])],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive { front: false, darkened: false },
    }
}

/// The id [`crate::fake_catalogs::FakeCatalogs::seed_discogs_test_release`]
/// renders for a fixture's own spelling of a release or master id.
///
/// Discogs' release endpoint numbers its ids, so a fixture that writes
/// `"master-exact"` is archived — and read back — under a number. A test that
/// asserts on the id it seeded asks for it here rather than hard-coding the
/// rendering.
pub fn discogs_fixture_id(fixture_id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fixture_id.hash(&mut hasher);
    // Kept inside 2^53 so the id survives any JSON reader that stores numbers
    // as doubles.
    (hasher.finish() % (1 << 53)).to_string()
}

/// One artist credit on a synthetic Discogs release.
pub fn discogs_artist(id: &str, name: &str) -> crate::discogs::DiscogsArtist {
    crate::discogs::DiscogsArtist {
        id: id.to_string(),
        name: name.to_string(),
        anv: None,
        join: None,
    }
}

/// One numbered track on a synthetic Discogs tracklist, with no credits of its
/// own and no sub-tracks.
pub fn discogs_track(position: &str, title: &str, duration: &str) -> crate::discogs::DiscogsTrack {
    crate::discogs::DiscogsTrack {
        type_: "track".to_string(),
        position: position.to_string(),
        title: title.to_string(),
        duration: Some(duration.to_string()),
        artists: vec![],
        extraartists: None,
        sub_tracks: vec![],
    }
}

/// A synthetic Discogs release for
/// [`crate::fake_catalogs::FakeCatalogs::seed_discogs_test_release`]: `tracks` are
/// (title, duration) pairs numbered from 1, credited to one "Artist Name"
/// artist, on a 2024 US "Test Label" pressing with no cover art and no master.
///
/// A fixture that differs in one of those names that field and takes the rest
/// from here:
///
/// ```ignore
/// DiscogsRelease {
///     master_id: Some("test-master".to_string()),
///     artists: vec![test_releases::discogs_artist("test-artist-1", "Test Artist")],
///     ..test_releases::discogs_test_release("cue-flac-test", "Test Album", &[("Track One", "0:10")])
/// }
/// ```
pub fn discogs_test_release(
    id: &str,
    title: &str,
    tracks: &[(&str, &str)],
) -> crate::discogs::DiscogsRelease {
    crate::discogs::DiscogsRelease {
        id: id.to_string(),
        title: title.to_string(),
        year: Some(2024),
        formats: vec![],
        country: Some("US".to_string()),
        labels: bae_types::pressing::ReleaseLabel::list([(Some("Test Label".to_string()), None)]),
        covers: vec![],
        barcodes: vec![],
        artists: vec![discogs_artist("discogs-artist-1", "Artist Name")],
        extraartists: Some(vec![]),
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(index, (title, duration))| {
                discogs_track(&format!("{}", index + 1), title, duration)
            })
            .collect(),
        master_id: None,
        companies: vec![],
        identifiers: vec![],
        notes: None,
    }
}

pub(crate) fn discogs_fixture_artist_ids(
    release: &crate::discogs::DiscogsRelease,
) -> std::collections::BTreeSet<String> {
    fn collect_track(
        track: &crate::discogs::DiscogsTrack,
        ids: &mut std::collections::BTreeSet<String>,
    ) {
        ids.extend(track.artists.iter().map(|artist| artist.id.clone()));
        ids.extend(track.extraartists.iter().flatten().filter_map(|artist| artist.id.clone()));
        for sub_track in &track.sub_tracks {
            collect_track(sub_track, ids);
        }
    }

    let mut ids = release
        .artists
        .iter()
        .map(|artist| artist.id.clone())
        .chain(release.extraartists.iter().flatten().filter_map(|artist| artist.id.clone()))
        .collect();
    for track in &release.tracklist {
        collect_track(track, &mut ids);
    }
    ids
}
