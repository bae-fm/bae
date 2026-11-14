use super::*;

// ──────────────────────────────────── media ─────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn stream_raw_serves_original_bytes_and_ranges() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let full = call(&router, "stream", &authed(&format!("id={song_id}&format=raw"))).await;
    assert_eq!(full.status, StatusCode::OK);
    assert_eq!(full.content_type, "audio/flac");
    let total: u64 = full.content_length.as_ref().unwrap().parse().unwrap();
    assert_eq!(full.body.len() as u64, total, "full body matches Content-Length");
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

/// A CUE image's track asked for unconverted is served as its own stream, not
/// the image: its samples from the first one the track holds (track 2's
/// INDEX 00 at 0:08) to its last (track 3's INDEX 01 at 0:20), as uncompressed
/// PCM at the image's depth, since no stored file holds exactly those samples.
/// The song describes that stream rather than the image.
#[tokio::test(flavor = "multi_thread")]
async fn stream_raw_serves_a_cue_track_as_its_own_pcm() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.cue_release))).await;
    let song = album.sub()["album"]["song"][1].clone();
    assert_eq!(song["title"], "Track Two (White Noise)");
    let song_id = song["id"].as_str().unwrap();
    assert_eq!(song["contentType"], "audio/wav", "{song}");
    assert_eq!(song["suffix"], "wav", "{song}");
    assert!(song.get("size").is_none(), "the stream's size is not known: {song}");

    let served = call(&router, "stream", &authed(&format!("id={song_id}&format=raw"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.content_type, "audio/wav");
    assert!(served.content_length.is_none(), "the stream's length is not known before it ends");
    let wav = bae_audio::parse_streamed_wav(&served.body).expect("a WAV stream");
    let image = decode_bytes(&std::fs::read(cue_fixture("Test Album.flac")).unwrap());
    assert_eq!(wav.sample_rate, image.sample_rate);
    assert_eq!(wav.channels, image.channels);
    assert_eq!(wav.bits_per_sample, 16, "the image's depth");

    let rate = image.sample_rate as usize;
    let channels = image.channels as usize;
    let expected: Vec<u8> = image.samples[8 * rate * channels..20 * rate * channels]
        .iter()
        .flat_map(|&sample| ((sample >> 16) as i16).to_le_bytes())
        .collect();
    assert_eq!(wav.data.len(), expected.len(), "12 s: the track's INDEX 00 pregap, then its 10 s");
    assert!(wav.data == expected, "the served PCM is the image's samples from 0:08 to 0:20");
}

/// `format=wav` — what a Cast device set to lossless conversion fetches — is
/// the track's stream as uncompressed PCM at its stored depth: a WAV whose
/// RIFF and data sizes say "unknown" (the length isn't known until the encode
/// ends, so nothing claims one), whose `fmt ` chunk matches the song, and whose
/// data is exactly the stored file's decoded samples, every frame of them.
#[tokio::test(flavor = "multi_thread")]
async fn stream_wav_is_the_tracks_pcm_in_an_unsized_wav() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let served = call(&router, "stream", &authed(&format!("id={song_id}&format=wav"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.content_type, "audio/wav");
    assert!(served.content_length.is_none(), "no length is claimed");
    let wav = bae_audio::parse_streamed_wav(&served.body).expect("a WAV stream");
    assert_eq!(wav.riff_size, u32::MAX, "RIFF size: unknown");
    assert_eq!(wav.data_size, u32::MAX, "data size: unknown");
    assert_eq!(&served.body[36..40], b"data", "data follows fmt directly");

    let song = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let song = &song.sub()["song"];
    assert_eq!(i64::from(wav.channels), song["channelCount"].as_i64().unwrap());
    assert_eq!(i64::from(wav.sample_rate), song["samplingRate"].as_i64().unwrap());
    assert_eq!(i64::from(wav.bits_per_sample), song["bitDepth"].as_i64().unwrap());

    let raw = call(&router, "stream", &authed(&format!("id={song_id}&format=raw"))).await;
    let stored = decode_bytes(&raw.body);
    let expected: Vec<u8> =
        stored.samples.iter().flat_map(|&sample| ((sample >> 16) as i16).to_le_bytes()).collect();
    assert_eq!(
        wav.data.len(),
        expected.len(),
        "the data holds every frame of the track, and nothing else"
    );
    assert!(wav.data == expected, "the data is the track's own samples");
}

/// A track whose original file is gone from this device is refused with
/// core's reason before the response commits to a status and length, whether
/// it is asked for as stored or transcoded; the release's other tracks, whose
/// originals are still there, keep streaming.
#[tokio::test(flavor = "multi_thread")]
async fn a_track_whose_original_is_gone_is_refused_with_the_reason() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let songs = album.sub()["album"]["song"].clone();
    assert_eq!(songs[0]["title"], "First Track");
    let gone = songs[0]["id"].as_str().unwrap().to_string();
    let kept = songs[1]["id"].as_str().unwrap().to_string();
    std::fs::remove_file(lib.per_track_source.join("01.flac")).unwrap();

    for format in ["raw", "mp3", "wav"] {
        let refused =
            call(&router, "stream", &authed(&format!("f=json&id={gone}&format={format}"))).await;
        assert_eq!(refused.status, StatusCode::OK, "{format}");
        assert!(refused.content_type.contains("json"), "{format}");
        let sub = refused.sub();
        assert_eq!(sub["status"], "failed", "{format}: {sub}");
        assert_eq!(
            sub["error"]["message"],
            bae_track_audio::TrackReadError::OriginalMissing.to_string(),
            "{format}: {sub}"
        );
    }

    let served = call(&router, "stream", &authed(&format!("id={kept}&format=raw"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(&served.body[0..4], b"fLaC", "the other track's original streams");
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_transcode_mp3_is_chunked_and_decodes() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp =
        call(&router, "stream", &authed(&format!("id={song_id}&format=mp3&maxBitRate=128"))).await;
    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.content_type, "audio/mpeg");
    // Chunked: no (bogus) Content-Length on a streaming transcode.
    assert!(resp.content_length.is_none(), "transcode must not send Content-Length");
    assert!(resp.body.len() > 1000, "mp3 body present: {} bytes", resp.body.len());

    // The bytes decode back with the source's channel count and duration.
    let song = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let expected_channels = song.sub()["song"]["channelCount"].as_i64().unwrap() as u32;
    let source_secs = song.sub()["song"]["duration"].as_i64().unwrap();
    let decoded = decode_bytes(&resp.body);
    assert_eq!(decoded.channels, expected_channels, "transcode preserves channels");
    assert!(!decoded.samples.is_empty());
    let decoded_secs =
        (decoded.samples.len() as f64 / decoded.channels as f64) / decoded.sample_rate as f64;
    assert!(
        (decoded_secs - source_secs as f64).abs() <= 1.0,
        "decoded duration {decoded_secs:.2}s must match source {source_secs}s (±1s for codec delay)"
    );

    // maxBitRate has a bitrate-dependent effect: a CBR MP3 of fixed duration is
    // larger at a higher bitrate.
    let low =
        call(&router, "stream", &authed(&format!("id={song_id}&format=mp3&maxBitRate=64"))).await;
    let high =
        call(&router, "stream", &authed(&format!("id={song_id}&format=mp3&maxBitRate=320"))).await;
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
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );

    // Album id serves the release cover.
    let by_album =
        call(&router, "getCoverArt", &authed(&format!("id=al-{}", lib.per_track_release))).await;
    assert_eq!(by_album.status, StatusCode::OK);
    assert!(by_album.content_type.starts_with("image/"));
    assert!(!by_album.body.is_empty());

    // Track id resolves to its release's cover.
    let song_id = first_song_id(&router, &lib.per_track_release).await;
    let by_track = call(&router, "getCoverArt", &authed(&format!("id={song_id}"))).await;
    assert_eq!(by_track.status, StatusCode::OK);
    assert!(by_track.content_type.starts_with("image/"));

    // An artist's art is the artist's own image; this artist has none, so
    // its id finds nothing rather than one of its release covers.
    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let artist_id = album.sub()["album"]["artistId"].as_str().unwrap().to_string();
    let by_artist = call(&router, "getCoverArt", &authed(&format!("id={artist_id}"))).await;
    assert!(
        by_artist.text().contains(r#"code="70""#),
        "an artist with no image has no art: {}",
        by_artist.text()
    );

    // The CUE release has no cover art → error 70.
    let missing =
        call(&router, "getCoverArt", &authed(&format!("id=al-{}", lib.cue_release))).await;
    assert!(missing.text().contains(r#"code="70""#), "missing art: {}", missing.text());
}

/// A `coverArt` id carries the image's version, so a client that caches art
/// by id fetches it again once the cover changes; the new id serves the art.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_cover_gets_a_new_cover_art_id() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let album_query = authed(&format!("f=json&id=al-{}", lib.per_track_release));
    let cover_art = |album: &Resp| {
        album.sub()["album"]["coverArt"].as_str().expect("the release has a cover").to_string()
    };
    let before = cover_art(&call(&router, "getAlbum", &album_query).await);

    lib.library
        .manager()
        .store_library_image(
            &bae_records::DbLibraryImage {
                id: lib.per_track_release.clone(),
                image_type: bae_records::LibraryImageType::Cover,
                content_type: bae_util::content_type::ContentType::Png,
            },
            &bae_catalogs::test_image_host::cover_png(),
        )
        .await
        .unwrap();

    let after = cover_art(&call(&router, "getAlbum", &album_query).await);
    assert_ne!(before, after, "the replaced cover has a new id");
    let served = call(&router, "getCoverArt", &authed(&format!("id={after}"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.body, bae_catalogs::test_image_host::cover_png());
}

/// An artist's `coverArt` names the artist's own image, and serves it.
#[tokio::test(flavor = "multi_thread")]
async fn an_artist_cover_art_is_the_artist_image() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let album =
        call(&router, "getAlbum", &authed(&format!("f=json&id=al-{}", lib.per_track_release)))
            .await;
    let artist_wire_id = album.sub()["album"]["artistId"].as_str().unwrap().to_string();
    let artist_id = artist_wire_id.strip_prefix("ar-").unwrap().to_string();
    let image = bae_catalogs::test_image_host::cover_png();
    lib.library
        .manager()
        .store_library_image(
            &bae_records::DbLibraryImage {
                id: artist_id,
                image_type: bae_records::LibraryImageType::Artist,
                content_type: bae_util::content_type::ContentType::Png,
            },
            &image,
        )
        .await
        .unwrap();

    let artist = call(&router, "getArtist", &authed(&format!("f=json&id={artist_wire_id}"))).await;
    let cover_art =
        artist.sub()["artist"]["coverArt"].as_str().expect("the artist has an image").to_string();
    let served = call(&router, "getCoverArt", &authed(&format!("id={cover_art}"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.body, image, "the artist's own image, not a release cover");
}

#[tokio::test(flavor = "multi_thread")]
async fn scrobble_is_an_accepted_no_op() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(
        lib.services.clone(),
        credential(),
        bae_diagnostics::Diagnostics::noop(),
    );
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp = call(&router, "scrobble", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(resp.sub()["status"], "ok");
}

// ─────────────────────────── musicBrainzId + lossy ─────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn musicbrainz_id_surfaces_when_present() {
    // Spec: ArtistID3 carries musicBrainzId when the artist has one. Seed an
    // artist with a MusicBrainz id and a minimal album/release, then assert both
    // getArtists and getArtist emit it.
    let (library, _t) = new_library().await;
    // coven takes only canonical UUIDs on a synced row, so the fixture ids are
    // UUIDs derived from readable monikers.
    let artist_id = bae_util::test_ids::test_uuid("mb-artist-1");
    let artist = DbArtist {
        id: artist_id.clone(),
        name: "MB Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mbid-abc-123".to_string()),
        created_at: chrono::Utc::now(),
    };
    library.manager().insert_artist(&artist).await.unwrap();
    let now = chrono::Utc::now();
    let album = DbAlbum {
        id: bae_util::test_ids::test_uuid("mb-album-1"),
        title: "MB Album".to_string(),
        year: Some(2010),
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: bae_util::test_ids::test_uuid("mb-release-1"),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_types::pressing::Pressing::blank(),
        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let medium = bae_records::DbReleaseMedium::new(
        &coven::UuidIds,
        &release.id,
        std::num::NonZeroU16::MIN,
        bae_types::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd)),
        now,
    );
    let track = DbTrack {
        id: bae_util::test_ids::test_uuid("mb-track-1"),
        release_id: release.id.clone(),
        title: "MB Track".to_string(),
        medium_id: medium.id.clone(),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::MIN,
        duration_ms: 1000,
        created_at: now,
    };
    library
        .manager()
        .insert_album_with_release_and_tracks(
            &album,
            &[bae_records::DbAlbumArtist::new_test(&album.id, &artist.id)],
            &release,
            &[medium],
            &[track],
            &[],
        )
        .await
        .unwrap();

    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let artists = call(&router, "getArtists", &authed("f=json")).await;
    let sub = artists.sub();
    let indexes = sub["artists"]["index"].as_array().unwrap();
    let mb = find_artist(indexes, "MB Artist");
    assert_eq!(mb["musicBrainzId"], "mbid-abc-123", "getArtists emits mbid");

    let one = call(&router, "getArtist", &authed(&format!("f=json&id=ar-{artist_id}"))).await;
    assert_eq!(one.sub()["artist"]["musicBrainzId"], "mbid-abc-123", "getArtist emits mbid");

    let group = bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-group");
    let pressing = bae_catalogs::records::MetadataRef::new(Catalog::MusicBrainz, "mb-pressing");
    for (record, expected) in [
        (bae_catalogs::records::ReleaseRecord::album(&group), None),
        (
            bae_catalogs::records::ReleaseRecord::new(&pressing, Some(group.key.clone())),
            Some(pressing.key.as_str()),
        ),
    ] {
        library.manager().set_records(&release.id, vec![record]).await.unwrap();
        let response =
            call(&router, "getAlbumList2", &authed("f=json&type=alphabeticalByName")).await;
        assert_eq!(
            response.sub()["albumList2"]["album"][0]["musicBrainzId"].as_str(),
            expected,
            "a Subsonic album names only a known MusicBrainz pressing, never its group"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lossy_track_reports_bit_depth_zero() {
    // Spec/OpenSubsonic: bitDepth is required on every Child; a lossy codec has
    // no fixed sample depth, so it reports 0. Import a lossy Opus track and assert
    // its Child bitDepth is 0, in contrast to the lossless bitDepth=16 case.
    // (AAC-in-MP4 is unsuitable: the MP4 sound sample entry always declares 16,
    // so FFmpeg reports a bit depth for it; Opus carries none.)
    let (services, release_id, _temps) = seed_lossy_release().await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let album = call(&router, "getAlbum", &authed(&format!("f=json&id=al-{release_id}"))).await;
    let song = &album.sub()["album"]["song"][0];
    assert_eq!(song["bitDepth"], 0, "a lossy track reports bitDepth 0");
    assert!(song["samplingRate"].as_i64().unwrap() > 0);
    assert!(song["channelCount"].as_i64().unwrap() > 0);

    let song_id = song["id"].as_str().unwrap().to_string();
    let one = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(one.sub()["song"]["bitDepth"], 0, "getSong reports bitDepth 0 for lossy");
}

/// Import the single-file lossy Opus fixture through the normal import path.
async fn seed_lossy_release() -> (AppServices, String, Vec<TempDir>) {
    seed_lossy_release_within(coven::FileBudget::for_test()).await
}

/// [`seed_lossy_release`] over a library that opens the person's files within
/// `files`, a budget the test sizes and watches.
async fn seed_lossy_release_within(
    files: coven::FileBudget,
) -> (AppServices, String, Vec<TempDir>) {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, _album_dir, db_temp) =
        bae_library::test_libraries::setup_test_library_with_album_dir_within(
            fakes.providers(),
            bae_util::http::Http::for_test(),
            files,
        )
        .await;
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("lossy album");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = bae_audio::test_fixtures::audio_format_dir().join("placeholder-opus.opus");
    std::fs::copy(&fixture, dir.join("track.opus")).unwrap();

    let discogs_key = fakes.seed_discogs_test_release(DiscogsRelease {
        country: None,
        labels: vec![],
        artists: vec![bae_catalogs::test_releases::discogs_artist(
            "discogs-lossy-artist",
            "Lossy Artist",
        )],
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-lossy",
            "Lossy Album",
            &[("Only Track", "0:10")],
        )
    });
    let import = library.start_discogs_import().await;
    let import_id = "lossy".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: bae_import::CandidateKey::from(b"lossy".to_vec()),
            source: CandidateSource {
                path: dir,
                scope: ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            selected_cover: None,
            destination: ImportDestination::ThisDevice,
            read_file_tags: false,
            pick: Some(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_key,
                ),
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = import.subscribe_import(import_id);
    let (release_id, _album) = bae_import::test_imports::wait_for_import_complete(&mut rx).await;
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    (services, release_id, vec![db_temp, temp])
}

async fn first_song_id(router: &Router, release_id: &str) -> String {
    let album = call(router, "getAlbum", &authed(&format!("f=json&id=al-{release_id}"))).await;
    album.sub()["album"]["song"][0]["id"].as_str().unwrap().to_string()
}

/// Decode encoded audio bytes by writing them to a temp file, filling a sparse
/// buffer through the local reader, and running the real decoder.
fn decode_bytes(bytes: &[u8]) -> bae_audio::DecodedAudio {
    use bae_audio::sparse_buffer::create_sparse_buffer;
    use bae_track_audio::data_source::{AudioDataReader, LocalReader};
    use std::io::Write;

    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    let buffer = create_sparse_buffer(bytes.len() as u64);
    let reader = Box::new(LocalReader::new(
        file.path().to_str().unwrap(),
        bae_diagnostics::Diagnostics::noop(),
        bae_util::test_files::lease(),
    ));
    reader.start_reading(buffer.clone(), Box::new(|_| {}));
    bae_audio::decode_audio(buffer, None, None).expect("decode transcoded bytes")
}

// ─────────────────────────── credit lines ───────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn an_album_prints_its_credit_line_and_lists_every_credited_artist() {
    // OpenSubsonic: `artist` is the display line, `artists` every credited
    // artist; `artistId` stays the protocol's one id, the first credit's.
    let (library, _t) = new_library().await;
    let now = chrono::Utc::now();
    let artist_a = bae_util::test_ids::test_uuid("credit-artist-a");
    let artist_b = bae_util::test_ids::test_uuid("credit-artist-b");
    for (id, name) in [(&artist_a, "Artist A"), (&artist_b, "Artist B")] {
        library
            .manager()
            .insert_artist(&DbArtist {
                id: id.clone(),
                name: name.to_string(),
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: now,
            })
            .await
            .unwrap();
    }
    let album = DbAlbum {
        id: bae_util::test_ids::test_uuid("credit-album"),
        title: "Credited Album".to_string(),
        year: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: bae_util::test_ids::test_uuid("credit-release"),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_types::pressing::Pressing::blank(),
        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let credits = [
        bae_records::DbAlbumArtist {
            join_phrase: Some(" & ".to_string()),
            ..bae_records::DbAlbumArtist::new(&coven::UuidIds, &album.id, &artist_a, 0, now)
        },
        bae_records::DbAlbumArtist {
            credit: Some(bae_records::credits::CreditName {
                key: "b alias".to_string(),
                name: "B Alias".to_string(),
            }),
            ..bae_records::DbAlbumArtist::new(&coven::UuidIds, &album.id, &artist_b, 1, now)
        },
    ];
    library
        .manager()
        .insert_album_with_release_and_tracks(&album, &credits, &release, &[], &[], &[])
        .await
        .unwrap();
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());

    let response = call(&router, "getAlbumList2", &authed("f=json&type=alphabeticalByName")).await;

    let sub = response.sub();
    let listed = &sub["albumList2"]["album"][0];
    assert_eq!(listed["artist"], "Artist A & B Alias");
    assert_eq!(listed["artistId"], format!("ar-{artist_a}"));
    let artists: Vec<(&str, &str)> = listed["artists"]
        .as_array()
        .expect("every credited artist")
        .iter()
        .map(|artist| (artist["id"].as_str().unwrap(), artist["name"].as_str().unwrap()))
        .collect();
    assert_eq!(
        artists,
        [
            (format!("ar-{artist_a}").as_str(), "Artist A"),
            (format!("ar-{artist_b}").as_str(), "Artist B"),
        ]
    );
}

/// A track stored as a WAV file is served as stored under the type of its
/// container, `audio/wav`, and its song says so: the codec it holds (PCM)
/// names no type a client plays.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_wav_is_served_as_audio_wav() {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, _db_temp) = bae_library::test_libraries::setup_test_library_with(
        fakes.providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let source = TempDir::new().unwrap();
    let dir = source.path().join("wav album");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        bae_audio::test_fixtures::audio_format_dir().join("placeholder-pcm.wav"),
        dir.join("01.wav"),
    )
    .unwrap();
    let discogs_key =
        fakes.seed_discogs_test_release(bae_catalogs::test_releases::discogs_test_release(
            "test-wav",
            "WAV Album",
            &[("Only Track", "0:00")],
        ));
    let import = library.start_discogs_import().await;
    import
        .send_command(ImportCommand {
            import_id: "wav".to_string(),
            candidate_key: bae_import::CandidateKey::from(b"wav".to_vec()),
            source: CandidateSource {
                path: dir,
                scope: ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            selected_cover: None,
            destination: ImportDestination::ThisDevice,
            pick: Some(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_key,
                ),
                partners: vec![],
            }),
            read_file_tags: false,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = import.subscribe_import("wav".to_string());
    let (release, _album) = bae_import::test_imports::wait_for_import_complete(&mut rx).await;
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());
    let song_id = first_song_id(&router, &release).await;

    let song = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(song.sub()["song"]["contentType"], "audio/wav", "{}", song.sub());
    let served = call(&router, "stream", &authed(&format!("id={song_id}&format=raw"))).await;
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.content_type, "audio/wav");
    assert_eq!(&served.body[0..4], b"RIFF", "the stored WAV's own bytes");
}

/// A stream request takes its places of the budget when it opens the track's
/// files: with the only place of a budget of one held, the request waits, and
/// it is answered once the place goes back, for the original bytes and for a
/// transcode alike. The places are back once each response has ended.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_request_holds_its_places_until_its_response_ends() {
    let budget = coven::FileBudget::new(std::num::NonZeroUsize::MIN);
    let watch = budget.watch();
    let (services, release_id, _temps) = seed_lossy_release_within(budget.clone()).await;
    let router = bae_subsonic::router(services, credential(), bae_diagnostics::Diagnostics::noop());
    let song_id = first_song_id(&router, &release_id).await;

    for format in ["raw", "mp3"] {
        let held = budget.lease(1).await.unwrap();
        let waited = watch.lease_waits_from_now(1);
        let query = authed(&format!("id={song_id}&format={format}"));
        let request = call(&router, "stream", &query);
        tokio::pin!(request);
        tokio::select! {
            _ = &mut request => panic!("the {format} request opened a file without a place"),
            () = waited => {}
        }
        drop(held);

        let response = request.await;
        assert_eq!(response.status, StatusCode::OK, "{format}");
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            while watch.holding() != 0 || watch.leased() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("every place is back once the response has ended");
    }
}
