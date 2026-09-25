// ── Artist credits resolve against the library when the import commits ─────
//
// Two folders credit one artist: "Album One" is picked from a Discogs release
// that names "Artist Name" with its Discogs id, and "Album Two" carries only
// file tags naming "Artist Name". Whichever commits first, the library ends
// with one "Artist Name", holding the Discogs id.

/// The Discogs artist every `discogs_test_release` credits.
const DISCOGS_ARTIST_FIXTURE: &str = "discogs-artist-1";

/// A folder whose release is picked from a seeded Discogs release by "Artist
/// Name".
fn discogs_album_folder(f: &ImportFixture, dir_name: &str, title: &str) -> (std::path::PathBuf, MetadataProvenance) {
    let release = discogs_release(title, &["Track One"]);
    let release_key = seed_discogs_test_release(f.library_manager.providers(), release);
    let dir = f.temp_path().join(dir_name);
    fs::create_dir_all(&dir).unwrap();
    generate_album_files(&dir, &["01 Track One.flac"]);
    (dir, support::discogs_release(release_key))
}

/// A folder whose only metadata is its files' tags, crediting `artist`.
fn tagged_album_folder(f: &ImportFixture, dir_name: &str, title: &str, artist: &str) -> std::path::PathBuf {
    let dir = f.temp_path().join(dir_name);
    fs::create_dir_all(&dir).unwrap();
    generate_tagged_album_files(
        &dir,
        title,
        artist,
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );
    dir
}

async fn import_and_wait(
    f: &ImportFixture,
    dir: std::path::PathBuf,
    provenance: MetadataProvenance,
) -> (String, String) {
    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(&import_id, dir, provenance))
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    support::wait_for_import_complete(&mut progress_rx).await
}

/// Every library artist whose name folds to `name`'s.
async fn artists_named(f: &ImportFixture, name: &str) -> Vec<bae_core::db::DbArtist> {
    let fold = |text: &str| {
        use unicode_normalization::UnicodeNormalization;
        text.nfd()
            .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
            .collect::<String>()
            .to_lowercase()
    };
    f.db.search_artists("rtist", 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|artist| fold(&artist.name) == fold(name))
        .collect()
}

async fn album_artist_id(f: &ImportFixture, album_id: &str) -> String {
    f.db.find_album_by_id(album_id)
        .await
        .unwrap()
        .expect("the imported album")
        .artist_id
}

fn library_artist(name: &str, discogs_artist_id: Option<&str>) -> bae_core::db::DbArtist {
    bae_core::db::DbArtist {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: None,
        discogs_artist_id: discogs_artist_id.map(str::to_string),
        musicbrainz_artist_id: None,
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn a_tagged_album_after_a_catalog_album_joins_its_artist() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let (catalog_dir, catalog) = discogs_album_folder(&f, "album-one", "Album One");
    let tagged_dir = tagged_album_folder(&f, "album-two", "Album Two", "Artist Name");

    let (_, catalog_album) = import_and_wait(&f, catalog_dir, catalog).await;
    let (_, tagged_album) =
        import_and_wait(&f, tagged_dir, MetadataProvenance::FileMetadata).await;

    let artists = artists_named(&f, "Artist Name").await;
    assert_eq!(artists.len(), 1, "one artist, got {artists:?}");
    assert_eq!(
        artists[0].discogs_artist_id,
        Some(support::discogs_fixture_id(DISCOGS_ARTIST_FIXTURE))
    );
    assert_eq!(album_artist_id(&f, &catalog_album).await, artists[0].id);
    assert_eq!(album_artist_id(&f, &tagged_album).await, artists[0].id);
}

#[tokio::test]
async fn a_catalog_album_after_a_tagged_album_gives_its_artist_the_catalog_id() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let (catalog_dir, catalog) = discogs_album_folder(&f, "album-one", "Album One");
    let tagged_dir = tagged_album_folder(&f, "album-two", "Album Two", "Artist Name");

    let (_, tagged_album) =
        import_and_wait(&f, tagged_dir, MetadataProvenance::FileMetadata).await;
    let tagged_artist = album_artist_id(&f, &tagged_album).await;
    let (_, catalog_album) = import_and_wait(&f, catalog_dir, catalog).await;

    let artists = artists_named(&f, "Artist Name").await;
    assert_eq!(artists.len(), 1, "one artist, got {artists:?}");
    assert_eq!(artists[0].id, tagged_artist, "the artist keeps its own id");
    assert_eq!(
        artists[0].discogs_artist_id,
        Some(support::discogs_fixture_id(DISCOGS_ARTIST_FIXTURE))
    );
    assert_eq!(album_artist_id(&f, &catalog_album).await, tagged_artist);
}

