use super::*;
use crate::import::folder_registry::host_root;
use crate::import::folder_scanner::{
    FolderReleaseTreeRow, FolderReleaseTreeRowKind, InvalidReason, ReleaseFileScope,
    ResolvedFolderReleaseBoundary,
};
use std::path::PathBuf;

fn empty_categorized() -> CategorizedFiles {
    CategorizedFiles {
        files: Vec::new(),
        format_label: "FLAC".to_string(),
    }
}

fn folder_candidate(path: &str, watched: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: format!("Candidate {path}"),
        files: empty_categorized(),
        watched_folder_path: watched.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path
            .strip_prefix(watched)
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

fn invalid_candidate(path: &str, watched: &str) -> InvalidCandidate {
    InvalidCandidate {
        path: PathBuf::from(path),
        name: format!("Invalid {path}"),
        watched_folder_path: watched.to_string(),
        display_path: path
            .strip_prefix(watched)
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_string(),
        resolved_boundaries: Vec::new(),
        reason: InvalidReason::NoValidAudio,
    }
}

fn boundary(watched: &str, relative: &str, candidate_keys: Vec<&str>) -> FolderReleaseBoundary {
    FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: watched.to_string(),
            relative_folder_path: relative.to_string(),
        },
        name: relative.rsplit('/').next().unwrap_or(relative).to_string(),
        display_path: relative.to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: candidate_keys.into_iter().map(str::to_string).collect(),
    }
}

fn resolved(
    watched: &str,
    relative: &str,
    decision: FolderReleaseDecision,
) -> ResolvedFolderReleaseBoundary {
    ResolvedFolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: watched.to_string(),
            relative_folder_path: relative.to_string(),
        },
        decision,
        name: relative.to_string(),
        display_path: relative.to_string(),
    }
}

fn watched(path: &str) -> WatchedFolder {
    WatchedFolder::from_path(path.to_string())
}

fn entries(keys: &[(&str, bool)]) -> Vec<StoredEntryKey> {
    keys.iter()
        .map(|(key, is_boundary)| StoredEntryKey {
            key: key.to_string(),
            is_boundary: *is_boundary,
        })
        .collect()
}

fn scan(watched_folder_path: &str, items: Vec<ScanItem>) -> crate::db::DbFolderScanSnapshot {
    crate::db::DbFolderScanSnapshot {
        watched_folder_path: watched_folder_path.to_string(),
        generation: 1,
        status: FolderScanStatus::Complete,
        items,
    }
}

#[test]
fn the_snapshot_orders_by_watched_folder_then_natural_path() {
    let snapshot = build_snapshot(
        vec![watched("/watch/b"), watched("/watch/a")],
        vec![
            scan(
                "/watch/a",
                vec![
                    ScanItem::Valid(folder_candidate("/watch/a/rel10", "/watch/a")),
                    ScanItem::Valid(folder_candidate("/watch/a/rel2", "/watch/a")),
                    ScanItem::Discovered(folder_candidate("/watch/a/tentative", "/watch/a")),
                    ScanItem::Invalid(invalid_candidate("/watch/a/broken", "/watch/a")),
                ],
            ),
            scan(
                "/watch/b",
                vec![ScanItem::Valid(folder_candidate(
                    "/watch/b/rel1",
                    "/watch/b",
                ))],
            ),
        ],
        &HashSet::from([("/watch/a".to_string(), "rel2".to_string())]),
        &HashSet::from([folder_candidate("/watch/a/rel10", "/watch/a")
            .files
            .content_hash()]),
    )
    .unwrap();

    let keys: Vec<_> = snapshot
        .folder_candidates
        .iter()
        .map(|row| row.candidate.path.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        keys,
        vec![
            "/watch/b/rel1",
            "/watch/a/rel2",
            "/watch/a/rel10",
            "/watch/a/tentative"
        ]
    );
    let row_for = |key: &str| snapshot.folder_candidate(key).unwrap();
    assert!(row_for("/watch/a/rel2").skipped);
    assert!(!row_for("/watch/a/rel10").skipped);
    assert!(
        row_for("/watch/a/rel10").is_added,
        "every candidate with that content is added"
    );
    assert!(
        row_for("/watch/a/rel2").is_added,
        "same empty file set, same content hash"
    );
    assert!(!row_for("/watch/a/tentative").actionable);
    assert!(row_for("/watch/a/rel2").actionable);
    assert_eq!(snapshot.invalid_candidates.len(), 1);
    assert_eq!(
        snapshot
            .folder_scan_statuses
            .iter()
            .map(|status| status.watched_folder_path.as_str())
            .collect::<Vec<_>>(),
        vec!["/watch/b", "/watch/a"]
    );
}

