use super::*;

fn row(position: &str, sub_tracks: Vec<DiscogsTrack>) -> DiscogsTrack {
    DiscogsTrack {
        position: position.to_string(),
        title: format!("Track Title {position}"),
        duration: None,
        artists: vec![],
        extraartists: None,
        type_: if sub_tracks.is_empty() { "track" } else { "index" }.to_string(),
        sub_tracks,
    }
}

fn rows(positions: &[&str]) -> Vec<DiscogsTrack> {
    positions.iter().map(|position| row(position, vec![])).collect()
}

/// The positions of the top-level rows `tracks` names.
fn named(tracklist: &[DiscogsTrack], tracks: &str) -> Vec<String> {
    named_rows(tracklist, tracks)
        .rows
        .into_iter()
        .map(|path| {
            let mut row = &tracklist[path[0]];
            for index in &path[1..] {
                row = &row.sub_tracks[*index];
            }
            row.position.clone()
        })
        .collect()
}

#[test]
fn items_name_positions_in_tracklist_order() {
    let tracklist = rows(&["1", "2", "3", "9", "21"]);
    assert_eq!(named(&tracklist, "3, 9, 21"), ["3", "9", "21"]);
    assert_eq!(named(&tracklist, "21,1"), ["1", "21"]);
}

#[test]
fn a_range_names_the_rows_from_its_first_through_its_last() {
    let tracklist = rows(&["14", "15", "16", "17", "18", "26"]);
    assert_eq!(named(&tracklist, "15 to 17, 26"), ["15", "16", "17", "26"]);
}

#[test]
fn a_range_runs_across_sides_and_discs() {
    let sides = rows(&["A1", "A2", "B1", "B2", "B3", "B4", "B5"]);
    assert_eq!(named(&sides, "A1 to B2, B4, B5"), ["A1", "A2", "B1", "B2", "B4", "B5"]);
    let discs = rows(&["50-1", "50-2", "50-3", "50-9", "51-1"]);
    assert_eq!(named(&discs, "50-1 to 50-9"), ["50-1", "50-2", "50-3", "50-9"]);
}

#[test]
fn spaces_and_empty_items_are_ignored() {
    let tracklist = rows(&["4", "5", "14-11", "14-12", "14-13"]);
    assert_eq!(named(&tracklist, "14-11,14-13"), ["14-11", "14-13"]);
    assert_eq!(named(&tracklist, "4, 5, "), ["4", "5"]);
    assert_eq!(named(&tracklist, ""), Vec::<String>::new());
}

#[test]
fn positions_compare_as_written_with_case_aside() {
    let tracklist = rows(&["A1", "a2", "B1"]);
    assert_eq!(named(&tracklist, "a1, A2"), ["A1", "a2"]);
    assert_eq!(named(&tracklist, "A1 TO a2"), ["A1", "a2"]);
}

#[test]
fn a_row_s_sub_tracks_are_part_of_it() {
    let tracklist = vec![
        row("1", vec![]),
        row("2", vec![row("2a", vec![]), row("2b", vec![])]),
        row("3", vec![]),
    ];
    assert_eq!(named(&tracklist, "2"), ["2", "2a", "2b"]);
    assert_eq!(named(&tracklist, "1 to 2"), ["1", "2", "2a", "2b"]);
    assert_eq!(named(&tracklist, "2b"), ["2b"]);
}

#[test]
fn an_item_naming_no_row_is_reported_and_the_others_still_name_theirs() {
    let tracklist = rows(&["1", "2", "3"]);
    let named = named_rows(&tracklist, "2, 7, 3 to 9, 3 to 1");
    assert_eq!(named.rows, [vec![1]]);
    assert_eq!(named.unnamed, ["7", "3 to 9", "3 to 1"]);
}

fn credit(tracks: Option<&str>) -> DiscogsRoleArtist {
    DiscogsRoleArtist {
        id: Some("1".to_string()),
        name: "Artist Name".to_string(),
        role: "Written-By".to_string(),
        credited_name: None,
        tracks: tracks.map(str::to_string),
    }
}

fn credited(tracklist: &[DiscogsTrack]) -> Vec<&str> {
    tracklist
        .iter()
        .filter(|row| row.extraartists.as_ref().is_some_and(|credits| !credits.is_empty()))
        .map(|row| row.position.as_str())
        .collect()
}

#[test]
fn a_credit_that_names_tracks_is_added_to_each_without_its_scope() {
    let tracklist = rows(&["1", "2", "3", "4", "5"]);

    let scoped = scoped("release-1", &tracklist, &[credit(Some("2, 5")), credit(None)]);

    assert_eq!(credited(&scoped), ["2", "5"]);
    assert_eq!(scoped[1].extraartists.as_ref().unwrap(), &[credit(None)]);
}

#[test]
fn a_credit_none_of_whose_tracks_exist_is_dropped_with_a_warning() {
    let tracklist = rows(&["1", "2"]);
    let mut result = Vec::new();
    let logs = bae_util::test_logs::capture_warn_logs(|| {
        result = scoped("release-1", &tracklist, &[credit(Some("8, 9"))]);
    });
    assert!(credited(&result).is_empty());
    assert!(logs.contains("release-1") && logs.contains("8"), "{logs}");
}

#[test]
fn a_credit_is_not_added_to_a_heading() {
    let mut tracklist = rows(&["1", "2"]);
    tracklist[0].type_ = "heading".to_string();
    let scoped = scoped("release-1", &tracklist, &[credit(Some("1 to 2"))]);
    assert_eq!(credited(&scoped), ["2"]);
}
