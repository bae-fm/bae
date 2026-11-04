use super::*;
use crate::test_fixtures::labels::*;

async fn label_count(db: &Database) -> i64 {
    db.read(|sql| Ok(sql.query_row("SELECT count(*) FROM labels", [], |row| row.get(0))?))
        .await
        .unwrap()
}

/// A catalog id decides: the library label holding it is the label, whatever
/// it is called, and it takes the other catalog's id the label carries; a
/// number-only label names no library label.
#[tokio::test]
async fn a_label_is_the_library_label_holding_its_catalog_id() {
    let (db, _tmp) = library().await;
    add_label(&db, "held", "Warner Strategic Marketing", None, Some("7190")).await;

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
    assert_eq!(ids_of(&rows), [Some("held"), None]);
    assert_eq!(rows[1].1.as_deref(), Some("XY 3"));
    let held = stored_label(&db, "held").await;
    assert_eq!(held.name, "Warner Strategic Marketing");
    assert_eq!(held.musicbrainz_label_id.as_deref(), Some("mb-rhino"));
    assert_eq!(label_count(&db).await, 1);
    assert_eq!(
        read_labels(&db).await,
        vec![label(Some("Warner Strategic Marketing"), Some("R2 1")), label(None, Some("XY 3"))]
    );
}

/// A label carrying no id is the one library label of its name key, trade
/// words aside; two of one key, or one holding a contradicting id, leave it
/// new.
#[tokio::test]
async fn a_label_without_ids_is_the_one_library_label_of_its_name_key() {
    let (db, _tmp) = library().await;
    add_label(&db, "rhino", "Rhino Records", None, None).await;
    add_label(&db, "columbia-1", "Columbia", None, Some("1866")).await;
    add_label(&db, "columbia-2", "Columbia Records", None, Some("5320")).await;
    add_label(&db, "harbor", "Harbor", None, Some("10")).await;

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
    assert_eq!(rows[0].0.as_deref(), Some("rhino"));
    for new in [&rows[1].0, &rows[2].0] {
        let new = new.as_deref().unwrap();
        assert!(!["rhino", "columbia-1", "columbia-2", "harbor"].contains(&new));
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
    let (db, _tmp) = library().await;

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
    let (db, _tmp) = library().await;
    add_label(&db, "columbia-1", "Columbia", None, Some("1866")).await;
    add_label(&db, "columbia-2", "Columbia", None, Some("5320")).await;
    write(&db, vec![label(Some("Columbia"), Some("C 1"))]).await.unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!["columbia-1", "columbia-2"].contains(&made.as_str()));

    write(&db, vec![label(Some("Columbia"), Some("C 2"))]).await.unwrap();

    assert_eq!(placed(&db).await, vec![(Some(made), Some("C 2".to_string()))]);
    assert_eq!(label_count(&db).await, 3);
}

/// A label whose two catalog ids two library labels hold refuses the write
/// with both labels for the person to answer, and writes nothing.
#[tokio::test]
async fn two_labels_holding_the_labels_two_ids_are_the_identity_conflict() {
    let (db, _tmp) = library().await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name", Some("mb-label"), None).await;

    let error =
        write(&db, vec![with_ids(label(Some("Label Name"), None), Some("mb-label"), Some("7190"))])
            .await
            .unwrap_err();

    let IdentityWriteError::LabelIdentityConflict(conflict) = error else {
        panic!("expected a label identity conflict, got {error:?}");
    };
    assert_eq!(conflict.discogs_label.label_id, "by-discogs");
    assert_eq!(conflict.musicbrainz_label.label_id, "by-musicbrainz");
    assert_eq!(conflict.incoming_label_name, "Label Name");
    assert!(placed(&db).await.is_empty());
}

/// Fill in `discogs` on the label `id` on `db`, as a device does once a
/// catalog names it.
async fn give_discogs_id(db: &Database, id: &'static str, discogs: &'static str) {
    db.call(move |sql| {
        sql.execute("UPDATE labels SET discogs_label_id = ? WHERE id = ?", params![discogs, id])?;
        Ok(())
    })
    .await
    .unwrap();
}

/// Two devices apart each give one Discogs id to a different label of one
/// name. Once each has the other's write, the next write naming that id
/// merges the two into the smaller id and names it, and the other device
/// reads the release's label as that one.
#[test]
fn a_catalog_id_two_devices_gave_two_labels_merges_them() {
    crate::test_devices::run_two_store_test(check_two_devices_give_one_label_id);
}

async fn check_two_devices_give_one_label_id(devices: &crate::test_devices::TwoStores) {
    let album = crate::DbAlbum::new_test("Album Title");
    let release = crate::DbRelease::new_test(&album.id, RELEASE);
    devices
        .a()
        .insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();
    add_label(devices.a(), "label-a", "Label Name", None, None).await;
    add_label(devices.a(), "label-b", "Label Name", None, None).await;
    let labels = "SELECT id || ':' || COALESCE(discogs_label_id, '') FROM labels \
                  UNION ALL SELECT 'release:' || id FROM releases";
    devices.converge(labels).await;

    devices.pause().await;
    give_discogs_id(devices.a(), "label-a", "7190").await;
    give_discogs_id(devices.b(), "label-b", "7190").await;
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

    for device in [devices.a(), devices.b()] {
        assert_eq!(ids_of(&placed(device).await), [Some("label-a")]);
        assert_eq!(read_labels(device).await, vec![label(Some("Label Name"), Some("LN 1"))]);
    }
}
