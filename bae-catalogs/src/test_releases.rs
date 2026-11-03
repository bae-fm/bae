//! Synthetic MusicBrainz and Discogs releases a test builds its fixtures
//! from, for [`FakeCatalogs`] to answer with.

use crate::fake_catalogs::FakeCatalogs;

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

/// The id [`FakeCatalogs::seed_discogs_test_release`]
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
/// [`FakeCatalogs::seed_discogs_test_release`]: `tracks` are
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

/// A credit to the MusicBrainz artist `id`, printed as `name`.
pub fn mb_credit(id: &str, name: &str) -> crate::musicbrainz::MbArtistCredit {
    crate::musicbrainz::MbArtistCredit {
        joinphrase: None,
        name: name.to_string(),
        artist: Some(crate::musicbrainz::MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: Some(name.to_string()),
        }),
    }
}

/// One track on a synthetic MusicBrainz medium printed `number` (a side's
/// `A1`), with no position of its own.
pub fn mb_printed_track(number: &str, title: &str) -> crate::musicbrainz::MbTrack {
    crate::musicbrainz::MbTrack {
        position: None,
        number: Some(number.to_string()),
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

/// A 2-side vinyl release credited to "Artist Name": A1, A2 on side 1; B1 on
/// side 2, pressed in 1969 on "Label" (CAT-1, US).
pub fn mb_vinyl_release(release_id: &str) -> crate::musicbrainz::MbReleaseResponse {
    crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        disambiguation: None,
        annotation: None,
        date: Some("1969".to_string()),
        country: Some("US".to_string()),
        status: None,
        packaging: None,
        barcode: None,
        artist_credit: vec![mb_credit("mb-artist-1", "Artist Name")],
        release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
            id: "rg-1".to_string(),
            first_release_date: Some("1969".to_string()),
            relations: None,
        }),
        label_info: vec![crate::musicbrainz::MbLabelInfo {
            catalog_number: Some("CAT-1".to_string()),
            label: Some(crate::musicbrainz::MbLabel { id: None, name: Some("Label".to_string()) }),
        }],
        media: vec![crate::musicbrainz::MbMedium {
            discs: vec![],
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                mb_printed_track("A1", "A1 title"),
                mb_printed_track("A2", "A2 title"),
                mb_printed_track("B1", "B1 title"),
            ],
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive { front: false, darkened: false },
    }
}

/// What a reading of the MusicBrainz release `response` alone reads: its
/// editable values.
pub fn mb_release_values(
    response: &crate::musicbrainz::MbReleaseResponse,
) -> crate::release_values::ReleaseValues {
    crate::payloads::ReleasePayloads::for_test(
        crate::records::LookupRef::new(crate::catalog::LookupCatalog::MusicBrainz, &response.id),
        serde_json::to_string(response).expect("MusicBrainz fixture serializes"),
        Vec::new(),
    )
    .extract()
    .unwrap()
    .every_medium_values()
    .expect("synthetic MB response reads")
}

// The draft holds every value the import writes; nothing reads the source
// again after a source is applied.

/// A MusicBrainz release of `titles.len()` tracks on one CD, each stating its
/// length, credited to its own composer and performing its own work, the
/// composers' and works' ids starting `credits`.
pub fn seed_mb_release_with_works(
    fakes: &FakeCatalogs,
    release_id: &str,
    credits: &str,
    titles: &[&str],
    lengths_ms: &[u64],
) {
    let tracks: Vec<_> = titles
        .iter()
        .zip(lengths_ms)
        .enumerate()
        .map(|(index, (title, length))| {
            let number = index + 1;
            serde_json::json!({
                "position": number,
                "number": number.to_string(),
                "length": length,
                "recording": {
                    "title": title,
                    "relations": [
                        {
                            "target-type": "artist",
                            "type": "composer",
                            "artist": {
                                "id": format!("{credits}-composer-{number}"),
                                "name": format!("Composer Name {number}"),
                            },
                        },
                        {
                            "target-type": "work",
                            "type": "performance",
                            "work": {
                                "id": format!("{credits}-work-{number}"),
                                "title": format!("Work Title {number}"),
                            },
                        },
                    ],
                },
            })
        })
        .collect();
    fakes.seed_musicbrainz_release(
        release_id,
        serde_json::json!({
            "id": release_id,
            "title": "Album Title",
            "artist-credit": [{
                "name": "Artist Name",
                "artist": { "id": "mb-artist-1", "name": "Artist Name" },
            }],
            "media": [{ "format": "CD", "tracks": tracks }],
            "cover-art-archive": { "front": false, "darkened": false },
        })
        .to_string(),
    );
}