#[test]
fn a_scan_root_that_is_not_watched_is_an_invariant_break() {
    let error = build_snapshot(
        vec![watched("/watch/a")],
        vec![scan("/watch/gone", Vec::new())],
        &HashSet::new(),
        &HashSet::new(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("/watch/gone"));
}

#[test]
fn a_valid_candidate_supersedes_what_its_resolved_boundaries_hid() {
    let existing = entries(&[
        (&host_root("/watch/a/Box"), true),
        (&host_root("/watch/a/Box/CD1"), false),
        (&host_root("/watch/a/Box/CD2"), false),
        (&host_root("/watch/a/Other"), false),
    ]);
    let mut combined = folder_candidate(&host_root("/watch/a/Box"), &host_root("/watch/a"));
    combined.resolved_boundaries = vec![resolved(
        &host_root("/watch/a"),
        "Box",
        FolderReleaseDecision::CombineAsOneRelease,
    )];
    assert_eq!(
        superseded_entry_keys(&existing, &ScanItem::Valid(combined)),
        vec![host_root("/watch/a/Box/CD1"), host_root("/watch/a/Box/CD2")],
        "combining replaces everything below the folder, and never the item's own key"
    );

    let mut separate = folder_candidate(&host_root("/watch/a/Box/CD1"), &host_root("/watch/a"));
    separate.resolved_boundaries = vec![resolved(
        &host_root("/watch/a"),
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
    )];
    assert_eq!(
        superseded_entry_keys(&existing, &ScanItem::Valid(separate)),
        vec![host_root("/watch/a/Box")],
        "keeping separate replaces exactly the row at the folder — the boundary entry"
    );
}

#[test]
fn an_invalid_candidate_supersedes_only_the_boundaries_it_resolved() {
    let existing = entries(&[
        (&host_root("/watch/a/Box"), true),
        (&host_root("/watch/a/Box/CD1"), false),
    ]);
    let mut invalid = invalid_candidate(&host_root("/watch/a/Box/CD1"), &host_root("/watch/a"));
    invalid.resolved_boundaries = vec![resolved(
        &host_root("/watch/a"),
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
    )];
    assert_eq!(
        superseded_entry_keys(&existing, &ScanItem::Invalid(invalid)),
        vec![host_root("/watch/a/Box")]
    );
    let folder_not_boundary = entries(&[(&host_root("/watch/a/Box"), false)]);
    let mut invalid = invalid_candidate(&host_root("/watch/a/Box/CD1"), &host_root("/watch/a"));
    invalid.resolved_boundaries = vec![resolved(
        &host_root("/watch/a"),
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
    )];
    assert!(
        superseded_entry_keys(&folder_not_boundary, &ScanItem::Invalid(invalid)).is_empty(),
        "a folder row at the boundary path is not a boundary entry"
    );
}

#[test]
fn a_boundary_supersedes_the_tentative_candidates_it_hides_and_a_discovery_nothing() {
    let existing = entries(&[("/watch/a/Box/CD1", false), ("/watch/a/Box/CD2", false)]);
    assert_eq!(
        superseded_entry_keys(
            &existing,
            &ScanItem::Boundary(boundary(
                "/watch/a",
                "Box",
                vec!["/watch/a/Box/CD2", "/watch/a/Box/CD1"]
            ))
        ),
        vec!["/watch/a/Box/CD1", "/watch/a/Box/CD2"]
    );
    assert!(superseded_entry_keys(
        &existing,
        &ScanItem::Discovered(folder_candidate("/watch/a/Box/CD3", "/watch/a"))
    )
    .is_empty());
}

#[test]
fn grouping_folder_is_a_current_release_decision_target() {
    let items = vec![
        ScanItem::Valid(folder_candidate("/watch/a/Group/Release", "/watch/a")),
        ScanItem::Valid(folder_candidate("/watch/a/Group/Release2", "/watch/a")),
    ];
    let key = |relative: &str| FolderReleaseDecisionKey {
        watched_folder_path: "/watch/a".to_string(),
        relative_folder_path: relative.to_string(),
    };
    assert_eq!(
        release_boundary_ancestor_keys(&items, &key("Group")),
        Some(Vec::new())
    );
    assert_eq!(release_boundary_ancestor_keys(&items, &key("Other")), None);
}

#[test]
fn a_boundary_and_a_resolved_boundary_are_current_targets_with_no_ancestors() {
    let mut exposed = folder_candidate("/watch/a/Box/CD1", "/watch/a");
    exposed.resolved_boundaries = vec![resolved(
        "/watch/a",
        "Box",
        FolderReleaseDecision::KeepAsSeparateReleases,
    )];
    let items = vec![
        ScanItem::Valid(exposed),
        ScanItem::Boundary(boundary("/watch/a", "Set", Vec::new())),
    ];
    let key = |relative: &str| FolderReleaseDecisionKey {
        watched_folder_path: "/watch/a".to_string(),
        relative_folder_path: relative.to_string(),
    };
    assert_eq!(
        release_boundary_ancestor_keys(&items, &key("Box")),
        Some(Vec::new())
    );
    assert_eq!(
        release_boundary_ancestor_keys(&items, &key("Set")),
        Some(Vec::new())
    );
}

#[test]
fn a_nested_tree_row_carries_its_ancestors() {
    let mut set = boundary("/watch/a", "Set", Vec::new());
    let ancestor = FolderReleaseDecisionKey {
        watched_folder_path: "/watch/a".to_string(),
        relative_folder_path: "Set".to_string(),
    };
    let nested = FolderReleaseDecisionKey {
        watched_folder_path: "/watch/a".to_string(),
        relative_folder_path: "Set/Disc".to_string(),
    };
    set.tree_rows = vec![FolderReleaseTreeRow {
        name: "Disc".to_string(),
        display_path: "Set/Disc".to_string(),
        depth: 1,
        kind: FolderReleaseTreeRowKind::Folder,
        decision_key: nested.clone(),
        ancestor_decision_keys: vec![ancestor.clone()],
    }];
    let items = vec![ScanItem::Boundary(set)];
    assert_eq!(
        release_boundary_ancestor_keys(&items, &nested),
        Some(vec![ancestor])
    );
    let unknown = FolderReleaseDecisionKey {
        watched_folder_path: "/watch/a".to_string(),
        relative_folder_path: "Set/Other".to_string(),
    };
    assert_eq!(release_boundary_ancestor_keys(&items, &unknown), None);
}

#[test]
fn files_for_identity_matches_content_hash_and_revision() {
    let mut edited = folder_candidate("/watch/a/rel2", "/watch/a");
    edited.file_edit_revision = 1;
    let items = vec![
        ScanItem::Valid(folder_candidate("/watch/a/rel1", "/watch/a")),
        ScanItem::Discovered(folder_candidate("/watch/a/rel3", "/watch/a")),
        ScanItem::Valid(edited),
        ScanItem::Invalid(invalid_candidate("/watch/a/broken", "/watch/a")),
    ];
    let hash = empty_categorized().content_hash();
    let mut keys: Vec<_> = files_for_identity(&items, &hash, 0)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["/watch/a/rel1", "/watch/a/rel3"]);
    assert_eq!(files_for_identity(&items, &hash, 1).len(), 1);
    assert!(files_for_identity(&items, "other", 0).is_empty());
}
