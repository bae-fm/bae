use super::*;
use crate::test_fixtures::labels::*;

async fn label_count(store: &Store) -> i64 {
    store
        .read(|sql| Ok(sql.query_row("SELECT count(*) FROM labels", [], |row| row.get(0))?))
        .await
        .unwrap()
}

/// A catalog id decides: the library label holding it is the label, whatever
/// it is called, and it takes the other catalog's id the label carries; a
/// number-only label names no library label.
#[tokio::test]
async fn a_label_is_the_library_label_holding_its_catalog_id() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let held = add_label(&db, "Label Name Held", None, Some("7190")).await;

    write(
        &db,
        vec![
            with_ids(
                label(Some("Label Name Credited"), Some("R2 1")),
                Some("mb-credited"),
                Some("7190"),
            ),
            label(None, Some("XY 3")),
        ],
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(ids_of(&rows), [Some(held.as_str()), None]);
    assert_eq!(rows[1].1.as_deref(), Some("XY 3"));
    let held = stored_label(&db, &held).await;
    assert_eq!(held.name, "Label Name Held");
    assert_eq!(held.musicbrainz_label_id.as_deref(), Some("mb-credited"));
    assert_eq!(label_count(&db).await, 1);
    assert_eq!(
        read_labels(&records).await,
        vec![label(Some("Label Name Held"), Some("R2 1")), label(None, Some("XY 3"))]
    );
}

/// A label carrying no id is the one library label of its name key, trade
/// words aside; two of one key, or one holding a contradicting id, leave it
/// new.
#[tokio::test]
async fn a_label_without_ids_is_the_one_library_label_of_its_name_key() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let alpha = add_label(&db, "Label Name Alpha Records", None, None).await;
    let held = [
        alpha.clone(),
        add_label(&db, "Label Name Beta", None, Some("1866")).await,
        add_label(&db, "Label Name Beta Records", None, Some("5320")).await,
        add_label(&db, "Label Name Gamma", None, Some("10")).await,
    ];

    write(
        &db,
        vec![
            label(Some("LABEL NAME ALPHA"), None),
            label(Some("Label Name Beta"), None),
            with_ids(label(Some("Label Name Gamma Music"), None), None, Some("11")),
        ],
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(rows[0].0.as_deref(), Some(alpha.as_str()));
    for new in [&rows[1].0, &rows[2].0] {
        assert!(!held.contains(new.as_ref().unwrap()));
    }
    assert_eq!(
        rows[2].0,
        crate::identity::label_id(None, Some("11")),
        "a new label a catalog names takes that catalog's identity"
    );
    assert_eq!(label_count(&db).await, 6);
}

/// One write makes one label of two places naming one new label, and every
/// device that meets a catalog's label makes the one row its id names.
#[tokio::test]
async fn one_write_makes_one_new_label_of_two_places_naming_it() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;

    write(
        &db,
        vec![
            label(Some("Label Name"), Some("LN 1")),
            label(Some("Label Name Records"), Some("LN 2")),
            with_ids(label(Some("Other"), None), Some("mb-other"), None),
        ],
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(rows[0].0, rows[1].0);
    assert_eq!(rows[2].0, crate::identity::label_id(Some("mb-other"), None));
    assert_eq!(label_count(&db).await, 2);
}

/// Saving a release keeps the label it names by that label's name, even
/// where the name alone would not say which label it is.
#[tokio::test]
async fn an_edit_keeps_the_label_the_release_names_by_its_name() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let held = [
        add_label(&db, "Label Name Beta", None, Some("1866")).await,
        add_label(&db, "Label Name Beta", None, Some("5320")).await,
    ];
    write(&db, vec![label(Some("Label Name Beta"), Some("C 1"))]).await.unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!held.contains(&made));

    write(&db, vec![label(Some("Label Name Beta"), Some("C 2"))]).await.unwrap();

    assert_eq!(placed(&db).await, vec![(Some(made), Some("C 2".to_string()))]);
    assert_eq!(label_count(&db).await, 3);
}