/// A two-track Discogs release with no master, seeded into the release cache.
pub fn seed_two_track_discogs_release(fakes: &FakeCatalogs, release_id: &str) {
    seed_two_track_discogs_release_in(fakes, release_id, None, None);
}

/// A two-track Discogs release filed under `master`, when it names one,
/// seeded into the release cache; the master's own document is seeded beside
/// it, and MusicBrainz's answer about both addresses: the release's linked
/// from `linked`, when it names a MusicBrainz release, and the master's from
/// no release group.
pub fn seed_two_track_discogs_release_in(
    fakes: &FakeCatalogs,
    release_id: &str,
    master: Option<&str>,
    linked: Option<&str>,
) {
    if let Some(master) = master {
        fakes.seed_discogs_master(
            master,
            serde_json::json!({ "id": master.parse::<u64>().unwrap(), "title": "Album Title" })
                .to_string(),
        );
    }
    let raw_release = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "master_id": master.map(|master| master.parse::<u64>().unwrap()),
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }, {
            "position": "2", "title": "Track Two", "duration": "0:01", "type_": "track", "artists": []
        }],
    })
    .to_string();
    crate::discogs::documents::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    fakes.seed_discogs_release(release_id, raw_release);
    fakes.seed_discogs_artist_image("1", None);
    let lookups: Vec<_> =
        std::iter::once((crate::musicbrainz::DiscogsPage::Release(release_id.to_string()), linked))
            .chain(
                master.map(|master| {
                    (crate::musicbrainz::DiscogsPage::Master(master.to_string()), None)
                }),
            )
            .collect();
    fakes.seed_discogs_page_lookup(&lookups);
}

/// A two-track MusicBrainz release, seeded into the caches the fetch path
/// reads, so a partner resolves without a network call.
pub fn seed_two_track_mb_release(fakes: &FakeCatalogs, release_id: &str, release_group_id: &str) {
    fakes.seed_musicbrainz_release(
        release_id,
        two_track_mb_release_json(release_id, release_group_id),
    );
    fakes.seed_musicbrainz_release_group(
        release_group_id,
        serde_json::json!({ "id": release_group_id }).to_string(),
    );
}

/// The release endpoint's document for [`seed_two_track_mb_release`]'s release.
pub fn two_track_mb_release_json(release_id: &str, release_group_id: &str) -> String {
    serde_json::to_string(&two_track_mb_release(release_id, release_group_id))
        .expect("the test response serializes")
}

/// A two-track MusicBrainz release in `release_group_id`, both tracks titled
/// "Track One", with no release status.
pub fn two_track_mb_release(
    release_id: &str,
    release_group_id: &str,
) -> crate::musicbrainz::MbReleaseResponse {
    crate::musicbrainz::MbReleaseResponse {
        status: None,
        media: vec![mb_medium((1..=2).map(|number| mb_track(number, "Track One")).collect())],
        ..mb_release(release_id, release_group_id, "Album Title")
    }
}

/// A Discogs release with pressing metadata and no barcode.
pub fn seed_discogs_pressing(
    fakes: &FakeCatalogs,
    release_id: &str,
    label: &str,
    catalog_number: &str,
) {
    seed_discogs_pressing_printed_with(fakes, release_id, label, catalog_number, &[]);
}

/// [`seed_discogs_pressing`], printed with `barcodes`.
pub fn seed_discogs_pressing_printed_with(
    fakes: &FakeCatalogs,
    release_id: &str,
    label: &str,
    catalog_number: &str,
    barcodes: &[&str],
) {
    let raw_release = serde_json::json!({
        "identifiers": barcodes
            .iter()
            .map(|value| serde_json::json!({ "type": "Barcode", "value": value }))
            .collect::<Vec<_>>(),
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "country": "US",
        "labels": [{ "name": label, "catno": catalog_number }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }, {
            "position": "2", "title": "Track Two", "duration": "0:01", "type_": "track", "artists": []
        }],
    })
    .to_string();
    crate::discogs::documents::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    fakes.seed_discogs_release(release_id, raw_release);
    fakes.seed_discogs_artist_image("1", None);
    fakes.seed_discogs_page_lookup(&[(
        crate::musicbrainz::DiscogsPage::Release(release_id.to_string()),
        None,
    )]);
}

/// A two-track MusicBrainz release carrying `catalog_number`, in no release
/// group so the pick fetches no cover.
pub fn seed_mb_release_with_catalog(fakes: &FakeCatalogs, release_id: &str, catalog_number: &str) {
    seed_labelled_mb_release(fakes, release_id, catalog_number, false);
}

