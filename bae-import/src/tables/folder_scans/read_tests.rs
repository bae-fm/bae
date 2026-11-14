/// The entries reading a folder's files are looked up by the root and the
/// folders they read, never by passing over the root's entries: a scan asks
/// for them for every folder it writes, so a pass would make filling a
/// watched folder cost the square of its size. That holds with no
/// statistics, as a small library has, and with those of one watched folder
/// of 33,790 entries, both as coven's analysis gathers them, from a sample
/// that puts 286 entries under the root, and as they are.
#[test]
fn the_entries_reading_a_folder_are_looked_up_by_the_folders_they_read() {
    let key = crate::CandidateKey::grouping("key");
    let mut passes = Vec::new();
    for under_root in [None, Some(286), Some(33_790)] {
        let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
        crate::TEST_SCHEMA.open_bare_library(&conn);
        if let Some(under_root) = under_root {
            plant_one_folders_statistics(&conn, under_root);
        }
        for readers in [super::Readers::Settled, super::Readers::Besides(&key)] {
            for below in [false, true] {
                let plan = plan_of(&conn, &super::entries_reading_sql(readers, below));
                let keyed =
                    plan.iter().filter(|step| step.contains("scan_candidate")).all(|step| {
                        step.starts_with(
                            "SEARCH scan_candidate USING INDEX idx_scan_candidate_file_root \
                             (watched_folder_path=? AND file_root",
                        )
                    });
                if !keyed {
                    passes.push((
                        format!("{under_root:?} under the root, {readers:?}, below {below}"),
                        plan,
                    ));
                }
            }
        }
    }
    assert!(passes.is_empty(), "{passes:#?}");
}

/// The steps SQLite plans `statement` in on `conn`.
fn plan_of(conn: &rusqlite::Connection, statement: &str) -> Vec<String> {
    let mut explain = conn.prepare(&format!("EXPLAIN QUERY PLAN {statement}")).unwrap();
    // The plan is of the statement, whatever it is bound to.
    for at in 1..=explain.parameter_count() {
        explain.raw_bind_parameter(at, rusqlite::types::Null).unwrap();
    }
    explain.raw_query().mapped(|row| row.get(3)).collect::<Result<_, _>>().unwrap()
}

/// Plant in `conn` the statistics of `scan_candidate` holding one watched
/// folder of 33,790 entries: an index's columns up to the root, a grouping
/// or a content hash pick out `under_root` entries, any up to another column
/// one.
fn plant_one_folders_statistics(conn: &rusqlite::Connection, under_root: u64) {
    conn.execute_batch("ANALYZE; DELETE FROM sqlite_stat1 WHERE tbl = 'scan_candidate';").unwrap();
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM pragma_index_list('scan_candidate')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for index in indexes {
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
            .unwrap()
            .query_map([&index], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let mut stat = String::from("33790");
        let mut picked = 33_790;
        for column in &columns {
            let per_value = match column.as_str() {
                "watched_folder_path" | "grouping_key" | "content_hash" => under_root,
                _ => 1,
            };
            picked = picked.min(per_value);
            stat.push_str(&format!(" {picked}"));
        }
        conn.execute(
            "INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES ('scan_candidate', ?, ?)",
            [&index, &stat],
        )
        .unwrap();
    }
    conn.execute_batch("ANALYZE sqlite_schema").unwrap();
}
