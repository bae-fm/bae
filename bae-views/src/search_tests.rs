use super::*;

#[test]
fn blank_and_whitespace_are_not_searches() {
    assert_eq!(LibrarySearchQuery::parse(""), None);
    assert_eq!(LibrarySearchQuery::parse("   "), None);
    assert_eq!(LibrarySearchQuery::parse("\t \n"), None);
}

#[test]
fn a_query_is_trimmed() {
    let query = LibrarySearchQuery::parse("  Album Title  ").expect("non-blank");
    assert_eq!(query.as_str(), "Album Title");
}

#[test]
fn interior_whitespace_is_preserved() {
    let query = LibrarySearchQuery::parse("dark side").expect("non-blank");
    assert_eq!(query.as_str(), "dark side");
}

mod stored_title_keys {
    //! The track and work search entries hold their titles' sort keys,
    //! written by the triggers that write the titles, so a search orders its
    //! tracks and works by them.

    use crate::*;
    use bae_records::test_fixtures::library::RecordsFixture;
    use bae_records::test_fixtures::test_records;
    use bae_store::test_fixtures::temp_db;

    const ARTIST: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a01";
    const ALBUM: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a02";
    const RELEASE: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a03";
    const TRACK_OE: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a11";
    const TRACK_Z: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a12";
    const TRACK_AA: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a13";
    const WORK_OE: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a21";
    const WORK_Z: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a22";
    const WORK_AA: &str = "5d0f7c84-2b1e-4c36-9a51-0e7b6f3c2a23";

    /// Three tracks and three works, titled so that the root order and
    /// Swedish order them differently.
    fn titled_library() -> RecordsFixture {
        RecordsFixture::new()
            .artist(ARTIST, "Artist Name")
            .album(ALBUM, "Album Title", Some(2026), &[ARTIST])
            .release(ALBUM, RELEASE)
            .tracks(
                RELEASE,
                &[(TRACK_OE, "Öl Track"), (TRACK_Z, "Zebra Track"), (TRACK_AA, "Åke Track")],
            )
            .work(WORK_OE, "Öl Work")
            .work(WORK_Z, "Zebra Work")
            .work(WORK_AA, "Åke Work")
    }

    /// How many track and work search entries hold a title key other than
    /// their title's in the device's language, and how many entries there
    /// are of each.
    async fn stale_keys(db: &Store) -> [i64; 4] {
        db.read(|sql| {
            Ok(sql.query_row(
                "SELECT (SELECT COUNT(*) FROM track_search_entry e JOIN tracks t ON t.id = e.track_id
                         WHERE e.title_sort_key
                               IS NOT bae_name_key(t.title, (SELECT language FROM name_sort))),
                        (SELECT COUNT(*) FROM track_search_entry),
                        (SELECT COUNT(*) FROM work_search_entry e JOIN works w ON w.id = e.work_id
                         WHERE e.title_sort_key
                               IS NOT bae_name_key(w.title, (SELECT language FROM name_sort))),
                        (SELECT COUNT(*) FROM work_search_entry)",
                [],
                |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?]),
            )?)
        })
        .await
        .unwrap()
    }

    /// The ids of the tracks and of the works a search for `query` lists, in
    /// its order.
    async fn found(db: &Store, query: &str) -> (Vec<String>, Vec<String>) {
        let results = LibraryViews::new(db.clone()).search_rows(query, 10).await.unwrap();
        (
            results.tracks.into_iter().map(|track| track.id).collect(),
            results.works.into_iter().map(|work| work.work.id).collect(),
        )
    }

    #[tokio::test]
    async fn a_written_title_writes_its_key_and_a_language_change_writes_it_again() {
        let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
        titled_library().write(&test_records(&db)).await;
        assert_eq!(stale_keys(&db).await, [0, 3, 0, 3], "each entry holds its title's key");
        assert_eq!(found(&db, "track").await.0, [TRACK_AA, TRACK_OE, TRACK_Z]);
        assert_eq!(found(&db, "work").await.1, [WORK_AA, WORK_OE, WORK_Z]);

        test_records(&db).set_sort_language("sv").await.unwrap();

        assert_eq!(stale_keys(&db).await, [0, 3, 0, 3], "each key is the new language's");
        assert_eq!(
            found(&db, "track").await.0,
            [TRACK_Z, TRACK_AA, TRACK_OE],
            "Swedish puts å and ö after z"
        );
        assert_eq!(found(&db, "work").await.1, [WORK_Z, WORK_AA, WORK_OE]);
    }

    #[tokio::test]
    async fn a_renamed_track_moves_by_its_new_key() {
        let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
        titled_library().write(&test_records(&db)).await;

        test_records(&db).rename_track_for_test(TRACK_Z, "Abacus Track").await.unwrap();

        assert_eq!(stale_keys(&db).await, [0, 3, 0, 3]);
        assert_eq!(found(&db, "track").await.0, [TRACK_Z, TRACK_AA, TRACK_OE]);
    }

    /// No edit on this device retitles a work, so the work takes another
    /// device's later title as sync applies it.
    #[test]
    fn a_retitled_work_moves_by_its_new_key() {
        bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
            let db = devices.a();
            devices.pause().await;
            titled_library().write(&test_records(db)).await;
            RecordsFixture::new()
                .work(WORK_Z, "Abacus Work")
                .write(&test_records(devices.b()))
                .await;
            devices.resume().await;
            devices
                .settle("the later title", async |_, db| {
                    db.read(|sql| {
                        Ok(sql.query_row(
                            "SELECT title FROM works WHERE id = ?1",
                            [WORK_Z],
                            |row| row.get::<_, String>(0),
                        )?)
                    })
                    .await
                    .unwrap()
                        == "Abacus Work"
                })
                .await;

            assert_eq!(stale_keys(db).await, [0, 3, 0, 3]);
            assert_eq!(found(db, "work").await.1, [WORK_Z, WORK_AA, WORK_OE]);
        });
    }
}
