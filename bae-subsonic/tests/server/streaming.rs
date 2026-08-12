// ──────────────────────────────────── media ─────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn stream_raw_serves_original_bytes_and_ranges() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let full = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=raw")),
    )
    .await;
    assert_eq!(full.status, StatusCode::OK);
    assert_eq!(full.content_type, "audio/flac");
    let total: u64 = full.content_length.as_ref().unwrap().parse().unwrap();
    assert_eq!(
        full.body.len() as u64,
        total,
        "full body matches Content-Length"
    );
    assert_eq!(&full.body[0..4], b"fLaC", "raw FLAC bytes");

    // A range request returns 206 with the right slice.
    let ranged = call_with_range(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=raw")),
        Some("bytes=0-9"),
    )
    .await;
    assert_eq!(ranged.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.body.len(), 10);
    assert_eq!(ranged.content_length.as_deref(), Some("10"));
    assert!(ranged.content_range.unwrap().starts_with("bytes 0-9/"));
    assert_eq!(&ranged.body, &full.body[0..10], "range slice matches");
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_transcode_mp3_is_chunked_and_decodes() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=128")),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.content_type, "audio/mpeg");
    // Chunked: no (bogus) Content-Length on a streaming transcode.
    assert!(
        resp.content_length.is_none(),
        "transcode must not send Content-Length"
    );
    assert!(
        resp.body.len() > 1000,
        "mp3 body present: {} bytes",
        resp.body.len()
    );

    // The bytes decode back with the source's channel count and duration.
    let song = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let expected_channels = song.sub()["song"]["channelCount"].as_i64().unwrap() as u32;
    let source_secs = song.sub()["song"]["duration"].as_i64().unwrap();
    let decoded = decode_bytes(&resp.body);
    assert_eq!(
        decoded.channels, expected_channels,
        "transcode preserves channels"
    );
    assert!(!decoded.samples.is_empty());
    let decoded_secs =
        (decoded.samples.len() as f64 / decoded.channels as f64) / decoded.sample_rate as f64;
    assert!(
        (decoded_secs - source_secs as f64).abs() <= 1.0,
        "decoded duration {decoded_secs:.2}s must match source {source_secs}s (±1s for codec delay)"
    );

    // maxBitRate has a bitrate-dependent effect: a CBR MP3 of fixed duration is
    // larger at a higher bitrate.
    let low = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=64")),
    )
    .await;
    let high = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=320")),
    )
    .await;
    assert!(
        high.body.len() > low.body.len(),
        "320kbps ({} bytes) must exceed 64kbps ({} bytes)",
        high.body.len(),
        low.body.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cover_art_resolves_every_id_kind() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    // Album id serves the release cover.
    let by_album = call(
        &router,
        "getCoverArt",
        &authed(&format!("id=al-{}", lib.per_track_release)),
    )
    .await;
    assert_eq!(by_album.status, StatusCode::OK);
    assert!(by_album.content_type.starts_with("image/"));
    assert!(!by_album.body.is_empty());

    // Track id resolves to its release's cover.
    let song_id = first_song_id(&router, &lib.per_track_release).await;
    let by_track = call(&router, "getCoverArt", &authed(&format!("id={song_id}"))).await;
    assert_eq!(by_track.status, StatusCode::OK);
    assert!(by_track.content_type.starts_with("image/"));

    // Artist id resolves to one of the artist's release covers.
    let album = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{}", lib.per_track_release)),
    )
    .await;
    let artist_id = album.sub()["album"]["artistId"]
        .as_str()
        .unwrap()
        .to_string();
    let by_artist = call(&router, "getCoverArt", &authed(&format!("id={artist_id}"))).await;
    assert_eq!(
        by_artist.status,
        StatusCode::OK,
        "artist id resolves to a release cover"
    );
    assert!(by_artist.content_type.starts_with("image/"));

    // The CUE release has no cover art → error 70.
    let missing = call(
        &router,
        "getCoverArt",
        &authed(&format!("id=al-{}", lib.cue_release)),
    )
    .await;
    assert!(
        missing.text().contains(r#"code="70""#),
        "missing art: {}",
        missing.text()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scrobble_is_an_accepted_no_op() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp = call(
        &router,
        "scrobble",
        &authed(&format!("f=json&id={song_id}")),
    )
    .await;
    assert_eq!(resp.sub()["status"], "ok");
}

// ─────────────────────────── musicBrainzId + lossy ─────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn musicbrainz_id_surfaces_when_present() {
    // Spec: ArtistID3 carries musicBrainzId when the artist has one. Seed an
    // artist with a MusicBrainz id and a minimal album/release, then assert both
    // getArtists and getArtist emit it.
    let (manager, _t) = new_manager().await;
    // coven takes only canonical UUIDs on a synced row, so the fixture ids are
    // UUIDs derived from readable monikers.
    let artist_id = bae_test_support::test_uuid("mb-artist-1");
    let artist = DbArtist {
        id: artist_id.clone(),
        name: "MB Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mbid-abc-123".to_string()),
        created_at: chrono::Utc::now(),
    };
    manager.insert_artist(&artist).await.unwrap();
    let now = chrono::Utc::now();
    let album = DbAlbum {
        id: bae_test_support::test_uuid("mb-album-1"),
        title: "MB Album".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2010),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: bae_test_support::test_uuid("mb-release-1"),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_core::db::Pressing::blank(),
        disc_id: None,
        metadata_source: bae_core::db::ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = DbTrack {
        id: bae_test_support::test_uuid("mb-track-1"),
        release_id: release.id.clone(),
        title: "MB Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    manager
        .insert_album_with_release_and_tracks(&album, &release, &[track], &[])
        .await
        .unwrap();

    let services = AppServices::for_test(manager).await.expect("app services");
    let router = bae_subsonic::router(services, credential());

    let artists = call(&router, "getArtists", &authed("f=json")).await;
    let sub = artists.sub();
    let indexes = sub["artists"]["index"].as_array().unwrap();
    let mb = find_artist(indexes, "MB Artist");
    assert_eq!(mb["musicBrainzId"], "mbid-abc-123", "getArtists emits mbid");

    let one = call(
        &router,
        "getArtist",
        &authed(&format!("f=json&id=ar-{artist_id}")),
    )
    .await;
    assert_eq!(
        one.sub()["artist"]["musicBrainzId"],
        "mbid-abc-123",
        "getArtist emits mbid"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lossy_track_reports_bit_depth_zero() {
    // Spec/OpenSubsonic: bitDepth is required on every Child; a lossy codec has
    // no fixed sample depth, so it reports 0. Import a lossy Opus track and assert
    // its Child bitDepth is 0, in contrast to the lossless bitDepth=16 case.
    // (AAC-in-MP4 is unsuitable: the MP4 sound sample entry always declares 16,
    // so FFmpeg reports a bit depth for it; Opus carries none.)
    let (services, release_id, _temps) = seed_lossy_release().await;
    let router = bae_subsonic::router(services, credential());

    let album = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{release_id}")),
    )
    .await;
    let song = &album.sub()["album"]["song"][0];
    assert_eq!(song["bitDepth"], 0, "a lossy track reports bitDepth 0");
    assert!(song["samplingRate"].as_i64().unwrap() > 0);
    assert!(song["channelCount"].as_i64().unwrap() > 0);

    let song_id = song["id"].as_str().unwrap().to_string();
    let one = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(
        one.sub()["song"]["bitDepth"],
        0,
        "getSong reports bitDepth 0 for lossy"
    );
}

/// Import the single-file lossy Opus fixture through the normal import path.
async fn seed_lossy_release() -> (AppServices, String, Vec<TempDir>) {
    let (manager, db_temp) = new_manager().await;
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("lossy album");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bae-core/test-fixtures/audio-format/placeholder-opus.opus");
    std::fs::copy(&fixture, dir.join("track.opus")).unwrap();

    let discogs_key = support::seed_discogs_test_release(DiscogsRelease {
        id: "test-lossy".to_string(),
        title: "Lossy Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-lossy-artist".to_string(),
            name: "Lossy Artist".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![cue_track("1", "Only Track")],
        master_id: None,
    });
    let import =
        support::start_test_import(tokio::runtime::Handle::current(), manager.clone()).await;
    let import_id = "lossy".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "lossy".to_string(),
            folder: dir,
            scope: ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(discogs_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = import.subscribe_import(import_id);
    let (release_id, _album) = support::wait_for_import_complete(&mut rx).await;
    let services = AppServices::for_test(manager).await.expect("app services");
    (services, release_id, vec![db_temp, temp])
}

async fn first_song_id(router: &Router, release_id: &str) -> String {
    let album = call(
        router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{release_id}")),
    )
    .await;
    album.sub()["album"]["song"][0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Decode encoded audio bytes by writing them to a temp file, filling a sparse
/// buffer through the local reader, and running the real decoder.
fn decode_bytes(bytes: &[u8]) -> bae_core::audio_codec::DecodedAudio {
    use bae_core::playback::data_source::{AudioDataReader, LocalReader};
    use bae_core::playback::sparse_buffer::create_sparse_buffer;
    use std::io::Write;

    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    let buffer = create_sparse_buffer(bytes.len() as u64);
    let reader = Box::new(LocalReader::new(file.path().to_str().unwrap()));
    reader.start_reading(buffer.clone(), Box::new(|_| {}));
    bae_core::audio_codec::decode_audio(buffer, None, None).expect("decode transcoded bytes")
}
