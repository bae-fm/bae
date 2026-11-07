use super::*;
use crate::test_fixtures::labels::{add_label, label, library, placed, read_labels, stored_label};
use bae_store::test_fixtures::{empty_db, fixed_now, row_count, temp_db_hearing_bugs};

const AT: &str = "2026-01-15T12:00:00+00:00";

fn artist(seed: &str, discogs: Option<&str>, musicbrainz: Option<&str>) -> DbArtist {
    DbArtist {
        id: bae_util::test_ids::test_uuid(seed),
        name: "Artist Name".to_string(),
        sort_name: None,
        discogs_artist_id: discogs.map(str::to_string),
        musicbrainz_artist_id: musicbrainz.map(str::to_string),
        created_at: fixed_now(),
    }
}

/// Merge `absorbed` into `survivor` in a write of its own, the survivor
/// taking `discogs-1`, `mb-1` and `sort_name`.
async fn merge_artist(
    store: &Store,
    survivor: &str,
    absorbed: &str,
    sort_name: Option<&str>,
) -> Result<(), DbError> {
    let (survivor, absorbed) = (survivor.to_string(), absorbed.to_string());
    let sort_name = sort_name.map(str::to_string);
    store
        .call(move |sql| {
            merge_artist_on(
                sql,
                &ArtistMerge {
                    survivor: &survivor,
                    absorbed: &absorbed,
                    discogs_artist_id: Some("discogs-1"),
                    musicbrainz_artist_id: Some("mb-1"),
                    sort_name: sort_name.as_deref(),
                    at: AT,
                },
            )
        })
        .await
}

