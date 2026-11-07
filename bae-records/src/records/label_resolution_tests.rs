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
    let held = add_label(&db, "Warner Strategic Marketing", None, Some("7190")).await;

    write(
        &db,
        vec![
            with_ids(label(Some("Rhino"), Some("R2 1")), Some("mb-rhino"), Some("7190")),
            label(None, Some("XY 3")),
        ],
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(ids_of(&rows), [Some(held.as_str()), None]);
    assert_eq!(rows[1].1.as_deref(), Some("XY 3"));
    let held = stored_label(&db, &held).await;
    assert_eq!(held.name, "Warner Strategic Marketing");
    assert_eq!(held.musicbrainz_label_id.as_deref(), Some("mb-rhino"));
    assert_eq!(label_count(&db).await, 1);
    assert_eq!(
        read_labels(&records).await,
        vec![label(Some("Warner Strategic Marketing"), Some("R2 1")), label(None, Some("XY 3"))]
    );
}

/// A label carrying no id is the one library label of its name key, trade
/// words aside; two of one key, or one holding a contradicting id, leave it
/// new.
#[tokio::test]
async fn a_label_without_ids_is_the_one_library_label_of_its_name_key() {
    let (db, _tmp) = library(crate::test_fixtures::TEST_SCHEMA).await;
    let rhino = add_label(&db, "Rhino Records", None, None).await;
    let held = [
        rhino.clone(),
        add_label(&db, "Columbia", None, Some("1866")).await,
        add_label(&db, "Columbia Records", None, Some("5320")).await,
        add_label(&db, "Harbor", None, Some("10")).await,
    ];

    write(
        &db,
        vec![
            label(Some("RHINO"), None),
            label(Some("Columbia"), None),
            with_ids(label(Some("Harbor Music"), None), None, Some("11")),
        ],
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(rows[0].0.as_deref(), Some(rhino.as_str()));
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
        add_label(&db, "Columbia", None, Some("1866")).await,
        add_label(&db, "Columbia", None, Some("5320")).await,
    ];
    write(&db, vec![label(Some("Columbia"), Some("C 1"))]).await.unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!held.contains(&made));

    write(&db, vec![label(Some("Columbia"), Some("C 2"))]).await.unwrap();

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

/// Two devices apart each give one Discogs id to a different label, each by
/// importing a release naming its label with that id. Once each has the
/// other's write, the next write naming that id merges the two into the
/// smaller id and names it, and the other device reads the release's label
/// as that one.
#[test]
fn a_catalog_id_two_devices_gave_two_labels_merges_them() {
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        check_two_devices_give_one_label_id,
    );
}

async fn check_two_devices_give_one_label_id(devices: &bae_store::test_devices::TwoStores) {
    let album = crate::DbAlbum::new_test("Album Title");
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

    write(devices.a(), vec![with_ids(label(Some("Label Name"), Some("LN 1")), None, Some("7190"))])
        .await
        .unwrap();
    devices
        .converge(
            "SELECT id || '>' || into_label_id FROM label_merges \
             UNION ALL SELECT 'placed:' || COALESCE(label_id, '') FROM release_labels",
        )
        .await;

    let (survivor, name) =
        if first < second { (&first, "Label Name A") } else { (&second, "Label Name B") };
    for device in [devices.a(), devices.b()] {
        assert_eq!(ids_of(&placed(device).await), [Some(survivor.as_str())]);
        assert_eq!(
            read_labels(&crate::test_fixtures::test_records(device)).await,
            vec![label(Some(name), Some("LN 1"))]
        );
    }
}
