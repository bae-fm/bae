//! Found's groups: which releases a folder's header gathers, and the date the
//! header and its members sort by.

use super::*;

/// A release nested below a folder is listed under that folder's header, and
/// the group sorts by its newest member. A member sorts in its header's
/// place, so one that moves the group's date moves only the header: a scan
/// writing a large group's members newest last writes two rows each.
#[tokio::test]
async fn nested_releases_group_under_their_first_folder_by_the_newest() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.discovered("Artist/Early", 100),
        lib.discovered("Artist/Late", 300)
    ))
    .await;
    assert_eq!(
        lib.moved(&lib.scan("Artist/Early")).await,
        moves([
            (
                lib.release("Artist/Early"),
                Some(found("Artist/Early").dated(100).grouped("Artist"))
            ),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
    assert_eq!(
        lib.moved(&lib.scan("Artist/Late")).await,
        moves([
            (
                lib.release("Artist/Late"),
                Some(found("Artist/Late").dated(300).grouped("Artist"))
            ),
            (lib.group("Artist"), Some(header("Artist", Some(300)))),
        ])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM folder_discovery WHERE folder = '{}';",
            lib.path("Artist/Late")
        ))
        .await,
        moves([
            (
                lib.release("Artist/Late"),
                Some(found("Artist/Late").grouped("Artist"))
            ),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
}

/// The release at a folder joins that folder's group once another release
/// is nested below it — even one that failed validation, which Found never
/// lists — and leaves it when none is.
#[tokio::test]
async fn a_release_at_a_folder_groups_while_another_is_nested_below_it() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Box")).await;
    assert_eq!(
        lib.moved(&lib.scan_as("Box/Disc 1", "invalid")).await,
        moves([
            (lib.release("Box"), Some(found("Box").grouped("Box"))),
            (lib.group("Box"), Some(header("Box", None))),
            (
                lib.invalid("Box/Disc 1"),
                Some(failed_validation("Box/Disc 1"))
            ),
        ])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM scan_candidate WHERE path = '{}';",
            lib.path("Box/Disc 1")
        ))
        .await,
        moves([
            (lib.release("Box"), Some(found("Box"))),
            (lib.group("Box"), None),
            (lib.invalid("Box/Disc 1"), None),
        ])
    );
}

/// A folder whose name only starts with another's is not nested below it.
#[tokio::test]
async fn a_folder_sharing_a_name_prefix_is_not_nested() {
    let lib = Library::new().await;
    lib.run(&lib.scan("Box")).await;
    assert_eq!(
        lib.moved(&lib.scan("Box Set/Disc 1")).await,
        moves([
            (
                lib.release("Box Set/Disc 1"),
                Some(found("Box Set/Disc 1").grouped("Box Set"))
            ),
            (lib.group("Box Set"), Some(header("Box Set", None))),
        ])
    );
}

/// Only Found groups: a member that leaves Found leaves its group, and the
/// group's date is its remaining members'.
#[tokio::test]
async fn a_member_leaving_found_leaves_its_group() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.discovered("Artist/Early", 100),
        lib.discovered("Artist/Late", 300),
        lib.scan("Artist/Early"),
        lib.scan("Artist/Late")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{}');",
            lib.path("Artist/Late")
        ))
        .await,
        moves([
            (
                lib.release("Artist/Late"),
                Some(found("Artist/Late").dated(300).on("skipped"))
            ),
            (lib.group("Artist"), Some(header("Artist", Some(100)))),
        ])
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{}');",
            lib.path("Artist/Early")
        ))
        .await,
        moves([
            (
                lib.release("Artist/Early"),
                Some(found("Artist/Early").dated(100).on("skipped"))
            ),
            (lib.group("Artist"), None),
        ])
    );
}

/// Moving a release to another folder moves it between groups.
#[tokio::test]
async fn a_moved_release_changes_group() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}",
        lib.scan("Old/Album"),
        lib.scan("Old/Other")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE scan_candidate SET display_path = 'New/Album' WHERE path = '{}';",
            lib.path("Old/Album")
        ))
        .await,
        moves([
            (
                lib.release("Old/Album"),
                Some(Placed {
                    display_path: Some("New/Album".to_string()),
                    ..found("Old/Album").grouped("New")
                })
            ),
            (lib.group("New"), Some(header("New", None))),
        ])
    );
}

/// Placing a release under a group costs the same however many releases the
/// group already holds, each newer than the last as a scan writing a
/// copied-in collection finds them: only the header holds the group's date,
/// and the newest member is read off the group's index. Counted in SQLite's
/// own steps for the write of the release's scan row, its triggers' included.
#[test]
fn a_member_costs_the_same_however_large_its_group() {
    let conn = crate::library::bare_library_connection();
    let root = crate::import::watched_folder::host_root("/music");
    conn.execute_batch(&format!(
        "INSERT INTO watched_import_folders (path, position) VALUES ('{root}', 0);
         INSERT INTO folder_scan_roots (watched_folder_path, generation, status, volume)
         VALUES ('{root}', 1, 'scanning', 'local');"
    ))
    .unwrap();
    let place = |n: i64| -> i32 {
        let display_path = format!("Collection/Album {n}");
        let path = folder_below(&root, &display_path)
            .to_string_lossy()
            .into_owned();
        conn.execute(
            "INSERT INTO folder_discovery (folder, first_seen_at, settled) VALUES (?, ?, 1)",
            rusqlite::params![path, n],
        )
        .unwrap();
        let mut write = conn
            .prepare(
                "INSERT INTO scan_candidate
                     (watched_folder_path, path, generation, kind, name, display_path, folder,
                      file_root, scope, content_hash)
                 VALUES (?1, ?2, 1, 'valid', ?3, ?4, ?2, ?2, 'direct', ?2)",
            )
            .unwrap();
        write
            .execute(rusqlite::params![
                root,
                path,
                format!("Album {n}"),
                display_path
            ])
            .unwrap();
        write.get_status(rusqlite::StatementStatus::VmStep)
    };
    let early = (0..20).map(place).last().unwrap();
    let late = (20..2_000).map(place).last().unwrap();
    assert!(
        late < early * 3 / 2,
        "placing the 20th member took {early} steps, the 2,000th {late}"
    );
}
