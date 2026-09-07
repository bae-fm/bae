//! Synthetic Discogs releases, and seeding them into the caches an import
//! reads instead of the network.

/// The id [`seed_discogs_test_release`] renders for a fixture's own spelling of
/// a release or master id.
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
pub fn discogs_artist(id: &str, name: &str) -> bae_core::discogs::DiscogsArtist {
    bae_core::discogs::DiscogsArtist {
        id: id.to_string(),
        name: name.to_string(),
    }
}

/// One numbered track on a synthetic Discogs tracklist, with no credits of its
/// own and no sub-tracks.
pub fn discogs_track(
    position: &str,
    title: &str,
    duration: &str,
) -> bae_core::discogs::DiscogsTrack {
    bae_core::discogs::DiscogsTrack {
        type_: "track".to_string(),
        position: position.to_string(),
        title: title.to_string(),
        duration: Some(duration.to_string()),
        artists: vec![],
        extraartists: None,
        sub_tracks: vec![],
    }
}

/// A synthetic Discogs release for [`seed_discogs_test_release`]: `tracks` are
/// (title, duration) pairs numbered from 1, credited to one "Artist Name"
/// artist, on a 2024 US "Test Label" pressing with no cover art and no master.
///
/// A fixture that differs in one of those names that field and takes the rest
/// from here:
///
/// ```ignore
/// DiscogsRelease {
///     master_id: Some("test-master".to_string()),
///     artists: vec![support::discogs_artist("test-artist-1", "Test Artist")],
///     ..support::discogs_test_release("cue-flac-test", "Test Album", &[("Track One", "0:10")])
/// }
/// ```
pub fn discogs_test_release(
    id: &str,
    title: &str,
    tracks: &[(&str, &str)],
) -> bae_core::discogs::DiscogsRelease {
    bae_core::discogs::DiscogsRelease {
        id: id.to_string(),
        title: title.to_string(),
        year: Some(2024),
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        covers: vec![],
        catno: None,
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
    }
}

fn discogs_fixture_artist_ids(
    release: &bae_core::discogs::DiscogsRelease,
) -> std::collections::BTreeSet<String> {
    fn collect_track(
        track: &bae_core::discogs::DiscogsTrack,
        ids: &mut std::collections::BTreeSet<String>,
    ) {
        ids.extend(track.artists.iter().map(|artist| artist.id.clone()));
        ids.extend(
            track
                .extraartists
                .iter()
                .flatten()
                .filter_map(|artist| artist.id.clone()),
        );
        for sub_track in &track.sub_tracks {
            collect_track(sub_track, ids);
        }
    }

    let mut ids = release
        .artists
        .iter()
        .map(|artist| artist.id.clone())
        .chain(
            release
                .extraartists
                .iter()
                .flatten()
                .filter_map(|artist| artist.id.clone()),
        )
        .collect();
    for track in &release.tracklist {
        collect_track(track, &mut ids);
    }
    ids
}

/// Pre-populate the Discogs release cache, master cache (if the release carries
/// a `master_id`), and the MB URL-lookup cache for a synthetic test release, and
/// return the release id the import should pick. The worker's
/// `prepare_release` → `client.get_release` chain hits the cache; the
/// cross-reference call resolves to "no MB link" without touching the network.
///
/// `release` describes what the test wants; this renders it as the release
/// endpoint's own JSON and reads it back through the production parser, so the
/// two halves of the cache entry cannot disagree. That matters because the raw
/// JSON is what the import archives, and every later projection of the release
/// — a reset, a re-open — replays from the archived bytes rather than from
/// whatever a test handed over alongside them.
///
/// The rendered ids are numeric, as the endpoint's are, so the returned id is
/// not the (arbitrary) one the caller wrote on the fixture.
pub fn seed_discogs_test_release(release: bae_core::discogs::DiscogsRelease) -> String {
    let numeric = |value: &str| -> u64 {
        discogs_fixture_id(value)
            .parse()
            .expect("a rendered Discogs id is numeric")
    };
    let credit = |artist: &bae_core::discogs::DiscogsArtist| serde_json::json!({ "id": numeric(&artist.id), "name": artist.name });
    let role_credit = |artist: &bae_core::discogs::DiscogsRoleArtist| {
        serde_json::json!({
            "id": artist.id.as_deref().map(numeric),
            "name": artist.name,
            "role": artist.role,
            "anv": artist.credited_name,
        })
    };
    let master_id = release.master_id.as_deref().map(numeric);

    for artist_id in discogs_fixture_artist_ids(&release) {
        bae_core::discogs::client::seed_artist_image_response(
            &numeric(&artist_id).to_string(),
            None,
        );
    }

    if let Some(master_id) = master_id {
        // Keyed by the rendered id, which is the one the parsed release names
        // and therefore the one the master fetch asks for.
        let master_json = serde_json::json!({ "id": master_id, "year": release.year });
        bae_core::discogs::client::seed_master_cache(
            &master_id.to_string(),
            release.year,
            master_json.to_string(),
        );
    }

    let raw_json = serde_json::json!({
        "id": numeric(&release.id),
        "title": release.title,
        "year": release.year,
        "country": release.country,
        "master_id": master_id,
        "formats": release.format.iter().map(|name| serde_json::json!({ "name": name })).collect::<Vec<_>>(),
        "labels": release.label.iter().enumerate().map(|(index, name)| serde_json::json!({
            "name": name,
            "catno": if index == 0 { release.catno.clone() } else { None },
        })).collect::<Vec<_>>(),
        "images": release.covers.iter().enumerate().map(|(index, cover)| serde_json::json!({
            "type": if index == 0 { "primary" } else { "secondary" },
            "uri": cover.url,
            "uri150": cover.thumbnail_url,
        })).collect::<Vec<_>>(),
        "artists": release.artists.iter().map(credit).collect::<Vec<_>>(),
        "extraartists": release.extraartists.as_ref().map(|artists| {
            artists.iter().map(role_credit).collect::<Vec<_>>()
        }),
        "tracklist": release.tracklist.iter().map(|track| serde_json::json!({
            "position": track.position,
            "title": track.title,
            "duration": track.duration,
            "type_": track.type_,
            "artists": track.artists.iter().map(credit).collect::<Vec<_>>(),
            "extraartists": track.extraartists.as_ref().map(|artists| {
                artists.iter().map(role_credit).collect::<Vec<_>>()
            }),
        })).collect::<Vec<_>>(),
    })
    .to_string();

    let parsed = bae_core::discogs::client::parse_discogs_release_json(&raw_json)
        .expect("the rendered test release parses as the endpoint's own JSON");
    let id = parsed.id.clone();
    bae_core::discogs::client::seed_release_cache(&id, (parsed, raw_json));
    bae_core::musicbrainz::seed_discogs_url_lookup(&id, None);
    id
}
