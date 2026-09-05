use super::*;

fn dated(path: &str, at: i64) -> ScanCandidateListRow {
    ScanCandidateListRow {
        discovered_at: Some(at),
        ..candidate(path)
    }
}

#[test]
fn default_is_newest_first_and_groups_use_their_newest_member() {
    let mut rows = queue();
    rows.candidates = vec![
        dated("A/Old", 10),
        dated("B/Recent", 30),
        dated("A/New", 50),
        dated("Single", 40),
        candidate("Unknown"),
    ];
    let view = ImportListView::default();
    assert_eq!(view.order, ImportListOrder::NewestFirst);
    assert_eq!(
        sequence(&rows, &flattened(&rows, &view)),
        [
            "group A",
            "candidate A/New",
            "candidate A/Old",
            "candidate Single",
            "group B",
            "candidate B/Recent",
            "candidate Unknown",
        ]
    );
    let oldest = ImportListView {
        order: ImportListOrder::OldestFirst,
        ..view.clone()
    };
    assert_eq!(
        sequence(&rows, &flattened(&rows, &oldest)),
        [
            "group B",
            "candidate B/Recent",
            "candidate Single",
            "group A",
            "candidate A/Old",
            "candidate A/New",
            "candidate Unknown",
        ]
    );
    let location = locate_candidate(&rows, &request(view), &key("Single"))
        .unwrap()
        .unwrap();
    assert_eq!(location.visible_position, 3);
}

#[test]
fn dates_cross_watched_roots_and_ties_have_a_stable_natural_path_order() {
    let mut rows = queue();
    let second = host_root("/second");
    rows.watched_folders
        .push(WatchedFolder::from_path(second.clone()));
    rows.candidates = vec![
        dated("Release 10", 10),
        dated("Release 2", 10),
        ScanCandidateListRow {
            watched_folder_path: second.clone(),
            path: format!("{second}/Newest"),
            ..dated("Newest", 20)
        },
    ];
    let expected = [
        "candidate Newest",
        "candidate Release 2",
        "candidate Release 10",
    ];
    assert_eq!(
        sequence(&rows, &flattened(&rows, &ImportListView::default())),
        expected
    );
    rows.candidates.reverse();
    assert_eq!(
        sequence(&rows, &flattened(&rows, &ImportListView::default())),
        expected
    );
}

#[test]
fn name_orders_ignore_dates_and_case_ties_do_not_split_groups() {
    let mut rows = queue();
    rows.candidates = vec![dated("Box/1", 10), dated("box/2", 30), dated("Box/3", 20)];
    for (order, expected) in [
        (
            ImportListOrder::PathAscending,
            vec![
                "group Box",
                "candidate Box/1",
                "candidate Box/3",
                "group box",
                "candidate box/2",
            ],
        ),
        (
            ImportListOrder::PathDescending,
            vec![
                "group box",
                "candidate box/2",
                "group Box",
                "candidate Box/3",
                "candidate Box/1",
            ],
        ),
    ] {
        assert_eq!(
            sequence(
                &rows,
                &flattened(
                    &rows,
                    &ImportListView {
                        order,
                        ..ImportListView::default()
                    }
                )
            ),
            expected
        );
    }
}

#[test]
fn filtering_collapsing_and_paging_preserve_group_date_order() {
    let mut rows = queue();
    rows.candidates = vec![dated("A/Old", 10), dated("B/Old", 20), dated("A/New", 30)];
    let filtered = ImportListView {
        filter_text: "Old".into(),
        ..ImportListView::default()
    };
    let flat = flattened(&rows, &filtered);
    assert_eq!(
        sequence(&rows, &flat),
        ["group A", "candidate A/Old", "group B", "candidate B/Old"]
    );
    assert_eq!(
        window_refs(
            &flat.items,
            &crate::library::LibraryPageWindow {
                offset: 1,
                limit: 2
            }
        ),
        &flat.items[1..3]
    );
    let collapsed = ImportListView {
        collapsed_groups: BTreeSet::from([FolderReleaseDecisionKey {
            watched_folder_path: root(),
            relative_folder_path: "A".into(),
        }]),
        ..filtered
    };
    assert_eq!(
        sequence(&rows, &flattened(&rows, &collapsed)),
        ["group A", "group B", "candidate B/Old"]
    );
}

#[test]
fn done_honors_selected_order_within_upload_standings() {
    let mut rows = queue();
    rows.candidates = vec![dated("A", 300), dated("B", 200), dated("C", 100)];
    imported(&mut rows, "A", "rel-a", 10);
    imported(&mut rows, "B", "rel-b", 20);
    imported(&mut rows, "C", "rel-c", 30);
    for (order, expected) in [
        (
            ImportListOrder::NewestFirst,
            vec!["candidate A", "candidate C", "candidate B"],
        ),
        (
            ImportListOrder::OldestFirst,
            vec!["candidate A", "candidate B", "candidate C"],
        ),
        (
            ImportListOrder::PathAscending,
            vec!["candidate A", "candidate B", "candidate C"],
        ),
        (
            ImportListOrder::PathDescending,
            vec!["candidate A", "candidate C", "candidate B"],
        ),
    ] {
        let flat = flatten(
            &rows,
            &ImportListRequest {
                view: ImportListView {
                    tab: TriageTab::Done,
                    order,
                    ..ImportListView::default()
                },
                upload_standing: BTreeMap::from([("rel-a".into(), UploadStanding::Working)]),
                ..ImportListRequest::default()
            },
        )
        .unwrap();
        assert_eq!(sequence(&rows, &flat), expected);
    }
}