#[tokio::test]
async fn two_albums_queued_together_make_one_artist() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let (catalog_dir, catalog) = discogs_album_folder(&f, "album-one", "Album One");
    let tagged_dir = tagged_album_folder(&f, "album-two", "Album Two", "Artist Name");

    let catalog_import = uuid::Uuid::new_v4().to_string();
    let tagged_import = uuid::Uuid::new_v4().to_string();
    let mut catalog_rx = f.handle.subscribe_import(catalog_import.clone());
    let mut tagged_rx = f.handle.subscribe_import(tagged_import.clone());
    f.handle
        .send_command(support::folder_import(&catalog_import, catalog_dir, catalog))
        .await
        .unwrap();
    f.handle
        .send_command(support::folder_import(
            &tagged_import,
            tagged_dir,
            MetadataProvenance::FileMetadata,
        ))
        .await
        .unwrap();
    let (_, catalog_album) = support::wait_for_import_complete(&mut catalog_rx).await;
    let (_, tagged_album) = support::wait_for_import_complete(&mut tagged_rx).await;

    let artists = artists_named(&f, "Artist Name").await;
    assert_eq!(artists.len(), 1, "one artist, got {artists:?}");
    assert_eq!(album_artist_id(&f, &catalog_album).await, artists[0].id);
    assert_eq!(album_artist_id(&f, &tagged_album).await, artists[0].id);
}

/// Two library artists share the name, and nothing in the tags tells which
/// one the folder means: the import makes a new artist rather than guess.
#[tokio::test]
async fn a_name_two_library_artists_share_imports_as_a_new_artist() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let first = library_artist("Artist Name", None);
    let second = library_artist("Artist Name", Some("discogs-other"));
    f.library_manager.insert_artist(&first).await.unwrap();
    f.library_manager.insert_artist(&second).await.unwrap();
    let tagged_dir = tagged_album_folder(&f, "album-two", "Album Two", "Artist Name");

    let (_, album) = import_and_wait(&f, tagged_dir, MetadataProvenance::FileMetadata).await;

    let album_artist = album_artist_id(&f, &album).await;
    assert_ne!(album_artist, first.id);
    assert_ne!(album_artist, second.id);
    assert_eq!(artists_named(&f, "Artist Name").await.len(), 3);
}

/// A library artist of the same name that holds a different Discogs id is a
/// different artist.
#[tokio::test]
async fn a_library_artist_with_another_discogs_id_is_not_the_credit() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let other = library_artist("Artist Name", Some("discogs-other"));
    f.library_manager.insert_artist(&other).await.unwrap();
    let (catalog_dir, catalog) = discogs_album_folder(&f, "album-one", "Album One");

    let (_, album) = import_and_wait(&f, catalog_dir, catalog).await;

    let album_artist = album_artist_id(&f, &album).await;
    assert_ne!(album_artist, other.id);
    let unchanged = f.db.find_artist_by_id(&other.id).await.unwrap().unwrap();
    assert_eq!(unchanged.discogs_artist_id.as_deref(), Some("discogs-other"));
    assert_eq!(artists_named(&f, "Artist Name").await.len(), 2);
}

#[tokio::test]
async fn a_credit_meets_its_artist_however_the_name_is_cased_or_accented() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let existing = library_artist("artist name", None);
    f.library_manager.insert_artist(&existing).await.unwrap();
    let accented_dir = tagged_album_folder(&f, "album-two", "Album Two", "Ärtist Name");
    let cased_dir = tagged_album_folder(&f, "album-three", "Album Three", "ARTIST NAME");

    let (_, accented) = import_and_wait(&f, accented_dir, MetadataProvenance::FileMetadata).await;
    let (_, cased) = import_and_wait(&f, cased_dir, MetadataProvenance::FileMetadata).await;

    assert_eq!(album_artist_id(&f, &accented).await, existing.id);
    assert_eq!(album_artist_id(&f, &cased).await, existing.id);
    assert_eq!(artists_named(&f, "Artist Name").await.len(), 1);
}
