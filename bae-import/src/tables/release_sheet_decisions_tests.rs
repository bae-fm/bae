//! These tests check how the release sheet decisions part declares its
//! tables. Both tables sync:
//! `release_sheet_places` is keyed by the sheet file, and
//! `release_sheet_bindings` by the sheet file and the `FILE` reference.

use std::collections::BTreeSet;

/// The part's SCHEMA declares both of its tables synced, each with a shared
/// key on its primary-key columns, and declares every table the part creates.
#[test]
fn the_sheet_decision_tables_are_the_part_s_synced_tables() {
    let create =
        regex::Regex::new(r"(?m)^\s*CREATE (?:VIRTUAL )?TABLE (?:IF NOT EXISTS )?(\w+)").unwrap();
    let schema = bae_store::Schema::new(&[&[super::SCHEMA]]);
    let created: BTreeSet<&str> = schema
        .sql()
        .flat_map(|sql| create.captures_iter(sql))
        .map(|captures| captures.get(1).unwrap().as_str())
        .collect();
    let tables = schema.synced_tables();
    let declared: BTreeSet<&str> = tables.iter().map(coven::SyncedTable::name).collect();
    assert_eq!(created, BTreeSet::from(["release_sheet_bindings", "release_sheet_places"]));
    assert_eq!(declared, created);
    for table in &tables {
        assert_eq!(table.identity(), &coven::RowIdentity::SharedKey);
        assert!(table.file_columns().is_empty());
        let keys: &[&str] = match table.name() {
            "release_sheet_bindings" => &["sheet_file_id", "file_reference"],
            _ => &["sheet_file_id"],
        };
        assert_eq!(table.get_key_columns(), keys);
    }
}

/// A decision that names a path none of the release's files has fails the
/// write, whether the path is the sheet's or the audio a binding names.
#[tokio::test]
async fn a_decision_naming_a_path_outside_the_release_fails_its_write() {
    use crate::folder_scanner::{CandidateFileEdits, SheetPlace, UserSheetBinding};
    use std::collections::HashMap;

    let (db, _dir) = bae_store::test_fixtures::empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db);

    let mut disc = CandidateFileEdits::default();
    disc.sheet_places
        .set("CD2.cue".to_string(), SheetPlace::On(bae_types::mediums::MediumSide::first_of(1)));
    let error = tables.store_release_sheet_decisions_for_test(disc, HashMap::new()).await;
    assert!(
        matches!(&error, Err(bae_store::DbError::Unreadable { stored, .. }) if stored.contains("CD2.cue")),
        "{error:?}"
    );

    let mut binding = CandidateFileEdits::default();
    binding.sheet_bindings.set_reference(
        "Album.cue".to_string(),
        "Album.wav".to_string(),
        UserSheetBinding::Describes { file_id: "Album.flac".to_string() },
    );
    let sheet_only = HashMap::from([("Album.cue".to_string(), "sheet-id".to_string())]);
    let error = tables.store_release_sheet_decisions_for_test(binding, sheet_only).await;
    assert!(
        matches!(&error, Err(bae_store::DbError::Unreadable { stored, .. }) if stored.contains("Album.flac")),
        "{error:?}"
    );
}