/// A label whose two catalog ids two library labels hold is the MusicBrainz
/// one, and the Discogs id that disagrees is dropped: neither label changes.
#[tokio::test]
async fn two_labels_holding_the_labels_two_ids_leave_the_musicbrainz_one() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let by_discogs = add_label(&db, "Label Name", None, Some("7190")).await;
    let by_musicbrainz = add_label(&db, "Other Label Name", Some("mb-label"), None).await;

    write(&db, vec![with_ids(label(Some("Label Name"), None), Some("mb-label"), Some("7190"))])
        .await
        .unwrap();

    assert_eq!(ids_of(&placed(&db).await), [Some(by_musicbrainz.as_str())]);
    assert_eq!(stored_label(&db, &by_musicbrainz).await.discogs_label_id, None);
    assert_eq!(stored_label(&db, &by_discogs).await.musicbrainz_label_id, None);
    assert_eq!(label_count(&db).await, 2);
}

/// A label whose MusicBrainz id names a label holding another Discogs id is
/// that label; the Discogs id it carries is dropped.
#[tokio::test]
async fn a_musicbrainz_label_holding_another_discogs_id_is_the_label() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let held = add_label(&db, "Label Name", Some("mb-label"), Some("9999")).await;

    write(&db, vec![with_ids(label(Some("Label Name"), None), Some("mb-label"), Some("7190"))])
        .await
        .unwrap();

    assert_eq!(ids_of(&placed(&db).await), [Some(held.as_str())]);
    assert_eq!(stored_label(&db, &held).await.discogs_label_id.as_deref(), Some("9999"));
    assert_eq!(label_count(&db).await, 1);
}

/// A label whose Discogs id names a label holding another MusicBrainz id goes
/// by its MusicBrainz id: no library label holds it, so it is a new label
/// carrying that id alone.
#[tokio::test]
async fn a_discogs_label_holding_another_musicbrainz_id_leaves_the_label_its_musicbrainz_id() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    add_label(&db, "Label Name", Some("mb-other"), Some("7190")).await;

    write(&db, vec![with_ids(label(Some("Label Name"), None), Some("mb-label"), Some("7190"))])
        .await
        .unwrap();

    let made = crate::identity::label_id(Some("mb-label"), None);
    assert_eq!(placed(&db).await[0].0, made);
    let made = stored_label(&db, made.as_deref().unwrap()).await;
    assert_eq!(
        (made.musicbrainz_label_id.as_deref(), made.discogs_label_id.as_deref()),
        (Some("mb-label"), None)
    );
}

/// Two labels of one write sharing a MusicBrainz id are one label, which
/// keeps the Discogs id the first gives it: the second's MusicBrainz id names
/// that label, and its Discogs id that disagrees is dropped.
#[tokio::test]
async fn labels_of_one_write_sharing_a_musicbrainz_id_keep_the_first_discogs_id() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;

    write(
        &db,
        vec![
            with_ids(label(Some("Label Name"), Some("LN 1")), Some("mb-label"), Some("7190")),
            with_ids(label(Some("Label Name"), Some("LN 2")), Some("mb-label"), Some("9999")),
        ],
    )
    .await
    .unwrap();

    let made = crate::identity::label_id(Some("mb-label"), None).unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some(made.as_str()), Some(made.as_str())]);
    assert_eq!(stored_label(&db, &made).await.discogs_label_id.as_deref(), Some("7190"));
    assert_eq!(label_count(&db).await, 1);
}

/// Two labels of one write sharing a Discogs id and carrying two MusicBrainz
/// ids are two labels: the second goes by its MusicBrainz id alone.
#[tokio::test]
async fn labels_of_one_write_sharing_a_discogs_id_with_two_musicbrainz_ids_are_two_labels() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;

    write(
        &db,
        vec![
            with_ids(label(Some("Label Name"), Some("LN 1")), Some("mb-first"), Some("7190")),
            with_ids(label(Some("Label Name"), Some("LN 2")), Some("mb-second"), Some("7190")),
        ],
    )
    .await
    .unwrap();

    let first = crate::identity::label_id(Some("mb-first"), None).unwrap();
    let second = crate::identity::label_id(Some("mb-second"), None).unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some(first.as_str()), Some(second.as_str())]);
    let held = |label: LabelRow| (label.musicbrainz_label_id, label.discogs_label_id);
    assert_eq!(
        held(stored_label(&db, &first).await),
        (Some("mb-first".to_string()), Some("7190".to_string()))
    );
    assert_eq!(held(stored_label(&db, &second).await), (Some("mb-second".to_string()), None));
}

