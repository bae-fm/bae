use super::*;

// ─────────────────────────── auth + envelope + system ───────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn auth_gate() {
    // Spec: token auth. valid → ok; wrong token → 40; missing s/t → 10; short
    // salt → 40; token hex is case-insensitive.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let ok = call(&router, "ping", &authed("")).await;
    assert!(ok.text().contains(r#"status="ok""#), "valid: {}", ok.text());

    let bad = call(&router, "ping", &format!("u={USER}&s={SALT}&t=deadbeef&v=1&c=t")).await;
    assert!(bad.text().contains(r#"code="40""#), "bad token: {}", bad.text());

    let missing_t = call(&router, "ping", &format!("u={USER}&s={SALT}&v=1&c=t")).await;
    assert!(missing_t.text().contains(r#"code="10""#), "missing t");
    let missing_s = call(&router, "ping", &format!("u={USER}&t=x&v=1&c=t")).await;
    assert!(missing_s.text().contains(r#"code="10""#), "missing s");

    let short =
        call(&router, "ping", &format!("u={USER}&s=abc&t={}&v=1&c=t", token(PASS, "abc"))).await;
    assert!(short.text().contains(r#"code="40""#), "short salt");

    let upper = call(
        &router,
        "ping",
        &format!("u={USER}&s={SALT}&t={}&v=1&c=t", token(PASS, SALT).to_uppercase()),
    )
    .await;
    assert!(upper.text().contains(r#"status="ok""#), "uppercase hex token");
}

#[tokio::test(flavor = "multi_thread")]
async fn envelope_formats() {
    // Spec: xml is the default; f=json yields JSON; f=jsonp wraps in the
    // callback; every envelope carries openSubsonic, version, and type.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let xml = call(&router, "ping", &authed("")).await;
    assert!(xml.content_type.contains("xml"));
    assert!(xml.text().contains(r#"openSubsonic="true""#));
    assert!(xml.text().contains(r#"version="1.16.1""#));
    assert!(xml.text().contains(r#"type="bae""#));

    let json = call(&router, "ping", &authed("f=json")).await;
    assert!(json.content_type.contains("json"));
    let sub = json.sub();
    assert_eq!(sub["status"], "ok");
    assert_eq!(sub["openSubsonic"], true);
    assert_eq!(sub["version"], "1.16.1");
    assert_eq!(sub["type"], "bae");

    let jsonp = call(&router, "ping", &authed("f=jsonp&callback=cb")).await;
    assert!(jsonp.content_type.contains("javascript"));
    let text = jsonp.text();
    assert!(text.starts_with("cb("), "jsonp wrap: {text}");
    assert!(text.trim_end().ends_with(");"), "jsonp wrap: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn system_endpoints() {
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let license = call(&router, "getLicense", &authed("f=json")).await;
    assert_eq!(license.sub()["license"]["valid"], true);

    let folders = call(&router, "getMusicFolders", &authed("f=json")).await;
    let list = &folders.sub()["musicFolders"]["musicFolder"];
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], 0);
    assert_eq!(list[0]["name"], "bae");
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_and_wrong_kind_ids_are_not_found() {
    // Spec: a malformed id, or a wrong-kind id to getAlbum, is error 70.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let malformed = call(&router, "getAlbum", &authed("id=not-namespaced")).await;
    assert!(malformed.text().contains(r#"code="70""#));

    let wrong_kind = call(&router, "getAlbum", &authed("id=ar-someartist")).await;
    assert!(wrong_kind.text().contains(r#"code="70""#));
}

// ───────────────────────────────── browse ───────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn get_artists_indexes_and_counts() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let resp = call(&router, "getArtists", &authed("f=json")).await;
    let artists = &resp.sub()["artists"];
    let indexes = artists["index"].as_array().unwrap();
    // Two artists in distinct buckets: "Artist Name" (A) and "Solo Artist" (S).
    let letters: Vec<&str> = indexes.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(letters.contains(&"A"), "buckets: {letters:?}");
    assert!(letters.contains(&"S"), "buckets: {letters:?}");

    // Each artist's albumCount is its release count (one release each here), and
    // a file-tags artist carries no musicBrainzId.
    let solo = find_artist(indexes, "Solo Artist");
    assert_eq!(solo["albumCount"], 1);
    assert!(solo.get("musicBrainzId").is_none(), "file-tags artist has no mbid");

    // getIndexes shares the body under the legacy payload name.
    let legacy = call(&router, "getIndexes", &authed("f=json")).await;
    assert!(legacy.sub()["indexes"]["index"].is_array());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_artist_lists_releases_as_albums() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    // Resolve the per-track release's artist id, then browse the artist.
    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let artist_id = album.sub()["album"]["artistId"].as_str().unwrap().to_string();

    let resp = call(&router, "getArtist", &authed(&format!("f=json&id={artist_id}"))).await;
    let artist = &resp.sub()["artist"];
    let albums = artist["album"].as_array().unwrap();
    assert_eq!(albums.len(), 1, "one release for this artist");
    assert!(albums[0]["id"].as_str().unwrap().starts_with("al-"));

    let unknown = call(&router, "getArtist", &authed("f=json&id=ar-nope")).await;
    assert!(unknown.text().contains(r#""code":70"#));
}

// ──────────────────────────────── album + song ──────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn get_album_lists_songs_with_required_child_fields() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let resp =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let album = &resp.sub()["album"];
    assert_eq!(album["songCount"], 2);
    let songs = album["song"].as_array().unwrap();
    assert_eq!(songs.len(), 2);
    for song in songs {
        // OpenSubsonic requires these on every Child.
        assert!(song["bitDepth"].is_number(), "bitDepth required");
        assert!(song["samplingRate"].as_i64().unwrap() > 0);
        assert!(song["channelCount"].as_i64().unwrap() > 0);
        // A lossless FLAC track reports its real depth, not 0.
        assert_eq!(song["bitDepth"], 16);
        assert!(song["id"].as_str().unwrap().starts_with("tr-"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_album_cue_lists_its_windowed_tracks() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let resp =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.cue_release))).await;
    let album = &resp.sub()["album"];
    assert_eq!(album["songCount"], 3, "CUE image has three tracks");
    let songs = album["song"].as_array().unwrap();
    for song in songs {
        assert_eq!(song["bitDepth"], 16);
        assert!(song["samplingRate"].as_i64().unwrap() > 0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_song_and_unknown() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let song_id = album.sub()["album"]["song"][0]["id"].as_str().unwrap().to_string();

    let resp = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let song = &resp.sub()["song"];
    assert_eq!(song["id"], song_id);
    assert!(song["bitDepth"].is_number());
    assert!(song["samplingRate"].as_i64().unwrap() > 0);
    assert!(song["channelCount"].as_i64().unwrap() > 0);

    let unknown = call(&router, "getSong", &authed("f=json&id=tr-missing")).await;
    assert!(unknown.text().contains(r#""code":70"#));
}

/// A release whose pressing states no year shows its album's, on the album
/// and on each of its songs alike: the per-track release's files name one
/// year, which is its album's.
#[tokio::test(flavor = "multi_thread")]
async fn a_song_shows_the_year_its_album_shows() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let album = &album.sub()["album"];
    assert_eq!(album["year"], 2001);
    let song = &album["song"][0];
    assert_eq!(song["year"], 2001, "getAlbum's song");

    let song_id = song["id"].as_str().unwrap().to_string();
    let one = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(one.sub()["song"]["year"], 2001, "getSong");

    let found = call(&router, "search3", &authed("f=json&query=First%20Track")).await;
    let hit = &found.sub()["searchResult3"]["song"][0];
    assert_eq!(hit["title"], "First Track");
    assert_eq!(hit["year"], 2001, "search3's song");
}

// ──────────────────────────────── lists + search ────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn album_list2_orderings_and_paging() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    let names = |resp: &Resp| -> Vec<String> {
        resp.sub()["albumList2"]["album"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect()
    };

    let alpha = call(&router, "getAlbumList2", &authed("f=json&type=alphabeticalByName")).await;
    let alpha_names = names(&alpha);
    assert_eq!(alpha_names.len(), 2);
    let mut sorted = alpha_names.clone();
    sorted.sort_by_key(|s| s.to_lowercase());
    assert_eq!(alpha_names, sorted, "alphabetical order");

    // newest orders by created_at, most-recent first. The CUE release imports
    // after the per-track one, so "Test Album" leads "Solo Album".
    let newest = call(&router, "getAlbumList2", &authed("f=json&type=newest")).await;
    assert_eq!(names(&newest), vec!["Test Album", "Solo Album"], "newest by created_at");

    // Paging: size=1 returns one, offset=1 returns the next.
    let first =
        call(&router, "getAlbumList2", &authed("f=json&type=alphabeticalByName&size=1")).await;
    assert_eq!(names(&first).len(), 1);
    let second =
        call(&router, "getAlbumList2", &authed("f=json&type=alphabeticalByName&size=1&offset=1"))
            .await;
    assert_eq!(names(&second).len(), 1);
    assert_ne!(names(&first)[0], names(&second)[0]);

    // byYear range filters (2001 for the per-track release; the CUE release is 2024).
    let by_year =
        call(&router, "getAlbumList2", &authed("f=json&type=byYear&fromYear=2000&toYear=2010"))
            .await;
    assert_eq!(names(&by_year).len(), 1, "only the 2001 release is in range");

    // byYear with toYear < fromYear reverses the order (descending by year):
    // both releases are in range, newest year first.
    let descending =
        call(&router, "getAlbumList2", &authed("f=json&type=byYear&fromYear=2024&toYear=2000"))
            .await;
    assert_eq!(
        names(&descending),
        vec!["Test Album", "Solo Album"],
        "byYear reversed is descending by year (2024 then 2001)"
    );

    // byGenre has no genre store → empty, not an error.
    let by_genre = call(&router, "getAlbumList2", &authed("f=json&type=byGenre&genre=Rock")).await;
    let genre_empty =
        by_genre.sub()["albumList2"].get("album").and_then(|v| v.as_array()).map_or(0, |a| a.len());
    assert_eq!(genre_empty, 0, "byGenre is empty");
    assert!(!by_genre.text().contains("\"error\""), "byGenre is not an error");

    // random returns `size` items.
    let random = call(&router, "getAlbumList2", &authed("f=json&type=random&size=1")).await;
    assert_eq!(names(&random).len(), 1);

    // Unsupported types return an empty list, not an error. An empty list omits
    // the `album` array key entirely (Subsonic JSON convention).
    let frequent = call(&router, "getAlbumList2", &authed("f=json&type=frequent")).await;
    let empty =
        frequent.sub()["albumList2"].get("album").and_then(|v| v.as_array()).map_or(0, |a| a.len());
    assert_eq!(empty, 0);
    assert!(!frequent.text().contains("\"error\""), "frequent is not an error");
}

#[tokio::test(flavor = "multi_thread")]
async fn search3_matches_and_caps() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    // A query matching the per-track album/artist.
    let hit = call(&router, "search3", &authed("f=json&query=Solo")).await;
    let result = &hit.sub()["searchResult3"];
    assert!(
        result["artist"].as_array().unwrap().iter().any(|a| a["name"] == "Solo Artist"),
        "artist hit"
    );
    assert!(
        result["album"].as_array().unwrap().iter().any(|a| a["name"] == "Solo Album"),
        "album hit"
    );

    // A query matching song titles returns song hits (the per-track titles are
    // "First Track"/"Second Track"; the CUE titles are "Track One/Two/Three").
    let song_hit = call(&router, "search3", &authed("f=json&query=Track")).await;
    let hit_songs = song_hit.sub()["searchResult3"]["song"].as_array().unwrap().len();
    assert!(hit_songs > 0, "song query returns song hits");

    // Per-kind caps: an empty query returns the whole library, capped by each
    // count. artistCount/albumCount cap their kinds independently.
    let capped =
        call(&router, "search3", &authed("f=json&query=&artistCount=1&albumCount=1&songCount=1"))
            .await;
    let capped = &capped.sub()["searchResult3"];
    assert_eq!(capped["artist"].as_array().unwrap().len(), 1, "artistCount caps artists");
    assert_eq!(capped["album"].as_array().unwrap().len(), 1, "albumCount caps albums");
    assert_eq!(capped["song"].as_array().unwrap().len(), 1, "songCount caps songs");

    // songOffset skips into the song list, so offset 1 differs from offset 0.
    let song_ids = |resp: &Resp| -> String {
        resp.sub()["searchResult3"]["song"][0]["id"].as_str().unwrap().to_string()
    };
    let first = call(&router, "search3", &authed("f=json&query=&songCount=1&songOffset=0")).await;
    let second = call(&router, "search3", &authed("f=json&query=&songCount=1&songOffset=1")).await;
    assert_ne!(song_ids(&first), song_ids(&second), "songOffset pages the song list");
}
