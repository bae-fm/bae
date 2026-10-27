//! Releases read from several folders picked together: what their grouping
//! places, and the folders it takes in.

use super::*;

impl Library {
    /// The grouping that reads the folders picked together as the release at
    /// `display_path`.
    fn grouping(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO release_grouping (key, anchor_folder, combined, author)
             VALUES ('{}', NULL, 1, 'user');",
            self.path(display_path)
        )
    }

    /// The release the grouping at `display_path` builds, shown as its first
    /// folder, `first`.
    fn grouping_release(&self, display_path: &str, first: &str) -> String {
        let key = self.path(display_path);
        format!(
            "INSERT INTO scan_candidate
                 (watched_folder_path, path, generation, kind, name, display_path, folder,
                  file_root, scope, content_hash, grouping_key, source_kind)
             VALUES ('{root}', '{key}', 1, 'valid', '{display_path}', '{display_path}',
                     '{folder}', '{folder}', 'recursive', '{hash}', '{key}', 'grouping');",
            root = self.root,
            folder = self.path(first),
            hash = hash(display_path),
        )
    }

    /// Take the release at `member` into the grouping at `display_path`.
    fn member(&self, display_path: &str, position: i64, member: &str) -> String {
        format!(
            "INSERT INTO release_grouping_member
                 (grouping_key, position, member_key, member_folder)
             VALUES ('{}', {position}, '{}', '{}');",
            self.path(display_path),
            self.path(member),
            self.path(member)
        )
    }
}

/// A folder a grouping takes in leaves the list while the grouping stands, and
/// the grouping's release is dated by the earliest of its folders.
#[tokio::test]
async fn a_grouping_takes_its_folders_out_and_is_dated_by_the_earliest() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}",
        lib.discovered("Disc 1", 100),
        lib.discovered("Disc 2", 200),
        lib.scan("Disc 1"),
        lib.scan("Disc 2"),
        lib.grouping("Discs")
    ))
    .await;
    assert_eq!(
        lib.moved(&lib.member("Discs", 0, "Disc 1")).await,
        moves([(lib.release("Disc 1"), None)])
    );
    assert_eq!(
        lib.moved(&lib.grouping_release("Discs", "Disc 1")).await,
        moves([(lib.release("Discs"), Some(found("Discs").dated(100)))])
    );
    assert_eq!(
        lib.moved(&lib.member("Discs", 1, "Disc 2")).await,
        moves([(lib.release("Disc 2"), None)])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE folder_discovery SET first_seen_at = 500 WHERE folder = '{}';",
            lib.path("Disc 1")
        ))
        .await,
        moves([(lib.release("Discs"), Some(found("Discs").dated(200)))])
    );
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM release_grouping_member WHERE member_key = '{}';",
            lib.path("Disc 2")
        ))
        .await,
        moves([
            (lib.release("Disc 2"), Some(found("Disc 2").dated(200))),
            (lib.release("Discs"), Some(found("Discs").dated(500))),
        ])
    );
}

/// A grouping's release is skipped by its grouping, and one that cannot be
/// worked on as it stands is Found's import error.
#[tokio::test]
async fn a_grouping_skips_and_blocks_its_release() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.scan("Disc 1"),
        lib.grouping("Discs"),
        lib.member("Discs", 0, "Disc 1"),
        lib.grouping_release("Discs", "Disc 1")
    ))
    .await;
    let grouping = |set: &str| format!("{set} WHERE key = '{}';", lib.path("Discs"));
    assert_eq!(
        lib.moved(&grouping("UPDATE release_grouping SET skipped = 1"))
            .await,
        moves([(lib.release("Discs"), Some(found("Discs").on("skipped")))])
    );
    assert_eq!(
        lib.moved(&grouping(
            "UPDATE release_grouping SET skipped = 0, blocked = 'source_gone', \
             blocked_subject = 'Disc 1'"
        ))
        .await,
        moves([(
            lib.release("Discs"),
            Some(found("Discs").state("import_error"))
        )])
    );
}

/// Undoing a grouping takes its release out and gives its folders back.
#[tokio::test]
async fn undoing_a_grouping_gives_its_folders_back() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}",
        lib.scan("Disc 1"),
        lib.grouping("Discs"),
        lib.member("Discs", 0, "Disc 1"),
        lib.grouping_release("Discs", "Disc 1")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "DELETE FROM release_grouping WHERE key = '{}';",
            lib.path("Discs")
        ))
        .await,
        moves([
            (lib.release("Disc 1"), Some(found("Disc 1"))),
            (lib.release("Discs"), None),
        ])
    );
}

/// A grouping stored after the release it builds, in the same write, places
/// that release by it.
#[tokio::test]
async fn a_grouping_stored_after_its_release_places_it() {
    let lib = Library::new().await;
    assert_eq!(
        lib.moved(&format!(
            "{}INSERT INTO release_grouping (key, anchor_folder, combined, author, skipped)
             VALUES ('{}', NULL, 1, 'user', 1);",
            lib.grouping_release("Discs", "Disc 1"),
            lib.path("Discs")
        ))
        .await,
        moves([(lib.release("Discs"), Some(found("Discs").on("skipped")))])
    );
}

/// A grouping that takes in another folder is dated by that folder.
#[tokio::test]
async fn a_member_moved_to_another_folder_redates_its_grouping() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}{}{}",
        lib.discovered("Disc 1", 100),
        lib.discovered("Disc 2", 200),
        lib.grouping("Discs"),
        lib.member("Discs", 0, "Disc 1"),
        lib.grouping_release("Discs", "Disc 1")
    ))
    .await;
    assert_eq!(
        lib.moved(&format!(
            "UPDATE release_grouping_member SET member_folder = '{}' WHERE position = 0;",
            lib.path("Disc 2")
        ))
        .await,
        moves([(lib.release("Discs"), Some(found("Discs").dated(200)))])
    );
}