/// The artist each of `table`'s merge rows was absorbed into, by absorbed id.
async fn merges(store: &Store, table: &'static str) -> Vec<(String, String)> {
    store
        .read(move |sql| {
            let column = if table == "artist_merges" { "into_artist_id" } else { "into_label_id" };
            Ok(sql.query(&format!("SELECT id, {column} FROM {table} ORDER BY id"), [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?)
        })
        .await
        .unwrap()
}

async fn store_artist_image(db: &Records, artist_id: &str, bytes: &[u8]) {
    let image = DbLibraryImage {
        id: artist_id.to_string(),
        image_type: LibraryImageType::Artist,
        content_type: ContentType::Jpeg,
    };
    db.write_library_image(&image, bytes).await.unwrap();
}

async fn artist_image_file(db: &Records, artist_id: &str) -> Option<coven::FileRef> {
    db.library_image_file(artist_id, &LibraryImageType::Artist).await.unwrap()
}

/// The survivor takes the absorbed artist's catalog ids and the sort name
/// it lacks; the absorbed artist stays, recorded as merged; and a credit
/// naming the absorbed artist stays as written and reads as the survivor.
#[tokio::test]
async fn the_survivor_takes_the_absorbed_artists_ids_and_its_credits_read_as_the_survivor() {
    let (db, _tmp) = empty_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let survivor = artist("survivor", Some("discogs-1"), None);
    let absorbed = artist("absorbed", None, Some("mb-1"));
    records.insert_artist(&survivor).await.unwrap();
    records.insert_artist(&absorbed).await.unwrap();
    let album = DbAlbum::new_test("Album Title");
    records.insert_album(&album).await.unwrap();
    records
        .insert_album_artist(&DbAlbumArtist::new(&album.id, &survivor.id, 0, fixed_now()))
        .await
        .unwrap();
    records
        .insert_album_artist(&DbAlbumArtist::new(&album.id, &absorbed.id, 1, fixed_now()))
        .await
        .unwrap();

    merge_artist(&db, &survivor.id, &absorbed.id, Some("Artist Name, The")).await.unwrap();

    let merged = records.find_artist_by_id(&survivor.id).await.unwrap().unwrap();
    assert_eq!(merged.discogs_artist_id.as_deref(), Some("discogs-1"));
    assert_eq!(merged.musicbrainz_artist_id.as_deref(), Some("mb-1"));
    assert_eq!(merged.sort_name.as_deref(), Some("Artist Name, The"));
    assert!(records.find_artist_by_id(&absorbed.id).await.unwrap().is_some());
    assert_eq!(merges(&db, "artist_merges").await, [(absorbed.id.clone(), survivor.id.clone())]);
    let album_id = album.id.clone();
    let written: Vec<String> = db
        .read(move |sql| {
            Ok(sql.query(
                "SELECT artist_id FROM album_artists WHERE album_id = ? ORDER BY position",
                [album_id],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(written, [survivor.id.clone(), absorbed.id.clone()], "credits stay as written");
    let shown = records.get_artists_for_album(&album.id).await.unwrap();
    assert_eq!(
        shown.into_iter().map(|credit| credit.artist.id).collect::<Vec<_>>(),
        [survivor.id.clone(), survivor.id.clone()]
    );
}

#[tokio::test]
async fn a_survivor_keeps_the_sort_name_it_has() {
    let (db, _tmp) = empty_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let mut survivor = artist("survivor", Some("discogs-1"), None);
    survivor.sort_name = Some("Name, Artist".to_string());
    let absorbed = artist("absorbed", None, Some("mb-1"));
    records.insert_artist(&survivor).await.unwrap();
    records.insert_artist(&absorbed).await.unwrap();

    merge_artist(&db, &survivor.id, &absorbed.id, Some("Artist Name, The")).await.unwrap();

    let merged = records.find_artist_by_id(&survivor.id).await.unwrap().unwrap();
    assert_eq!(merged.sort_name.as_deref(), Some("Name, Artist"));
}

/// A survivor with an image of its own keeps it; the absorbed artist's
/// image row goes.
#[tokio::test]
async fn a_survivor_with_an_image_keeps_it_and_the_absorbed_artists_goes() {
    let (db, _tmp) = empty_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let survivor = artist("survivor", Some("discogs-1"), None);
    let absorbed = artist("absorbed", None, Some("mb-1"));
    records.insert_artist(&survivor).await.unwrap();
    records.insert_artist(&absorbed).await.unwrap();
    store_artist_image(&records, &survivor.id, b"survivor").await;
    store_artist_image(&records, &absorbed.id, b"absorbed").await;
    let own = artist_image_file(&records, &survivor.id).await.unwrap().id();

    merge_artist(&db, &survivor.id, &absorbed.id, None).await.unwrap();

    let kept = artist_image_file(&records, &survivor.id).await.unwrap();
    assert_eq!(kept.id(), own);
    assert_eq!(db.read_file(&kept).await.unwrap(), b"survivor");
    assert!(records
        .find_library_image(&absorbed.id, &LibraryImageType::Artist)
        .await
        .unwrap()
        .is_none());
}

/// When only the absorbed artist has an image, the survivor takes it: its
/// row references the absorbed artist's coven file, which nothing stages or
/// uploads again. One write moves the reference, whichever artist's id
/// sorts first.
#[tokio::test]
async fn a_survivor_without_an_image_takes_the_absorbed_artists_file() {
    const LOW: &str = "00000000-0000-4000-8000-000000000001";
    const HIGH: &str = "00000000-0000-4000-8000-000000000002";
    for (survivor_id, absorbed_id) in [(LOW, HIGH), (HIGH, LOW)] {
        let (db, _tmp) = empty_db(crate::test_fixtures::TEST_SCHEMA).await;
        let records = crate::test_fixtures::test_records(&db);
        let mut survivor = artist("survivor", Some("discogs-1"), None);
        survivor.id = survivor_id.to_string();
        let mut absorbed = artist("absorbed", None, Some("mb-1"));
        absorbed.id = absorbed_id.to_string();
        records.insert_artist(&survivor).await.unwrap();
        records.insert_artist(&absorbed).await.unwrap();
        store_artist_image(&records, &absorbed.id, b"absorbed").await;
        let file = artist_image_file(&records, &absorbed.id).await.unwrap().id();

        merge_artist(&db, &survivor.id, &absorbed.id, None).await.unwrap_or_else(|error| {
            panic!("survivor {survivor_id}, absorbed {absorbed_id}: {error}")
        });

        let taken = artist_image_file(&records, &survivor.id).await.unwrap();
        assert_eq!(taken.id(), file);
        assert_eq!(db.read_file(&taken).await.unwrap(), b"absorbed");
        assert_eq!(
            db.queued_upload_count_in_namespace_for_test(crate::synced::ARTIST_IMAGES_NAMESPACE)
                .await
                .unwrap(),
            1,
            "the merge stages no second copy"
        );
        assert!(records
            .find_library_image(&absorbed.id, &LibraryImageType::Artist)
            .await
            .unwrap()
            .is_none());
    }
}

/// An artist merged earlier into the one now absorbed is then recorded as
/// merged into the survivor, so its credits read as the survivor directly.
#[tokio::test]
async fn a_merge_chain_ending_at_the_absorbed_artist_ends_at_the_survivor() {
    let (db, _tmp) = empty_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let earlier = artist("earlier", None, None);
    let absorbed = artist("absorbed", None, Some("mb-1"));
    let survivor = artist("survivor", Some("discogs-1"), None);
    for artist in [&earlier, &absorbed, &survivor] {
        records.insert_artist(artist).await.unwrap();
    }
    let (earlier_id, absorbed_id) = (earlier.id.clone(), absorbed.id.clone());
    db.call(move |sql| {
        merge_artist_on(
            sql,
            &ArtistMerge {
                survivor: &absorbed_id,
                absorbed: &earlier_id,
                discogs_artist_id: Some("discogs-earlier"),
                musicbrainz_artist_id: Some("mb-1"),
                sort_name: None,
                at: AT,
            },
        )
    })
    .await
    .unwrap();

    merge_artist(&db, &survivor.id, &absorbed.id, None).await.unwrap();

    let mut expected =
        vec![(earlier.id.clone(), survivor.id.clone()), (absorbed.id.clone(), survivor.id.clone())];
    expected.sort();
    assert_eq!(merges(&db, "artist_merges").await, expected);
}

/// The one bug `heard` was reported, which `failed` carries back from the
/// panic that failed its write: its message.
fn reported_bug(heard: &coven::task::HeardBugs, failed: &DbError) -> String {
    let reported = heard.take();
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert_eq!(coven::task::CarriesBug::bug(failed), Some(&reported[0]));
    reported[0].message.clone()
}

/// A merge's survivor is an artist its write just read, and no write deletes
/// an artist, so a merge naming no artist is a bug of the code that built
/// it: the write fails with that bug and records no merge.
#[tokio::test]
async fn an_artist_merge_into_a_missing_survivor_is_a_bug() {
    let (db, _tmp, heard) = temp_db_hearing_bugs(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let absorbed = artist("absorbed", None, Some("mb-1"));
    records.insert_artist(&absorbed).await.unwrap();
    let missing = bae_util::test_ids::test_uuid("missing");

    let failed = merge_artist(&db, &missing, &absorbed.id, None).await.unwrap_err();

    let bug = reported_bug(&heard, &failed);
    assert!(bug.contains("exactly its surviving artist"), "{bug}");
    assert_eq!(row_count(&db, "artist_merges").await, 0);
}

/// Merge `absorbed` into `survivor` in a write of its own, the survivor
/// taking `7190` and `mb-label`.
async fn merge_label(store: &Store, survivor: &str, absorbed: &str) -> Result<(), DbError> {
    let (survivor, absorbed) = (survivor.to_string(), absorbed.to_string());
    store
        .call(move |sql| {
            merge_label_on(
                sql,
                &LabelMerge {
                    survivor: &survivor,
                    absorbed: &absorbed,
                    discogs_label_id: Some("7190"),
                    musicbrainz_label_id: Some("mb-label"),
                    at: AT,
                },
            )
        })
        .await
}

/// The surviving label takes the catalog id it lacks and keeps its own; the
/// absorbed label stays, recorded as merged; and a release naming the
/// absorbed label keeps naming it and reads the survivor.
#[tokio::test]
async fn the_surviving_label_takes_the_absorbed_ones_id_and_its_releases_read_the_survivor() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let by_discogs = add_label(&db, "Label Name", None, Some("7190")).await;
    let by_musicbrainz = add_label(&db, "Other Label Name", Some("mb-label"), None).await;
    crate::test_fixtures::labels::write(
        &db,
        vec![crate::test_fixtures::labels::with_ids(
            label(Some("Other Label Name"), None),
            Some("mb-label"),
            None,
        )],
    )
    .await
    .unwrap();

    merge_label(&db, &by_discogs, &by_musicbrainz).await.unwrap();

    let survivor = stored_label(&db, &by_discogs).await;
    assert_eq!(survivor.musicbrainz_label_id.as_deref(), Some("mb-label"));
    assert_eq!(survivor.discogs_label_id.as_deref(), Some("7190"));
    assert_eq!(stored_label(&db, &by_musicbrainz).await.id, by_musicbrainz);
    assert_eq!(merges(&db, "label_merges").await, [(by_musicbrainz.clone(), by_discogs.clone())]);
    assert_eq!(placed(&db).await[0].0, Some(by_musicbrainz), "stays as written");
    assert_eq!(read_labels(&records).await, vec![label(Some("Label Name"), None)]);
}

#[tokio::test]
async fn a_label_merge_chain_ending_at_the_absorbed_label_ends_at_the_survivor() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let earlier = add_label(&db, "Label Name First", None, None).await;
    let by_musicbrainz = add_label(&db, "Label Name Second", Some("mb-label"), None).await;
    let by_discogs = add_label(&db, "Label Name Third", None, Some("7190")).await;
    merge_label(&db, &by_musicbrainz, &earlier).await.unwrap();

    merge_label(&db, &by_discogs, &by_musicbrainz).await.unwrap();

    let mut expected = vec![(by_musicbrainz, by_discogs.clone()), (earlier, by_discogs)];
    expected.sort();
    assert_eq!(merges(&db, "label_merges").await, expected);
}

/// A merge's survivor is a label its write just read, and no write deletes a
/// label, so a merge naming no label is a bug of the code that built it.
#[tokio::test]
async fn a_label_merge_into_a_missing_survivor_is_a_bug() {
    let (db, _tmp, heard) = temp_db_hearing_bugs(crate::test_fixtures::TEST_SCHEMA).await;
    let by_musicbrainz = add_label(&db, "Label Name", Some("mb-label"), None).await;

    let failed = merge_label(&db, "missing", &by_musicbrainz).await.unwrap_err();

    let bug = reported_bug(&heard, &failed);
    assert!(bug.contains("exactly its surviving label"), "{bug}");
    assert_eq!(row_count(&db, "label_merges").await, 0);
}
