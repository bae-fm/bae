use super::*;

const RELEASE: &str = "5d1c2c9e-4f0b-4a43-9a51-0e6c1d3b7a10";
const CANDIDATE: &str = "candidate-hash";

async fn library() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let album = DbAlbum::new_test("Album Title");
    let release = DbRelease::new_test(&album.id, RELEASE);
    db.insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();
    (db, tmp)
}

fn label(name: Option<&str>, number: Option<&str>) -> ReleaseLabel {
    ReleaseLabel::new(name.map(str::to_string), number.map(str::to_string)).unwrap()
}

fn with_ids(label: ReleaseLabel, musicbrainz: Option<&str>, discogs: Option<&str>) -> ReleaseLabel {
    label.with_ids(musicbrainz.map(str::to_string), discogs.map(str::to_string))
}

async fn add_label(
    db: &Database,
    id: &str,
    name: &str,
    musicbrainz: Option<&str>,
    discogs: Option<&str>,
) {
    let (id, name) = (id.to_string(), name.to_string());
    let (musicbrainz, discogs) = (musicbrainz.map(str::to_string), discogs.map(str::to_string));
    db.call(move |sql| {
        sql.execute(
            "INSERT INTO labels \
                 (id, name, name_key, musicbrainz_label_id, discogs_label_id, created_at) \
             VALUES (?, ?, ?, ?, ?, '2026-01-01T00:00:00Z')",
            params![
                id,
                name,
                bae_types::text_match::label_name_key(&name),
                musicbrainz,
                discogs
            ],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

/// Write `RELEASE`'s labels as an import of `candidate` would, or an edit.
async fn write(
    db: &Database,
    labels: Vec<ReleaseLabel>,
    candidate: Option<&str>,
) -> Result<(), IdentityWriteError> {
    let parts = db.label_parts(&labels);
    let candidate = candidate.map(str::to_string);
    db.write_resolving_identities(move |sql| {
        write_release_labels_on(
            &sql,
            RELEASE,
            &labels,
            &import_label_write(&parts, candidate.as_deref()),
        )
    })
    .await
}

/// The label each of `RELEASE`'s label rows names, as stored, with its number.
async fn placed(db: &Database) -> Vec<(Option<String>, Option<String>)> {
    db.read(|sql| {
        Ok(sql.query(
            "SELECT label_id, catalog_number FROM release_labels \
             WHERE release_id = ? ORDER BY position",
            [RELEASE],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
    .await
    .unwrap()
}

async fn stored_label(db: &Database, id: &str) -> LabelRow {
    let id = id.to_string();
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT * FROM labels WHERE id = ?",
            [id],
            LabelRow::from_row,
        )?)
    })
    .await
    .unwrap()
}

async fn label_count(db: &Database) -> i64 {
    db.read(|sql| Ok(sql.query_row("SELECT count(*) FROM labels", [], |row| row.get(0))?))
        .await
        .unwrap()
}

/// The labels `RELEASE` reads with, as the library shows them.
async fn read_labels(db: &Database) -> Vec<ReleaseLabel> {
    db.find_release_by_id(RELEASE)
        .await
        .unwrap()
        .unwrap()
        .pressing
        .labels
}

fn ids_of(rows: &[(Option<String>, Option<String>)]) -> Vec<Option<&str>> {
    rows.iter().map(|(id, _)| id.as_deref()).collect()
}

/// A catalog id decides: the library label holding it is the label, whatever
/// it is called, and it takes the other catalog's id the label carries; a
/// number-only label names no library label.
#[tokio::test]
async fn a_label_is_the_library_label_holding_its_catalog_id() {
    let (db, _tmp) = library().await;
    add_label(
        &db,
        "held",
        "Warner Strategic Marketing",
        None,
        Some("7190"),
    )
    .await;

    write(
        &db,
        vec![
            with_ids(
                label(Some("Rhino"), Some("R2 1")),
                Some("mb-rhino"),
                Some("7190"),
            ),
            label(None, Some("XY 3")),
        ],
        None,
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
        vec![
            label(Some("Warner Strategic Marketing"), Some("R2 1")),
            label(None, Some("XY 3"))
        ]
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
        None,
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
        crate::db::identity::label_id(None, Some("11")),
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
        None,
    )
    .await
    .unwrap();

    let rows = placed(&db).await;
    assert_eq!(rows[0].0, rows[1].0);
    assert_eq!(
        rows[2].0,
        crate::db::identity::label_id(Some("mb-other"), None)
    );
    assert_eq!(label_count(&db).await, 2);
}

/// Saving a release keeps the label it names by that label's name, even
/// where the name alone would not say which label it is.
#[tokio::test]
async fn an_edit_keeps_the_label_the_release_names_by_its_name() {
    let (db, _tmp) = library().await;
    add_label(&db, "columbia-1", "Columbia", None, Some("1866")).await;
    add_label(&db, "columbia-2", "Columbia", None, Some("5320")).await;
    write(&db, vec![label(Some("Columbia"), Some("C 1"))], None)
        .await
        .unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!["columbia-1", "columbia-2"].contains(&made.as_str()));

    write(&db, vec![label(Some("Columbia"), Some("C 2"))], None)
        .await
        .unwrap();

    assert_eq!(
        placed(&db).await,
        vec![(Some(made), Some("C 2".to_string()))]
    );
    assert_eq!(label_count(&db).await, 3);
}

/// A label whose two catalog ids two library labels hold refuses the write
/// with both labels for the person to answer, and writes nothing.
#[tokio::test]
async fn two_labels_holding_the_labels_two_ids_are_the_identity_conflict() {
    let (db, _tmp) = library().await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name", Some("mb-label"), None).await;

    let error = write(
        &db,
        vec![with_ids(
            label(Some("Label Name"), None),
            Some("mb-label"),
            Some("7190"),
        )],
        None,
    )
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

/// Record `CANDIDATE`'s failed import stopping on the conflict over
/// `by-discogs` and `by-musicbrainz`; the first time, the candidate too.
async fn record_conflict(db: &Database, first: bool) {
    let candidate = if first {
        "INSERT INTO import_candidate_state (content_hash, candidate_key) \
             VALUES ('candidate-hash', CAST('/music/Album' AS BLOB));"
    } else {
        ""
    };
    let batch = format!(
        "{candidate} \
         INSERT INTO import_candidate_failure (content_hash, error, failed_at) \
             VALUES ('candidate-hash', 'conflict', '2026-01-01T00:00:00Z'); \
         INSERT INTO import_candidate_label_identity_conflict \
             (content_hash, incoming_label_name, discogs_label_id, musicbrainz_label_id, \
              discogs_library_label_id, musicbrainz_library_label_id) \
             VALUES ('candidate-hash', 'Label Name', '7190', 'mb-label', \
                     'by-discogs', 'by-musicbrainz');"
    );
    db.call(move |sql| {
        sql.execute_batch(&batch)?;
        Ok(())
    })
    .await
    .unwrap();
}

async fn has_failure(db: &Database) -> bool {
    db.read(|sql| {
        Ok(sql
            .query_row(
                "SELECT 1 FROM import_candidate_failure WHERE content_hash = ?",
                [CANDIDATE],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    })
    .await
    .unwrap()
}

fn conflicting() -> ReleaseLabel {
    with_ids(
        label(Some("Label Name"), Some("LN 1")),
        Some("mb-label"),
        Some("7190"),
    )
}

/// "Same label" merges the MusicBrainz library label into the Discogs one,
/// which takes its id; a release naming the absorbed label reads the
/// survivor; and the import then names the survivor.
#[tokio::test]
async fn the_same_label_answer_merges_the_two_and_the_import_names_the_survivor() {
    let (db, _tmp) = library().await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(
        &db,
        "by-musicbrainz",
        "Label Name Records",
        Some("mb-label"),
        None,
    )
    .await;
    write(
        &db,
        vec![with_ids(
            label(Some("Label Name Records"), None),
            Some("mb-label"),
            None,
        )],
        None,
    )
    .await
    .unwrap();
    record_conflict(&db, true).await;

    db.choose_import_label_identity(
        CANDIDATE,
        0,
        crate::import::LabelIdentityConflictChoice::SameLabel,
        &coven::UuidIds,
    )
    .await
    .unwrap();

    assert!(!has_failure(&db).await);
    let survivor = stored_label(&db, "by-discogs").await;
    assert_eq!(survivor.musicbrainz_label_id.as_deref(), Some("mb-label"));
    assert_eq!(
        read_labels(&db).await,
        vec![label(Some("Label Name"), None)]
    );
    let into: String = db
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT into_label_id FROM label_merges WHERE id = 'by-musicbrainz'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(into, "by-discogs");

    write(&db, vec![conflicting()], Some(CANDIDATE))
        .await
        .unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some("by-discogs")]);
}

/// "Neither" makes a label carrying neither id, and the import names it;
/// "It's the Discogs label" names that label and attaches no MusicBrainz id.
#[tokio::test]
async fn the_neither_and_one_label_answers_decide_the_import() {
    let (db, _tmp) = library().await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name", Some("mb-label"), None).await;
    record_conflict(&db, true).await;

    db.choose_import_label_identity(
        CANDIDATE,
        0,
        crate::import::LabelIdentityConflictChoice::Neither,
        &coven::UuidIds,
    )
    .await
    .unwrap();
    write(&db, vec![conflicting()], Some(CANDIDATE))
        .await
        .unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!["by-discogs", "by-musicbrainz"].contains(&made.as_str()));
    let made = stored_label(&db, &made).await;
    assert_eq!(
        (made.musicbrainz_label_id, made.discogs_label_id),
        (None, None)
    );

    record_conflict(&db, false).await;
    db.choose_import_label_identity(
        CANDIDATE,
        1,
        crate::import::LabelIdentityConflictChoice::DiscogsLabel,
        &coven::UuidIds,
    )
    .await
    .unwrap();
    write(&db, vec![conflicting()], Some(CANDIDATE))
        .await
        .unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some("by-discogs")]);
    assert_eq!(
        stored_label(&db, "by-discogs").await.musicbrainz_label_id,
        None
    );
}