/// A label whose Discogs id names a new label of this write and whose
/// MusicBrainz id names a library label makes them one, where no id either
/// holds contradicts the other: the library label takes the new one's
/// Discogs id, and the place that made the new one names it.
#[tokio::test]
async fn a_new_label_agreeing_with_a_library_label_both_ids_name_is_that_label() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let held = add_label(&db, "Label Name", Some("mb-label"), None).await;

    write(
        &db,
        vec![
            with_ids(label(Some("Label Name"), Some("LN 1")), None, Some("7190")),
            with_ids(label(Some("Label Name"), Some("LN 2")), Some("mb-label"), Some("7190")),
        ],
    )
    .await
    .unwrap();

    assert_eq!(ids_of(&placed(&db).await), [Some(held.as_str()), Some(held.as_str())]);
    assert_eq!(stored_label(&db, &held).await.discogs_label_id.as_deref(), Some("7190"));
    assert_eq!(label_count(&db).await, 1);
}

/// A label whose Discogs id names a new label of this write holding another
/// MusicBrainz id, and whose MusicBrainz id names a library label, is the
/// library label: the new label stays its own, and the library label is not
/// given its Discogs id.
#[tokio::test]
async fn a_new_label_contradicting_a_library_label_both_ids_name_stays_its_own() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let held = add_label(&db, "Label Name", Some("mb-held"), None).await;

    write(
        &db,
        vec![
            with_ids(label(Some("Label Name"), Some("LN 1")), Some("mb-new"), Some("7190")),
            with_ids(label(Some("Label Name"), Some("LN 2")), Some("mb-held"), Some("7190")),
        ],
    )
    .await
    .unwrap();

    let made = crate::identity::label_id(Some("mb-new"), None).unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some(made.as_str()), Some(held.as_str())]);
    let held = stored_label(&db, &held).await;
    assert_eq!(
        (held.musicbrainz_label_id.as_deref(), held.discogs_label_id.as_deref()),
        (Some("mb-held"), None)
    );
    let made = stored_label(&db, &made).await;
    assert_eq!(
        (made.musicbrainz_label_id.as_deref(), made.discogs_label_id.as_deref()),
        (Some("mb-new"), Some("7190"))
    );
}

/// Two devices apart each give one Discogs id to a different label, each by
/// importing a release naming its label with that id. Once each has the
/// other's write, the next write naming that id merges the two into the
/// smaller id and names it, and the other device reads the release's label
/// as that one. A label of that write named as the absorbed label is not it.
#[test]
fn a_catalog_id_two_devices_gave_two_labels_merges_them() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_two_devices_give_one_label_id,
    );
}

async fn check_two_devices_give_one_label_id(devices: &bae_store::test_devices::TwoStores) {
    let album = crate::DbAlbum::new_test("Album Title", &coven::UuidIds);
    let release = crate::DbRelease::new_test(&album.id, RELEASE);
    crate::test_fixtures::test_records(devices.a())
        .insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();
    let first = add_label(devices.a(), "Label Name A", None, None).await;
    let second = add_label(devices.a(), "Label Name B", None, None).await;
    let labels = "SELECT id || ':' || COALESCE(discogs_label_id, '') FROM labels \
                  UNION ALL SELECT 'release:' || id FROM releases";
    devices.converge(labels).await;

    devices.pause().await;
    assert_eq!(add_label(devices.a(), "Label Name A", None, Some("7190")).await, first);
    assert_eq!(add_label(devices.b(), "Label Name B", None, Some("7190")).await, second);
    devices.resume().await;
    devices.converge(labels).await;

    let (survivor, name, absorbed_name) = if first < second {
        (&first, "Label Name A", "Label Name B")
    } else {
        (&second, "Label Name B", "Label Name A")
    };
    write(
        devices.a(),
        vec![
            with_ids(label(Some("Label Name"), Some("LN 1")), None, Some("7190")),
            label(Some(absorbed_name), Some("LN 2")),
        ],
    )
    .await
    .unwrap();
    devices
        .converge(
            "SELECT id || '>' || into_label_id FROM label_merges \
             UNION ALL SELECT 'placed:' || COALESCE(label_id, '') FROM release_labels",
        )
        .await;

    for device in [devices.a(), devices.b()] {
        let placed = placed(device).await;
        assert_eq!(placed[0].0.as_deref(), Some(survivor.as_str()));
        assert!(![&first, &second].contains(&placed[1].0.as_ref().unwrap()), "{placed:?}");
        assert_eq!(
            read_labels(&crate::test_fixtures::test_records(device)).await,
            vec![label(Some(name), Some("LN 1")), label(Some(absorbed_name), Some("LN 2"))]
        );
    }
}