/// A two-track MusicBrainz release in no release group, on "Label Name" as
/// `catalog_number`, the Cover Art Archive stating a front image when `front`.
fn seed_labelled_mb_release(
    fakes: &FakeCatalogs,
    release_id: &str,
    catalog_number: &str,
    front: bool,
) {
    let response = crate::musicbrainz::MbReleaseResponse {
        release_group: None,
        label_info: vec![crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                id: None,
                name: Some("Label Name".to_string()),
            }),
            catalog_number: Some(catalog_number.to_string()),
        }],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive { front, darkened: false },
        ..two_track_mb_release(release_id, "")
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    fakes.seed_musicbrainz_release(release_id, raw_json);
}

/// [`seed_mb_release_with_catalog`]'s release, with the Cover Art Archive
/// stating a front image for it.
pub fn seed_mb_release_with_front(fakes: &FakeCatalogs, release_id: &str, catalog_number: &str) {
    seed_labelled_mb_release(fakes, release_id, catalog_number, true);
}

/// A MusicBrainz release as the disc-ID and release-lookup endpoints return it: one medium
/// whose tracks carry `length`, which is what makes the disc-ID path free.
pub fn mb_release_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    let tracks: Vec<String> = track_lengths
        .iter()
        .enumerate()
        .map(|(i, length)| {
            format!(
                r#"{{"position":{},"number":"{}","title":"Track {}","length":{length}}}"#,
                i + 1,
                i + 1,
                i + 1
            )
        })
        .collect();
    format!(
        r#"{{"id":"{release_id}","title":"Album","artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[{}]}}],"relations":[],
            "cover-art-archive":{{"front":false,"darkened":false}}}}"#,
        tracks.join(",")
    )
}

/// A MusicBrainz search hit as `ws/2/release?query=…` returns it: no `media`, hence no
/// count, so the auto-import check has nothing to check until the lead is settled.
pub fn mb_search_json(release_id: &str, group_id: &str) -> String {
    format!(
        r#"{{"releases":[{{"id":"{release_id}","title":"Album",
            "artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},"label-info":[]}}]}}"#
    )
}

/// The same, for hits that state the barcode they were found by — what pairs
/// a MusicBrainz release with the Discogs record of the same pressing, and
/// what tells two different pressings apart.
pub fn mb_barcode_search_json(releases: &[(&str, &str, &str)]) -> String {
    mb_barcode_search_json_placed(
        &releases
            .iter()
            .map(|(release_id, group_id, barcode)| (*release_id, *group_id, *barcode, ""))
            .collect::<Vec<_>>(),
    )
}

/// The same, each release stating the country it was released in; an empty
/// country states none.
pub fn mb_barcode_search_json_placed(releases: &[(&str, &str, &str, &str)]) -> String {
    let releases: Vec<String> = releases
        .iter()
        .map(|(release_id, group_id, barcode, country)| {
            format!(
                r#"{{"id":"{release_id}","title":"Album",
                    "artist-credit":[{{"name":"Artist"}}],
                    "release-group":{{"id":"{group_id}"}},
                    "country":"{country}",
                    "barcode":"{barcode}","label-info":[]}}"#
            )
        })
        .collect();
    format!(r#"{{"releases":[{}]}}"#, releases.join(","))
}

/// A Discogs search hit as `database/search?barcode=…` returns it. The title
/// is the "Artist - Album" form Discogs answers with, which is what decides
/// whether this lands on the same album card as the MusicBrainz result.
pub fn discogs_search_json(release_id: &str, barcode: &str) -> String {
    format!(
        r#"{{"results":[{{"id":{release_id},"type":"release",
            "title":"Artist - Album","year":"1996","barcode":["{barcode}"]}}]}}"#
    )
}

/// A one-track Discogs release with no master, as `releases/{id}` returns it.
pub fn discogs_release_json(release_id: &str) -> String {
    serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist" }],
        "tracklist": [{
            "position": "1",
            "title": "Track 1",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string()
}

/// A MusicBrainz release under a title and artist of its own — for a test that has to tell
/// two releases apart by what a row shows.
pub fn mb_titled_release_json(
    release_id: &str,
    group_id: &str,
    title: &str,
    artist: &str,
) -> String {
    format!(
        r#"{{"id":"{release_id}","title":"{title}",
            "artist-credit":[{{"name":"{artist}"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[
                {{"position":1,"number":"1","title":"Track Title 1","length":180000}},
                {{"position":2,"number":"2","title":"Track Title 2","length":180000}}
            ]}}],"relations":[],
            "cover-art-archive":{{"front":true,"darkened":false}}}}"#
    )
}

/// [`mb_release_json`] for a release that carries `catalog_number`.
pub fn mb_release_json_with_catalog(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    catalog_number: &str,
) -> String {
    let mut release: serde_json::Value =
        serde_json::from_str(&mb_release_json(release_id, group_id, track_lengths))
            .expect("release fixture parses");
    release["label-info"] = serde_json::json!([{ "catalog-number": catalog_number }]);
    release.to_string()
}
