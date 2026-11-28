use bae_store::test_fixtures::query_plan::{plan_on, plant_table_statistics};

/// The entries reading a folder's files are looked up by the root and the
/// folders they read, never by passing over the root's entries: a scan asks
/// for them for every folder it writes, so a pass would make filling a
/// watched folder cost the square of its size. That holds with no
/// statistics, as a small library has, and with those of one watched folder
/// of 33,790 entries, both as coven's analysis gathers them, from a sample
/// that puts 286 entries under the root, and as they are.
#[test]
fn the_entries_reading_a_folder_are_looked_up_by_the_folders_they_read() {
    let mut passes = Vec::new();
    for under_root in [None, Some(286), Some(33_790)] {
        let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
        crate::TEST_SCHEMA.open_bare_library(&conn);
        if let Some(under_root) = under_root {
            plant_one_folders_statistics(&conn, under_root);
        }
        let plan = plan_on(&conn, &readers_covering());
        let searched: Vec<&String> =
            plan.iter().filter(|step| step.contains("candidate")).collect();
        // One lookup for each part: the folder, those above it, those below.
        let keyed = searched.len() == 3
            && searched.iter().all(|step| {
                step.starts_with(
                    "SEARCH candidate USING INDEX idx_scan_candidate_file_root \
                     (watched_folder_path=? AND file_root",
                )
            });
        if !keyed {
            passes.push((format!("{under_root:?} under the root"), plan));
        }
    }
    assert!(passes.is_empty(), "{passes:#?}");
}

/// A scan reads the entry stored at a folder's key for every folder it
/// writes, and a pass reads what the store knows of its root: the first looks
/// each key up, and the second searches the root's entries by the root and
/// looks up what it joins to each of its rows, so neither passes over every
/// entry, and no join passes over what it joins for each row. That holds with
/// no statistics and with those of one watched folder of 33,790 entries.
#[test]
fn the_entries_a_read_takes_in_are_searched_by_an_index() {
    for under_root in [None, Some(286), Some(33_790)] {
        let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
        crate::TEST_SCHEMA.open_bare_library(&conn);
        if let Some(under_root) = under_root {
            plant_one_folders_statistics(&conn, under_root);
        }
        let searched: Vec<String> = plan_on(&conn, &items_at_keys())
            .into_iter()
            .filter(|step| {
                step.starts_with("SCAN candidate ") || step.starts_with("SEARCH candidate ")
            })
            .collect();
        assert_eq!(
            searched,
            ["SEARCH candidate USING INDEX idx_scan_candidate_path (path=?)"],
            "{under_root:?} under the root"
        );

        let plan = plan_on(&conn, &known_of_root_rows());
        let passes: Vec<&String> = plan
            .iter()
            .filter(|step| {
                step.starts_with("SCAN candidate")
                    || step.starts_with("SCAN entry")
                    || (step.starts_with("SCAN ") && step.ends_with("LEFT-JOIN"))
            })
            .collect();
        assert!(passes.is_empty(), "{under_root:?} under the root: {plan:#?}");
    }
}

/// A read of the entries reading a folder's files.
fn readers_covering() -> super::ReadersCoveringParams {
    super::ReadersCoveringParams {
        root: b"/music".to_vec(),
        folder: b"/music/Album".to_vec(),
        above: "[]".to_owned(),
        below_first: Some(b"/music/Album/".to_vec()),
        below_past: Some(b"/music/Album0".to_vec()),
        besides: None,
    }
}

/// A read of the entries stored at some keys.
fn items_at_keys() -> super::ItemsAtKeysParams {
    super::ItemsAtKeysParams { keys: "[]".to_owned(), grouping_releases: false }
}

/// A read of what the store knows of a root.
fn known_of_root_rows() -> super::super::file_reads::KnownOfRootRowsParams {
    super::super::file_reads::KnownOfRootRowsParams {
        root: b"/music".to_vec(),
        below_first: b"/music/".to_vec(),
        below_past: b"/music0".to_vec(),
    }
}

/// Plant in `conn` the statistics of `scan_candidate` holding one watched
/// folder of 33,790 entries: an index's columns up to the root, a grouping
/// or a content hash pick out `under_root` entries, any up to another column
/// one.
fn plant_one_folders_statistics(conn: &rusqlite::Connection, under_root: u64) {
    plant_table_statistics(conn, "scan_candidate", 33_790, |column| match column {
        "watched_folder_path" | "grouping_key" | "content_hash" => under_root,
        _ => 1,
    });
}
